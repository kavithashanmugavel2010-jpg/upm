@echo off
setlocal EnableDelayedExpansion

cd /d "%~dp0"

echo ==================================================================
echo   UPM - Universal Package Manager Windows Uninstaller
echo ==================================================================
echo.

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0uninstall.ps1" %*
if %ERRORLEVEL% NEQ 0 (
    echo.
    echo [ERROR] UPM Uninstallation encountered an error.
    pause
    exit /b %ERRORLEVEL%
)

echo.
pause
