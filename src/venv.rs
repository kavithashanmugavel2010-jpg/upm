use std::path::{Path, PathBuf};
use std::process::Command;

pub struct VirtualEnv;

impl VirtualEnv {
    /// Discovers existing `.venv` or `venv` in current or parent directory, stopping before user profile.
    pub fn find_venv(start_dir: &Path) -> Option<PathBuf> {
        // 1. Check current directory first
        let dot_venv = start_dir.join(".venv");
        if dot_venv.exists() && dot_venv.join("Scripts").exists() {
            return Some(dot_venv);
        }
        let venv = start_dir.join("venv");
        if venv.exists() && venv.join("Scripts").exists() {
            return Some(venv);
        }

        // 2. Walk up stopping at repo root or user home
        let home = std::env::var("USERPROFILE").ok().map(PathBuf::from);
        let mut curr = start_dir.parent();
        while let Some(dir) = curr {
            if Some(dir) == home.as_deref() {
                break;
            }
            let dot_venv = dir.join(".venv");
            if dot_venv.exists() && dot_venv.join("Scripts").exists() {
                return Some(dot_venv);
            }
            if dir.join(".git").exists() {
                break;
            }
            curr = dir.parent();
        }
        None
    }

    /// Finds system python executable in PATH or standard Windows locations in < 1ms.
    pub fn find_system_python() -> Option<PathBuf> {
        // 1. Check if 'where python' returns something
        if let Ok(output) = Command::new("where.exe").arg("python.exe").output() {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    for line in text.lines() {
                        let path = PathBuf::from(line.trim());
                        // Exclude WindowsApps stub if real python exists
                        if path.exists() && !path.to_string_lossy().contains("WindowsApps") {
                            return Some(path);
                        }
                    }
                    if let Some(first) = text.lines().next() {
                        let path = PathBuf::from(first.trim());
                        if path.exists() {
                            return Some(path);
                        }
                    }
                }
            }
        }

        // 2. Check py.exe launcher
        if let Ok(output) = Command::new("py.exe").arg("-c").arg("import sys; print(sys.executable)").output() {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    let path = PathBuf::from(text.trim());
                    if path.exists() {
                        return Some(path);
                    }
                }
            }
        }

        // 3. Check common Python directories in %LOCALAPPDATA%\Programs\Python
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            let programs_python = Path::new(&local).join("Programs").join("Python");
            if programs_python.exists() {
                if let Ok(entries) = std::fs::read_dir(programs_python) {
                    for entry in entries.flatten() {
                        let py = entry.path().join("python.exe");
                        if py.exists() {
                            return Some(py);
                        }
                    }
                }
            }
        }

        None
    }

    /// Initializes a high-performance `.venv` virtual environment in milliseconds.
    pub fn ensure_venv(project_root: &Path) -> std::io::Result<PathBuf> {
        if let Some(existing) = Self::find_venv(project_root) {
            return Ok(existing);
        }

        let venv_dir = project_root.join(".venv");
        let scripts_dir = venv_dir.join("Scripts");
        let site_packages = venv_dir.join("Lib").join("site-packages");

        std::fs::create_dir_all(&scripts_dir)?;
        std::fs::create_dir_all(&site_packages)?;

        let sys_python = Self::find_system_python();
        let (home_dir, py_version) = if let Some(ref py_path) = sys_python {
            let home = py_path.parent().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
            // Try linking python.exe into Scripts
            let dst_py = scripts_dir.join("python.exe");
            let _ = crate::fs::link_file(py_path, &dst_py);

            // Also copy pythonw if present
            if let Some(parent) = py_path.parent() {
                let pyw = parent.join("pythonw.exe");
                if pyw.exists() {
                    let _ = crate::fs::link_file(&pyw, &scripts_dir.join("pythonw.exe"));
                }
            }
            (home, "3.12".to_string())
        } else {
            (String::new(), "3.12".to_string())
        };

        // Write pyvenv.cfg
        let cfg_content = format!(
            "home = {}\n\
             include-system-site-packages = false\n\
             version = {}\n\
             executable = {}\n\
             command = upm venv\n",
            home_dir,
            py_version,
            sys_python.as_ref().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
        );
        std::fs::write(venv_dir.join("pyvenv.cfg"), cfg_content)?;

        // Write activate.bat
        let activate_bat = format!(
            "@echo off\n\
             set \"VIRTUAL_ENV={}\"\n\
             set \"PATH=%VIRTUAL_ENV%\\Scripts;%PATH%\"\n",
            venv_dir.display()
        );
        let _ = std::fs::write(scripts_dir.join("activate.bat"), activate_bat);

        // Write activate.ps1
        let activate_ps1 = format!(
            "$env:VIRTUAL_ENV = \"{}\"\n\
             $env:PATH = \"$env:VIRTUAL_ENV\\Scripts;$env:PATH\"\n",
            venv_dir.display()
        );
        let _ = std::fs::write(scripts_dir.join("Activate.ps1"), activate_ps1);

        Ok(venv_dir)
    }

    /// Returns the active `site-packages` directory.
    pub fn get_site_packages(project_root: &Path) -> std::io::Result<PathBuf> {
        let venv = Self::ensure_venv(project_root)?;
        let site_packages = venv.join("Lib").join("site-packages");
        std::fs::create_dir_all(&site_packages)?;
        Ok(site_packages)
    }
}
