use std::path::{Path, PathBuf};
use crate::detector::Ecosystem;

pub struct ContentAddressableStorage {
    root: PathBuf,
}

#[derive(Default, Debug)]
pub struct StoreStats {
    pub package_count: usize,
    pub total_files: usize,
    pub total_size_bytes: u64,
}

impl ContentAddressableStorage {
    /// Initializes Content-Addressable Storage at `%LOCALAPPDATA%\upm\store\`
    pub fn new() -> std::io::Result<Self> {
        let root = if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(local_app_data).join("upm").join("store")
        } else if let Ok(user_profile) = std::env::var("USERPROFILE") {
            PathBuf::from(user_profile).join(".upm").join("store")
        } else {
            PathBuf::from(".upm_store")
        };

        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn root_dir(&self) -> &Path {
        &self.root
    }

    /// Computes BLAKE3 hash of an in-memory buffer at 10+ GB/s using SIMD instructions.
    pub fn hash_buffer(buf: &[u8]) -> String {
        let hash = blake3::hash(buf);
        hash.to_hex().to_string()
    }

    /// Returns path for an unpacked package in CAS.
    pub fn package_dir(&self, ecosystem: Ecosystem, name: &str, version: &str) -> PathBuf {
        let eco_dir = match ecosystem {
            Ecosystem::Python => "python",
            Ecosystem::JavaScript => "npm",
            Ecosystem::Binary => "bin",
            Ecosystem::System => "sys",
        };
        self.root
            .join(eco_dir)
            .join(name.to_lowercase())
            .join(version)
    }

    /// Checks if a package version already exists in the local CAS store.
    pub fn has_package(&self, ecosystem: Ecosystem, name: &str, version: &str) -> bool {
        let dir = self.package_dir(ecosystem, name, version);
        dir.exists() && dir.is_dir()
    }

    /// Calculates statistics for the CAS store.
    pub fn stats(&self) -> std::io::Result<StoreStats> {
        let mut stats = StoreStats::default();

        fn scan_dir(dir: &Path, stats: &mut StoreStats, is_pkg_level: bool) -> std::io::Result<()> {
            if !dir.exists() {
                return Ok(());
            }
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    if is_pkg_level {
                        stats.package_count += 1;
                    }
                    scan_dir(&path, stats, false)?;
                } else if path.is_file() {
                    stats.total_files += 1;
                    stats.total_size_bytes += entry.metadata()?.len();
                }
            }
            Ok(())
        }

        for eco in &["python", "npm", "bin", "sys"] {
            let eco_dir = self.root.join(eco);
            if eco_dir.exists() {
                for pkg in std::fs::read_dir(eco_dir)?.flatten() {
                    if pkg.path().is_dir() {
                        scan_dir(&pkg.path(), &mut stats, true)?;
                    }
                }
            }
        }

        Ok(stats)
    }

    /// Cleans the CAS store by removing all files.
    pub fn clean(&self) -> std::io::Result<u64> {
        let stats = self.stats()?;
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root)?;
            std::fs::create_dir_all(&self.root)?;
        }
        Ok(stats.total_size_bytes)
    }
}
