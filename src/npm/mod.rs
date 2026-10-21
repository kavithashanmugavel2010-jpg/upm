use std::io::Cursor;
use std::path::{Path, PathBuf};
use serde::Deserialize;

use crate::cas::ContentAddressableStorage;
use crate::detector::Ecosystem;
use crate::fs::LinkStats;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NpmDist {
    pub tarball: String,
    pub shasum: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NpmPackageVersion {
    pub name: String,
    pub version: String,
    pub dist: NpmDist,
    pub bin: Option<serde_json::Value>,
}

pub struct NpmClient {
    http: reqwest::Client,
}

impl NpmClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .user_agent("upm/0.1.0 (Windows x86_64; Universal Package Manager)")
            .pool_idle_timeout(Some(std::time::Duration::from_secs(30)))
            .tcp_nodelay(true)
            .build()
            .expect("Failed to initialize HTTP client");
        Self { http }
    }

    /// Fetches package metadata from npm registry.
    pub async fn fetch_version_info(&self, pkg_name: &str) -> Result<NpmPackageVersion, String> {
        let clean_name = pkg_name.split('@').next().unwrap_or(pkg_name).trim();
        let url = format!("https://registry.npmjs.org/{}/latest", clean_name);

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("npm network error for '{}': {}", clean_name, e))?;

        if !response.status().is_success() {
            return Err(format!("npm registry returned HTTP {} for package '{}'", response.status(), clean_name));
        }

        let info: NpmPackageVersion = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse npm JSON for '{}': {}", clean_name, e))?;

        Ok(info)
    }

    /// Streams tarball directly into memory.
    pub async fn download_tarball(&self, url: &str) -> Result<bytes::Bytes, String> {
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to download tarball from '{}': {}", url, e))?;

        if !resp.status().is_success() {
            return Err(format!("Tarball download failed with HTTP {}", resp.status()));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to stream tarball bytes: {}", e))?;

        Ok(bytes)
    }

    /// Decompresses npm .tgz archive in-memory using SIMD libdeflater and unpacks to CAS.
    pub fn unpack_tgz_to_cas(
        tgz_bytes: &[u8],
        cas: &ContentAddressableStorage,
        pkg_name: &str,
        version: &str,
    ) -> Result<PathBuf, String> {
        let target_cas_dir = cas.package_dir(Ecosystem::JavaScript, pkg_name, version);
        if cas.has_package(Ecosystem::JavaScript, pkg_name, version) {
            return Ok(target_cas_dir);
        }

        std::fs::create_dir_all(&target_cas_dir)
            .map_err(|e| format!("Failed to create CAS directory: {}", e))?;

        // 1. Decompress gzip stream using SIMD libdeflater with flate2 fallback
        let mut tar_bytes = Vec::new();
        let mut decompressor = libdeflater::Decompressor::new();

        // Estimate uncompressed size (approx 4x to 8x compressed size)
        let mut uncompressed_buf = vec![0u8; tgz_bytes.len() * 6 + 1024 * 1024];

        match decompressor.gzip_decompress(tgz_bytes, &mut uncompressed_buf) {
            Ok(size) => {
                uncompressed_buf.truncate(size);
                tar_bytes = uncompressed_buf;
            }
            Err(_) => {
                // Fallback to flate2 GzDecoder
                use flate2::read::GzDecoder;
                use std::io::Read;
                let mut decoder = GzDecoder::new(tgz_bytes);
                decoder.read_to_end(&mut tar_bytes)
                    .map_err(|e| format!("Gzip decompression failed: {}", e))?;
            }
        }

        // 2. Unpack tar archive entries in-memory into CAS
        let mut archive = tar::Archive::new(Cursor::new(tar_bytes));
        let entries = archive.entries().map_err(|e| format!("Tar error: {}", e))?;

        for entry in entries {
            let mut entry = entry.map_err(|e| format!("Tar entry error: {}", e))?;
            let path = entry.path().map_err(|e| format!("Tar path error: {}", e))?;

            // npm tarball files typically reside in "package/..."
            let stripped_path = match path.strip_prefix("package") {
                Ok(p) => p.to_path_buf(),
                Err(_) => path.into_owned(),
            };

            let out_path = target_cas_dir.join(stripped_path);
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {}", e))?;
            }

            entry.unpack(&out_path).map_err(|e| format!("Unpack failed for {:?}: {}", out_path, e))?;
        }

        Ok(target_cas_dir)
    }

    /// Links unpacked package from CAS into project `node_modules/{name}`.
    pub fn link_to_node_modules(
        cas_pkg_dir: &Path,
        project_root: &Path,
        pkg_name: &str,
    ) -> Result<LinkStats, String> {
        let node_modules = project_root.join("node_modules");
        let dest = node_modules.join(pkg_name);
        std::fs::create_dir_all(&dest).map_err(|e| format!("Failed to create node_modules: {}", e))?;

        let stats = crate::fs::link_directory_tree(cas_pkg_dir, &dest)
            .map_err(|e| format!("Failed to zero-copy link package: {}", e))?;

        // Handle binary shims in node_modules/.bin
        let pkg_json_path = cas_pkg_dir.join("package.json");
        if pkg_json_path.exists() {
            let _ = Self::create_npm_bin_shims(&pkg_json_path, &dest, &node_modules);
        }

        Ok(stats)
    }

    fn create_npm_bin_shims(pkg_json_path: &Path, pkg_installed_path: &Path, node_modules: &Path) -> std::io::Result<()> {
        let content = std::fs::read_to_string(pkg_json_path)?;
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            let bin_dir = node_modules.join(".bin");
            std::fs::create_dir_all(&bin_dir)?;

            if let Some(bin) = val.get("bin") {
                if let Some(obj) = bin.as_object() {
                    for (name, target) in obj {
                        if let Some(target_str) = target.as_str() {
                            let target_file = pkg_installed_path.join(target_str);
                            let cmd_file = bin_dir.join(format!("{}.cmd", name));
                            let cmd_content = format!(
                                "@ECHO off\n\
                                 node \"{}\" %*\n",
                                target_file.display()
                            );
                            let _ = std::fs::write(cmd_file, cmd_content);
                        }
                    }
                } else if let Some(bin_str) = bin.as_str() {
                    let pkg_name = val.get("name").and_then(|n| n.as_str()).unwrap_or("tool");
                    let target_file = pkg_installed_path.join(bin_str);
                    let cmd_file = bin_dir.join(format!("{}.cmd", pkg_name));
                    let cmd_content = format!(
                        "@ECHO off\n\
                         node \"{}\" %*\n",
                        target_file.display()
                    );
                    let _ = std::fs::write(cmd_file, cmd_content);
                }
            }
        }
        Ok(())
    }
}
