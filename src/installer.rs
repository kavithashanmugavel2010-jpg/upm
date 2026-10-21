use std::path::{Path, PathBuf};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use std::io::{self, IsTerminal, Write};
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteKeyW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE,
};

use crate::terminal::{init_console_vt, BADGE_UPM};

pub struct Installer;

#[allow(dead_code)]
impl Installer {
    /// Returns the default installation directory (%LOCALAPPDATA%\upm\bin)
    pub fn default_install_dir() -> PathBuf {
        if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
            PathBuf::from(appdata).join("upm").join("bin")
        } else if let Ok(userprofile) = std::env::var("USERPROFILE") {
            PathBuf::from(userprofile).join(".upm").join("bin")
        } else {
            PathBuf::from(".upm").join("bin")
        }
    }

    /// Reads the current User PATH from the Windows Registry (HKCU\Environment)
    pub fn get_user_path() -> Result<(String, u32), String> {
        unsafe {
            let mut hkey: HKEY = std::ptr::null_mut();
            let subkey: Vec<u16> = OsStr::new("Environment\0").encode_wide().collect();
            let status = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_READ,
                &mut hkey,
            );

            if status != 0 {
                return Err(format!("Failed to open HKCU\\Environment registry key (code: {})", status));
            }

            let value_name: Vec<u16> = OsStr::new("Path\0").encode_wide().collect();
            let mut val_type: u32 = 0;
            let mut byte_size: u32 = 0;

            // Query size first
            let q_status = RegQueryValueExW(
                hkey,
                value_name.as_ptr(),
                std::ptr::null_mut(),
                &mut val_type,
                std::ptr::null_mut(),
                &mut byte_size,
            );

            if q_status != 0 {
                RegCloseKey(hkey);
                // Value might not exist yet, which is fine
                return Ok((String::new(), REG_EXPAND_SZ));
            }

            let mut buffer = vec![0u8; byte_size as usize + 2];
            let read_status = RegQueryValueExW(
                hkey,
                value_name.as_ptr(),
                std::ptr::null_mut(),
                &mut val_type,
                buffer.as_mut_ptr(),
                &mut byte_size,
            );

            RegCloseKey(hkey);

            if read_status != 0 {
                return Err(format!("Failed to read User PATH value (code: {})", read_status));
            }

            let u16_slice: &[u16] = std::slice::from_raw_parts(
                buffer.as_ptr() as *const u16,
                (byte_size as usize) / 2,
            );
            let s = String::from_utf16_lossy(u16_slice)
                .trim_matches('\0')
                .to_string();

            Ok((s, val_type))
        }
    }

    /// Sets the User PATH in the Windows Registry (HKCU\Environment) and broadcasts WM_SETTINGCHANGE
    pub fn set_user_path(new_path: &str, val_type: u32) -> Result<(), String> {
        unsafe {
            let mut hkey: HKEY = std::ptr::null_mut();
            let subkey: Vec<u16> = OsStr::new("Environment\0").encode_wide().collect();
            let status = RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_WRITE,
                &mut hkey,
            );

            if status != 0 {
                return Err(format!("Failed to open HKCU\\Environment for writing (code: {})", status));
            }

            let value_name: Vec<u16> = OsStr::new("Path\0").encode_wide().collect();
            let mut wide_val: Vec<u16> = OsStr::new(new_path).encode_wide().collect();
            wide_val.push(0); // Null terminator

            let byte_size = (wide_val.len() * std::mem::size_of::<u16>()) as u32;
            let reg_type = if val_type == REG_SZ { REG_SZ } else { REG_EXPAND_SZ };

            let set_status = RegSetValueExW(
                hkey,
                value_name.as_ptr(),
                0,
                reg_type,
                wide_val.as_ptr() as *const u8,
                byte_size,
            );

            RegCloseKey(hkey);

            if set_status != 0 {
                return Err(format!("Failed to set User PATH registry value (code: {})", set_status));
            }

            // Broadcast WM_SETTINGCHANGE to notify running applications (Explorer, Cmd, etc.)
            Self::broadcast_environment_change();

            Ok(())
        }
    }

    /// Broadcasts WM_SETTINGCHANGE with lParam="Environment" so all shells update their environment
    pub fn broadcast_environment_change() {
        unsafe {
            let env_str: Vec<u16> = OsStr::new("Environment\0").encode_wide().collect();
            let mut result: usize = 0;
            SendMessageTimeoutW(
                HWND_BROADCAST as HWND,
                WM_SETTINGCHANGE,
                0 as WPARAM,
                env_str.as_ptr() as LPARAM,
                SMTO_ABORTIFHUNG,
                3000,
                &mut result,
            );
        }
    }

    /// Checks if a directory is currently in the User PATH
    pub fn is_dir_in_path(dir: &Path) -> bool {
        let (current_path, _) = match Self::get_user_path() {
            Ok(p) => p,
            Err(_) => return false,
        };

        let dir_str = dir.to_string_lossy().to_lowercase();
        current_path
            .split(';')
            .any(|segment| segment.trim().to_lowercase() == dir_str)
    }

    /// Adds the given directory to the Windows User PATH if not already present
    pub fn add_dir_to_path(dir: &Path) -> Result<bool, String> {
        let (current_path, val_type) = Self::get_user_path()?;
        let dir_str = dir.to_string_lossy().to_string();

        let already_present = current_path
            .split(';')
            .any(|seg| seg.trim().eq_ignore_ascii_case(&dir_str));

        if already_present {
            return Ok(false);
        }

        let new_path = if current_path.trim().is_empty() {
            dir_str
        } else if current_path.ends_with(';') {
            format!("{}{}", current_path, dir_str)
        } else {
            format!("{};{}", current_path, dir_str)
        };

        Self::set_user_path(&new_path, val_type)?;
        Ok(true)
    }

    /// Removes the given directory from the Windows User PATH
    pub fn remove_dir_from_path(dir: &Path) -> Result<bool, String> {
        let (current_path, val_type) = Self::get_user_path()?;
        let dir_str = dir.to_string_lossy().to_string();

        let parts: Vec<&str> = current_path
            .split(';')
            .filter(|seg| !seg.trim().is_empty() && !seg.trim().eq_ignore_ascii_case(&dir_str))
            .collect();

        if parts.len() == current_path.split(';').filter(|s| !s.trim().is_empty()).count() {
            // Nothing removed
            return Ok(false);
        }

        let new_path = parts.join(";");
        Self::set_user_path(&new_path, val_type)?;
        Ok(true)
    }

    /// Prompts the user with a yes/no question. Returns the default if stdin is not a terminal.
    pub fn prompt_yes_no(question: &str, default_yes: bool) -> bool {
        if !io::stdin().is_terminal() {
            return default_yes;
        }

        let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
        print!("{} {} {} ", BADGE_UPM, question, hint);
        let _ = io::stdout().flush();

        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return default_yes;
        }

        let trimmed = input.trim().to_lowercase();
        if trimmed.is_empty() {
            default_yes
        } else if trimmed == "y" || trimmed == "yes" {
            true
        } else if trimmed == "n" || trimmed == "no" {
            false
        } else {
            default_yes
        }
    }

    /// Finds a valid upm.exe binary to install
    pub fn locate_upm_binary(explicit: Option<&Path>) -> Result<PathBuf, String> {
        if let Some(p) = explicit {
            if p.exists() && p.is_file() {
                return Ok(p.to_path_buf());
            }
            return Err(format!("Specified binary '{}' does not exist", p.display()));
        }

        if let Ok(current) = std::env::current_exe() {
            let file_name = current.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            if file_name == "upm.exe" || file_name == "upm" {
                return Ok(current);
            }

            // Check adjacent directory
            if let Some(parent) = current.parent() {
                let adjacent = parent.join("upm.exe");
                if adjacent.exists() {
                    return Ok(adjacent);
                }
            }
        }

        // Check relative paths from current working directory
        let candidates = [
            PathBuf::from("upm.exe"),
            PathBuf::from("target/release/upm.exe"),
            PathBuf::from("target/debug/upm.exe"),
        ];

        for candidate in &candidates {
            if candidate.exists() && candidate.is_file() {
                return Ok(candidate.canonicalize().unwrap_or_else(|_| candidate.clone()));
            }
        }

        Err("Could not find 'upm.exe'. Please build it first with 'cargo build --release' or specify the path.".to_string())
    }

    /// Registers UPM in Windows Add/Remove Programs (HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\UPM)
    pub fn register_windows_uninstall(install_dir: &Path, exe_path: &Path) -> Result<(), String> {
        unsafe {
            let mut hkey: HKEY = std::ptr::null_mut();
            let subkey: Vec<u16> = OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\UPM\0")
                .encode_wide()
                .collect();

            let status = RegCreateKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                std::ptr::null_mut(),
                0,
                KEY_WRITE,
                std::ptr::null_mut(),
                &mut hkey,
                std::ptr::null_mut(),
            );

            if status != 0 {
                return Err(format!("Failed to create uninstall registry key (code: {})", status));
            }

            let write_string_val = |name: &str, val: &str| {
                let name_w: Vec<u16> = OsStr::new(name).encode_wide().chain(std::iter::once(0)).collect();
                let mut val_w: Vec<u16> = OsStr::new(val).encode_wide().chain(std::iter::once(0)).collect();
                let byte_size = (val_w.len() * std::mem::size_of::<u16>()) as u32;
                RegSetValueExW(
                    hkey,
                    name_w.as_ptr(),
                    0,
                    REG_SZ,
                    val_w.as_mut_ptr() as *const u8,
                    byte_size,
                );
            };

            let uninstaller_exe = install_dir.join("upm-uninstaller.exe");
            let uninstall_cmd = if uninstaller_exe.exists() {
                format!("\"{}\"", uninstaller_exe.display())
            } else {
                format!("\"{}\" self-uninstall", exe_path.display())
            };
            let quiet_cmd = if uninstaller_exe.exists() {
                format!("\"{}\" -y", uninstaller_exe.display())
            } else {
                format!("\"{}\" self-uninstall --remove-from-path", exe_path.display())
            };

            write_string_val("DisplayName", "UPM - Universal Package Manager");
            write_string_val("DisplayVersion", "0.1.0");
            write_string_val("Publisher", "UPM Contributors");
            write_string_val("InstallLocation", &install_dir.to_string_lossy());
            write_string_val("UninstallString", &uninstall_cmd);
            write_string_val("QuietUninstallString", &quiet_cmd);
            write_string_val("DisplayIcon", &exe_path.to_string_lossy());

            RegCloseKey(hkey);
            Ok(())
        }
    }

    /// Removes UPM entry from Windows Add/Remove Programs
    pub fn unregister_windows_uninstall() -> Result<(), String> {
        unsafe {
            let subkey: Vec<u16> = OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\UPM\0")
                .encode_wide()
                .collect();
            let status = RegDeleteKeyW(HKEY_CURRENT_USER, subkey.as_ptr());
            if status == 0 || status == 2 {
                Ok(())
            } else {
                Err(format!("Failed to delete uninstall registry key (code: {})", status))
            }
        }
    }

    /// Installs UPM to the target directory and optionally adds it to PATH
    pub fn install(
        source_exe: Option<&Path>,
        target_dir: Option<PathBuf>,
        add_to_path: bool,
    ) -> Result<PathBuf, String> {
        init_console_vt();
        let install_dir = target_dir.unwrap_or_else(Self::default_install_dir);

        std::fs::create_dir_all(&install_dir)
            .map_err(|e| format!("Failed to create installation directory {}: {}", install_dir.display(), e))?;

        let src_exe = Self::locate_upm_binary(source_exe)?;
        let dest_exe = install_dir.join("upm.exe");

        // If the source exe is already the destination exe, skip copying to avoid locking errors
        let is_same_file = src_exe.canonicalize().ok() == dest_exe.canonicalize().ok();
        if !is_same_file {
            std::fs::copy(&src_exe, &dest_exe)
                .map_err(|e| format!("Failed to copy {} to {}: {}", src_exe.display(), dest_exe.display(), e))?;
        }

        println!("{} \x1b[1;32m✓\x1b[0m Installed \x1b[1mupm.exe\x1b[0m to \x1b[1;36m{}\x1b[0m", BADGE_UPM, dest_exe.display());

        // Copy upm-uninstaller.exe if present
        let uninstaller_candidates = [
            src_exe.parent().unwrap_or(Path::new("")).join("upm-uninstaller.exe"),
            PathBuf::from("upm-uninstaller.exe"),
            PathBuf::from("target/release/upm-uninstaller.exe"),
            PathBuf::from("target/debug/upm-uninstaller.exe"),
        ];

        for cand in &uninstaller_candidates {
            if cand.exists() && cand.is_file() {
                let dest_uninstaller = install_dir.join("upm-uninstaller.exe");
                if cand.canonicalize().ok() != dest_uninstaller.canonicalize().ok() {
                    let _ = std::fs::copy(cand, &dest_uninstaller);
                    println!("{} \x1b[1;32m✓\x1b[0m Installed \x1b[1mupm-uninstaller.exe\x1b[0m to \x1b[1;36m{}\x1b[0m", BADGE_UPM, dest_uninstaller.display());
                }
                break;
            }
        }

        // Register in Windows Add/Remove Programs
        if let Err(e) = Self::register_windows_uninstall(&install_dir, &dest_exe) {
            eprintln!("{} \x1b[90m(Notice: Windows Settings uninstall registration skipped: {})\x1b[0m", BADGE_UPM, e);
        } else {
            println!("{} \x1b[1;32m✓\x1b[0m Registered in Windows Installed Apps (Settings / Control Panel)", BADGE_UPM);
        }

        if add_to_path {
            match Self::add_dir_to_path(&install_dir) {
                Ok(true) => {
                    println!("{} \x1b[1;32m✓\x1b[0m Added \x1b[1m{}\x1b[0m to User \x1b[1;33mPATH\x1b[0m in Windows Registry", BADGE_UPM, install_dir.display());
                    println!("{} \x1b[90m(Broadcasted WM_SETTINGCHANGE; restart terminal or open a new window to use 'upm')\x1b[0m", BADGE_UPM);
                }
                Ok(false) => {
                    println!("{} \x1b[90mDirectory is already in User PATH: {}\x1b[0m", BADGE_UPM, install_dir.display());
                }
                Err(e) => {
                    eprintln!("{} \x1b[1;31mWarning:\x1b[0m Failed to update User PATH: {}", BADGE_UPM, e);
                }
            }
        } else {
            println!("{} Skipped adding to PATH. To use 'upm' globally, add \x1b[1m{}\x1b[0m to your PATH.", BADGE_UPM, install_dir.display());
        }

        println!();
        crate::terminal::print_version_and_guide();

        Ok(dest_exe)
    }

    /// Uninstalls UPM from the installation directory, cleans PATH, and optionally cleans cache
    pub fn uninstall(
        target_dir: Option<PathBuf>,
        remove_from_path: bool,
        clean_cache: bool,
    ) -> Result<(), String> {
        init_console_vt();
        let install_dir = target_dir.unwrap_or_else(Self::default_install_dir);
        let dest_exe = install_dir.join("upm.exe");
        let dest_uninstaller = install_dir.join("upm-uninstaller.exe");

        // 1. Unregister from Windows Add/Remove Programs
        let _ = Self::unregister_windows_uninstall();
        println!("{} \x1b[1;32m✓\x1b[0m Unregistered UPM from Windows Settings / Control Panel", BADGE_UPM);

        // 2. Remove from PATH
        if remove_from_path {
            match Self::remove_dir_from_path(&install_dir) {
                Ok(true) => {
                    println!("{} \x1b[1;32m✓\x1b[0m Removed \x1b[1m{}\x1b[0m from User PATH", BADGE_UPM, install_dir.display());
                    println!("{} \x1b[90m(Broadcasted WM_SETTINGCHANGE)\x1b[0m", BADGE_UPM);
                }
                Ok(false) => {
                    println!("{} \x1b[90mDirectory was not in User PATH\x1b[0m", BADGE_UPM);
                }
                Err(e) => {
                    eprintln!("{} Failed to update User PATH: {}", BADGE_UPM, e);
                }
            }
        } else {
            println!("{} Preserved PATH entry for {}", BADGE_UPM, install_dir.display());
        }

        // 3. Clean cache if requested
        if clean_cache {
            if let Ok(appdata) = std::env::var("LOCALAPPDATA") {
                let upm_store = PathBuf::from(&appdata).join("upm").join("store");
                let upm_run = PathBuf::from(&appdata).join("upm").join("run");
                if upm_store.exists() {
                    let _ = std::fs::remove_dir_all(&upm_store);
                    println!("{} \x1b[1;32m✓\x1b[0m Removed package cache store: {}", BADGE_UPM, upm_store.display());
                }
                if upm_run.exists() {
                    let _ = std::fs::remove_dir_all(&upm_run);
                    println!("{} \x1b[1;32m✓\x1b[0m Removed ephemeral runtime directory: {}", BADGE_UPM, upm_run.display());
                }
            }
        }

        // 4. Remove binaries
        let current_exe = std::env::current_exe().ok();
        let running_inside_install_dir = current_exe
            .as_ref()
            .and_then(|p| p.parent())
            .and_then(|p| p.canonicalize().ok())
            == install_dir.canonicalize().ok();

        if running_inside_install_dir {
            // Schedule delayed self-deletion via cmd.exe so running file unlocks
            let script = format!(
                "ping 127.0.0.1 -n 2 > nul & del /f /q \"{}\" 2>nul & del /f /q \"{}\" 2>nul & rmdir \"{}\" 2>nul",
                dest_exe.display(),
                dest_uninstaller.display(),
                install_dir.display()
            );
            std::process::Command::new("cmd.exe")
                .arg("/c")
                .arg(script)
                .spawn()
                .map_err(|e| format!("Failed to spawn background deletion process: {}", e))?;
            println!("{} \x1b[1;32m✓\x1b[0m Scheduled file cleanup and removal of \x1b[1m{}\x1b[0m", BADGE_UPM, install_dir.display());
        } else {
            if dest_exe.exists() {
                let _ = std::fs::remove_file(&dest_exe);
                println!("{} \x1b[1;32m✓\x1b[0m Removed \x1b[1m{}\x1b[0m", BADGE_UPM, dest_exe.display());
            }
            if dest_uninstaller.exists() {
                let _ = std::fs::remove_file(&dest_uninstaller);
                println!("{} \x1b[1;32m✓\x1b[0m Removed \x1b[1m{}\x1b[0m", BADGE_UPM, dest_uninstaller.display());
            }
            let _ = std::fs::remove_dir(&install_dir);
        }

        println!("{} \x1b[1;32m✓\x1b[0m UPM has been successfully uninstalled.", BADGE_UPM);
        Ok(())
    }
}
