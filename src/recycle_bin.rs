use std::path::Path;

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_WANTNUKEWARNING, SHFILEOPSTRUCTW, SHFileOperationW,
};
use windows::core::{BOOL, PCWSTR};

pub(crate) fn recycle(path: &Path) -> Result<(), String> {
    match trash::delete(path) {
        Ok(()) => Ok(()),
        Err(error) if should_retry_with_shell(&error) => shell_recycle(path),
        Err(error) => Err(format_trash_error(path, &error)),
    }
}

fn should_retry_with_shell(error: &trash::Error) -> bool {
    matches!(
        error,
        trash::Error::Unknown { description }
            if description.eq_ignore_ascii_case("Some operations were aborted")
    )
}

fn shell_recycle(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt as _;

    let mut from = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if from.contains(&0) {
        return Err("移到回收站失败：路径无效。".to_string());
    }
    from.extend([0, 0]);

    let mut operation = SHFILEOPSTRUCTW {
        hwnd: HWND::default(),
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO.0 | FOF_WANTNUKEWARNING.0) as u16,
        fAnyOperationsAborted: BOOL(0),
        ..Default::default()
    };
    let code = unsafe { SHFileOperationW(&mut operation) };

    if operation.fAnyOperationsAborted.as_bool() {
        return if path.exists() {
            Err("操作已取消。项目未移动。".to_string())
        } else {
            Ok(())
        };
    }
    if code != 0 {
        return Err(format!(
            "移到回收站失败：Windows Shell 返回错误 {code:#x}。请确认项目未被占用，并重试。"
        ));
    }
    Ok(())
}

fn format_trash_error(path: &Path, error: &trash::Error) -> String {
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    match error {
        trash::Error::Os { code: 5, .. } => {
            format!("移到回收站失败：“{name}”拒绝访问。请检查文件权限。")
        }
        trash::Error::Os { code: 2, .. } => {
            format!("移到回收站失败：“{name}”不存在或已被移动。")
        }
        trash::Error::Unknown { description }
            if description.eq_ignore_ascii_case("Some operations were aborted") =>
        {
            format!("移到回收站失败：“{name}”无法进入回收站。Windows Shell 没有完成操作。")
        }
        _ => format!("移到回收站失败：“{name}”无法移动。请确认项目未被占用，并重试。"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retries_only_the_aborted_operation_error() {
        assert!(should_retry_with_shell(&trash::Error::Unknown {
            description: "Some operations were aborted".to_string(),
        }));
        assert!(!should_retry_with_shell(&trash::Error::Unknown {
            description: "other".to_string(),
        }));
    }

    #[test]
    fn access_denied_message_names_the_item() {
        let message = format_trash_error(
            Path::new(r"C:\Work\locked.txt"),
            &trash::Error::Os {
                code: 5,
                description: "denied".to_string(),
            },
        );
        assert!(message.contains("locked.txt"));
        assert!(message.contains("权限"));
    }
}
