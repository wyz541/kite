use std::ffi::c_void;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, HANDLE, HGLOBAL, POINT, S_OK,
};
use windows::Win32::System::Com::{IBindCtx, IDataObject};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DoDragDrop, IDropSource,
    IDropSource_Impl,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::{
    Common::ITEMIDLIST, DROPFILES, DragQueryFileW, HDROP, ILFindLastID, ILFree, SHCreateDataObject,
    SHParseDisplayName,
};
use windows::core::{BOOL, HRESULT, Owned, PCWSTR, implement, w};

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

pub(crate) fn drag_file_paths(paths: &[PathBuf]) -> Result<(), String> {
    let (_pidls, data_object) = shell_drag_data_object(paths)?;
    unsafe {
        let drop_source: IDropSource = KiteDropSource.into();
        let mut effect = DROPEFFECT_COPY;
        DoDragDrop(
            &data_object,
            &drop_source,
            DROPEFFECT_COPY,
            &mut effect as *mut DROPEFFECT,
        )
        .ok()
        .map_err(|error| format!("文件拖放失败：{error}"))
    }
}

fn shell_drag_data_object(paths: &[PathBuf]) -> Result<(Vec<ShellPidl>, IDataObject), String> {
    let parent = shared_parent(paths)?;
    let parent_pidl = ShellPidl::parse(parent)?;
    let mut full_pidls = Vec::with_capacity(paths.len());
    for path in paths {
        full_pidls.push(ShellPidl::parse(path)?);
    }
    let child_pidls = full_pidls
        .iter()
        .map(|pidl| unsafe { ILFindLastID(pidl.as_ptr()) as *const ITEMIDLIST })
        .collect::<Vec<_>>();
    let data_object = unsafe {
        SHCreateDataObject(
            Some(parent_pidl.as_ptr()),
            Some(&child_pidls),
            None::<&IDataObject>,
        )
        .map_err(|error| format!("无法创建系统文件拖放数据：{error}"))?
    };

    let mut pidls = Vec::with_capacity(paths.len() + 1);
    pidls.push(parent_pidl);
    pidls.extend(full_pidls);
    Ok((pidls, data_object))
}

fn shared_parent(paths: &[PathBuf]) -> Result<&Path, String> {
    let parent = paths
        .first()
        .and_then(|path| path.parent())
        .ok_or_else(|| "文件路径缺少父文件夹".to_string())?;
    if paths.iter().any(|path| path.parent() != Some(parent)) {
        return Err("一次只能拖动同一文件夹中的项目".to_string());
    }
    Ok(parent)
}

struct ShellPidl(*mut ITEMIDLIST);

impl ShellPidl {
    fn parse(path: &Path) -> Result<Self, String> {
        let wide = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut pidl = std::ptr::null_mut();
        unsafe {
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>, &mut pidl, 0, None)
                .map_err(|error| format!("无法解析文件路径 {}：{error}", path.display()))?;
        }
        if pidl.is_null() {
            return Err(format!("无法解析文件路径：{}", path.display()));
        }
        Ok(Self(pidl))
    }

    fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0
    }
}

impl Drop for ShellPidl {
    fn drop(&mut self) {
        unsafe { ILFree(Some(self.0)) };
    }
}

#[implement(IDropSource)]
struct KiteDropSource;

#[allow(non_snake_case)]
impl IDropSource_Impl for KiteDropSource_Impl {
    fn QueryContinueDrag(&self, escape_pressed: BOOL, key_state: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape_pressed.as_bool() {
            DRAGDROP_S_CANCEL
        } else if key_state.0 & MK_LBUTTON.0 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
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
    use super::{DROPFILES, drop_files_bytes, shared_parent};
    use std::mem::size_of;
    use std::path::{Path, PathBuf};

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

    #[test]
    fn accepts_sibling_paths_from_one_folder() {
        let paths = [
            PathBuf::from(r"C:\folder\one.txt"),
            PathBuf::from(r"C:\folder\two.txt"),
        ];

        assert_eq!(shared_parent(&paths).unwrap(), Path::new(r"C:\folder"));
    }

    #[test]
    fn rejects_native_drag_paths_from_different_folders() {
        let paths = [
            PathBuf::from(r"C:\one\a.txt"),
            PathBuf::from(r"C:\two\b.txt"),
        ];

        assert!(shared_parent(&paths).is_err());
    }
}
