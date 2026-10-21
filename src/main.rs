mod binary;
mod cas;
mod detector;
mod fs;
mod installer;
mod npm;
mod pypi;
mod terminal;
mod venv;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::time::Instant;

use cas::ContentAddressableStorage;
use detector::{Ecosystem, EcosystemDetector};
use terminal::{
    format_bytes, format_duration, init_console_vt, print_banner, print_version_and_guide,
    BADGE_BIN, BADGE_JS, BADGE_PY, BADGE_SYS, BADGE_UPM,
};

#[derive(Parser)]
#[command(
    name = "upm",
    about = "Universal Package Manager - Ultra-high-performance cross-ecosystem installer for Windows",
    version = "0.1.0"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Install software packages into the active project or system environment
    #[command(alias = "add", alias = "i")]
    Install {
        /// Packages to install (e.g. requests, pypi:flask, npm:chalk, bin:ripgrep)
        #[arg(required = true)]
        packages: Vec<String>,

        /// Force re-download and re-link even if already in cache
        #[arg(short, long)]
        reinstall: bool,

        /// Verbose output showing microsecond timings and Win32 linker syscall details
        #[arg(short, long)]
        verbose: bool,
    },

    /// Run a command inside the project environment (.venv or node_modules/.bin)
    #[command(disable_help_flag = true, disable_version_flag = true)]
    Run {
        /// Command to execute (falls back to ephemeral runner if not found locally)
        command: String,

        /// Arguments passed to the command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Run any package directly on-the-fly without project install (like npx / uvx)
    #[command(alias = "exec", disable_help_flag = true, disable_version_flag = true)]
    X {
        /// Package to run (e.g. cowsay, ruff, npm:cowsay, pypi:flask)
        package: String,

        /// Arguments passed to the package executable
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },

    /// Manage or inspect the project environment (.venv or node_modules)
    Env {
        /// Force re-creation of .venv
        #[arg(long)]
        reset: bool,
    },

    /// Inspect and manage Content-Addressable Storage (CAS)
    Cache {
        #[command(subcommand)]
        action: Option<CacheAction>,
    },

    /// Install UPM itself onto your system and configure PATH
    #[command(alias = "setup")]
    SelfInstall {
        /// Target installation directory (defaults to %LOCALAPPDATA%\upm\bin)
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// Add UPM installation directory to the User PATH
        #[arg(long, conflicts_with = "no_add_to_path")]
        add_to_path: bool,

        /// Do NOT add UPM installation directory to the User PATH
        #[arg(long, conflicts_with = "add_to_path")]
        no_add_to_path: bool,
    },

    /// Uninstall UPM from the system and clean up PATH
    SelfUninstall {
        /// Target installation directory (defaults to %LOCALAPPDATA%\upm\bin)
        #[arg(short, long)]
        dir: Option<PathBuf>,

        /// Remove UPM directory from the User PATH
        #[arg(long, conflicts_with = "no_remove_from_path")]
        remove_from_path: bool,

        /// Do NOT remove UPM directory from the User PATH
        #[arg(long, conflicts_with = "remove_from_path")]
        no_remove_from_path: bool,

        /// Also delete UPM package cache (%LOCALAPPDATA%\upm\store)
        #[arg(long)]
        clean_cache: bool,

        /// Non-interactive uninstall (skip confirmation prompts)
        #[arg(short = 'y', long = "yes")]
        assume_yes: bool,
    },

    /// Display hardware acceleration, SIMD diagnostics, and kernel driver capabilities
    Info,

    /// Display UPM version, graphical banner, author credits, and quickstart guide
    Version,
}

#[derive(Subcommand)]
enum CacheAction {
    /// Print the CAS store path
    Dir,
    /// Display storage statistics and cached packages
    Stats,
    /// Clear all cached packages from the global store
    Clean,
}

#[tokio::main]
async fn main() {
    init_console_vt();
    let start_total = Instant::now();

    let cli = Cli::parse();
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    match cli.command {
        Commands::Install {
            packages,
            reinstall,
            verbose,
        } => {
            handle_install(&packages, &current_dir, reinstall, verbose).await;
        }
        Commands::Run { command, args } => {
            handle_run(&command, &args, &current_dir).await;
        }
        Commands::X { package, args } => {
            handle_x(&package, &args, &current_dir).await;
        }
        Commands::Env { reset } => {
            handle_env(&current_dir, reset);
        }
        Commands::Cache { action } => {
            handle_cache(action.unwrap_or(CacheAction::Stats));
        }
        Commands::SelfInstall {
            dir,
            add_to_path,
            no_add_to_path,
        } => {
            let should_add = if add_to_path {
                true
            } else if no_add_to_path {
                false
            } else {
                installer::Installer::prompt_yes_no("Add UPM to your User PATH?", true)
            };
            if let Err(e) = installer::Installer::install(None, dir, should_add) {
                eprintln!("{} Failed to install UPM: {}", BADGE_UPM, e);
                std::process::exit(1);
            }
        }
        Commands::SelfUninstall {
            dir,
            remove_from_path,
            no_remove_from_path,
            clean_cache,
            assume_yes,
        } => {
            if !assume_yes {
                let confirmed = installer::Installer::prompt_yes_no(
                    "Are you sure you want to uninstall UPM?",
                    true,
                );
                if !confirmed {
                    println!("{} Uninstallation cancelled.", BADGE_UPM);
                    return;
                }
            }

            let should_remove = if remove_from_path {
                true
            } else if no_remove_from_path {
                false
            } else if assume_yes {
                true
            } else {
                installer::Installer::prompt_yes_no("Remove UPM from your User PATH?", true)
            };

            let should_clean = if clean_cache {
                true
            } else if assume_yes {
                false
            } else {
                installer::Installer::prompt_yes_no(
                    "Delete UPM package store cache (%LOCALAPPDATA%\\upm\\store)?",
                    false,
                )
            };

            if let Err(e) = installer::Installer::uninstall(dir, should_remove, should_clean) {
                eprintln!("{} Failed to uninstall UPM: {}", BADGE_UPM, e);
                std::process::exit(1);
            }
        }
        Commands::Info => {
            handle_info(&current_dir);
        }
        Commands::Version => {
            print_version_and_guide();
        }
    }

    if std::env::var("UPM_BENCH").is_ok() {
        println!(
            "\x1b[90mExecution finished in {}\x1b[0m",
            format_duration(start_total.elapsed())
        );
    }
}

async fn handle_install(packages: &[String], current_dir: &Path, reinstall: bool, verbose: bool) {
    let cas = ContentAddressableStorage::new().expect("Failed to initialize CAS store");
    let pypi = pypi::PyPiClient::new();
    let npm = npm::NpmClient::new();

    for pkg_arg in packages {
        let t_detect_start = Instant::now();
        let detection = EcosystemDetector::detect(pkg_arg, current_dir);
        let detect_duration = t_detect_start.elapsed();

        if verbose {
            println!(
                "{} Detected {} in {} (via {})",
                detection.ecosystem.badge(),
                detection.package_query,
                format_duration(detect_duration),
                detection.detected_by
            );
        }

        match detection.ecosystem {
            Ecosystem::Python => {
                install_python_package(&pypi, &cas, &detection.package_query, current_dir, reinstall, verbose).await;
            }
            Ecosystem::JavaScript => {
                install_npm_package(&npm, &cas, &detection.package_query, current_dir, reinstall, verbose).await;
            }
            Ecosystem::Binary => {
                install_binary_package(&detection.package_query, verbose).await;
            }
            Ecosystem::System => {
                println!(
                    "{} System package requested: '{}'. System shim provisioning...",
                    BADGE_SYS, detection.package_query
                );
            }
        }
    }
}

async fn install_python_package(
    pypi: &pypi::PyPiClient,
    cas: &ContentAddressableStorage,
    pkg_query: &str,
    current_dir: &Path,
    reinstall: bool,
    verbose: bool,
) {
    let t_start = Instant::now();

    // 1. Ensure Python .venv exists in milliseconds
    let site_packages = match venv::VirtualEnv::get_site_packages(current_dir) {
        Ok(sp) => sp,
        Err(e) => {
            eprintln!("{} Failed to initialize .venv: {}", BADGE_PY, e);
            return;
        }
    };
    let scripts_dir = site_packages.parent().unwrap().parent().unwrap().join("Scripts");

    // 2. Fetch metadata from PyPI
    let metadata = match pypi.fetch_metadata(pkg_query).await {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{} {}", BADGE_PY, e);
            return;
        }
    };

    let name = &metadata.info.name;
    let version = &metadata.info.version;

    // Check CAS cache hit
    let cas_pkg_dir = cas.package_dir(Ecosystem::Python, name, version);
    let from_cache = cas.has_package(Ecosystem::Python, name, version) && !reinstall;

    let final_cas_dir = if from_cache {
        if verbose {
            println!("{} \x1b[32mCAS Cache Hit:\x1b[0m Using existing store entry for {} {}", BADGE_PY, name, version);
        }
        cas_pkg_dir
    } else {
        // Select optimal Windows x86_64 / universal wheel
        let wheel = match pypi.select_optimal_wheel(&metadata.urls) {
            Some(w) => w,
            None => {
                eprintln!(
                    "{} No compatible Windows x86_64 or universal wheel found for {} {}",
                    BADGE_PY, name, version
                );
                return;
            }
        };

        if verbose {
            println!(
                "{} Streaming {} ({}) via async HTTP/2...",
                BADGE_PY,
                wheel.filename,
                format_bytes(wheel.size)
            );
        }

        let t_download = Instant::now();
        let wheel_bytes = match pypi.download_wheel(&wheel.url).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{} {}", BADGE_PY, e);
                return;
            }
        };
        let download_dur = t_download.elapsed();
        let blake3_hash = ContentAddressableStorage::hash_buffer(&wheel_bytes);

        let t_unpack = Instant::now();
        let unpacked = match pypi::PyPiClient::unpack_wheel_to_cas(&wheel_bytes, cas, name, version) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{} SIMD decompression failed: {}", BADGE_PY, e);
                return;
            }
        };
        let unpack_dur = t_unpack.elapsed();

        if verbose {
            println!(
                "{} Download: {} | BLAKE3: {} | SIMD Decompress (libdeflater): {}",
                BADGE_PY,
                format_duration(download_dur),
                &blake3_hash[..16],
                format_duration(unpack_dur)
            );
        }

        unpacked
    };

    // 3. Zero-Copy Link into .venv/Lib/site-packages
    let t_link = Instant::now();
    match pypi::PyPiClient::link_to_venv(&final_cas_dir, &site_packages, &scripts_dir) {
        Ok(stats) => {
            let total_dur = t_start.elapsed();
            let link_dur = t_link.elapsed();
            let total_files = stats.block_cloned + stats.hardlinked + stats.copied;

            println!(
                "{} \x1b[1;32m✓\x1b[0m Installed \x1b[1m{}\x1b[0m \x1b[36mv{}\x1b[0m in \x1b[1;33m{}\x1b[0m (zero-copy linked {} files: {} hardlink, {} reflink, {} copy) in {}",
                BADGE_PY,
                name,
                version,
                format_duration(total_dur),
                total_files,
                stats.hardlinked,
                stats.block_cloned,
                stats.copied,
                format_duration(link_dur)
            );
        }
        Err(e) => {
            eprintln!("{} Failed to link package: {}", BADGE_PY, e);
        }
    }
}

async fn install_npm_package(
    npm: &npm::NpmClient,
    cas: &ContentAddressableStorage,
    pkg_query: &str,
    current_dir: &Path,
    reinstall: bool,
    verbose: bool,
) {
    let t_start = Instant::now();

    let info = match npm.fetch_version_info(pkg_query).await {
        Ok(i) => i,
        Err(e) => {
            eprintln!("{} {}", BADGE_JS, e);
            return;
        }
    };

    let name = &info.name;
    let version = &info.version;

    let cas_pkg_dir = cas.package_dir(Ecosystem::JavaScript, name, version);
    let from_cache = cas.has_package(Ecosystem::JavaScript, name, version) && !reinstall;

    let final_cas_dir = if from_cache {
        if verbose {
            println!("{} \x1b[32mCAS Cache Hit:\x1b[0m Using existing store entry for {} {}", BADGE_JS, name, version);
        }
        cas_pkg_dir
    } else {
        if verbose {
            println!("{} Streaming {}@{} tarball...", BADGE_JS, name, version);
        }

        let t_download = Instant::now();
        let bytes = match npm.download_tarball(&info.dist.tarball).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{} {}", BADGE_JS, e);
                return;
            }
        };
        let download_dur = t_download.elapsed();

        let t_unpack = Instant::now();
        let unpacked = match npm::NpmClient::unpack_tgz_to_cas(&bytes, cas, name, version) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{} Gzip/Tar unpack failed: {}", BADGE_JS, e);
                return;
            }
        };
        let unpack_dur = t_unpack.elapsed();

        if verbose {
            println!(
                "{} Download: {} | SIMD Gzip: {}",
                BADGE_JS,
                format_duration(download_dur),
                format_duration(unpack_dur)
            );
        }

        unpacked
    };

    let t_link = Instant::now();
    match npm::NpmClient::link_to_node_modules(&final_cas_dir, current_dir, name) {
        Ok(stats) => {
            let total_dur = t_start.elapsed();
            let link_dur = t_link.elapsed();
            let total_files = stats.block_cloned + stats.hardlinked + stats.copied;

            println!(
                "{} \x1b[1;32m✓\x1b[0m Installed \x1b[1m{}\x1b[0m \x1b[36mv{}\x1b[0m in \x1b[1;93m{}\x1b[0m (linked {} files: {} hardlink, {} reflink) in {}",
                BADGE_JS,
                name,
                version,
                format_duration(total_dur),
                total_files,
                stats.hardlinked,
                stats.block_cloned,
                format_duration(link_dur)
            );
        }
        Err(e) => {
            eprintln!("{} Failed to link package: {}", BADGE_JS, e);
        }
    }
}

async fn install_binary_package(pkg_query: &str, _verbose: bool) {
    let t_start = Instant::now();

    if pkg_query.starts_with("http://") || pkg_query.starts_with("https://") {
        let bin_name = pkg_query
            .split('/')
            .last()
            .unwrap_or("tool.exe")
            .split('?')
            .next()
            .unwrap_or("tool.exe");

        match binary::BinaryInstaller::install_from_url(pkg_query, bin_name).await {
            Ok(dest) => {
                println!(
                    "{} \x1b[1;32m✓\x1b[0m Installed \x1b[1m{}\x1b[0m to {} in {}",
                    BADGE_BIN,
                    bin_name,
                    dest.display(),
                    format_duration(t_start.elapsed())
                );
            }
            Err(e) => eprintln!("{} {}", BADGE_BIN, e),
        }
    } else {
        let local_path = Path::new(pkg_query);
        if local_path.exists() && local_path.is_file() {
            let bin_name = local_path.file_name().unwrap().to_string_lossy();
            match binary::BinaryInstaller::install_binary(local_path, &bin_name) {
                Ok(dest) => {
                    println!(
                        "{} \x1b[1;32m✓\x1b[0m Installed local binary \x1b[1m{}\x1b[0m to {} in {}",
                        BADGE_BIN,
                        bin_name,
                        dest.display(),
                        format_duration(t_start.elapsed())
                    );
                }
                Err(e) => eprintln!("{} {}", BADGE_BIN, e),
            }
        } else {
            eprintln!(
                "{} Binary '{}' not found locally. Specify a URL or path to an .exe.",
                BADGE_BIN, pkg_query
            );
        }
    }
}

async fn handle_run(cmd: &str, args: &[String], current_dir: &Path) {
    let mut search_paths = Vec::new();
    if let Some(venv_dir) = venv::VirtualEnv::find_venv(current_dir) {
        search_paths.push(venv_dir.join("Scripts"));
    }
    let node_bin = current_dir.join("node_modules").join(".bin");
    if node_bin.exists() {
        search_paths.push(node_bin);
    }
    let upm_bin = binary::BinaryInstaller::get_bin_dir();
    if upm_bin.exists() {
        search_paths.push(upm_bin);
    }

    if !execute_command_with_paths(cmd, args, current_dir, &search_paths) {
        println!(
            "{} '{}' not found in local project. Provisioning on-the-fly via UPM ephemeral runner...",
            BADGE_UPM, cmd
        );
        handle_x(cmd, args, current_dir).await;
    }
}

async fn handle_x(pkg_arg: &str, args: &[String], current_dir: &Path) {
    let cas = ContentAddressableStorage::new().expect("Failed to initialize CAS store");
    let detection = EcosystemDetector::detect(pkg_arg, current_dir);

    let run_root = cas.root_dir().parent().unwrap().join("run");
    let eco_name = match detection.ecosystem {
        Ecosystem::Python => "python",
        Ecosystem::JavaScript => "npm",
        Ecosystem::Binary => "bin",
        Ecosystem::System => "sys",
    };
    let clean_cmd = detection
        .package_query
        .split(['=', '>', '<', '~', '@', ';'])
        .next()
        .unwrap_or(&detection.package_query)
        .trim();

    let ephemeral_dir = run_root.join(eco_name).join(clean_cmd);
    std::fs::create_dir_all(&ephemeral_dir).expect("Failed to create ephemeral runner dir");

    match detection.ecosystem {
        Ecosystem::Python => {
            let pypi = pypi::PyPiClient::new();
            install_python_package(&pypi, &cas, &detection.package_query, &ephemeral_dir, false, false).await;
            let scripts_dir = ephemeral_dir.join(".venv").join("Scripts");
            if !execute_command_with_paths(clean_cmd, args, current_dir, &[scripts_dir]) {
                eprintln!("{} Failed to find executable for package '{}'", BADGE_PY, clean_cmd);
                std::process::exit(1);
            }
        }
        Ecosystem::JavaScript => {
            let npm = npm::NpmClient::new();
            install_npm_package(&npm, &cas, &detection.package_query, &ephemeral_dir, false, false).await;
            let bin_dir = ephemeral_dir.join("node_modules").join(".bin");
            if !execute_command_with_paths(clean_cmd, args, current_dir, &[bin_dir]) {
                eprintln!("{} Failed to find executable for npm package '{}'", BADGE_JS, clean_cmd);
                std::process::exit(1);
            }
        }
        Ecosystem::Binary => {
            let bin_dir = binary::BinaryInstaller::get_bin_dir();
            if !execute_command_with_paths(clean_cmd, args, current_dir, &[bin_dir]) {
                eprintln!("{} Failed to find binary '{}'", BADGE_BIN, clean_cmd);
                std::process::exit(1);
            }
        }
        Ecosystem::System => {
            println!("{} System tool requested: '{}'", BADGE_SYS, clean_cmd);
        }
    }
}

fn execute_command_with_paths(
    cmd: &str,
    args: &[String],
    current_dir: &Path,
    search_paths: &[PathBuf],
) -> bool {
    let extensions = ["", ".cmd", ".bat", ".exe", ".ps1"];
    let mut resolved = None;

    // 1. Search in custom search paths
    'outer: for dir in search_paths {
        for ext in &extensions {
            let candidate = if ext.is_empty() {
                dir.join(cmd)
            } else if cmd.to_lowercase().ends_with(ext) {
                dir.join(cmd)
            } else {
                dir.join(format!("{}{}", cmd, ext))
            };

            if candidate.exists() && candidate.is_file() {
                resolved = Some(candidate);
                break 'outer;
            }
        }
    }

    // 2. Search in system PATH
    if resolved.is_none() {
        if let Ok(path_var) = std::env::var("PATH") {
            'path_outer: for dir_str in path_var.split(';') {
                let dir = PathBuf::from(dir_str.trim());
                for ext in &extensions {
                    let candidate = if ext.is_empty() {
                        dir.join(cmd)
                    } else if cmd.to_lowercase().ends_with(ext) {
                        dir.join(cmd)
                    } else {
                        dir.join(format!("{}{}", cmd, ext))
                    };

                    if candidate.exists() && candidate.is_file() {
                        resolved = Some(candidate);
                        break 'path_outer;
                    }
                }
            }
        }
    }

    // 1. Direct Python module execution if inside or pointing to a Python environment
    for dir in search_paths {
        let site_packages = if dir.ends_with("Scripts") {
            dir.parent().unwrap().join("Lib").join("site-packages")
        } else {
            dir.join("Lib").join("site-packages")
        };

        if site_packages.exists() {
            let sys_py = venv::VirtualEnv::find_system_python().unwrap_or_else(|| PathBuf::from("python.exe"));
            let mut c = std::process::Command::new(&sys_py);
            c.arg("-m").arg(cmd).args(args).current_dir(current_dir);
            c.env("PYTHONPATH", &site_packages);

            if let Ok(status) = c.status() {
                if status.success() || status.code().is_some() {
                    std::process::exit(status.code().unwrap_or(0));
                }
            }
        }
    }

    // 2. Direct Node script execution if inside or pointing to node_modules
    for dir in search_paths {
        let node_modules = if dir.ends_with(".bin") {
            dir.parent().unwrap()
        } else {
            dir
        };
        let pkg_entry = node_modules.join(cmd);
        if pkg_entry.exists() {
            let pkg_json = pkg_entry.join("package.json");
            if let Ok(content) = std::fs::read_to_string(&pkg_json) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                    let bin_target = val.get("bin").and_then(|b| {
                        if let Some(s) = b.as_str() {
                            Some(s.to_string())
                        } else if let Some(obj) = b.as_object() {
                            obj.values().next().and_then(|v| v.as_str()).map(|s| s.to_string())
                        } else {
                            None
                        }
                    });

                    if let Some(rel) = bin_target {
                        let script_path = pkg_entry.join(rel);
                        if script_path.exists() {
                            let mut c = std::process::Command::new("node.exe");
                            c.arg(&script_path).args(args).current_dir(current_dir);
                            if let Ok(status) = c.status() {
                                std::process::exit(status.code().unwrap_or(0));
                            }
                        }
                    }
                }
            }
        }
    }

    if let Some(target_file) = resolved {
        let ext = target_file
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();

        let mut command = if ext == "cmd" || ext == "bat" {
            let mut c = std::process::Command::new("cmd.exe");
            c.arg("/c").arg(&target_file);
            c
        } else if ext == "ps1" {
            let mut c = std::process::Command::new("powershell.exe");
            c.arg("-ExecutionPolicy")
                .arg("Bypass")
                .arg("-File")
                .arg(&target_file);
            c
        } else {
            std::process::Command::new(&target_file)
        };

        command.args(args);
        command.current_dir(current_dir);

        // Prepend search paths to PATH
        if let Ok(current_path) = std::env::var("PATH") {
            let mut new_path_str = String::new();
            for p in search_paths {
                new_path_str.push_str(&p.to_string_lossy());
                new_path_str.push(';');
            }
            new_path_str.push_str(&current_path);
            command.env("PATH", new_path_str);
        }

        match command.status() {
            Ok(status) => {
                std::process::exit(status.code().unwrap_or(0));
            }
            Err(e) => {
                eprintln!(
                    "{} Failed to execute '{}': {}",
                    BADGE_UPM,
                    target_file.display(),
                    e
                );
                std::process::exit(1);
            }
        }
    }

    false
}

fn handle_env(current_dir: &Path, reset: bool) {
    if reset {
        let venv_dir = current_dir.join(".venv");
        if venv_dir.exists() {
            let _ = std::fs::remove_dir_all(&venv_dir);
            println!("{} Removed existing .venv", BADGE_UPM);
        }
    }

    let t0 = Instant::now();
    match venv::VirtualEnv::ensure_venv(current_dir) {
        Ok(v) => {
            println!(
                "{} Active Python environment: \x1b[1;36m{}\x1b[0m (ready in {})",
                BADGE_PY,
                v.display(),
                format_duration(t0.elapsed())
            );
            if let Some(py) = venv::VirtualEnv::find_system_python() {
                println!("  \x1b[90mBase Python:\x1b[0m {}", py.display());
            }
        }
        Err(e) => {
            eprintln!("{} Failed to configure environment: {}", BADGE_UPM, e);
        }
    }
}

fn handle_cache(action: CacheAction) {
    let cas = match ContentAddressableStorage::new() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{} Failed to access CAS: {}", BADGE_UPM, e);
            return;
        }
    };

    match action {
        CacheAction::Dir => {
            println!("{}", cas.root_dir().display());
        }
        CacheAction::Stats => {
            match cas.stats() {
                Ok(stats) => {
                    println!("\x1b[1mUPM Content-Addressable Storage (CAS):\x1b[0m");
                    println!("  Path:        {}", cas.root_dir().display());
                    println!("  Packages:    \x1b[1;36m{}\x1b[0m", stats.package_count);
                    println!("  Total Files: \x1b[1;33m{}\x1b[0m", stats.total_files);
                    println!("  Total Size:  \x1b[1;32m{}\x1b[0m", format_bytes(stats.total_size_bytes));
                }
                Err(e) => eprintln!("{} Failed to read cache stats: {}", BADGE_UPM, e),
            }
        }
        CacheAction::Clean => {
            match cas.clean() {
                Ok(freed) => {
                    println!(
                        "{} Cleaned CAS store. Reclaimed \x1b[1;32m{}\x1b[0m of disk space.",
                        BADGE_UPM,
                        format_bytes(freed)
                    );
                }
                Err(e) => eprintln!("{} Failed to clean cache: {}", BADGE_UPM, e),
            }
        }
    }
}

fn handle_info(current_dir: &Path) {
    print_banner();

    let has_avx2 = is_x86_feature_detected!("avx2");
    let has_avx512 = is_x86_feature_detected!("avx512f");
    let has_sse42 = is_x86_feature_detected!("sse4.2");

    let num_cpus = rayon::current_num_threads();
    let cas = ContentAddressableStorage::new().ok();

    println!("\x1b[1;37mSystem & Hardware Accelerator Diagnostics:\x1b[0m");
    println!("  Target Architecture:     \x1b[1;36mx86_64-pc-windows-msvc (Native MSVC ABI)\x1b[0m");
    println!("  CPU Worker Threads:      \x1b[1;33m{} threads\x1b[0m (Rayon work-stealing pool)", num_cpus);
    println!(
        "  SIMD Vectorization:      AVX2: {}, AVX-512: {}, SSE4.2: {}",
        if has_avx2 { "\x1b[1;32mSupported\x1b[0m" } else { "\x1b[90mDisabled\x1b[0m" },
        if has_avx512 { "\x1b[1;32mSupported\x1b[0m" } else { "\x1b[90mDisabled\x1b[0m" },
        if has_sse42 { "\x1b[1;32mSupported\x1b[0m" } else { "\x1b[90mDisabled\x1b[0m" }
    );
    println!("  Decompression Engine:    \x1b[1;32mlibdeflater SIMD (in-memory zero disk write)\x1b[0m");
    println!("  Cryptographic Hashing:   \x1b[1;32mBLAKE3 SIMD (10+ GB/s)\x1b[0m");
    println!("  Windows Link Primitives: \x1b[1;32mCreateHardLinkW + FSCTL_DUPLICATE_EXTENTS_TO_FILE\x1b[0m");
    if let Some(ref c) = cas {
        println!("  CAS Store Directory:     {}", c.root_dir().display());
    }

    println!("\n\x1b[1;37mSupported Ecosystems:\x1b[0m");
    println!("  {} Python               (Wheels, pyproject.toml, requirements.txt -> .venv)", BADGE_PY);
    println!("  {} JavaScript / Node    (npm tarballs, package.json -> node_modules)", BADGE_JS);
    println!("  {} Standalone Binaries  (Direct .exe, GitHub assets -> ~/.upm/bin)", BADGE_BIN);
    println!("  {} System Toolchains    (Toolchains, shims, compilers)", BADGE_SYS);

    println!("\n\x1b[1;37mActive Directory Context:\x1b[0m");
    println!("  Working Directory:       {}", current_dir.display());
    if let Some(venv_dir) = venv::VirtualEnv::find_venv(current_dir) {
        println!("  Active VirtualEnv:       \x1b[1;32m{}\x1b[0m", venv_dir.display());
    } else {
        println!("  Active VirtualEnv:       \x1b[90mNone (will auto-create on install)\x1b[0m");
    }
}
