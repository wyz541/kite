use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoUninitialize, IPersistFile,
};
use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
use windows::core::{HSTRING, Interface};
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;

const FOLDER_OPEN_KEY: &str = r"Software\Classes\Folder\shell\open";
const FOLDER_OPEN_COMMAND_KEY: &str = r"Software\Classes\Folder\shell\open\command";
const BACKUP_KEY: &str = r"Software\Kite\ExplorerIntegration";
const APP_PATH_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths\Kite.exe";
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
    let mut message = enable_default_file_manager(&mut registry, &command)?;
    if let Err(error) = register_start_menu_shortcut() {
        message.push_str(&format!("；开始菜单注册失败：{error}"));
    }
    notify_shell_association_change();
    Ok(message)
}

pub(crate) fn restore_previous_file_manager() -> Result<String, String> {
    let mut registry = WindowsExplorerRegistry;
    let message = restore_default_file_manager(&mut registry)?;
    notify_shell_association_change();
    Ok(message)
}

pub(crate) fn register_start_menu_shortcut() -> Result<String, String> {
    let executable = current_executable()?;
    register_app_path(&executable)?;
    let shortcut = start_menu_shortcut_path()?;
    create_shell_link(&executable, &shortcut)?;
    notify_shell_association_change();
    Ok(format!(
        "已创建开始菜单快捷方式。重启 PowerToys Run 后可搜索 Kite：{}",
        shortcut.display()
    ))
}

fn current_executable() -> Result<std::path::PathBuf, String> {
    std::env::current_exe().map_err(|error| format!("无法定位 Kite.exe：{error}"))
}

fn register_app_path(executable: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey(APP_PATH_KEY)
        .map_err(|error| format!("无法注册 App Paths：{error}"))?;
    let executable = dunce::simplified(executable);
    key.set_value("", &executable.to_string_lossy().to_string())
        .map_err(|error| format!("无法保存 App Paths 路径：{error}"))?;
    if let Some(directory) = executable.parent() {
        key.set_value("Path", &directory.to_string_lossy().to_string())
            .map_err(|error| format!("无法保存 App Paths 目录：{error}"))?;
    }
    Ok(())
}

fn start_menu_shortcut_path() -> Result<PathBuf, String> {
    let appdata = dirs::data_dir()
        .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
        .ok_or_else(|| "无法定位当前用户的 AppData 目录".to_string())?;
    let programs = appdata
        .join("Microsoft")
        .join("Windows")
        .join("Start Menu")
        .join("Programs");
    std::fs::create_dir_all(&programs).map_err(|error| format!("无法创建开始菜单目录：{error}"))?;
    Ok(programs.join("Kite.lnk"))
}

fn create_shell_link(executable: &Path, shortcut: &Path) -> Result<(), String> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
        let result = (|| -> Result<(), String> {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|error| format!("无法创建开始菜单链接：{error}"))?;
            let executable_path = dunce::simplified(executable);
            let executable = HSTRING::from(executable_path.as_os_str());
            link.SetPath(&executable)
                .map_err(|error| format!("无法设置快捷方式目标：{error}"))?;
            if let Some(directory) = executable_path.parent() {
                link.SetWorkingDirectory(&HSTRING::from(directory.as_os_str()))
                    .map_err(|error| format!("无法设置快捷方式目录：{error}"))?;
            }
            link.SetDescription(&HSTRING::from("Kite 文件管理器"))
                .map_err(|error| format!("无法设置快捷方式说明：{error}"))?;
            link.SetIconLocation(&executable, 0)
                .map_err(|error| format!("无法设置快捷方式图标：{error}"))?;
            let persist: IPersistFile = link
                .cast()
                .map_err(|error| format!("无法保存快捷方式：{error}"))?;
            persist
                .Save(&HSTRING::from(shortcut.as_os_str()), true)
                .map_err(|error| format!("无法写入开始菜单快捷方式：{error}"))?;
            Ok(())
        })();
        if initialized {
            CoUninitialize();
        }
        result
    }
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
        ExplorerIntegrationBackup, ExplorerRegistry, FolderOpenCommand, create_shell_link,
        enable_default_file_manager, kite_folder_open_command, restore_default_file_manager,
    };
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

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
    fn creates_a_windows_shell_link() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("kite-shell-link-{unique}"));
        std::fs::create_dir_all(&directory).expect("create shell link directory");
        let executable = std::env::var_os("WINDIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
            .join("System32")
            .join("notepad.exe");
        let shortcut = directory.join("Kite.lnk");

        create_shell_link(&executable, &shortcut).expect("create shell link");
        assert!(shortcut.is_file());

        std::fs::remove_file(shortcut).expect("remove shell link");
        std::fs::remove_dir(directory).expect("remove shell link directory");
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
