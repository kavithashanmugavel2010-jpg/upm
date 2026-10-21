# UPM - Universal Package Manager PowerShell Installer
# Usage:
#   powershell -ExecutionPolicy Bypass -File .\install.ps1
#   powershell -ExecutionPolicy Bypass -File .\install.ps1 -AddToPath
#   powershell -ExecutionPolicy Bypass -File .\install.ps1 -AddToPath:$false
#   powershell -ExecutionPolicy Bypass -File .\install.ps1 -InstallDir "C:\MyTools\upm"

[CmdletBinding()]
param(
    [Parameter(Mandatory = $false)]
    [switch]$AddToPath = $true,

    [Parameter(Mandatory = $false)]
    [switch]$NoPath = $false,

    [Parameter(Mandatory = $false)]
    [string]$InstallDir = "$env:LOCALAPPDATA\upm\bin",

    [Parameter(Mandatory = $false)]
    [string]$BinaryPath = "",

    [Parameter(Mandatory = $false)]
    [switch]$Help = $false
)

if ($Help) {
    Write-Host "UPM Windows Installer" -ForegroundColor Cyan
    Write-Host "Parameters:"
    Write-Host "  -AddToPath         Add UPM to User PATH (default: `$true)"
    Write-Host "  -NoPath            Do not add UPM to User PATH"
    Write-Host "  -InstallDir <path> Custom installation directory (default: %LOCALAPPDATA%\upm\bin)"
    Write-Host "  -BinaryPath <path> Specific path to upm.exe to install"
    exit 0
}

if ($NoPath) {
    $AddToPath = $false
}

Write-Host "==================================================================" -ForegroundColor Cyan
Write-Host "  UPM - Universal Package Manager Installer (Native Win32)" -ForegroundColor White
Write-Host "==================================================================" -ForegroundColor Cyan
Write-Host ""

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

# 1. Locate or build upm.exe
$targetExe = ""
if ($BinaryPath -and (Test-Path $BinaryPath)) {
    $targetExe = (Resolve-Path $BinaryPath).Path
} else {
    $candidates = @(
        (Join-Path $scriptDir "upm.exe"),
        (Join-Path $scriptDir "target\release\upm.exe"),
        (Join-Path $scriptDir "target\debug\upm.exe")
    )
    foreach ($cand in $candidates) {
        if (Test-Path $cand) {
            $targetExe = $cand
            break
        }
    }
}

if (-not $targetExe) {
    if (Test-Path (Join-Path $scriptDir "Cargo.toml")) {
        Write-Host "[UPM] upm.exe not found. Building release binary via 'cargo build --release'..." -ForegroundColor Yellow
        $prevPwd = Get-Location
        Set-Location $scriptDir
        & cargo build --release
        Set-Location $prevPwd
        $relPath = Join-Path $scriptDir "target\release\upm.exe"
        if (Test-Path $relPath) {
            $targetExe = $relPath
        } else {
            Write-Error "[UPM] Failed to find compiled binary at $relPath."
            exit 1
        }
    } else {
        Write-Error "[UPM] Could not locate upm.exe binary. Please provide -BinaryPath."
        exit 1
    }
}

Write-Host "[UPM] Source executable: $targetExe" -ForegroundColor Gray

# 2. Prepare destination directory
if (-not (Test-Path $InstallDir)) {
    Write-Host "[UPM] Creating directory: $InstallDir" -ForegroundColor Gray
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

$destExe = Join-Path $InstallDir "upm.exe"

# 3. Copy binary
try {
    Copy-Item -Path $targetExe -Destination $destExe -Force
    Write-Host "[UPM] Installed upm.exe to: $destExe" -ForegroundColor Green

    # Copy upm-uninstaller.exe if found
    $uninstallerCandidates = @(
        (Join-Path $scriptDir "upm-uninstaller.exe"),
        (Join-Path $scriptDir "target\release\upm-uninstaller.exe"),
        (Join-Path $scriptDir "target\debug\upm-uninstaller.exe")
    )
    foreach ($cand in $uninstallerCandidates) {
        if (Test-Path $cand) {
            $destUninstaller = Join-Path $InstallDir "upm-uninstaller.exe"
            Copy-Item -Path $cand -Destination $destUninstaller -Force
            Write-Host "[UPM] Installed upm-uninstaller.exe to: $destUninstaller" -ForegroundColor Green
            break
        }
    }

    # Copy uninstall scripts
    if (Test-Path (Join-Path $scriptDir "uninstall.bat")) {
        Copy-Item -Path (Join-Path $scriptDir "uninstall.bat") -Destination (Join-Path $InstallDir "uninstall.bat") -Force
    }
    if (Test-Path (Join-Path $scriptDir "uninstall.ps1")) {
        Copy-Item -Path (Join-Path $scriptDir "uninstall.ps1") -Destination (Join-Path $InstallDir "uninstall.ps1") -Force
    }

    # Register in Windows Installed Apps (Settings / Control Panel)
    $regPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\UPM"
    if (-not (Test-Path $regPath)) {
        New-Item -Path $regPath -Force | Out-Null
    }
    Set-ItemProperty -Path $regPath -Name "DisplayName" -Value "UPM - Universal Package Manager" -Force
    Set-ItemProperty -Path $regPath -Name "DisplayVersion" -Value "0.1.0" -Force
    Set-ItemProperty -Path $regPath -Name "Publisher" -Value "UPM Contributors" -Force
    Set-ItemProperty -Path $regPath -Name "InstallLocation" -Value $InstallDir -Force
    $uninstallerExe = Join-Path $InstallDir "upm-uninstaller.exe"
    if (Test-Path $uninstallerExe) {
        Set-ItemProperty -Path $regPath -Name "UninstallString" -Value "`"$uninstallerExe`"" -Force
        Set-ItemProperty -Path $regPath -Name "QuietUninstallString" -Value "`"$uninstallerExe`" -y" -Force
    } else {
        Set-ItemProperty -Path $regPath -Name "UninstallString" -Value "`"$destExe`" self-uninstall" -Force
        Set-ItemProperty -Path $regPath -Name "QuietUninstallString" -Value "`"$destExe`" self-uninstall -y" -Force
    }
    Set-ItemProperty -Path $regPath -Name "DisplayIcon" -Value $destExe -Force
    Write-Host "[UPM] Registered in Windows Installed Apps (Settings / Control Panel)" -ForegroundColor Green
} catch {
    Write-Error "[UPM] Failed to copy upm.exe to $destExe : $_"
    exit 1
}

# 4. PATH Configuration
if ($AddToPath) {
    try {
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if (-not $userPath) {
            $userPath = ""
        }

        $normalizedDirs = ($userPath -split ';') | ForEach-Object { $_.Trim().TrimEnd('\') }
        $normalizedInstall = $InstallDir.Trim().TrimEnd('\')

        $alreadyInPath = $normalizedDirs -contains $normalizedInstall

        if ($alreadyInPath) {
            Write-Host "[UPM] Directory already present in User PATH: $InstallDir" -ForegroundColor DarkGray
        } else {
            $newPath = if ([string]::IsNullOrWhiteSpace($userPath)) {
                $InstallDir
            } elseif ($userPath.EndsWith(';')) {
                "$userPath$InstallDir"
            } else {
                "$userPath;$InstallDir"
            }

            [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
            Write-Host "[UPM] Successfully added '$InstallDir' to User PATH!" -ForegroundColor Green
        }

        # Also update current PowerShell process session PATH
        if (($env:Path -split ';') -notcontains $InstallDir) {
            $env:Path = "$InstallDir;$env:Path"
        }
    } catch {
        Write-Warning "[UPM] Could not update User PATH automatically: $_"
    }
} else {
    Write-Host "[UPM] Skipped adding to PATH as requested (-AddToPath:`$false or -NoPath)." -ForegroundColor Yellow
    Write-Host "      To use 'upm' globally, manually add '$InstallDir' to your PATH." -ForegroundColor Gray
}

Write-Host ""
Write-Host "==================================================================" -ForegroundColor Cyan
Write-Host "  Installation Complete! Running 'upm version'..." -ForegroundColor Green
Write-Host "==================================================================" -ForegroundColor Cyan
Write-Host ""

& $destExe version

Write-Host ""
Write-Host "Thanks for choosing UPM! Built by Sarvesh.S" -ForegroundColor Green
Write-Host ""
