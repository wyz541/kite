use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{
    E_OUTOFMEMORY, GlobalFree, HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, POINT, WPARAM,
};
use windows::Win32::System::Com::{
    DVASPECT_CONTENT, FORMATETC, IBindCtx, IDataObject, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{CF_HDROP, DROPEFFECT_COPY, DROPEFFECT_MOVE, IDropSource};
use windows::Win32::UI::Shell::{
    BHID_DataObject, Common::ITEMIDLIST, DROPFILES, DefSubclassProc, DragQueryFileW, HDROP, ILFree,
    RemoveWindowSubclass, SHCreateShellItemArrayFromIDLists, SHDoDragDrop, SHParseDisplayName,
    SetWindowSubclass,
};
use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP, WM_NCDESTROY};
use windows::core::{BOOL, PCWSTR, w};

const DRAGDROP_GET_FILES_COUNT: u32 = 0xFFFF_FFFF;
const FILE_DRAG_SUBCLASS_ID: usize = 0x4B49_5447;
const WM_KITE_FILE_DRAG: u32 = WM_APP + 0x4B49;

struct PendingFileDrag {
    hwnd: isize,
    paths: Vec<PathBuf>,
}

static PENDING_FILE_DRAG: Mutex<Option<PendingFileDrag>> = Mutex::new(None);

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

pub(crate) fn request_file_drag(window: &Window, paths: &[PathBuf]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("没有可拖动的文件".to_string());
    }
    let hwnd = window_hwnd(window)?;
    {
        let mut pending = PENDING_FILE_DRAG
            .lock()
            .map_err(|_| "无法锁定 Windows 文件拖放状态".to_string())?;
        if pending.is_some() {
            return Err("Windows 文件拖放正在启动".to_string());
        }
        *pending = Some(PendingFileDrag {
            hwnd: hwnd.0 as isize,
            paths: paths.to_vec(),
        });
    }

    if !unsafe {
        SetWindowSubclass(
            hwnd,
            Some(file_drag_subclass_proc),
            FILE_DRAG_SUBCLASS_ID,
            0,
        )
    }
    .as_bool()
    {
        clear_pending_file_drag(hwnd);
        return Err("无法安装 Windows 文件拖放调度钩子".to_string());
    }

    if let Err(error) = unsafe { PostMessageW(Some(hwnd), WM_KITE_FILE_DRAG, WPARAM(0), LPARAM(0)) }
    {
        unsafe {
            let _ =
                RemoveWindowSubclass(hwnd, Some(file_drag_subclass_proc), FILE_DRAG_SUBCLASS_ID);
        }
        clear_pending_file_drag(hwnd);
        return Err(format!("无法调度 Windows 文件拖放：{error}"));
    }

    trace_drag("queued OLE file drag after the current window event");
    Ok(())
}

fn window_hwnd(window: &Window) -> Result<HWND, String> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("无法读取 Kite 窗口句柄：{error}"))?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut c_void)),
        _ => Err("Kite 窗口不是 Win32 窗口".to_string()),
    }
}

fn clear_pending_file_drag(hwnd: HWND) {
    if let Ok(mut pending) = PENDING_FILE_DRAG.lock()
        && pending
            .as_ref()
            .is_some_and(|request| request.hwnd == hwnd.0 as isize)
    {
        pending.take();
    }
}

unsafe extern "system" fn file_drag_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    if message == WM_KITE_FILE_DRAG {
        let pending = PENDING_FILE_DRAG
            .lock()
            .ok()
            .and_then(|mut pending| pending.take());
        unsafe {
            let _ =
                RemoveWindowSubclass(hwnd, Some(file_drag_subclass_proc), FILE_DRAG_SUBCLASS_ID);
        }
        if let Some(pending) = pending
            && pending.hwnd == hwnd.0 as isize
        {
            trace_drag("starting Shell file drag after the current window event returned");
            if let Err(error) = drag_file_paths(hwnd, &pending.paths) {
                trace_drag(&format!("native drag failed: {error}"));
                eprintln!("failed to start native file drag: {error}");
            }
        }
        return LRESULT(0);
    }

    if message == WM_NCDESTROY {
        clear_pending_file_drag(hwnd);
        unsafe {
            let _ =
                RemoveWindowSubclass(hwnd, Some(file_drag_subclass_proc), FILE_DRAG_SUBCLASS_ID);
        }
    }

    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

fn drag_file_paths(hwnd: HWND, paths: &[PathBuf]) -> Result<(), String> {
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
        trace_drag("calling SHDoDragDrop with the Kite window handle");
        let result = SHDoDragDrop(
            Some(hwnd),
            &data_object,
            None::<&IDropSource>,
            DROPEFFECT_COPY,
        );
        match result {
            Ok(effect) => {
                trace_drag(&format!("SHDoDragDrop returned effect={}", effect.0));
                Ok(())
            }
            Err(error) => Err(format!("文件拖放失败：{error}")),
        }
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
