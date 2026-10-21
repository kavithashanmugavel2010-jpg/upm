#![allow(dead_code)]

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
};

static VT_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initializes Windows Console for Virtual Terminal (ANSI Escape sequences) processing.
pub fn init_console_vt() {
    if VT_INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    unsafe {
        for &std_handle in &[STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let handle = GetStdHandle(std_handle);
            if handle != std::ptr::null_mut() && handle != windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
                let mut mode: u32 = 0;
                if GetConsoleMode(handle, &mut mode) != 0 {
                    let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }
}

// Zero-allocation ANSI badge constants as per specification
pub const BADGE_PY: &str = "\x1b[1;33m[PY]\x1b[0m";       // Yellow
pub const BADGE_JS: &str = "\x1b[1;93m[JS]\x1b[0m";       // Bright Yellow
pub const BADGE_BIN: &str = "\x1b[1;96m[BIN]\x1b[0m";     // Bright Cyan
pub const BADGE_SYS: &str = "\x1b[1;92m[SYS]\x1b[0m";     // Bright Green
pub const BADGE_UPM: &str = "\x1b[1;38;2;0;229;255m[UPM]\x1b[0m"; // Electric Cyan

pub fn print_banner() {
    init_console_vt();
    println!(
        "\x1b[1;38;2;0;229;255m╔══════════════════════════════════════════════════════════════════╗\x1b[0m\n\
         \x1b[1;38;2;0;229;255m║\x1b[0m  \x1b[1;37mUPM\x1b[0m \x1b[90m-\x1b[0m \x1b[1;38;2;0;229;255mUniversal Package Manager\x1b[0m \x1b[90m(Native Win32 x86_64 SIMD)\x1b[0m  \x1b[1;38;2;0;229;255m║\x1b[0m\n\
         \x1b[1;38;2;0;229;255m╚══════════════════════════════════════════════════════════════════╝\x1b[0m"
    );
}

pub fn print_version_and_guide() {
    init_console_vt();
    println!(
        "\x1b[1;38;2;0;229;255m\
 ██╗   ██╗██████╗ ███╗   ███╗\n\
 ██║   ██║██╔══██╗████╗ ████║   \x1b[1;37mUniversal Package Manager\x1b[0m\n\
 ██║   ██║██████╔╝██╔████╔██║   \x1b[1;38;2;0;229;255mv0.1.0\x1b[0m \x1b[90m(x86_64-pc-windows-msvc Native SIMD)\x1b[0m\n\
 ██║   ██║██╔═══╝ ██║╚██╔╝██║   \x1b[1;32mEngineered & Built by Sarvesh.S\x1b[0m\n\
 ╚██████╔╝██║     ██║ ╚═╝ ██║   \x1b[36mZero-Copy NTFS / In-Memory SIMD / CAS\x1b[0m\n\
  ╚═════╝ ╚═╝     ╚═╝     ╚═╝\x1b[0m\n\
"
    );

    println!("\x1b[1;37m══════════════════════════════════════════════════════════════════════════\x1b[0m");
    println!(" \x1b[1;33m⚡ SUPPORTED ECOSYSTEMS & RUNTIMES\x1b[0m");
    println!("  • \x1b[1;33mPython (PyPI)\x1b[0m      : Direct wheel streaming, libdeflater SIMD, instant .venv linking");
    println!("  • \x1b[1;93mNode.js (npm)\x1b[0m      : npm registry tarballs, SIMD gzip unpack, node_modules linking");
    println!("  • \x1b[1;96mStandalone Binaries\x1b[0m: Native .exe & GitHub releases provisioned to ~/.upm/bin");
    println!("  • \x1b[1;92mSystem Toolchains\x1b[0m  : Fast-path shimming and native tool execution");

    println!("\n\x1b[1;37m══════════════════════════════════════════════════════════════════════════\x1b[0m");
    println!(" \x1b[1;38;2;0;229;255m📖 QUICK USAGE GUIDE & COMMANDS\x1b[0m");
    println!("  \x1b[1;36mupm install <package>\x1b[0m       Install packages (e.g. requests, npm:chalk, bin:ripgrep)");
    println!("  \x1b[1;36mupm x <tool> [args...]\x1b[0m      Run any tool on-the-fly without project install (like npx/uvx)");
    println!("  \x1b[1;36mupm run <cmd> [args...]\x1b[0m     Run inside active .venv or node_modules/.bin");
    println!("  \x1b[1;36mupm env [--reset]\x1b[0m           Inspect or rebuild virtual environment (.venv)");
    println!("  \x1b[1;36mupm cache stats | clean\x1b[0m     Display or purge Content-Addressable Storage (CAS)");
    println!("  \x1b[1;36mupm self-install\x1b[0m            Install UPM into Windows and configure User PATH");
    println!("  \x1b[1;36mupm self-uninstall\x1b[0m          Cleanly remove UPM and deregister from PATH");
    println!("  \x1b[1;36mupm info\x1b[0m                    Display CPU SIMD vectorization & Win32 diagnostics");
    println!("  \x1b[1;36mupm version\x1b[0m                 Display this banner, author credits, and guide");

    println!("\n\x1b[1;37m══════════════════════════════════════════════════════════════════════════\x1b[0m");
    println!(" \x1b[1;35m🚀 EXAMPLES TO TRY RIGHT NOW\x1b[0m");
    println!("  \x1b[90m# Run cowsay on-the-fly in milliseconds (no prior install):\x1b[0m");
    println!("  upm x cowsay \"UPM engineered by Sarvesh.S\"");
    println!("  \x1b[90m# Install Python package with microsecond timings:\x1b[0m");
    println!("  upm install requests --verbose");
    println!("  \x1b[90m# Check Win32 hardware acceleration:\x1b[0m");
    println!("  upm info");
    println!("\x1b[1;37m══════════════════════════════════════════════════════════════════════════\x1b[0m\n");
}

pub fn format_duration(d: Duration) -> String {
    let micros = d.as_micros();
    if micros < 1000 {
        format!("{}µs", micros)
    } else {
        let millis = d.as_secs_f64() * 1000.0;
        if millis < 1000.0 {
            format!("{:.2}ms", millis)
        } else {
            format!("{:.2}s", d.as_secs_f64())
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
