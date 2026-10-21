use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use rayon::prelude::*;
use serde::Deserialize;
use zip::ZipArchive;

use crate::cas::ContentAddressableStorage;
use crate::detector::Ecosystem;
use crate::fs::LinkStats;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PyPiReleaseFile {
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub packagetype: String,
    pub python_version: Option<String>,
    pub digests: Option<PyPiDigests>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PyPiDigests {
    pub sha256: Option<String>,
    pub blake2_256: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PyPiInfo {
    pub name: String,
    pub version: String,
    pub summary: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PyPiPackageMetadata {
    pub info: PyPiInfo,
    pub urls: Vec<PyPiReleaseFile>,
}

pub struct PyPiClient {
    http: reqwest::Client,
}

impl PyPiClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("upm/0.1.0 (Windows x86_64; Universal Package Manager)")
            .pool_idle_timeout(Some(std::time::Duration::from_secs(30)))
            .tcp_nodelay(true)
            .build()
            .expect("Failed to initialize HTTP client");
        Self { http }
    }

    /// Fetches package metadata from PyPI JSON API.
    pub async fn fetch_metadata(&self, pkg_name: &str) -> Result<PyPiPackageMetadata, String> {
        let clean_name = pkg_name.split(['=', '>', '<', '~', ';', '@']).next().unwrap_or(pkg_name).trim();
        let url = format!("https://pypi.org/pypi/{}/json", clean_name);

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("PyPI network error for '{}': {}", clean_name, e))?;

        if !response.status().is_success() {
            return Err(format!(
                "PyPI returned HTTP {} for package '{}'",
                response.status(),
                clean_name
            ));
        }

        let metadata: PyPiPackageMetadata = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse PyPI JSON for '{}': {}", clean_name, e))?;

        Ok(metadata)
    }

    /// Selects the optimal Windows x86_64 or universal wheel from PyPI release files.
    pub fn select_optimal_wheel<'a>(&self, files: &'a [PyPiReleaseFile]) -> Option<&'a PyPiReleaseFile> {
        // Collect all wheels
        let wheels: Vec<&PyPiReleaseFile> = files
            .iter()
            .filter(|f| f.filename.ends_with(".whl"))
            .collect();

        if wheels.is_empty() {
            return None;
        }

        // Rank priority:
        // 1. Windows x86_64 CPython wheel (e.g. cp312-win_amd64, cp311-win_amd64, etc.)
        // 2. Windows x86_64 generic (e.g. py3-none-win_amd64)
        // 3. Universal Python 3 (py3-none-any.whl, py2.py3-none-any.whl)
        // 4. Any other wheel

        let mut scored: Vec<(i32, &PyPiReleaseFile)> = wheels
            .into_iter()
            .map(|w| {
                let lower = w.filename.to_ascii_lowercase();
                let mut score = 0;
                if lower.contains("win_amd64") {
                    score += 100;
                    if lower.contains("cp313") {
                        score += 50;
                    } else if lower.contains("cp312") {
                        score += 48;
                    } else if lower.contains("cp311") {
                        score += 46;
                    } else if lower.contains("cp310") {
                        score += 44;
                    }
                } else if lower.contains("none-any") {
                    score += 80;
                    if lower.contains("py3") || lower.contains("py2.py3") {
                        score += 10;
                    }
                } else if lower.contains("any") {
                    score += 50;
                }
                (score, w)
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.first().map(|(_, w)| *w)
    }

    /// Downloads wheel directly into RAM as raw bytes.
    pub async fn download_wheel(&self, url: &str) -> Result<bytes::Bytes, String> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to download wheel from '{}': {}", url, e))?;

        if !resp.status().is_success() {
            return Err(format!("Wheel download failed with HTTP {}", resp.status()));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to stream wheel bytes: {}", e))?;

        Ok(bytes)
    }

    /// Decompresses wheel in-memory using SIMD libdeflater across CPU cores with Rayon and unpacks to CAS.
    pub fn unpack_wheel_to_cas(
        wheel_bytes: &[u8],
        cas: &ContentAddressableStorage,
        pkg_name: &str,
        version: &str,
    ) -> Result<PathBuf, String> {
        let target_cas_dir = cas.package_dir(Ecosystem::Python, pkg_name, version);
        if cas.has_package(Ecosystem::Python, pkg_name, version) {
            return Ok(target_cas_dir);
        }

        std::fs::create_dir_all(&target_cas_dir)
            .map_err(|e| format!("Failed to create CAS directory: {}", e))?;

        // Parse ZIP Central Directory to discover all entries
        let cursor = Cursor::new(wheel_bytes);
        let mut zip = ZipArchive::new(cursor)
            .map_err(|e| format!("Failed to parse wheel ZIP archive: {}", e))?;

        struct ZipItem {
            name: String,
            is_dir: bool,
            is_deflated: bool,
            data_start: usize,
            comp_size: usize,
            uncomp_size: usize,
        }

        let mut items = Vec::with_capacity(zip.len());
        for i in 0..zip.len() {
            let file = zip.by_index(i).map_err(|e| format!("Zip entry error: {}", e))?;
            let is_deflated = file.compression() == zip::CompressionMethod::Deflated;
            items.push(ZipItem {
                name: file.name().to_string(),
                is_dir: file.is_dir(),
                is_deflated,
                data_start: file.data_start() as usize,
                comp_size: file.compressed_size() as usize,
                uncomp_size: file.size() as usize,
            });
        }

        let bytes_arc = Arc::new(wheel_bytes.to_vec());
        let cas_dir_arc = Arc::new(target_cas_dir.clone());

        // Hybrid parallel pipeline: Rayon CPU work-stealing thread pool + SIMD libdeflater
        items.par_iter().try_for_each(|item| -> Result<(), String> {
            let out_path = cas_dir_arc.join(&item.name);
            if item.is_dir {
                std::fs::create_dir_all(&out_path)
                    .map_err(|e| format!("Failed to create directory {:?}: {}", out_path, e))?;
                return Ok(());
            }

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent {:?}: {}", parent, e))?;
            }

            let slice = &bytes_arc[item.data_start..item.data_start + item.comp_size];

            if item.is_deflated {
                let mut decompressed = vec![0u8; item.uncomp_size];
                let mut decompressor = libdeflater::Decompressor::new();

                match decompressor.deflate_decompress(slice, &mut decompressed) {
                    Ok(written) => {
                        decompressed.truncate(written);
                        std::fs::write(&out_path, &decompressed)
                            .map_err(|e| format!("Failed to write decompressed file {:?}: {}", out_path, e))?;
                    }
                    Err(_) => {
                        // Fallback: Inflate using flate2 if libdeflater encounters non-standard stream
                        use flate2::read::DeflateDecoder;
                        use std::io::Read;
                        let mut decoder = DeflateDecoder::new(slice);
                        let mut buf = Vec::with_capacity(item.uncomp_size);
                        decoder.read_to_end(&mut buf)
                            .map_err(|e| format!("Decompression fallback error on {:?}: {}", out_path, e))?;
                        std::fs::write(&out_path, &buf)
                            .map_err(|e| format!("Failed to write file {:?}: {}", out_path, e))?;
                    }
                }
            } else {
                // Stored (no compression)
                std::fs::write(&out_path, slice)
                    .map_err(|e| format!("Failed to write stored file {:?}: {}", out_path, e))?;
            }

            Ok(())
        })?;

        Ok(target_cas_dir)
    }

    /// Links unpacked package from CAS into project virtual environment.
    pub fn link_to_venv(
        cas_pkg_dir: &Path,
        site_packages_dir: &Path,
        scripts_dir: &Path,
    ) -> Result<LinkStats, String> {
        let stats = crate::fs::link_directory_tree(cas_pkg_dir, site_packages_dir)
            .map_err(|e| format!("Failed to zero-copy link package: {}", e))?;

        // Scan for entry_points.txt in any .dist-info folder to create console_scripts shims
        if let Ok(entries) = std::fs::read_dir(cas_pkg_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.file_name().unwrap_or_default().to_string_lossy().ends_with(".dist-info") {
                    let ep_path = path.join("entry_points.txt");
                    if ep_path.exists() {
                        let _ = Self::create_console_scripts(&ep_path, scripts_dir);
                    }
                }
            }
        }

        Ok(stats)
    }

    /// Creates fast Windows batch and powershell script wrappers for console_scripts.
    fn create_console_scripts(entry_points_path: &Path, scripts_dir: &Path) -> std::io::Result<()> {
        let content = std::fs::read_to_string(entry_points_path)?;
        let mut in_console_scripts = false;

        for line in content.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_console_scripts = line == "[console_scripts]";
                continue;
            }

            if in_console_scripts && line.contains('=') {
                let parts: Vec<&str> = line.splitn(2, '=').collect();
                let cmd_name = parts[0].trim();
                let target = parts[1].trim();

                let module = target.split(':').next().unwrap_or("").trim();
                let func = target.split(':').nth(1).unwrap_or("main").trim();

                // Create .cmd and .bat wrappers
                let cmd_content = format!(
                    "@echo off\n\
                     setlocal\n\
                     set \"PYTHONPATH=%~dp0..\\Lib\\site-packages;%PYTHONPATH%\"\n\
                     python.exe -c \"import sys; from {} import {}; sys.exit({}() if callable({}) else 0)\" %*\n",
                    module, func, func, func,
                );

                let _ = std::fs::write(scripts_dir.join(format!("{}.cmd", cmd_name)), &cmd_content);
                let _ = std::fs::write(scripts_dir.join(format!("{}.bat", cmd_name)), &cmd_content);
            }
        }
        Ok(())
    }
}
