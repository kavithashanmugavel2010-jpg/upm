# UPM — Universal Package Manager

**UPM (Universal Package Manager)** is an ultra-high-performance, cross-ecosystem package installer and environment manager engineered natively in Rust for Windows (`x86_64-pc-windows-msvc`).

Built to surpass traditional tools like `pip`, `npm`, and `uv` in raw execution speed on Windows, UPM combines low-level Win32 kernel primitives, SIMD decompression, and in-memory streaming.

---

## Key Highlights

- **Standalone Native Binary**: Compiles to a single zero-dependency machine code executable (`upm.exe`, ~3.3 MB) with `<1 ms` launch latency.
- **Content-Addressable Storage (CAS)**: Packages download and unpack once into `%LOCALAPPDATA%\upm\store\`. Subsequent installs take zero additional disk space and zero network fetches.
- **Windows Zero-Copy Linker**: Employs Win32 NTFS Hardlinks (`CreateHardLinkW`) and Dev Drive / ReFS block cloning (`FSCTL_DUPLICATE_EXTENTS_TO_FILE` via `DeviceIoControl`) for instant file population.
- **Hybrid Parallel Pipeline**: Async HTTP/2 streaming via `tokio` and `reqwest` direct into a `rayon` work-stealing thread pool.
- **SIMD In-Memory Decompression**: Uses `libdeflater` for hardware-accelerated DEFLATE/GZIP decompression of ZIP/wheel and tarball buffers with zero intermediate disk writes.
- **SIMD Integrity Hashing**: High-throughput BLAKE3 hashing at 10+ GB/s.
- **Smart Ecosystem Detection**: Detects manifests (`pyproject.toml`, `requirements.txt`, `package.json`) and CLI prefixes (`pypi:`, `npm:`, `bin:`, `sys:`) in `< 0.2 ms`.
- **Zero-Allocation Terminal UI**: Sets Windows console to `ENABLE_VIRTUAL_TERMINAL_PROCESSING` with colored badges:
  - `[PY]` (Yellow) for Python
  - `[JS]` (Bright Yellow) for JavaScript / Node
  - `[BIN]` (Bright Cyan) for Standalone CLI Binaries
  - `[SYS]` (Bright Green) for System Tools

---

## Architectural Layout

```
upupm/
├── Cargo.toml               # Release optimizations (opt-level = 3, fat LTO, codegen-units = 1)
└── src/
    ├── main.rs              # CLI entry point, command dispatch, acceleration diagnostics
    ├── terminal.rs          # Win32 Console VT Mode, zero-allocation badges, timing formatters
    ├── detector.rs          # Sub-millisecond manifest & argument ecosystem detector
    ├── cas.rs               # Content-Addressable Storage manager (%LOCALAPPDATA%\upm\store\)
    ├── binary.rs            # Standalone CLI binary installer (~/.upm/bin)
    ├── venv.rs              # Sub-millisecond Windows .venv discovery and initialization
    ├── fs/
    │   ├── mod.rs
    │   └── linker.rs        # Win32 CreateHardLinkW & ReFS FSCTL_DUPLICATE_EXTENTS_TO_FILE
    ├── pypi/
    │   └── mod.rs           # PyPI resolver, wheel selector, Rayon + libdeflater SIMD unpacker
    └── npm/
        └── mod.rs           # npm registry resolver, SIMD gzip + tar unpacker, .bin shims
```

---

## Quick Start & CLI Usage

### 1. Show Hardware Diagnostics & Capabilities
```powershell
upm info
```

### 2. Python Package Installation
```powershell
# Auto-detects Python or uses pypi: prefix
upm install requests
upm install pypi:flask

# Verbose mode with microsecond timings and BLAKE3 verification
upm install requests --verbose
```

### 3. JavaScript / Node Package Installation
```powershell
upm install npm:chalk
upm install npm:express
```

### 4. Run Any Package On-The-Fly (like `uvx` / `npx`)
Run any tool or package without prior installation or project pollution:
```powershell
# Run Python tools on-the-fly
upm x cowsay -t "Universal Package Manager"
upm x cowsay -c dragon -t "Raw Speed"
upm x ruff check .

# Run via 'upm run' with automatic ephemeral fallback
upm run cowsay -c tux -t "Linux Tux on Windows"
```

### 6. Cache Management
```powershell
# Show CAS statistics (package count, total files, disk size)
upm cache stats

# Show CAS directory path
upm cache dir

# Clean global cache
upm cache clean
```

---

## Building from Source

Prerequisites: Rust 1.80+ with `x86_64-pc-windows-msvc` toolchain.

```powershell
# Development build
cargo build

# Maximum-performance Release Build (builds both upm.exe and upm-installer.exe)
cargo build --release
```
The compiled executables are placed at:
- `target\release\upm.exe`
- `target\release\upm-installer.exe`

---

## Installation & PATH Setup

UPM provides multiple installer methods with optional PATH registration:

### Option A: Standalone Native Installer (`upm-installer.exe`)
Run the interactive native Windows installer wizard:
```powershell
.\target\release\upm-installer.exe
```
Or run non-interactively with specific flags:
```powershell
# Install to default directory (%LOCALAPPDATA%\upm\bin) and add to User PATH
.\target\release\upm-installer.exe --add-to-path -y

# Install to custom directory without altering PATH
.\target\release\upm-installer.exe --dir "C:\Tools\upm" --no-add-to-path -y
```

### Option B: Self-Installation via `upm` CLI
If you already have `upm.exe`:
```powershell
# Interactive prompt for PATH configuration:
upm self-install

# Explicitly add to User PATH:
upm self-install --add-to-path

# Specify a custom directory:
upm self-install --dir "D:\bin" --add-to-path

# Clean uninstall and removal from PATH:
upm self-uninstall --remove-from-path
```

### Option C: PowerShell Installer (`install.ps1`)
```powershell
# Install and add to User PATH (default):
powershell -ExecutionPolicy Bypass -File .\install.ps1 -AddToPath

# Install without modifying PATH:
powershell -ExecutionPolicy Bypass -File .\install.ps1 -NoPath
```

### Option D: Double-Click Batch Installer (`install.bat`)
Double-click `install.bat` in Windows File Explorer or execute from CMD.

### Option E: Inno Setup GUI Wizard (`installer/upm_setup.iss`)
Compile `installer/upm_setup.iss` with Inno Setup to generate a standard graphical setup executable `UPM-Setup-0.1.0.exe` with a `[X] Add UPM directory to User PATH` checkbox and Windows Add/Remove Programs integration.

---

## Uninstallation & Cleanup

UPM provides multiple ways to cleanly remove itself, deregister from PATH, and delete cached packages:

### Option A: Standalone Native Uninstaller (`upm-uninstaller.exe`)
When installed, `upm-uninstaller.exe` is placed directly in the install directory (default: `%LOCALAPPDATA%\upm\bin`).
```powershell
# Interactive uninstaller wizard:
upm-uninstaller.exe

# Non-interactive uninstall (removes from PATH and cleans binaries):
upm-uninstaller.exe --remove-from-path -y

# Also clean package cache store (%LOCALAPPDATA%\upm\store):
upm-uninstaller.exe --remove-from-path --clean-cache -y
```

### Option B: Built-in `upm self-uninstall`
```powershell
# Interactive prompts for PATH and cache cleanup:
upm self-uninstall

# Fully automatic removal:
upm self-uninstall --remove-from-path --clean-cache -y
```

### Option C: Windows Installed Apps / Control Panel
UPM automatically registers in Windows **Settings → Apps → Installed apps** (or Control Panel → Programs and Features). You can click **Uninstall** directly from Windows Settings!

### Option D: PowerShell Uninstaller (`uninstall.ps1`)
```powershell
# Remove UPM and remove from PATH:
powershell -ExecutionPolicy Bypass -File .\uninstall.ps1

# Remove UPM, remove from PATH, and delete package cache:
powershell -ExecutionPolicy Bypass -File .\uninstall.ps1 -CleanCache -Force
```

### Option E: Double-Click Batch Uninstaller (`uninstall.bat`)
Double-click `uninstall.bat` in Windows File Explorer or run from CMD.


