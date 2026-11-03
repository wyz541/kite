use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString, c_void};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use crate::fs::{DirectorySnapshot, EntryKind, FileEntry, SortSpec, sort_entries};
use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
    CoUninitialize, IBindCtx,
};
use windows::Win32::System::SystemServices::{
    SFGAO_CANDELETE, SFGAO_CANMOVE, SFGAO_CANRENAME, SFGAO_FOLDER,
};
use windows::Win32::UI::Shell::{
    BHID_EnumItems, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_RENAMEONCOLLISION, FileOperation,
    IEnumShellItems, IFileOperation, IFileOperationProgressSink, IShellItem,
    SHCreateItemFromParsingName, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
    SIGDN_PARENTRELATIVEPARSING, ShellExecuteW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, PWSTR};

pub(crate) fn is_shell_namespace_path(path: &Path) -> bool {
    let value = path.as_os_str().to_string_lossy();
    value.starts_with("::{") || value.starts_with("shell:")
}

pub(crate) fn item_capabilities(path: &Path) -> (bool, bool, bool) {
    let Ok(_com) = ComGuard::new() else {
        return (false, false, false);
    };
    let Ok(item) = resolve_item(path) else {
        return (false, false, false);
    };
    let Ok(attributes) =
        (unsafe { item.GetAttributes(SFGAO_CANRENAME | SFGAO_CANDELETE | SFGAO_CANMOVE) })
    else {
        return (false, false, false);
    };
    (
        attributes.0 & SFGAO_CANRENAME.0 != 0,
        attributes.0 & SFGAO_CANDELETE.0 != 0,
        attributes.0 & SFGAO_CANMOVE.0 != 0,
    )
}

pub(crate) fn can_rename_item(path: &Path) -> bool {
    item_capabilities(path).0
}

pub(crate) fn can_delete_item(path: &Path) -> bool {
    item_capabilities(path).1
}

pub(crate) fn can_move_item(path: &Path) -> bool {
    item_capabilities(path).2
}

pub(crate) fn transfer_items(
    sources: &[PathBuf],
    destination: &Path,
    move_source: bool,
) -> Result<(), String> {
    let _com = ComGuard::new()?;
    let destination_item =
        resolve_item(destination).map_err(|error| format!("无法打开目标移动设备目录：{error}"))?;
    let operation = file_operation()?;
    for source in sources {
        let source_item = resolve_item(source)
            .map_err(|error| format!("无法读取源项目 {}：{error}", source.display()))?;
        let result = unsafe {
            if move_source {
                operation.MoveItem(
                    &source_item,
                    &destination_item,
                    PCWSTR::null(),
                    None::<&IFileOperationProgressSink>,
                )
            } else {
                operation.CopyItem(
                    &source_item,
                    &destination_item,
                    PCWSTR::null(),
                    None::<&IFileOperationProgressSink>,
                )
            }
        };
        result.map_err(|error| format!("无法写入移动设备：{error}"))?;
    }
    perform_operations(&operation, "写入移动设备")
}

pub(crate) fn delete_item(path: &Path) -> Result<(), String> {
    let _com = ComGuard::new()?;
    let item = resolve_item(path).map_err(|error| format!("无法读取设备项目：{error}"))?;
    let operation = file_operation()?;
    unsafe {
        operation
            .DeleteItem(&item, None::<&IFileOperationProgressSink>)
            .map_err(|error| format!("无法删除设备项目：{error}"))?;
    }
    perform_operations(&operation, "删除设备项目")
}

pub(crate) fn rename_item(path: &Path, name: &str) -> Result<(), String> {
    let _com = ComGuard::new()?;
    let item = resolve_item(path).map_err(|error| format!("无法读取设备项目：{error}"))?;
    let operation = file_operation()?;
    let name = wide(name);
    unsafe {
        operation
            .RenameItem(
                &item,
                PCWSTR(name.as_ptr()),
                None::<&IFileOperationProgressSink>,
            )
            .map_err(|error| format!("无法重命名设备项目：{error}"))?;
    }
    perform_operations(&operation, "重命名设备项目")
}

pub(crate) fn create_item_in(
    directory: &Path,
    name: &str,
    is_directory: bool,
) -> Result<(), String> {
    let _com = ComGuard::new()?;
    let folder = resolve_item(directory).map_err(|error| format!("无法打开设备目录：{error}"))?;
    let operation = file_operation()?;
    let name = wide(name);
    let attributes = if is_directory {
        FILE_ATTRIBUTE_DIRECTORY.0
    } else {
        FILE_ATTRIBUTE_NORMAL.0
    };
    unsafe {
        operation
            .NewItem(
                &folder,
                attributes,
                PCWSTR(name.as_ptr()),
                PCWSTR::null(),
                None::<&IFileOperationProgressSink>,
            )
            .map_err(|error| format!("无法在设备中创建项目：{error}"))?;
    }
    perform_operations(&operation, "创建设备项目")
}

fn file_operation() -> Result<IFileOperation, String> {
    let operation: IFileOperation = unsafe {
        CoCreateInstance(&FileOperation, None::<&windows::core::IUnknown>, CLSCTX_ALL)
            .map_err(|error| format!("无法创建 Windows 文件操作：{error}"))?
    };
    unsafe {
        operation
            .SetOperationFlags(FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_RENAMEONCOLLISION)
            .map_err(|error| format!("无法配置 Windows 文件操作：{error}"))?;
    }
    Ok(operation)
}

fn perform_operations(operation: &IFileOperation, action: &str) -> Result<(), String> {
    unsafe {
        operation
            .PerformOperations()
            .map_err(|error| format!("无法{action}：{error}"))?;
        if operation
            .GetAnyOperationsAborted()
            .map_err(|error| format!("无法读取{action}结果：{error}"))?
            .as_bool()
        {
            return Err(format!("{action}已取消"));
        }
    }
    Ok(())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn load_directory(path: &Path, sort: SortSpec) -> Result<DirectorySnapshot, String> {
    let _com = ComGuard::new()?;
    let folder = resolve_item(path).map_err(|error| format!("无法读取移动设备目录：{error}"))?;
    let enumerator: IEnumShellItems = unsafe {
        folder
            .BindToHandler(None::<&IBindCtx>, &BHID_EnumItems)
            .map_err(|error| format!("无法枚举移动设备目录：{error}"))?
    };
    let mut entries = Vec::new();
    let mut skipped = 0usize;
    loop {
        let mut batch: [Option<IShellItem>; 32] = std::array::from_fn(|_| None);
        let mut fetched = 0u32;
        unsafe {
            enumerator
                .Next(&mut batch, Some(&mut fetched))
                .map_err(|error| format!("读取移动设备项目失败：{error}"))?;
        }
        if fetched == 0 {
            break;
        }
        for item in batch.into_iter().take(fetched as usize).flatten() {
            match shell_entry(&item) {
                Some(entry) => entries.push(entry),
                None => skipped += 1,
            }
        }
    }
    sort_entries(&mut entries, sort);
    Ok(DirectorySnapshot {
        path: path.to_path_buf(),
        entries,
        skipped,
    })
}

pub(crate) fn parent_path(path: &Path) -> Option<PathBuf> {
    let _com = ComGuard::new().ok()?;
    let item = resolve_item(path).ok()?;
    let parent = unsafe { item.GetParent().ok()? };
    let value = display_name_for_item(&parent, SIGDN_DESKTOPABSOLUTEPARSING)?;
    (!value.is_empty()).then(|| PathBuf::from(value))
}

pub(crate) fn display_name(path: &Path) -> Option<String> {
    thread_local! {
        static DISPLAY_NAMES: RefCell<HashMap<PathBuf, String>> = RefCell::new(HashMap::new());
    }
    if let Some(name) = DISPLAY_NAMES.with(|names| names.borrow().get(path).cloned()) {
        return Some(name);
    }
    let _com = ComGuard::new().ok()?;
    let item = resolve_item(path).ok()?;
    let name = display_name_for_item(&item, SIGDN_NORMALDISPLAY)?;
    DISPLAY_NAMES.with(|names| {
        names.borrow_mut().insert(path.to_path_buf(), name.clone());
    });
    Some(name)
}

pub(crate) fn is_directory(path: &Path) -> bool {
    let Ok(_com) = ComGuard::new() else {
        return false;
    };
    let Ok(item) = resolve_item(path) else {
        return false;
    };
    unsafe { item.GetAttributes(SFGAO_FOLDER) }
        .map(|attributes| (attributes.0 & SFGAO_FOLDER.0) != 0)
        .unwrap_or(false)
}

pub(crate) fn open_item(path: &Path) -> Result<(), String> {
    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let file = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code <= 32 {
        Err(format!("无法打开移动设备项目：Windows 错误 {code}"))
    } else {
        Ok(())
    }
}

fn create_item(path: &Path) -> windows::core::Result<IShellItem> {
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    unsafe { SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>) }
}

/// Resolves shell items whose absolute parsing name cannot be reparsed.
///
/// WPD children can expose a desktop-absolute parsing name that
/// SHCreateItemFromParsingName rejects. Walk down from a reparsable
/// ancestor and match the child through the Shell namespace enumerator.
fn resolve_item(path: &Path) -> Result<IShellItem, String> {
    let mut current_path = path.to_path_buf();
    let mut pending_names: Vec<OsString> = Vec::new();

    loop {
        let create_error = match create_item(&current_path) {
            Ok(mut item) => {
                while let Some(name) = pending_names.pop() {
                    item = find_child_item(&item, &name)?;
                }
                return Ok(item);
            }
            Err(error) => error,
        };

        let Some(name) = current_path.file_name() else {
            return Err(format!("无法解析移动设备路径：{create_error}"));
        };
        pending_names.push(name.to_os_string());

        let Some(parent) = current_path.parent() else {
            return Err(format!("无法定位移动设备项目：{}", path.display()));
        };
        if parent == current_path {
            return Err(format!("无法定位移动设备项目：{}", path.display()));
        }
        current_path = parent.to_path_buf();
    }
}

fn find_child_item(parent: &IShellItem, relative_name: &OsStr) -> Result<IShellItem, String> {
    let expected = relative_name.to_string_lossy();
    let enumerator: IEnumShellItems = unsafe {
        parent
            .BindToHandler(None::<&IBindCtx>, &BHID_EnumItems)
            .map_err(|error| format!("无法枚举移动设备父目录：{error}"))?
    };

    loop {
        let mut batch: [Option<IShellItem>; 32] = std::array::from_fn(|_| None);
        let mut fetched = 0u32;
        unsafe {
            enumerator
                .Next(&mut batch, Some(&mut fetched))
                .map_err(|error| format!("读取移动设备子项目失败：{error}"))?;
        }
        if fetched == 0 {
            break;
        }
        for item in batch.into_iter().take(fetched as usize).flatten() {
            let matches_parent_relative = display_name_for_item(&item, SIGDN_PARENTRELATIVEPARSING)
                .is_some_and(|value| value.eq_ignore_ascii_case(&expected));
            let matches_display_name = display_name_for_item(&item, SIGDN_NORMALDISPLAY)
                .is_some_and(|value| value.eq_ignore_ascii_case(&expected));
            let matches_desktop_absolute =
                display_name_for_item(&item, SIGDN_DESKTOPABSOLUTEPARSING).is_some_and(|value| {
                    Path::new(&value)
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&expected))
                });
            if matches_parent_relative || matches_display_name || matches_desktop_absolute {
                return Ok(item);
            }
        }
    }

    Err(format!("无法定位移动设备项目：{expected}"))
}
fn shell_entry(item: &IShellItem) -> Option<FileEntry> {
    let name =
        display_name_for_item(item, SIGDN_NORMALDISPLAY).filter(|value| !value.is_empty())?;
    let path = display_name_for_item(item, SIGDN_DESKTOPABSOLUTEPARSING)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)?;
    let is_dir = unsafe { item.GetAttributes(SFGAO_FOLDER) }
        .map(|attributes| (attributes.0 & SFGAO_FOLDER.0) != 0)
        .unwrap_or(false);
    let extension = Path::new(&name)
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    Some(FileEntry {
        path,
        name,
        kind: if is_dir {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
        size: 0,
        modified: None,
        extension,
        git_status: None,
        svn_status: None,
    })
}

fn display_name_for_item(
    item: &IShellItem,
    kind: windows::Win32::UI::Shell::SIGDN,
) -> Option<String> {
    let value = unsafe { item.GetDisplayName(kind).ok()? };
    Some(take_pwstr(value))
}

fn take_pwstr(value: PWSTR) -> String {
    if value.0.is_null() {
        return String::new();
    }
    let mut length = 0usize;
    while unsafe { *value.0.add(length) } != 0 {
        length += 1;
    }
    let result = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(value.0, length) });
    unsafe { CoTaskMemFree(Some(value.0.cast::<c_void>())) };
    result
}

struct ComGuard(bool);

impl ComGuard {
    fn new() -> Result<Self, String> {
        let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        if result.is_err() && result != windows::Win32::Foundation::RPC_E_CHANGED_MODE {
            return Err(format!("无法初始化移动设备访问：{result:?}"));
        }
        Ok(Self(result.is_ok()))
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::is_shell_namespace_path;
    use std::path::Path;

    #[test]
    #[ignore = "requires a connected portable device"]
    fn live_portable_device_directory_loads() {
        let device = crate::devices::snapshot()
            .portable_devices
            .into_iter()
            .next()
            .expect("portable device");
        let snapshot = super::load_directory(
            Path::new(&device.shell_path),
            crate::fs::SortSpec::default(),
        )
        .expect("load portable device");
        eprintln!(
            "loaded {} entries from {}",
            snapshot.entries.len(),
            device.name
        );

        let child = snapshot
            .entries
            .iter()
            .find(|entry| entry.is_dir())
            .expect("portable device directory")
            .clone();
        let child_snapshot = super::load_directory(&child.path, crate::fs::SortSpec::default())
            .unwrap_or_else(|error| {
                panic!("load portable directory {}: {error}", child.path.display())
            });
        eprintln!(
            "loaded {} entries from {}",
            child_snapshot.entries.len(),
            child.name
        );
    }

    #[test]
    fn recognizes_shell_namespace_paths() {
        assert!(is_shell_namespace_path(Path::new(
            r"::{20D04FE0-3AEA-1069-A2D8-08002B30309D}\\\?\usb#vid_05ac"
        )));
        assert!(is_shell_namespace_path(Path::new("shell:MyComputerFolder")));
        assert!(!is_shell_namespace_path(Path::new(r"C:\Users")));
    }
}
