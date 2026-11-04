use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, E_OUTOFMEMORY, GlobalFree,
    HANDLE, HGLOBAL, POINT, S_OK,
};
use windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IBindCtx, IDataObject, TYMED_HGLOBAL,
};
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
    BHID_DataObject, Common::ITEMIDLIST, DROPFILES, DragQueryFileW, HDROP, ILFree,
    SHCreateShellItemArrayFromIDLists, SHParseDisplayName,
};
use windows::core::{BOOL, HRESULT, PCWSTR, implement, w};

const DRAGDROP_GET_FILES_COUNT: u32 = 0xFFFF_FFFF;

pub(crate) fn trace_drag(message: &str) {
    let Some(path) = std::env::var_os("KITE_DRAG_TRACE") else {
        return;
    };
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

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
    trace_drag(&format!(
        "creating shell data object for {} path(s): {:?}",
        paths.len(),
        paths
    ));
    let data_object = shell_drag_data_object(paths)?;
    unsafe {
        let format = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        let format_status = data_object.QueryGetData(&format);
        trace_drag(&format!(
            "shell data object CF_HDROP status={:#010x}",
            format_status.0
        ));
        if format_status.is_err() {
            return Err("系统拖放数据不支持 CF_HDROP".to_string());
        }
        let drop_source: IDropSource = KiteDropSource.into();
        let mut effect = DROPEFFECT_COPY;
        trace_drag("calling DoDragDrop");
        let result = DoDragDrop(
            &data_object,
            &drop_source,
            DROPEFFECT_COPY,
            &mut effect as *mut DROPEFFECT,
        );
        trace_drag(&format!(
            "DoDragDrop returned hresult={:#010x} effect={}",
            result.0, effect.0
        ));
        result
            .ok()
            .map_err(|error| format!("文件拖放失败：{error}"))
    }
}

fn shell_drag_data_object(paths: &[PathBuf]) -> Result<IDataObject, String> {
    if paths.is_empty() {
        return Err("没有可拖动的文件".to_string());
    }
    let pidls = paths
        .iter()
        .map(|path| ShellPidl::parse(path))
        .collect::<Result<Vec<_>, _>>()?;
    let raw_pidls = pidls.iter().map(ShellPidl::as_ptr).collect::<Vec<_>>();
    unsafe {
        let items = SHCreateShellItemArrayFromIDLists(&raw_pidls)
            .map_err(|error| format!("无法创建 Shell 项目数组：{error}"))?;
        items
            .BindToHandler(None::<&IBindCtx>, &BHID_DataObject)
            .map_err(|error| format!("无法创建系统文件拖放数据：{error}"))
    }
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
    let bytes =
        unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), size_of_val(data)) };
    let global =
        hglobal_from_bytes(bytes).map_err(|error| format!("无法分配剪贴板内存：{error}"))?;
    if let Err(error) = unsafe { SetClipboardData(format, Some(HANDLE(global.0))) } {
        let _ = unsafe { GlobalFree(Some(global)) };
        return Err(format!("无法写入系统剪贴板：{error}"));
    }
    Ok(())
}

fn hglobal_from_bytes(data: &[u8]) -> windows::core::Result<HGLOBAL> {
    if data.is_empty() {
        return Err(E_OUTOFMEMORY.into());
    }
    unsafe {
        let global = GlobalAlloc(GMEM_MOVEABLE, data.len())?;
        let pointer = GlobalLock(global);
        if pointer.is_null() {
            let _ = GlobalFree(Some(global));
            return Err(E_OUTOFMEMORY.into());
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), pointer.cast::<u8>(), data.len());
        // GlobalUnlock returns zero both on failure and when the final lock is
        // released. The Win32 error code distinguishes the successful case.
        let _ = GlobalUnlock(global);
        Ok(global)
    }
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
