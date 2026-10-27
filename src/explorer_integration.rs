use std::io::ErrorKind;
use std::path::Path;

use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const FOLDER_OPEN_KEY: &str = r"Software\Classes\Folder\shell\open";
const FOLDER_OPEN_COMMAND_KEY: &str = r"Software\Classes\Folder\shell\open\command";
const BACKUP_KEY: &str = r"Software\Kite\ExplorerIntegration";
const BACKUP_VERSION: u32 = 1;

const VALUE_VERSION: &str = "Version";
const VALUE_PREVIOUS_KEY_EXISTED: &str = "PreviousKeyExisted";
const VALUE_PREVIOUS_VALUE_EXISTED: &str = "PreviousValueExisted";
const VALUE_PREVIOUS_COMMAND: &str = "PreviousCommand";
const VALUE_KITE_COMMAND: &str = "KiteCommand";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FolderOpenCommand {
    key_existed: bool,
    value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExplorerIntegrationBackup {
    previous: FolderOpenCommand,
    kite_command: String,
}

trait ExplorerRegistry {
    fn folder_open_command(&self) -> Result<FolderOpenCommand, String>;
    fn set_folder_open_command(&mut self, state: &FolderOpenCommand) -> Result<(), String>;
    fn backup(&self) -> Result<Option<ExplorerIntegrationBackup>, String>;
    fn save_backup(&mut self, backup: &ExplorerIntegrationBackup) -> Result<(), String>;
    fn clear_backup(&mut self) -> Result<(), String>;
}

struct WindowsExplorerRegistry;

impl ExplorerRegistry for WindowsExplorerRegistry {
    fn folder_open_command(&self) -> Result<FolderOpenCommand, String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey(FOLDER_OPEN_COMMAND_KEY) {
            Ok(key) => {
                let value = match key.get_value::<String, _>("") {
                    Ok(value) if !value.trim().is_empty() => Some(value),
                    Ok(_) => None,
                    Err(error) if error.kind() == ErrorKind::NotFound => None,
                    Err(error) => {
                        return Err(format!("无法读取文件夹打开命令：{error}"));
                    }
                };
                Ok(FolderOpenCommand {
                    key_existed: true,
                    value,
                })
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(FolderOpenCommand {
                key_existed: false,
                value: None,
            }),
            Err(error) => Err(format!("无法读取文件夹关联：{error}")),
        }
    }

    fn set_folder_open_command(&mut self, state: &FolderOpenCommand) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Some(command) = state.value.as_ref() {
            let (key, _) = hkcu
                .create_subkey(FOLDER_OPEN_COMMAND_KEY)
                .map_err(|error| format!("无法写入文件夹打开命令：{error}"))?;
            key.set_value("", command)
                .map_err(|error| format!("无法保存文件夹打开命令：{error}"))?;
            return Ok(());
        }

        if state.key_existed {
            let key = hkcu
                .open_subkey_with_flags(FOLDER_OPEN_COMMAND_KEY, winreg::enums::KEY_SET_VALUE)
                .map_err(|error| format!("无法打开文件夹打开命令：{error}"))?;
            match key.delete_value("") {
                Ok(()) => return Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(format!("无法清除文件夹打开命令：{error}")),
            }
        }

        let open_key = match hkcu.open_subkey_with_flags(FOLDER_OPEN_KEY, winreg::enums::KEY_WRITE)
        {
            Ok(key) => key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("无法打开文件夹菜单配置：{error}")),
        };
        match open_key.delete_subkey_all("command") {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("无法移除文件夹打开命令：{error}")),
        }
    }

    fn backup(&self) -> Result<Option<ExplorerIntegrationBackup>, String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = match hkcu.open_subkey(BACKUP_KEY) {
            Ok(key) => key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("无法读取 Kite 文件管理器设置：{error}")),
        };
        let version = key.get_value::<u32, _>(VALUE_VERSION).unwrap_or(0);
        if version != BACKUP_VERSION {
            return Ok(None);
        }
        let kite_command = key
            .get_value::<String, _>(VALUE_KITE_COMMAND)
            .unwrap_or_default();
        if kite_command.is_empty() {
            return Ok(None);
        }
        let previous_key_existed = key
            .get_value::<u32, _>(VALUE_PREVIOUS_KEY_EXISTED)
            .unwrap_or(0)
            != 0;
        let previous_value_existed = key
            .get_value::<u32, _>(VALUE_PREVIOUS_VALUE_EXISTED)
            .unwrap_or(0)
            != 0;
        let previous_command = key
            .get_value::<String, _>(VALUE_PREVIOUS_COMMAND)
            .unwrap_or_default();
        Ok(Some(ExplorerIntegrationBackup {
            previous: FolderOpenCommand {
                key_existed: previous_key_existed,
                value: previous_value_existed.then_some(previous_command),
            },
            kite_command,
        }))
    }

    fn save_backup(&mut self, backup: &ExplorerIntegrationBackup) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(BACKUP_KEY)
            .map_err(|error| format!("无法保存 Kite 文件管理器设置：{error}"))?;
        let previous_command = backup.previous.value.clone().unwrap_or_default();
        key.set_value(VALUE_VERSION, &BACKUP_VERSION)
            .and_then(|_| {
                key.set_value(
                    VALUE_PREVIOUS_KEY_EXISTED,
                    &u32::from(backup.previous.key_existed),
                )
            })
            .and_then(|_| {
                key.set_value(
                    VALUE_PREVIOUS_VALUE_EXISTED,
                    &u32::from(backup.previous.value.is_some()),
                )
            })
            .and_then(|_| key.set_value(VALUE_PREVIOUS_COMMAND, &previous_command))
            .and_then(|_| key.set_value(VALUE_KITE_COMMAND, &backup.kite_command))
            .map_err(|error| format!("无法保存文件夹默认设置备份：{error}"))
    }

    fn clear_backup(&mut self) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let kite_key = match hkcu.open_subkey_with_flags(r"Software\Kite", winreg::enums::KEY_WRITE)
        {
            Ok(key) => key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("无法打开 Kite 设置：{error}")),
        };
        match kite_key.delete_subkey_all("ExplorerIntegration") {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("无法清除 Kite 文件管理器设置：{error}")),
        }
    }
}

pub(crate) fn set_as_default_file_manager() -> Result<String, String> {
    let command = kite_folder_open_command(&current_executable()?);
    let mut registry = WindowsExplorerRegistry;
    let message = enable_default_file_manager(&mut registry, &command)?;
    notify_shell_association_change();
    Ok(message)
}

pub(crate) fn restore_previous_file_manager() -> Result<String, String> {
    let mut registry = WindowsExplorerRegistry;
    let message = restore_default_file_manager(&mut registry)?;
    notify_shell_association_change();
    Ok(message)
}

fn current_executable() -> Result<std::path::PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("无法定位 Kite.exe：{error}"))
}

fn kite_folder_open_command(executable: &Path) -> String {
    format!("\"{}\" \"%1\"", dunce::simplified(executable).display())
}

fn enable_default_file_manager(
    registry: &mut impl ExplorerRegistry,
    kite_command: &str,
) -> Result<String, String> {
    if kite_command.trim().is_empty() {
        return Err("Kite 打开命令为空".to_string());
    }
    let current = registry.folder_open_command()?;
    let existing_backup = registry.backup()?;
    let current_is_backed_up_kite = existing_backup
        .as_ref()
        .is_some_and(|backup| current.value.as_deref() == Some(backup.kite_command.as_str()));
    let current_is_new_kite = current.value.as_deref() == Some(kite_command);

    if current_is_new_kite && existing_backup.is_none() {
        return Ok("Kite 已是默认文件夹打开方式".to_string());
    }

    let backup = if current_is_backed_up_kite {
        let previous = existing_backup
            .as_ref()
            .map(|backup| backup.previous.clone())
            .unwrap_or_else(|| current.clone());
        ExplorerIntegrationBackup {
            previous,
            kite_command: kite_command.to_string(),
        }
    } else {
        ExplorerIntegrationBackup {
            previous: current.clone(),
            kite_command: kite_command.to_string(),
        }
    };

    let kite_state = FolderOpenCommand {
        key_existed: true,
        value: Some(kite_command.to_string()),
    };
    if current.value.as_deref() != Some(kite_command) {
        registry
            .set_folder_open_command(&kite_state)
            .map_err(|error| format!("无法启用 Kite 默认文件管理器：{error}"))?;
    }
    if let Err(error) = registry.save_backup(&backup) {
        let _ = registry.set_folder_open_command(&current);
        return Err(format!("无法保存默认文件管理器设置：{error}"));
    }

    Ok("Kite 已设为默认文件夹打开方式".to_string())
}

fn restore_default_file_manager(registry: &mut impl ExplorerRegistry) -> Result<String, String> {
    let Some(backup) = registry.backup()? else {
        return Err("Kite 没有可恢复的默认文件管理器设置".to_string());
    };
    let current = registry.folder_open_command()?;
    if current.value.as_deref() != Some(backup.kite_command.as_str()) {
        return Err("文件夹默认打开方式已由其他应用更改。Kite 未覆盖当前设置。".to_string());
    }
    registry
        .set_folder_open_command(&backup.previous)
        .map_err(|error| format!("无法恢复文件夹默认打开方式：{error}"))?;
    if let Err(error) = registry.clear_backup() {
        return Err(format!(
            "文件夹打开方式已恢复，但清理 Kite 设置失败：{error}"
        ));
    }
    Ok("已恢复之前的文件夹打开方式".to_string())
}

fn notify_shell_association_change() {
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ExplorerIntegrationBackup, ExplorerRegistry, FolderOpenCommand,
        enable_default_file_manager, kite_folder_open_command, restore_default_file_manager,
    };
    use std::path::Path;

    #[derive(Default)]
    struct MemoryRegistry {
        command: FolderOpenCommand,
        backup: Option<ExplorerIntegrationBackup>,
    }

    impl ExplorerRegistry for MemoryRegistry {
        fn folder_open_command(&self) -> Result<FolderOpenCommand, String> {
            Ok(self.command.clone())
        }

        fn set_folder_open_command(&mut self, state: &FolderOpenCommand) -> Result<(), String> {
            self.command = state.clone();
            Ok(())
        }

        fn backup(&self) -> Result<Option<ExplorerIntegrationBackup>, String> {
            Ok(self.backup.clone())
        }

        fn save_backup(&mut self, backup: &ExplorerIntegrationBackup) -> Result<(), String> {
            self.backup = Some(backup.clone());
            Ok(())
        }

        fn clear_backup(&mut self) -> Result<(), String> {
            self.backup = None;
            Ok(())
        }
    }

    const KITE: &str = r#""C:\Apps\Kite.exe" "%1""#;
    const FILES: &str = r#""C:\Apps\Files.exe" "%1""#;

    #[test]
    fn command_quotes_executable_and_folder_argument() {
        assert_eq!(
            kite_folder_open_command(Path::new(r"C:\Program Files\Kite\Kite.exe")),
            r#""C:\Program Files\Kite\Kite.exe" "%1""#
        );
    }

    #[test]
    fn enabling_saves_and_restores_the_previous_handler() {
        let mut registry = MemoryRegistry {
            command: FolderOpenCommand {
                key_existed: true,
                value: Some(FILES.to_string()),
            },
            backup: None,
        };

        enable_default_file_manager(&mut registry, KITE).expect("enable Kite");
        assert_eq!(registry.command.value.as_deref(), Some(KITE));
        assert_eq!(
            registry
                .backup
                .as_ref()
                .and_then(|backup| backup.previous.value.as_deref()),
            Some(FILES)
        );

        restore_default_file_manager(&mut registry).expect("restore previous handler");
        assert_eq!(registry.command.value.as_deref(), Some(FILES));
        assert!(registry.backup.is_none());
    }

    #[test]
    fn enabling_twice_keeps_the_original_handler() {
        let mut registry = MemoryRegistry {
            command: FolderOpenCommand {
                key_existed: true,
                value: Some(FILES.to_string()),
            },
            backup: None,
        };
        enable_default_file_manager(&mut registry, KITE).expect("first enable");
        enable_default_file_manager(&mut registry, KITE).expect("second enable");
        restore_default_file_manager(&mut registry).expect("restore previous handler");

        assert_eq!(registry.command.value.as_deref(), Some(FILES));
    }

    #[test]
    fn missing_user_override_is_restored_as_missing() {
        let mut registry = MemoryRegistry::default();
        enable_default_file_manager(&mut registry, KITE).expect("enable Kite");
        restore_default_file_manager(&mut registry).expect("restore missing override");

        assert!(!registry.command.key_existed);
        assert!(registry.command.value.is_none());
    }

    #[test]
    fn restore_refuses_to_overwrite_an_external_change() {
        let mut registry = MemoryRegistry {
            command: FolderOpenCommand {
                key_existed: true,
                value: Some(FILES.to_string()),
            },
            backup: None,
        };
        enable_default_file_manager(&mut registry, KITE).expect("enable Kite");
        registry.command.value = Some(r#""C:\Apps\Other.exe" "%1""#.to_string());

        let error = restore_default_file_manager(&mut registry).expect_err("restore must fail");

        assert!(error.contains("其他应用"));
        assert_eq!(
            registry.command.value.as_deref(),
            Some(r#""C:\Apps\Other.exe" "%1""#)
        );
    }
}
