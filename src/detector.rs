use std::path::Path;
use crate::terminal::{BADGE_BIN, BADGE_JS, BADGE_PY, BADGE_SYS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecosystem {
    Python,
    JavaScript,
    Binary,
    System,
}

impl Ecosystem {
    pub fn badge(&self) -> &'static str {
        match self {
            Ecosystem::Python => BADGE_PY,
            Ecosystem::JavaScript => BADGE_JS,
            Ecosystem::Binary => BADGE_BIN,
            Ecosystem::System => BADGE_SYS,
        }
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &'static str {
        match self {
            Ecosystem::Python => "Python",
            Ecosystem::JavaScript => "JavaScript",
            Ecosystem::Binary => "Binary",
            Ecosystem::System => "System",
        }
    }

    #[allow(dead_code)]
    pub fn default_target_dir(&self, project_root: &Path) -> std::path::PathBuf {
        match self {
            Ecosystem::Python => project_root.join(".venv").join("Lib").join("site-packages"),
            Ecosystem::JavaScript => project_root.join("node_modules"),
            Ecosystem::Binary => {
                if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
                    Path::new(&appdata).join("upm").join("bin")
                } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
                    Path::new(&userprofile).join(".upm").join("bin")
                } else {
                    project_root.join("bin")
                }
            }
            Ecosystem::System => {
                if let Ok(userprofile) = std::env::var("USERPROFILE") {
                    Path::new(&userprofile).join(".upm").join("bin")
                } else {
                    project_root.join("bin")
                }
            }
        }
    }
}

pub struct DetectionResult {
    pub ecosystem: Ecosystem,
    pub package_query: String,
    pub detected_by: &'static str,
}

pub struct EcosystemDetector;

impl EcosystemDetector {
    /// Detects ecosystem from CLI package argument and/or local directory manifests in < 0.2ms.
    pub fn detect(pkg_arg: &str, current_dir: &Path) -> DetectionResult {
        let trimmed = pkg_arg.trim();

        // 1. Explicit CLI prefix detection
        if let Some(rest) = trimmed.strip_prefix("pypi:").or_else(|| trimmed.strip_prefix("py:")) {
            return DetectionResult {
                ecosystem: Ecosystem::Python,
                package_query: rest.to_string(),
                detected_by: "cli-prefix 'pypi:'",
            };
        }
        if let Some(rest) = trimmed.strip_prefix("npm:").or_else(|| trimmed.strip_prefix("js:")) {
            return DetectionResult {
                ecosystem: Ecosystem::JavaScript,
                package_query: rest.to_string(),
                detected_by: "cli-prefix 'npm:'",
            };
        }
        if let Some(rest) = trimmed.strip_prefix("bin:") {
            return DetectionResult {
                ecosystem: Ecosystem::Binary,
                package_query: rest.to_string(),
                detected_by: "cli-prefix 'bin:'",
            };
        }
        if let Some(rest) = trimmed.strip_prefix("sys:") {
            return DetectionResult {
                ecosystem: Ecosystem::System,
                package_query: rest.to_string(),
                detected_by: "cli-prefix 'sys:'",
            };
        }

        // 2. Direct file archive detection
        let lower = trimmed.to_ascii_lowercase();
        if lower.ends_with(".whl") {
            return DetectionResult {
                ecosystem: Ecosystem::Python,
                package_query: trimmed.to_string(),
                detected_by: "wheel file extension",
            };
        }
        if lower.ends_with(".tgz") || lower.ends_with(".tar.gz") {
            return DetectionResult {
                ecosystem: Ecosystem::JavaScript,
                package_query: trimmed.to_string(),
                detected_by: "tarball extension",
            };
        }
        if lower.ends_with(".exe") {
            return DetectionResult {
                ecosystem: Ecosystem::Binary,
                package_query: trimmed.to_string(),
                detected_by: "binary file extension",
            };
        }

        // 3. Ultra-fast local directory manifest scan (<0.2 ms)
        if current_dir.join("pyproject.toml").exists()
            || current_dir.join("requirements.txt").exists()
            || current_dir.join("setup.py").exists()
            || current_dir.join(".venv").exists()
        {
            return DetectionResult {
                ecosystem: Ecosystem::Python,
                package_query: trimmed.to_string(),
                detected_by: "local Python manifest (pyproject.toml / requirements.txt / .venv)",
            };
        }

        if current_dir.join("package.json").exists()
            || current_dir.join("node_modules").exists()
            || current_dir.join("pnpm-lock.yaml").exists()
        {
            return DetectionResult {
                ecosystem: Ecosystem::JavaScript,
                package_query: trimmed.to_string(),
                detected_by: "local JS manifest (package.json / node_modules)",
            };
        }

        // Default heuristic: If no manifests exist, defaults to Python (primary fast target)
        DetectionResult {
            ecosystem: Ecosystem::Python,
            package_query: trimmed.to_string(),
            detected_by: "default ecosystem heuristic",
        }
    }
}
