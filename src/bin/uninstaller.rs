#[path = "../terminal.rs"]
mod terminal;
#[path = "../installer.rs"]
mod installer;

use clap::Parser;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

use installer::Installer;
use terminal::{init_console_vt, print_banner, BADGE_UPM};

#[derive(Parser)]
#[command(
    name = "upm-uninstaller",
    about = "Uninstaller for UPM (Universal Package Manager) on Windows",
    version = "0.1.0"
)]
struct UninstallerCli {
    /// Directory where UPM is installed (default: %LOCALAPPDATA%\upm\bin)
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Remove UPM from the User PATH environment variable
    #[arg(long, conflicts_with = "no_remove_from_path")]
    remove_from_path: bool,

    /// Do NOT remove UPM from the User PATH
    #[arg(long, conflicts_with = "remove_from_path")]
    no_remove_from_path: bool,

    /// Delete UPM package cache store (%LOCALAPPDATA%\upm\store)
    #[arg(long)]
    clean_cache: bool,

    /// Non-interactive uninstallation with default options
    #[arg(short = 'y', long = "yes")]
    assume_yes: bool,
}

fn main() {
    init_console_vt();
    print_banner();

    println!("\x1b[1;37mWelcome to the UPM (Universal Package Manager) Uninstaller!\x1b[0m\n");

    let cli = UninstallerCli::parse();

    let target_dir = cli.dir.unwrap_or_else(Installer::default_install_dir);

    if !cli.assume_yes {
        println!(
            "{} Target installation directory: \x1b[1;36m{}\x1b[0m",
            BADGE_UPM,
            target_dir.display()
        );
        let confirmed = Installer::prompt_yes_no("Are you sure you want to completely uninstall UPM?", true);
        if !confirmed {
            println!("{} Uninstallation cancelled.", BADGE_UPM);
            pause_if_terminal();
            return;
        }
    }

    // Determine remove_from_path
    let should_remove_path = if cli.remove_from_path {
        true
    } else if cli.no_remove_from_path {
        false
    } else if cli.assume_yes {
        true
    } else {
        println!();
        println!("{} \x1b[1mOptions:\x1b[0m", BADGE_UPM);
        Installer::prompt_yes_no("Remove UPM from your User PATH environment variable? (Recommended)", true)
    };

    // Determine clean_cache
    let should_clean_cache = if cli.clean_cache {
        true
    } else if cli.assume_yes {
        false
    } else {
        Installer::prompt_yes_no("Delete UPM package store cache (%LOCALAPPDATA%\\upm\\store)?", false)
    };

    println!("\n{} Uninstalling UPM...", BADGE_UPM);

    match Installer::uninstall(Some(target_dir), should_remove_path, should_clean_cache) {
        Ok(_) => {
            println!("\n\x1b[1;32m══════════════════════════════════════════════════════════════════\x1b[0m");
            println!("\x1b[1;32m✓ UPM has been completely removed from your system.\x1b[0m");
            println!("\x1b[1;32m══════════════════════════════════════════════════════════════════\x1b[0m\n");
        }
        Err(e) => {
            eprintln!("{} \x1b[1;31mUninstallation encountered an issue:\x1b[0m {}", BADGE_UPM, e);
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
