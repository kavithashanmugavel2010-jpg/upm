use std::env;
use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=target/release/upm.exe");
    println!("cargo:rerun-if-changed=target/release/upm-uninstaller.exe");

    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_upm = Path::new(&out_dir).join("embedded_upm.bin");
    let dest_uninstaller = Path::new(&out_dir).join("embedded_uninstaller.bin");

    // Embed upm.exe if it exists
    let candidates_upm = [
        "target/release/upm.exe",
        "upm.exe",
    ];
    let mut found_upm = false;
    for cand in &candidates_upm {
        if let Ok(bytes) = fs::read(cand) {
            fs::write(&dest_upm, bytes).unwrap();
            found_upm = true;
            break;
        }
    }
    if !found_upm {
        fs::write(&dest_upm, b"").unwrap();
    }

    // Embed upm-uninstaller.exe if it exists
    let candidates_uninst = [
        "target/release/upm-uninstaller.exe",
        "upm-uninstaller.exe",
    ];
    let mut found_uninst = false;
    for cand in &candidates_uninst {
        if let Ok(bytes) = fs::read(cand) {
            fs::write(&dest_uninstaller, bytes).unwrap();
            found_uninst = true;
            break;
        }
    }
    if !found_uninst {
        fs::write(&dest_uninstaller, b"").unwrap();
    }
}
