use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(target_os = "windows", allow(dead_code))]
pub enum HostPlatform {
    Windows,
    MacOS,
    Other,
}

pub const fn host_platform() -> HostPlatform {
    #[cfg(target_os = "windows")]
    {
        HostPlatform::Windows
    }
    #[cfg(target_os = "macos")]
    {
        HostPlatform::MacOS
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        HostPlatform::Other
    }
}

pub fn root_directory(current_dir: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        current_dir
            .ancestors()
            .last()
            .unwrap_or(current_dir)
            .to_path_buf()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = current_dir;
        PathBuf::from("/")
    }
}

pub fn default_root_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        PathBuf::from(r"C:\")
    }
    #[cfg(not(target_os = "windows"))]
    {
        PathBuf::from("/")
    }
}

pub fn paths_equal(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    #[cfg(target_os = "windows")]
    {
        let normalize = |path: &Path| {
            let mut value = path.to_string_lossy().replace('/', "\\").to_lowercase();
            while value.len() > 3 && value.ends_with('\\') {
                value.pop();
            }
            value
        };
        normalize(left) == normalize(right)
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

pub(crate) fn validate_file_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("名称不能为空".to_string());
    }
    if name == "." || name == ".." {
        return Err("名称无效".to_string());
    }
    if name.contains('\0') {
        return Err("名称不能包含空字符".to_string());
    }
    if name.contains('/') {
        return Err("名称不能包含路径分隔符".to_string());
    }

    #[cfg(target_os = "windows")]
    {
        if name
            .chars()
            .any(|ch| matches!(ch, '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*'))
        {
            return Err("名称包含 Windows 不允许的字符".to_string());
        }
        if name.ends_with('.') || name.ends_with(' ') {
            return Err("名称不能以点或空格结尾".to_string());
        }
        let stem = name
            .split('.')
            .next()
            .unwrap_or(name)
            .trim_end()
            .to_ascii_uppercase();
        let reserved = matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        );
        if reserved {
            return Err("名称是 Windows 保留名称".to_string());
        }
    }

    Ok(())
}

pub fn volume_label(name: &str, path: &Path) -> String {
    #[cfg(target_os = "windows")]
    {
        let letter = path.to_string_lossy().chars().next().unwrap_or('?');
        format!("{name} ({letter}:)")
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_file_name_rules_reject_path_separators() {
        assert!(validate_file_name("valid-name.txt").is_ok());
        assert!(validate_file_name("").is_err());
        assert!(validate_file_name("..").is_err());
        assert!(validate_file_name("bad/name").is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_rules_reject_reserved_names_and_characters() {
        assert!(validate_file_name("bad:name").is_err());
        assert!(validate_file_name("bad.").is_err());
        assert!(validate_file_name("CON").is_err());
        assert!(paths_equal(Path::new(r"C:\Users"), Path::new("c:/users/")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_paths_preserve_case_sensitive_identity() {
        assert!(!paths_equal(
            Path::new("/Users/Example"),
            Path::new("/Users/example")
        ));
    }
}
