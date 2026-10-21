#[path = "../terminal.rs"]
mod terminal;
#[path = "../installer.rs"]
mod installer;

use clap::Parser;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use installer::Installer;
use terminal::{init_console_vt, print_banner, BADGE_UPM};

#[derive(Parser)]
#[command(
    name = "upm-installer",
    about = "Installer for UPM (Universal Package Manager) on Windows",
    version = "0.1.0"
)]
struct InstallerCli {
    /// Destination directory for upm.exe (default: %LOCALAPPDATA%\upm\bin)
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Add UPM installation directory to User PATH
    #[arg(long, conflicts_with = "no_add_to_path")]
    add_to_path: bool,

    /// Do NOT add UPM installation directory to User PATH
    #[arg(long, conflicts_with = "add_to_path")]
    no_add_to_path: bool,

    /// Non-interactive installation with default settings
    #[arg(short = 'y', long = "yes")]
    assume_yes: bool,

    /// Path to the upm.exe binary to install (optional)
    #[arg(short = 'b', long = "binary")]
    binary: Option<PathBuf>,
}

fn main() {
    init_console_vt();
    print_banner();

    println!("\x1b[1;37mWelcome to the UPM (Universal Package Manager) Installer!\x1b[0m\n");

    let cli = InstallerCli::parse();

    let target_dir = match cli.dir {
        Some(d) => d,
        None => {
            let default_dir = Installer::default_install_dir();
            if cli.assume_yes || !io::stdin().is_terminal() {
                default_dir
            } else {
                println!(
                    "{} Default installation directory: \x1b[1;36m{}\x1b[0m",
                    BADGE_UPM,
                    default_dir.display()
                );
                print!("{} Use this directory? [Y/n] (or enter custom path): ", BADGE_UPM);
                let _ = io::stdout().flush();

                let mut input = String::new();
                let _ = io::stdin().read_line(&mut input);
                let trimmed = input.trim();

                if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("y") || trimmed.eq_ignore_ascii_case("yes") {
                    default_dir
                } else if trimmed.eq_ignore_ascii_case("n") || trimmed.eq_ignore_ascii_case("no") {
                    print!("{} Enter custom installation directory: ", BADGE_UPM);
                    let _ = io::stdout().flush();
                    let mut custom = String::new();
                    let _ = io::stdin().read_line(&mut custom);
                    PathBuf::from(custom.trim())
                } else {
                    PathBuf::from(trimmed)
                }
            }
        }
    };

    // Determine add_to_path option
    let should_add_to_path = if cli.add_to_path {
        true
    } else if cli.no_add_to_path {
        false
    } else if cli.assume_yes {
        true
    } else {
        println!();
        println!("{} \x1b[1mConfiguration:\x1b[0m", BADGE_UPM);
        Installer::prompt_yes_no("Add UPM to your User PATH environment variable? (Recommended)", true)
    };

    println!("\n{} Installing UPM...", BADGE_UPM);

    // Locate source binary
    let source_binary = match Installer::locate_upm_binary(cli.binary.as_deref()) {
        Ok(p) => p,
        Err(err) => {
            // If in source directory without upm.exe built, offer to build release binary
            if Path::new("Cargo.toml").exists() {
                println!("{} upm.exe not found. Building release binary via cargo...", BADGE_UPM);
                let status = std::process::Command::new("cargo")
                    .args(["build", "--release"])
                    .status();
                if let Ok(s) = status {
                    if s.success() {
                        let built = PathBuf::from("target/release/upm.exe");
                        if built.exists() {
                            built
                        } else {
                            eprintln!("{} Build succeeded but target/release/upm.exe was not found.", BADGE_UPM);
                            pause_if_terminal();
                            std::process::exit(1);
                        }
                    } else {
                        eprintln!("{} Failed to build release binary.", BADGE_UPM);
                        pause_if_terminal();
                        std::process::exit(1);
                    }
                } else {
                    eprintln!("{} {}", BADGE_UPM, err);
                    pause_if_terminal();
                    std::process::exit(1);
                }
            } else {
                eprintln!("{} {}", BADGE_UPM, err);
                pause_if_terminal();
                std::process::exit(1);
            }
        }
    };

    match Installer::install(Some(&source_binary), Some(target_dir.clone()), should_add_to_path) {
        Ok(dest) => {
            println!("\n\x1b[1;32m══════════════════════════════════════════════════════════════════\x1b[0m");
            println!("\x1b[1;32m✓ UPM has been successfully installed!\x1b[0m");
            println!("  Binary: \x1b[1;36m{}\x1b[0m", dest.display());
            if should_add_to_path {
                println!("  PATH:   \x1b[1;32mConfigured\x1b[0m (Windows User PATH updated)");
            } else {
                println!("  PATH:   \x1b[1;33mNot added\x1b[0m (Add {} manually to PATH)", target_dir.display());
            }
            println!("\x1b[1;32m══════════════════════════════════════════════════════════════════\x1b[0m\n");
            println!("To get started, open a \x1b[1mnew terminal window\x1b[0m and run:");
            println!("  \x1b[1;33mupm info\x1b[0m");
            println!("  \x1b[1;33mupm install requests\x1b[0m");
            println!("  \x1b[1;33mupm x cowsay \"Hello World\"\x1b[0m\n");
        }
        Err(e) => {
            eprintln!("{} \x1b[1;31mInstallation failed:\x1b[0m {}", BADGE_UPM, e);
            pause_if_terminal();
            std::process::exit(1);
        }
    }

    pause_if_terminal();
}

fn pause_if_terminal() {
    if io::stdin().is_terminal() {
        print!("Press Enter to exit...");
        let _ = io::stdout().flush();
        let mut discard = String::new();
        let _ = io::stdin().read_line(&mut discard);
    }
}
