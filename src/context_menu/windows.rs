use std::ffi::c_void;
use std::mem::size_of;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use futures_channel::oneshot;
use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::Com::IBindCtx;
use windows::Win32::UI::Shell::{
    BHID_SFUIObject, CMF_EXTENDEDVERBS, CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO,
    CMINVOKECOMMANDINFOEX, DefSubclassProc, IContextMenu, IContextMenu2, IContextMenu3, IShellItem,
    IShellItemArray, RemoveWindowSubclass, SHCreateItemFromParsingName,
    SHCreateShellItemArrayFromShellItem, SetWindowSubclass,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreatePopupMenu, DestroyMenu, GetCursorPos, GetSystemMetrics, HMENU, PostMessageW,
    SM_MENUDROPALIGNMENT, SW_SHOWNORMAL, SetForegroundWindow, TPM_LEFTALIGN, TPM_RETURNCMD,
    TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_APP, WM_DRAWITEM, WM_INITMENUPOPUP,
    WM_MEASUREITEM, WM_MENUCHAR, WM_NCDESTROY,
};
use windows::core::{Interface, PCSTR, PCWSTR};

const ID_FIRST: u32 = 1;
const ID_LAST: u32 = 0x7FFF;
const MENU_SUBCLASS_ID: usize = 0x4B49_5445;
const DEFERRED_MENU_SUBCLASS_ID: usize = 0x4B49_5446;
const WM_KITE_SYSTEM_MENU: u32 = WM_APP + 0x4B;

struct PendingSystemMenu {
    hwnd: isize,
    path: PathBuf,
    sender: oneshot::Sender<Result<bool, String>>,
}

static PENDING_SYSTEM_MENU: Mutex<Option<PendingSystemMenu>> = Mutex::new(None);

struct ShellMenuHook {
    menu2: Option<IContextMenu2>,
    menu3: Option<IContextMenu3>,
}

struct SubclassGuard {
    hwnd: HWND,
    hook: *mut ShellMenuHook,
}

impl Drop for SubclassGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = RemoveWindowSubclass(self.hwnd, Some(menu_subclass_proc), MENU_SUBCLASS_ID);
            drop(Box::from_raw(self.hook));
        }
    }
}

struct MenuGuard(HMENU);

impl Drop for MenuGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}

pub(crate) fn request_system_menu(
    window: &Window,
    path: &Path,
) -> Result<oneshot::Receiver<Result<bool, String>>, String> {
    let hwnd = window_hwnd(window)?;
    let (sender, receiver) = oneshot::channel();
    {
        let mut pending = PENDING_SYSTEM_MENU
            .lock()
            .map_err(|_| "无法锁定 Windows 菜单状态".to_string())?;
        if pending.is_some() {
            return Err("Windows 原生菜单正在打开".to_string());
        }
        *pending = Some(PendingSystemMenu {
            hwnd: hwnd.0 as isize,
            path: path.to_path_buf(),
            sender,
        });
    }

    if !unsafe {
        SetWindowSubclass(
            hwnd,
            Some(deferred_menu_subclass_proc),
            DEFERRED_MENU_SUBCLASS_ID,
            0,
        )
    }
    .as_bool()
    {
        clear_pending_system_menu(hwnd);
        return Err("无法安装 Windows 菜单调度钩子".to_string());
    }

    if let Err(error) =
        unsafe { PostMessageW(Some(hwnd), WM_KITE_SYSTEM_MENU, WPARAM(0), LPARAM(0)) }
    {
        unsafe {
            let _ = RemoveWindowSubclass(
                hwnd,
                Some(deferred_menu_subclass_proc),
                DEFERRED_MENU_SUBCLASS_ID,
            );
        }
        clear_pending_system_menu(hwnd);
        return Err(format!("无法调度 Windows 原生菜单：{error}"));
    }

    Ok(receiver)
}

fn clear_pending_system_menu(hwnd: HWND) {
    if let Ok(mut pending) = PENDING_SYSTEM_MENU.lock()
        && pending
            .as_ref()
            .is_some_and(|request| request.hwnd == hwnd.0 as isize)
    {
        pending.take();
    }
}

fn show_native_context_menu_for_hwnd(hwnd: HWND, path: &Path) -> Result<bool, String> {
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();

    unsafe {
        let item: IShellItem =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None::<&IBindCtx>)
                .map_err(|error| format!("无法读取 Windows Shell 项目：{error}"))?;
        let items: IShellItemArray = SHCreateShellItemArrayFromShellItem(&item)
            .map_err(|error| format!("无法创建 Windows Shell 项目数组：{error}"))?;
        let menu: IContextMenu = items
            .BindToHandler(None::<&IBindCtx>, &BHID_SFUIObject)
            .map_err(|error| format!("无法读取 Windows 右键菜单：{error}"))?;
        let popup = MenuGuard(
            CreatePopupMenu().map_err(|error| format!("无法创建 Windows 菜单：{error}"))?,
        );
        menu.QueryContextMenu(
            popup.0,
            0,
            ID_FIRST,
            ID_LAST,
            CMF_NORMAL | CMF_EXTENDEDVERBS,
        )
        .ok()
        .map_err(|error| format!("无法生成 Windows 右键菜单：{error}"))?;

        let mut point = POINT::default();
        GetCursorPos(&mut point).map_err(|error| format!("无法读取鼠标位置：{error}"))?;
        let _ = SetForegroundWindow(hwnd);

        let hook = Box::into_raw(Box::new(ShellMenuHook {
            menu2: menu.cast::<IContextMenu2>().ok(),
            menu3: menu.cast::<IContextMenu3>().ok(),
        }));
        if !SetWindowSubclass(
            hwnd,
            Some(menu_subclass_proc),
            MENU_SUBCLASS_ID,
            hook as usize,
        )
        .as_bool()
        {
            drop(Box::from_raw(hook));
            return Err("无法安装 Windows 菜单消息钩子".to_string());
        }
        let _subclass = SubclassGuard { hwnd, hook };

        let alignment = if GetSystemMetrics(SM_MENUDROPALIGNMENT) == 0 {
            TPM_LEFTALIGN
        } else {
            TPM_RIGHTALIGN
        };
        let selected = TrackPopupMenuEx(
            popup.0,
            (TPM_RETURNCMD | TPM_RIGHTBUTTON | alignment).0,
            point.x,
            point.y,
            hwnd,
            None,
        )
        .0 as u32;
        if selected < ID_FIRST {
            return Ok(false);
        }

        let invoke = CMINVOKECOMMANDINFOEX {
            cbSize: size_of::<CMINVOKECOMMANDINFOEX>() as u32,
            fMask: CMIC_MASK_PTINVOKE,
            hwnd,
            lpVerb: PCSTR((selected - ID_FIRST) as usize as *const u8),
            lpVerbW: PCWSTR::null(),
            nShow: SW_SHOWNORMAL.0,
            hIcon: HANDLE::default(),
            ptInvoke: point,
            ..Default::default()
        };
        menu.InvokeCommand((&invoke as *const CMINVOKECOMMANDINFOEX).cast::<CMINVOKECOMMANDINFO>())
            .map_err(|error| format!("Windows 菜单命令执行失败：{error}"))?;
    }

    Ok(true)
}

fn window_hwnd(window: &Window) -> Result<HWND, String> {
    let handle = HasWindowHandle::window_handle(window)
        .map_err(|error| format!("无法读取 Kite 窗口句柄：{error}"))?;
    match handle.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(HWND(handle.hwnd.get() as *mut c_void)),
        _ => Err("Kite 窗口不是 Win32 窗口".to_string()),
    }
}

unsafe extern "system" fn deferred_menu_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    if message == WM_KITE_SYSTEM_MENU {
        let pending = PENDING_SYSTEM_MENU
            .lock()
            .ok()
            .and_then(|mut pending| pending.take());
        unsafe {
            let _ = RemoveWindowSubclass(
                hwnd,
                Some(deferred_menu_subclass_proc),
                DEFERRED_MENU_SUBCLASS_ID,
            );
        }
        if let Some(pending) = pending
            && pending.hwnd == hwnd.0 as isize
        {
            let result = show_native_context_menu_for_hwnd(hwnd, &pending.path);
            let _ = pending.sender.send(result);
        }
        return LRESULT(0);
    }

    if message == WM_NCDESTROY {
        clear_pending_system_menu(hwnd);
        unsafe {
            let _ = RemoveWindowSubclass(
                hwnd,
                Some(deferred_menu_subclass_proc),
                DEFERRED_MENU_SUBCLASS_ID,
            );
        }
    }

    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

unsafe extern "system" fn menu_subclass_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    ref_data: usize,
) -> LRESULT {
    if matches!(
        message,
        WM_INITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    ) {
        let hook = unsafe { &*(ref_data as *const ShellMenuHook) };
        if let Some(menu) = &hook.menu3 {
            let mut result = LRESULT::default();
            if unsafe { menu.HandleMenuMsg2(message, wparam, lparam, Some(&mut result)) }.is_ok() {
                return result;
            }
        }
        if let Some(menu) = &hook.menu2
            && unsafe { menu.HandleMenuMsg(message, wparam, lparam) }.is_ok()
        {
            return LRESULT::default();
        }
    }

    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, EndMenu, KillTimer, SW_SHOWNORMAL, SetTimer, ShowWindow,
        WINDOW_EX_STYLE, WM_TIMER, WS_POPUP,
    };
    use windows::core::w;

    const TEST_SUBCLASS_ID: usize = 0x4B49_5446;
    const TEST_TIMER_ID: usize = 1;

    unsafe extern "system" fn dismiss_menu_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _: usize,
        _: usize,
    ) -> LRESULT {
        if message == WM_TIMER {
            unsafe {
                let _ = KillTimer(Some(hwnd), TEST_TIMER_ID);
                let _ = EndMenu();
            }
            return LRESULT::default();
        }
        unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
    }

    #[test]
    #[ignore = "opens the native Windows context menu for 500 ms"]
    fn native_context_menu_bridge_opens_and_cancels() {
        unsafe {
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("Kite native menu smoke test"),
                WS_POPUP,
                100,
                100,
                1,
                1,
                None,
                None,
                None,
                None,
            )
            .expect("create smoke-test window");
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetWindowSubclass(hwnd, Some(dismiss_menu_proc), TEST_SUBCLASS_ID, 0);
            let _ = SetTimer(Some(hwnd), TEST_TIMER_ID, 500, None);
            let result =
                show_native_context_menu_for_hwnd(hwnd, Path::new(env!("CARGO_MANIFEST_DIR")));
            let _ = RemoveWindowSubclass(hwnd, Some(dismiss_menu_proc), TEST_SUBCLASS_ID);
            let _ = DestroyWindow(hwnd);
            assert!(result.is_ok(), "{result:?}");
        }
    }
}
