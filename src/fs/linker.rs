use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, CreateHardLinkW, SetEndOfFile, SetFilePointerEx,
    FILE_ATTRIBUTE_NORMAL, FILE_BEGIN, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, CREATE_ALWAYS,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::FSCTL_DUPLICATE_EXTENTS_TO_FILE;

#[repr(C)]
#[allow(non_snake_case)]
pub struct DUPLICATE_EXTENTS_DATA {
    pub FileHandle: HANDLE,
    pub SourceFileOffset: i64,
    pub TargetFileOffset: i64,
    pub ByteCount: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkMethod {
    BlockClone, // Dev Drive / ReFS
    HardLink,   // NTFS HardLink
    Copy,       // Standard fast copy (cross-volume fallback)
}

#[derive(Default, Debug)]
pub struct LinkStats {
    pub block_cloned: usize,
    pub hardlinked: usize,
    pub copied: usize,
    pub total_bytes: u64,
}

/// Attempts Dev Drive / ReFS block cloning using FSCTL_DUPLICATE_EXTENTS_TO_FILE.
pub fn try_block_clone(src: &Path, dst: &Path, byte_count: i64) -> bool {
    let src_w: Vec<u16> = src.as_os_str().encode_wide().chain(Some(0)).collect();
    let dst_w: Vec<u16> = dst.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        let src_handle = CreateFileW(
            src_w.as_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if src_handle == INVALID_HANDLE_VALUE {
            return false;
        }

        let dst_handle = CreateFileW(
            dst_w.as_ptr(),
            FILE_GENERIC_READ | FILE_GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if dst_handle == INVALID_HANDLE_VALUE {
            CloseHandle(src_handle);
            return false;
        }

        // Set target file size to match source file size
        let mut new_pos = 0i64;
        SetFilePointerEx(dst_handle, byte_count, &mut new_pos, FILE_BEGIN);
        SetEndOfFile(dst_handle);

        let dup = DUPLICATE_EXTENTS_DATA {
            FileHandle: src_handle,
            SourceFileOffset: 0,
            TargetFileOffset: 0,
            ByteCount: byte_count,
        };

        let mut bytes_returned = 0u32;
        let success = DeviceIoControl(
            dst_handle,
            FSCTL_DUPLICATE_EXTENTS_TO_FILE,
            &dup as *const _ as *const _,
            std::mem::size_of::<DUPLICATE_EXTENTS_DATA>() as u32,
            std::ptr::null_mut(),
            0,
            &mut bytes_returned,
            std::ptr::null_mut(),
        );

        CloseHandle(dst_handle);
        CloseHandle(src_handle);

        success != 0
    }
}

/// Creates a Win32 NTFS hardlink via CreateHardLinkW.
pub fn create_hardlink(src: &Path, dst: &Path) -> bool {
    let src_w: Vec<u16> = src.as_os_str().encode_wide().chain(Some(0)).collect();
    let dst_w: Vec<u16> = dst.as_os_str().encode_wide().chain(Some(0)).collect();

    unsafe {
        let ret = CreateHardLinkW(dst_w.as_ptr(), src_w.as_ptr(), std::ptr::null());
        ret != 0
    }
}

/// Links a single file using zero-copy primitives:
/// 1. Tries Dev Drive / ReFS block cloning
/// 2. Tries Win32 NTFS HardLink
/// 3. Falls back to fast copy (e.g. cross-volume boundary)
pub fn link_file(src: &Path, dst: &Path) -> std::io::Result<LinkMethod> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // If destination already exists, remove it before linking
    if dst.exists() {
        let _ = std::fs::remove_file(dst);
    }

    let metadata = std::fs::metadata(src)?;
    let file_size = metadata.len() as i64;

    // 1. ReFS / Dev Drive block clone (if non-zero file)
    if file_size > 0 && try_block_clone(src, dst, file_size) {
        return Ok(LinkMethod::BlockClone);
    }

    // 2. NTFS hardlink
    if create_hardlink(src, dst) {
        return Ok(LinkMethod::HardLink);
    }

    // 3. Fallback to standard fast copy
    std::fs::copy(src, dst)?;
    Ok(LinkMethod::Copy)
}

/// Zero-copy links an entire directory tree from CAS to project destination.
pub fn link_directory_tree(src_dir: &Path, dst_dir: &Path) -> std::io::Result<LinkStats> {
    let mut stats = LinkStats::default();

    fn walk_and_link(
        base_src: &Path,
        current_src: &Path,
        dst_base: &Path,
        stats: &mut LinkStats,
    ) -> std::io::Result<()> {
        for entry in std::fs::read_dir(current_src)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path.strip_prefix(base_src).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, e.to_string())
            })?;
            let target_path = dst_base.join(relative);

            if path.is_dir() {
                std::fs::create_dir_all(&target_path)?;
                walk_and_link(base_src, &path, dst_base, stats)?;
            } else if path.is_file() {
                let size = entry.metadata()?.len();
                match link_file(&path, &target_path)? {
                    LinkMethod::BlockClone => stats.block_cloned += 1,
                    LinkMethod::HardLink => stats.hardlinked += 1,
                    LinkMethod::Copy => stats.copied += 1,
                }
                stats.total_bytes += size;
            }
        }
        Ok(())
    }

    std::fs::create_dir_all(dst_dir)?;
    walk_and_link(src_dir, src_dir, dst_dir, &mut stats)?;
    Ok(stats)
}
