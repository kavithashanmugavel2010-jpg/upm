use std::path::{Path, PathBuf};
use crate::terminal::format_bytes;

pub struct BinaryInstaller;

impl BinaryInstaller {
    pub fn get_bin_dir() -> PathBuf {
        if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(appdata).join("upm").join("bin")
        } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
            PathBuf::from(userprofile).join(".upm").join("bin")
        } else {
            PathBuf::from(".upm").join("bin")
        }
    }

    /// Installs a local or downloaded binary into the UPM bin directory.
    pub fn install_binary(src_path: &Path, bin_name: &str) -> Result<PathBuf, String> {
        let bin_dir = Self::get_bin_dir();
        std::fs::create_dir_all(&bin_dir)
            .map_err(|e| format!("Failed to create bin dir: {}", e))?;

        let exe_name = if bin_name.ends_with(".exe") {
            bin_name.to_string()
        } else {
            format!("{}.exe", bin_name)
        };

        let target_path = bin_dir.join(exe_name);
        crate::fs::link_file(src_path, &target_path)
            .map_err(|e| format!("Failed to link binary to {:?}: {}", target_path, e))?;

        Ok(target_path)
    }

    /// Downloads and installs a binary from a direct URL.
    pub async fn install_from_url(url: &str, bin_name: &str) -> Result<PathBuf, String> {
        let http = reqwest::Client::new();
        let resp = http
            .get(url)
            .send()
            .await
            .map_err(|e| format!("Failed to download binary: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("Download failed with HTTP {}", resp.status()));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read binary bytes: {}", e))?;

        let bin_dir = Self::get_bin_dir();
        std::fs::create_dir_all(&bin_dir)
            .map_err(|e| format!("Failed to create bin dir: {}", e))?;

        let target_path = bin_dir.join(if bin_name.ends_with(".exe") {
            bin_name.to_string()
        } else {
            format!("{}.exe", bin_name)
        });

        std::fs::write(&target_path, &bytes)
            .map_err(|e| format!("Failed to write binary: {}", e))?;

        println!(
            "  -> Saved {} ({}) to {}",
            bin_name,
            format_bytes(bytes.len() as u64),
            target_path.display()
        );

        Ok(target_path)
    }
}
