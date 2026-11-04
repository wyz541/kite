use std::ffi::c_void;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::mem::{ManuallyDrop, size_of};
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use windows::Win32::Foundation::{
    DATA_S_SAMEFORMATETC, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS,
    DV_E_DVASPECT, DV_E_FORMATETC, DV_E_LINDEX, DV_E_TYMED, E_NOTIMPL, E_OUTOFMEMORY, E_POINTER,
    GlobalFree, HANDLE, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, POINT, S_OK,
};
use windows::Win32::System::Com::{
    DATADIR_GET, DVASPECT_CONTENT, FORMATETC, IAdviseSink, IDataObject, IDataObject_Impl,
    IEnumFORMATETC, IEnumSTATDATA, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{
    CF_HDROP, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DoDragDrop, IDropSource,
    IDropSource_Impl, OleInitialize, OleUninitialize,
};
use windows::Win32::System::SystemServices::{MK_LBUTTON, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::{DROPFILES, DragQueryFileW, HDROP, SHCreateStdEnumFmtEtc};
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
    let paths = paths.to_vec();
    std::thread::Builder::new()
        .name("kite-file-drag".to_string())
        .spawn(move || {
            unsafe { OleInitialize(None) }
                .map_err(|error| format!("无法初始化 OLE 拖放线程：{error}"))?;
            let result = drag_file_paths_on_sta(&paths);
            unsafe { OleUninitialize() };
            result
        })
        .map_err(|error| format!("无法启动 OLE 拖放线程：{error}"))?
        .join()
        .map_err(|_| "OLE 拖放线程异常退出".to_string())?
}

fn drag_file_paths_on_sta(paths: &[PathBuf]) -> Result<(), String> {
    trace_drag(&format!(
        "creating CF_HDROP data object for {} path(s): {:?}",
        paths.len(),
        paths
    ));
    let data_object: IDataObject = KiteFileDataObject::new(paths)?.into();
    unsafe {
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

#[implement(IDataObject)]
struct KiteFileDataObject {
    paths: Vec<PathBuf>,
    preferred_drop_effect_format: u16,
}

impl KiteFileDataObject {
    fn new(paths: &[PathBuf]) -> Result<Self, String> {
        if paths.is_empty() {
            return Err("没有可拖动的文件".to_string());
        }
        let preferred_drop_effect_format = register_format(w!("Preferred DropEffect"))?
            .try_into()
            .map_err(|_| "系统剪贴板格式超出范围".to_string())?;
        Ok(Self {
            paths: paths.to_vec(),
            preferred_drop_effect_format,
        })
    }

    fn format(cf_format: u16) -> FORMATETC {
        FORMATETC {
            cfFormat: cf_format,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        }
    }

    fn formats(&self) -> Vec<FORMATETC> {
        vec![
            Self::format(CF_HDROP.0),
            Self::format(self.preferred_drop_effect_format),
        ]
    }

    fn validate_format(&self, pformatetc: *const FORMATETC) -> HRESULT {
        if pformatetc.is_null() {
            return E_POINTER;
        }
        let format = unsafe { &*pformatetc };
        if format.cfFormat != CF_HDROP.0 && format.cfFormat != self.preferred_drop_effect_format {
            return DV_E_FORMATETC;
        }
        if format.dwAspect != DVASPECT_CONTENT.0 {
            return DV_E_DVASPECT;
        }
        if format.lindex != -1 {
            return DV_E_LINDEX;
        }
        if !format.ptd.is_null() {
            return DV_E_FORMATETC;
        }
        if format.tymed & TYMED_HGLOBAL.0 as u32 == 0 {
            return DV_E_TYMED;
        }
        S_OK
    }

    fn data_for_format(&self, format: &FORMATETC) -> Vec<u8> {
        if format.cfFormat == CF_HDROP.0 {
            drop_files_bytes(&self.paths)
        } else {
            DROPEFFECT_COPY.0.to_le_bytes().to_vec()
        }
    }
}

#[allow(non_snake_case)]
impl IDataObject_Impl for KiteFileDataObject_Impl {
    fn GetData(&self, pformatetc: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let status = self.this.validate_format(pformatetc);
        if status.is_err() {
            trace_drag(&format!(
                "GetData rejected cf={} result={:#010x}",
                format_id(pformatetc),
                status.0
            ));
            return Err(status.into());
        }
        let bytes = self.this.data_for_format(unsafe { &*pformatetc });
        trace_drag(&format!(
            "GetData served cf={} bytes={}",
            unsafe { (*pformatetc).cfFormat },
            bytes.len()
        ));
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 {
                hGlobal: hglobal_from_bytes(&bytes)?,
            },
            pUnkForRelease: ManuallyDrop::new(None),
        })
    }

    fn GetDataHere(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *mut STGMEDIUM,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
        let status = self.this.validate_format(pformatetc);
        trace_drag(&format!(
            "QueryGetData cf={} result={:#010x}",
            format_id(pformatetc),
            status.0
        ));
        status
    }

    fn GetCanonicalFormatEtc(
        &self,
        pformatectin: *const FORMATETC,
        pformatectout: *mut FORMATETC,
    ) -> HRESULT {
        if pformatectin.is_null() || pformatectout.is_null() {
            return E_POINTER;
        }
        let mut canonical = unsafe { *pformatectin };
        canonical.ptd = std::ptr::null_mut();
        unsafe { pformatectout.write(canonical) };
        DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        _pformatetc: *const FORMATETC,
        _pmedium: *const STGMEDIUM,
        _frelease: BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn EnumFormatEtc(&self, dwdirection: u32) -> windows::core::Result<IEnumFORMATETC> {
        trace_drag(&format!("EnumFormatEtc direction={dwdirection}"));
        if dwdirection != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        unsafe { SHCreateStdEnumFmtEtc(&self.this.formats()) }
    }

    fn DAdvise(
        &self,
        _pformatetc: *const FORMATETC,
        _advf: u32,
        _padvsink: windows::core::Ref<IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _dwconnection: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

fn format_id(format: *const FORMATETC) -> u16 {
    if format.is_null() {
        0
    } else {
        unsafe { (*format).cfFormat }
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
