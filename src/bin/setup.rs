#[path = "../terminal.rs"]
mod terminal;
#[path = "../installer.rs"]
mod installer;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, HBRUSH, HFONT, COLOR_WINDOW, FW_BOLD, FW_NORMAL,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, MessageBoxW, PostQuitMessage, RegisterClassW, SendMessageW,
    SetWindowTextW, ShowWindow, TranslateMessage, BM_GETCHECK, BM_SETCHECK,
    BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_PUSHBUTTON,
    CS_HREDRAW, CS_VREDRAW, MB_ICONINFORMATION, MB_OK, MSG, SM_CXSCREEN, SM_CYSCREEN,
    SW_SHOW, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_SETFONT, WNDCLASSW,
    WS_CHILD, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE,
};

use installer::Installer;

const WS_CAPTION: u32 = 0x00C00000;
const BST_CHECKED: usize = 1;

const EMBEDDED_UPM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded_upm.bin"));
const EMBEDDED_UNINSTALLER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/embedded_uninstaller.bin"));

const IDC_INSTALL_BTN: usize = 1001;
const IDC_CANCEL_BTN: usize = 1002;
const IDC_PATH_CHECKBOX: usize = 1003;

static PATH_CHECKBOX_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static STATUS_LABEL_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static INSTALL_BTN_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static CANCEL_BTN_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
static IS_INSTALLING: AtomicBool = AtomicBool::new(false);

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

fn create_segoe_font(size: i32, bold: bool) -> HFONT {
    unsafe {
        let font_name = to_wide("Segoe UI");
        CreateFontW(
            size,
            0,
            0,
            0,
            if bold { FW_BOLD as i32 } else { FW_NORMAL as i32 },
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            font_name.as_ptr(),
        )
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let hinstance = GetModuleHandleW(std::ptr::null());
            let font_title = create_segoe_font(22, true);
            let font_author = create_segoe_font(15, true);
            let font_normal = create_segoe_font(14, false);
            let font_small = create_segoe_font(13, false);

            let make_label = |text: &str, x: i32, y: i32, w: i32, h: i32, font: HFONT| -> HWND {
                let static_class = to_wide("STATIC");
                let label_text = to_wide(text);
                let h_lbl = CreateWindowExW(
                    0,
                    static_class.as_ptr(),
                    label_text.as_ptr(),
                    WS_CHILD | WS_VISIBLE,
                    x,
                    y,
                    w,
                    h,
                    hwnd,
                    std::ptr::null_mut(),
                    hinstance,
                    std::ptr::null(),
                );
                SendMessageW(h_lbl, WM_SETFONT, font as WPARAM, 1);
                h_lbl
            };

            // Title Header
            make_label("Install UPM 0.1.0 (64-bit)", 30, 20, 480, 30, font_title);

            // Author credit & subhead
            make_label("Universal Package Manager  •  Engineered & Built by Sarvesh.S", 30, 52, 480, 22, font_author);
            make_label("Ultra-high-performance cross-ecosystem package installer for Windows", 30, 76, 480, 20, font_small);

            // Feature Highlights
            make_label("✓ Zero-Copy Linker (Win32 Hardlinks & ReFS Dev Drive Block Cloning)", 40, 115, 460, 20, font_normal);
            make_label("✓ SIMD In-Memory Decompression (libdeflater AVX2/SSE4.2 hardware acceleration)", 40, 138, 460, 20, font_normal);
            make_label("✓ Content-Addressable Storage (CAS) with BLAKE3 cryptographic integrity", 40, 161, 460, 20, font_normal);
            make_label("✓ Native Python (PyPI) and Node.js (npm) sub-millisecond execution", 40, 184, 460, 20, font_normal);

            // Install Directory Notice
            let default_dir = Installer::default_install_dir();
            let dir_notice = format!("Destination: {}", default_dir.display());
            make_label(&dir_notice, 40, 222, 460, 20, font_small);

            // Add to PATH Checkbox (Checked by default!)
            let button_class = to_wide("BUTTON");
            let chk_text = to_wide("Add UPM to PATH (recommended for global command line access)");
            let h_chk = CreateWindowExW(
                0,
                button_class.as_ptr(),
                chk_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | (BS_AUTOCHECKBOX as u32),
                38,
                255,
                460,
                26,
                hwnd,
                IDC_PATH_CHECKBOX as *mut _,
                hinstance,
                std::ptr::null(),
            );
            SendMessageW(h_chk, WM_SETFONT, font_normal as WPARAM, 1);
            SendMessageW(h_chk, BM_SETCHECK, BST_CHECKED as WPARAM, 0);
            PATH_CHECKBOX_HWND.store(h_chk as isize, Ordering::SeqCst);

            // Status label (initially blank)
            let h_status = make_label("", 40, 290, 460, 20, font_author);
            STATUS_LABEL_HWND.store(h_status as isize, Ordering::SeqCst);

            // Install Now Button (Default button)
            let btn_text = to_wide("Install Now");
            let h_install = CreateWindowExW(
                0,
                button_class.as_ptr(),
                btn_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | (BS_DEFPUSHBUTTON as u32),
                270,
                325,
                125,
                36,
                hwnd,
                IDC_INSTALL_BTN as *mut _,
                hinstance,
                std::ptr::null(),
            );
            SendMessageW(h_install, WM_SETFONT, font_author as WPARAM, 1);
            INSTALL_BTN_HWND.store(h_install as isize, Ordering::SeqCst);

            // Cancel Button
            let cancel_text = to_wide("Cancel");
            let h_cancel = CreateWindowExW(
                0,
                button_class.as_ptr(),
                cancel_text.as_ptr(),
                WS_CHILD | WS_VISIBLE | (BS_PUSHBUTTON as u32),
                405,
                325,
                95,
                36,
                hwnd,
                IDC_CANCEL_BTN as *mut _,
                hinstance,
                std::ptr::null(),
            );
            SendMessageW(h_cancel, WM_SETFONT, font_normal as WPARAM, 1);
            CANCEL_BTN_HWND.store(h_cancel as isize, Ordering::SeqCst);

            0
        }
        WM_COMMAND => {
            let ctrl_id = (wparam & 0xFFFF) as usize;
            if ctrl_id == IDC_INSTALL_BTN {
                if !IS_INSTALLING.swap(true, Ordering::SeqCst) {
                    perform_gui_install(hwnd);
                }
            } else if ctrl_id == IDC_CANCEL_BTN {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn perform_gui_install(hwnd: HWND) {
    let chk_hwnd = PATH_CHECKBOX_HWND.load(Ordering::SeqCst) as HWND;
    let status_hwnd = STATUS_LABEL_HWND.load(Ordering::SeqCst) as HWND;

    let should_add_to_path = unsafe {
        SendMessageW(chk_hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT
    };

    unsafe {
        let installing_text = to_wide("Installing UPM and configuring environment...");
        SetWindowTextW(status_hwnd, installing_text.as_ptr());
    }

    let install_dir = Installer::default_install_dir();
    let dest_exe = install_dir.join("upm.exe");
    let dest_uninstaller = install_dir.join("upm-uninstaller.exe");

    let install_result: Result<(), String> = (|| {
        std::fs::create_dir_all(&install_dir)
            .map_err(|e| format!("Failed to create folder: {}", e))?;

        // 1. Write or copy upm.exe
        if !EMBEDDED_UPM.is_empty() {
            std::fs::write(&dest_exe, EMBEDDED_UPM)
                .map_err(|e| format!("Failed to write upm.exe: {}", e))?;
        } else {
            let src = Installer::locate_upm_binary(None)?;
            if src.canonicalize().ok() != dest_exe.canonicalize().ok() {
                std::fs::copy(&src, &dest_exe)
                    .map_err(|e| format!("Failed to copy upm.exe: {}", e))?;
            }
        }

        // 2. Write or copy upm-uninstaller.exe
        if !EMBEDDED_UNINSTALLER.is_empty() {
            let _ = std::fs::write(&dest_uninstaller, EMBEDDED_UNINSTALLER);
        } else if let Ok(current) = std::env::current_exe() {
            if let Some(parent) = current.parent() {
                let uninst = parent.join("upm-uninstaller.exe");
                if uninst.exists() {
                    let _ = std::fs::copy(uninst, &dest_uninstaller);
                }
            }
        }

        // 3. Register in Windows Add/Remove Programs
        let _ = Installer::register_windows_uninstall(&install_dir, &dest_exe);

        // 4. Configure PATH if selected
        if should_add_to_path {
            let _ = Installer::add_dir_to_path(&install_dir);
        }

        Ok(())
    })();

    match install_result {
        Ok(_) => {
            unsafe {
                let done_text = to_wide("Installation Complete!");
                SetWindowTextW(status_hwnd, done_text.as_ptr());

                let title = to_wide("UPM Setup - Complete");
                let message = to_wide(
                    "Thanks for choosing UPM!\n\
                    Engineered & Built by Sarvesh.S\n\n\
                    Installation completed successfully.\n\
                    Opening PowerShell for you now to get started!"
                );
                MessageBoxW(hwnd, message.as_ptr(), title.as_ptr(), MB_OK | MB_ICONINFORMATION);

                // Open a new interactive PowerShell terminal running 'upm version'
                let _ = std::process::Command::new("powershell.exe")
                    .arg("-NoExit")
                    .arg("-Command")
                    .arg("upm version")
                    .spawn();

                DestroyWindow(hwnd);
            }
        }
        Err(err) => {
            unsafe {
                let title = to_wide("UPM Setup - Error");
                let msg = to_wide(&format!("Installation failed:\n{}", err));
                MessageBoxW(hwnd, msg.as_ptr(), title.as_ptr(), MB_OK);
            }
            IS_INSTALLING.store(false, Ordering::SeqCst);
        }
    }
}

fn run_gui_setup() {
    unsafe {
        let hinstance = GetModuleHandleW(std::ptr::null());
        let class_name = to_wide("UPMSetupWindowClass");

        let wnd_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: hinstance,
            hIcon: 0 as _,
            hCursor: 0 as _,
            hbrBackground: (COLOR_WINDOW + 1) as HBRUSH,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };

        RegisterClassW(&wnd_class);

        let window_title = to_wide("UPM Setup - Universal Package Manager");
        let width = 540;
        let height = 410;

        // Center window on screen
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let x = (screen_w - width) / 2;
        let y = (screen_h - height) / 2;

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_title.as_ptr(),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE,
            x,
            y,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinstance,
            std::ptr::null(),
        );

        ShowWindow(hwnd, SW_SHOW);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // If silent or CLI flags passed, perform non-interactive install
    if args.iter().any(|a| a == "-y" || a == "--yes" || a == "--silent" || a == "/S") {
        let should_add_to_path = !args.iter().any(|a| a == "--no-add-to-path");
        let install_dir = Installer::default_install_dir();
        match Installer::install(None, Some(install_dir), should_add_to_path) {
            Ok(_) => {
                println!("Thanks for choosing UPM! Built by Sarvesh.S");
                let _ = std::process::Command::new("powershell.exe")
                    .arg("-NoExit")
                    .arg("-Command")
                    .arg("upm version")
                    .spawn();
            }
            Err(e) => {
                eprintln!("Installation failed: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        // Standard double-click mode: show the GUI installation screen!
        run_gui_setup();
    }
}
