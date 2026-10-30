use std::ffi::c_void;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use windows::Win32::Foundation::{HANDLE, HGLOBAL, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE};
use windows::Win32::UI::Shell::{DROPFILES, DragQueryFileW, HDROP};
use windows::core::{BOOL, Owned, PCWSTR, w};

const DRAGDROP_GET_FILES_COUNT: u32 = 0xFFFF_FFFF;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipboardMode {
    Copy,
    Cut,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FileClipboard {
    pub paths: Vec<PathBuf>,
    pub mode: ClipboardMode,
}

pub(crate) fn has_files() -> bool {
    unsafe { IsClipboardFormatAvailable(CF_HDROP.0 as u32) }.is_ok()
}

pub(crate) fn read_file_clipboard() -> Option<FileClipboard> {
    let _clipboard = ClipboardGuard::open()?;
    let paths = read_files()?;
    if paths.is_empty() {
        return None;
    }
    let mode = if read_drop_effect().is_some_and(|effect| effect & DROPEFFECT_MOVE.0 != 0) {
        ClipboardMode::Cut
    } else {
        ClipboardMode::Copy
    };
    Some(FileClipboard { paths, mode })
}

pub(crate) fn write_file_clipboard(paths: &[PathBuf], mode: ClipboardMode) -> Result<(), String> {
    if paths.is_empty() {
        return Err("没有可复制的文件".to_string());
    }
    let _clipboard = ClipboardGuard::open().ok_or_else(|| "无法打开系统剪贴板".to_string())?;
    unsafe { EmptyClipboard() }.map_err(|error| format!("无法清空系统剪贴板：{error}"))?;
    write_drop_files(paths)?;
    write_drop_effect(match mode {
        ClipboardMode::Copy => DROPEFFECT_COPY.0,
        ClipboardMode::Cut => DROPEFFECT_MOVE.0,
    })?;
    Ok(())
}

pub(crate) fn clear_if_matches(paths: &[PathBuf]) {
    let Some(current) = read_file_clipboard() else {
        return;
    };
    if current.paths != paths {
        return;
    }
    if let Some(_clipboard) = ClipboardGuard::open() {
        let _ = unsafe { EmptyClipboard() };
    }
}

fn write_drop_files(paths: &[PathBuf]) -> Result<(), String> {
    set_clipboard_bytes(&drop_files_bytes(paths), CF_HDROP.0 as u32)
}

fn drop_files_bytes(paths: &[PathBuf]) -> Vec<u8> {
    let mut wide = Vec::<u16>::new();
    for path in paths {
        wide.extend(path.as_os_str().encode_wide());
        wide.push(0);
    }
    wide.push(0);

    let header = DROPFILES {
        pFiles: size_of::<DROPFILES>() as u32,
        pt: POINT { x: 0, y: 0 },
        fNC: BOOL(0),
        fWide: BOOL(1),
    };
    let header_bytes = unsafe {
        std::slice::from_raw_parts(
            (&header as *const DROPFILES).cast::<u8>(),
            size_of::<DROPFILES>(),
        )
    };
    let wide_bytes = unsafe {
        std::slice::from_raw_parts(
            wide.as_ptr().cast::<u8>(),
            std::mem::size_of_val(wide.as_slice()),
        )
    };
    let mut data = Vec::with_capacity(header_bytes.len() + wide_bytes.len());
    data.extend_from_slice(header_bytes);
    data.extend_from_slice(wide_bytes);
    data
}

fn write_drop_effect(effect: u32) -> Result<(), String> {
    let format = register_format(w!("Preferred DropEffect"))?;
    set_clipboard_bytes(&[effect], format)
}

fn read_drop_effect() -> Option<u32> {
    let format = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
    if format == 0 {
        return None;
    }
    let locked = get_locked_clipboard_data(format)?;
    let bytes = locked.as_bytes();
    let value = bytes.get(0..4)?;
    Some(u32::from_le_bytes(value.try_into().ok()?))
}

fn read_files() -> Option<Vec<PathBuf>> {
    let locked = get_locked_clipboard_data(CF_HDROP.0 as u32)?;
    let hdrop = HDROP(locked.ptr.cast_mut().cast::<c_void>());
    let file_count = unsafe { DragQueryFileW(hdrop, DRAGDROP_GET_FILES_COUNT, None) };
    let mut paths = Vec::with_capacity(file_count as usize);
    for index in 0..file_count {
        let length = unsafe { DragQueryFileW(hdrop, index, None) } as usize;
        if length == 0 {
            continue;
        }
        let mut buffer = vec![0u16; length + 1];
        let written = unsafe { DragQueryFileW(hdrop, index, Some(buffer.as_mut_slice())) };
        if written == 0 {
            continue;
        }
        let path = String::from_utf16(&buffer[..length]).ok()?;
        paths.push(PathBuf::from(path));
    }
    Some(paths)
}

fn register_format(name: PCWSTR) -> Result<u32, String> {
    let format = unsafe { RegisterClipboardFormatW(name) };
    (format != 0)
        .then_some(format)
        .ok_or_else(|| "无法注册系统剪贴板格式".to_string())
}

fn get_locked_clipboard_data(format: u32) -> Option<LockedGlobal> {
    let handle = unsafe { GetClipboardData(format) }.ok()?;
    LockedGlobal::lock(HGLOBAL(handle.0))
}

fn set_clipboard_bytes<T>(data: &[T], format: u32) -> Result<(), String> {
    unsafe {
        let global = Owned::new(
            GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of_val(data))
                .map_err(|error| format!("无法分配剪贴板内存：{error}"))?,
        );
        let pointer = GlobalLock(*global);
        if pointer.is_null() {
            return Err("无法锁定剪贴板内存".to_string());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), pointer.cast::<T>(), data.len());
        let _ = GlobalUnlock(*global);
        SetClipboardData(format, Some(HANDLE(global.0)))
            .map_err(|error| format!("无法写入系统剪贴板：{error}"))?;
        std::mem::forget(global);
    }
    Ok(())
}

struct ClipboardGuard;

impl ClipboardGuard {
    fn open() -> Option<Self> {
        unsafe { OpenClipboard(None) }.ok().map(|_| Self)
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

struct LockedGlobal {
    global: HGLOBAL,
    ptr: *const u8,
    size: usize,
}

impl LockedGlobal {
    fn lock(global: HGLOBAL) -> Option<Self> {
        let pointer = unsafe { GlobalLock(global) }.cast::<u8>();
        if pointer.is_null() {
            return None;
        }
        Some(Self {
            global,
            ptr: pointer,
            size: unsafe { windows::Win32::System::Memory::GlobalSize(global) },
        })
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.size) }
    }
}

impl Drop for LockedGlobal {
    fn drop(&mut self) {
        let _ = unsafe { GlobalUnlock(self.global) };
    }
}

#[cfg(test)]
mod tests {
    use super::{DROPFILES, drop_files_bytes};
    use std::mem::size_of;
    use std::path::PathBuf;

    #[test]
    fn drop_files_payload_contains_utf16_paths() {
        let payload =
            drop_files_bytes(&[PathBuf::from(r"C:\one.txt"), PathBuf::from(r"C:\two.txt")]);
        let p_files_offset = std::mem::offset_of!(DROPFILES, pFiles);
        let f_wide_offset = std::mem::offset_of!(DROPFILES, fWide);
        let p_files = u32::from_ne_bytes(
            payload[p_files_offset..p_files_offset + size_of::<u32>()]
                .try_into()
                .expect("pFiles bytes"),
        );
        let f_wide = i32::from_ne_bytes(
            payload[f_wide_offset..f_wide_offset + size_of::<i32>()]
                .try_into()
                .expect("fWide bytes"),
        );
        assert_eq!(p_files, size_of::<DROPFILES>() as u32);
        assert_eq!(f_wide, 1);

        let wide_bytes = &payload[p_files as usize..];
        let wide = unsafe {
            std::slice::from_raw_parts(
                wide_bytes.as_ptr().cast::<u16>(),
                wide_bytes.len() / size_of::<u16>(),
            )
        };
        let decoded = String::from_utf16(wide).expect("decode drop files");
        assert!(decoded.contains(r"C:\one.txt"));
        assert!(decoded.contains(r"C:\two.txt"));
    }
}
