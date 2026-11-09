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
use winreg::RegValue;
use winreg::enums::{HKEY_CURRENT_USER, REG_EXPAND_SZ, REG_SZ};
use winreg::types::FromRegValue;

const FOLDER_OPEN_KEY: &str = r"Software\Classes\Folder\shell\open";
const FOLDER_OPEN_COMMAND_KEY: &str = r"Software\Classes\Folder\shell\open\command";
const FOLDER_EXPLORE_KEY: &str = r"Software\Classes\Folder\shell\explore";
const FOLDER_EXPLORE_COMMAND_KEY: &str = r"Software\Classes\Folder\shell\explore\command";
const BACKUP_KEY: &str = r"Software\Kite\ExplorerIntegration";
const APP_PATH_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths\Kite.exe";
const BACKUP_VERSION: u32 = 2;
const LEGACY_BACKUP_VERSION: u32 = 1;

const VALUE_VERSION: &str = "Version";
const VALUE_KITE_COMMAND: &str = "KiteCommand";
const VALUE_DELEGATE_EXECUTE: &str = "DelegateExecute";

const LEGACY_VALUE_PREVIOUS_KEY_EXISTED: &str = "PreviousKeyExisted";
const LEGACY_VALUE_PREVIOUS_VALUE_EXISTED: &str = "PreviousValueExisted";
const LEGACY_VALUE_PREVIOUS_COMMAND: &str = "PreviousCommand";

#[derive(Clone, Copy, Debug)]
enum FolderVerb {
    Open,
    Explore,
}

impl FolderVerb {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "文件夹打开",
            Self::Explore => "文件夹浏览",
        }
    }

    fn parent_key(self) -> &'static str {
        match self {
            Self::Open => FOLDER_OPEN_KEY,
            Self::Explore => FOLDER_EXPLORE_KEY,
        }
    }

    fn command_key(self) -> &'static str {
        match self {
            Self::Open => FOLDER_OPEN_COMMAND_KEY,
            Self::Explore => FOLDER_EXPLORE_COMMAND_KEY,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShellStringValue {
    text: String,
    expand: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ShellCommand {
    key_existed: bool,
    command: Option<ShellStringValue>,
    delegate_execute: Option<ShellStringValue>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExplorerVerbBackup {
    previous: ShellCommand,
    restore_delegate_execute: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExplorerIntegrationBackup {
    open: ExplorerVerbBackup,
    explore: Option<ExplorerVerbBackup>,
    kite_command: String,
}

trait ExplorerRegistry {
    fn shell_command(&self, verb: FolderVerb) -> Result<ShellCommand, String>;
    fn set_shell_command(&mut self, verb: FolderVerb, state: &ShellCommand) -> Result<(), String>;
    fn backup(&self) -> Result<Option<ExplorerIntegrationBackup>, String>;
    fn save_backup(&mut self, backup: &ExplorerIntegrationBackup) -> Result<(), String>;
    fn clear_backup(&mut self) -> Result<(), String>;
}

struct WindowsExplorerRegistry;

impl ExplorerRegistry for WindowsExplorerRegistry {
    fn shell_command(&self, verb: FolderVerb) -> Result<ShellCommand, String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey(verb.command_key()) {
            Ok(key) => {
                let command = read_optional_shell_string(&key, "", verb.label())?;
                let delegate_execute =
                    read_optional_shell_string(&key, VALUE_DELEGATE_EXECUTE, verb.label())?;
                Ok(ShellCommand {
                    key_existed: true,
                    command,
                    delegate_execute,
                })
            }
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(ShellCommand::default()),
            Err(error) => Err(format!("无法读取{}关联：{error}", verb.label())),
        }
    }

    fn set_shell_command(&mut self, verb: FolderVerb, state: &ShellCommand) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if !state.key_existed {
            let parent =
                match hkcu.open_subkey_with_flags(verb.parent_key(), winreg::enums::KEY_WRITE) {
                    Ok(key) => key,
                    Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
                    Err(error) => {
                        return Err(format!("无法打开{}菜单配置：{error}", verb.label()));
                    }
                };
            return match parent.delete_subkey_all("command") {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!("无法移除{}命令：{error}", verb.label())),
            };
        }

        let (key, _) = hkcu
            .create_subkey(verb.command_key())
            .map_err(|error| format!("无法写入{}命令：{error}", verb.label()))?;
        write_optional_shell_string(&key, "", state.command.as_ref(), verb.label())?;
        write_optional_shell_string(
            &key,
            VALUE_DELEGATE_EXECUTE,
            state.delegate_execute.as_ref(),
            verb.label(),
        )
    }

    fn backup(&self) -> Result<Option<ExplorerIntegrationBackup>, String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = match hkcu.open_subkey(BACKUP_KEY) {
            Ok(key) => key,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("无法读取 Kite 文件管理器设置：{error}")),
        };
        let version = key.get_value::<u32, _>(VALUE_VERSION).unwrap_or(0);
        match version {
            LEGACY_BACKUP_VERSION => read_legacy_backup(&key),
            BACKUP_VERSION => read_current_backup(&key),
            _ => Ok(None),
        }
    }

    fn save_backup(&mut self, backup: &ExplorerIntegrationBackup) -> Result<(), String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(BACKUP_KEY)
            .map_err(|error| format!("无法保存 Kite 文件管理器设置：{error}"))?;
        let explore = backup
            .explore
            .as_ref()
            .ok_or_else(|| "Kite 文件管理器备份缺少文件夹浏览设置".to_string())?;

        key.set_value(VALUE_KITE_COMMAND, &backup.kite_command)
            .and_then(|_| save_backup_verb(&key, "PreviousOpen", &backup.open))
            .and_then(|_| save_backup_verb(&key, "PreviousExplore", explore))
            .and_then(|_| key.set_value(VALUE_VERSION, &BACKUP_VERSION))
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

fn read_optional_shell_string(
    key: &RegKey,
    name: &str,
    label: &str,
) -> Result<Option<ShellStringValue>, String> {
    let raw = match key.get_raw_value(name) {
        Ok(value) => value,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("无法读取{label}命令值：{error}")),
    };
    let expand = if raw.vtype == REG_SZ {
        false
    } else if raw.vtype == REG_EXPAND_SZ {
        true
    } else {
        return Err(format!("{label}命令值类型不受支持：{:?}", raw.vtype));
    };
    let text =
        String::from_reg_value(&raw).map_err(|error| format!("无法解码{label}命令值：{error}"))?;
    Ok(Some(ShellStringValue { text, expand }))
}

fn write_optional_shell_string(
    key: &RegKey,
    name: &str,
    value: Option<&ShellStringValue>,
    label: &str,
) -> Result<(), String> {
    match value {
        Some(value) => write_shell_string(key, name, value, label),
        None => match key.delete_value(name) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("无法清除{label}命令值：{error}")),
        },
    }
}

fn write_shell_string(
    key: &RegKey,
    name: &str,
    value: &ShellStringValue,
    label: &str,
) -> Result<(), String> {
    let mut bytes = Vec::with_capacity((value.text.encode_utf16().count() + 1) * 2);
    for unit in value.text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes.extend_from_slice(&[0, 0]);
    let value_type = if value.expand { REG_EXPAND_SZ } else { REG_SZ };
    key.set_raw_value(
        name,
        &RegValue {
            bytes: bytes.into(),
            vtype: value_type,
        },
    )
    .map_err(|error| format!("无法保存{label}命令值：{error}"))
}

fn backup_flag(key: &RegKey, name: &str) -> bool {
    key.get_value::<u32, _>(name).unwrap_or(0) != 0
}

fn required_backup_flag(key: &RegKey, name: String) -> Result<bool, String> {
    key.get_value::<u32, _>(&name)
        .map(|value| value != 0)
        .map_err(|error| format!("Kite 文件管理器备份缺少 {name}：{error}"))
}

fn read_backup_value(key: &RegKey, prefix: &str) -> Result<Option<ShellStringValue>, String> {
    let existed_name = format!("{prefix}Existed");
    if !required_backup_flag(key, existed_name)? {
        return Ok(None);
    }
    let value_name = format!("{prefix}Value");
    let text = key
        .get_value::<String, _>(&value_name)
        .map_err(|error| format!("Kite 文件管理器备份缺少 {value_name}：{error}"))?;
    let expand = required_backup_flag(key, format!("{prefix}Expand"))?;
    Ok(Some(ShellStringValue { text, expand }))
}

fn read_backup_verb(key: &RegKey, prefix: &str) -> Result<ExplorerVerbBackup, String> {
    Ok(ExplorerVerbBackup {
        previous: ShellCommand {
            key_existed: required_backup_flag(key, format!("{prefix}KeyExisted"))?,
            command: read_backup_value(key, &format!("{prefix}Command"))?,
            delegate_execute: read_backup_value(key, &format!("{prefix}Delegate"))?,
        },
        restore_delegate_execute: true,
    })
}

fn save_backup_value(
    key: &RegKey,
    prefix: &str,
    value: Option<&ShellStringValue>,
) -> std::io::Result<()> {
    key.set_value(format!("{prefix}Existed"), &u32::from(value.is_some()))?;
    match value {
        Some(value) => {
            key.set_value(format!("{prefix}Value"), &value.text)?;
            key.set_value(format!("{prefix}Expand"), &u32::from(value.expand))?;
        }
        None => {
            for suffix in ["Value", "Expand"] {
                match key.delete_value(format!("{prefix}{suffix}")) {
                    Ok(()) => {}
                    Err(error) if error.kind() == ErrorKind::NotFound => {}
                    Err(error) => return Err(error),
                }
            }
        }
    }
    Ok(())
}

fn save_backup_verb(
    key: &RegKey,
    prefix: &str,
    backup: &ExplorerVerbBackup,
) -> std::io::Result<()> {
    key.set_value(
        format!("{prefix}KeyExisted"),
        &u32::from(backup.previous.key_existed),
    )?;
    save_backup_value(
        key,
        &format!("{prefix}Command"),
        backup.previous.command.as_ref(),
    )?;
    save_backup_value(
        key,
        &format!("{prefix}Delegate"),
        backup.previous.delegate_execute.as_ref(),
    )
}

fn read_current_backup(key: &RegKey) -> Result<Option<ExplorerIntegrationBackup>, String> {
    let kite_command = key
        .get_value::<String, _>(VALUE_KITE_COMMAND)
        .unwrap_or_default();
    if kite_command.is_empty() {
        return Ok(None);
    }
    Ok(Some(ExplorerIntegrationBackup {
        open: read_backup_verb(key, "PreviousOpen")?,
        explore: Some(read_backup_verb(key, "PreviousExplore")?),
        kite_command,
    }))
}

fn read_legacy_backup(key: &RegKey) -> Result<Option<ExplorerIntegrationBackup>, String> {
    let kite_command = key
        .get_value::<String, _>(VALUE_KITE_COMMAND)
        .unwrap_or_default();
    if kite_command.is_empty() {
        return Ok(None);
    }
    let previous_key_existed = backup_flag(key, LEGACY_VALUE_PREVIOUS_KEY_EXISTED);
    let previous_value_existed = backup_flag(key, LEGACY_VALUE_PREVIOUS_VALUE_EXISTED);
    let previous_command = key
        .get_value::<String, _>(LEGACY_VALUE_PREVIOUS_COMMAND)
        .unwrap_or_default();
    Ok(Some(ExplorerIntegrationBackup {
        open: ExplorerVerbBackup {
            previous: ShellCommand {
                key_existed: previous_key_existed,
                command: previous_value_existed.then_some(ShellStringValue {
                    text: previous_command,
                    expand: false,
                }),
                delegate_execute: None,
            },
            restore_delegate_execute: false,
        },
        explore: None,
        kite_command,
    }))
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

fn kite_shell_command(kite_command: &str) -> ShellCommand {
    ShellCommand {
        key_existed: true,
        command: Some(ShellStringValue {
            text: kite_command.to_string(),
            expand: false,
        }),
        delegate_execute: Some(ShellStringValue {
            text: String::new(),
            expand: false,
        }),
    }
}

fn command_is_kite(state: &ShellCommand, kite_command: &str) -> bool {
    state
        .command
        .as_ref()
        .is_some_and(|command| command.text == kite_command)
}

fn current_is_kite(
    state: &ShellCommand,
    expected: &ShellCommand,
    check_delegate_execute: bool,
) -> bool {
    if check_delegate_execute {
        state == expected
    } else {
        command_is_kite(
            state,
            expected
                .command
                .as_ref()
                .map(|command| command.text.as_str())
                .unwrap_or_default(),
        )
    }
}

fn matches_existing_kite(
    current: &ShellCommand,
    verb: FolderVerb,
    backup: &ExplorerIntegrationBackup,
) -> bool {
    let expected = kite_shell_command(&backup.kite_command);
    match verb {
        FolderVerb::Open => {
            current_is_kite(current, &expected, backup.open.restore_delegate_execute)
        }
        FolderVerb::Explore => backup
            .explore
            .as_ref()
            .is_some_and(|_| current_is_kite(current, &expected, true)),
    }
}

fn make_verb_backup(
    current: &ShellCommand,
    verb: FolderVerb,
    current_kite: &ShellCommand,
    existing: Option<&ExplorerIntegrationBackup>,
) -> ExplorerVerbBackup {
    if let Some(existing) = existing
        && (current == current_kite || matches_existing_kite(current, verb, existing))
    {
        let existing_verb = match verb {
            FolderVerb::Open => Some(&existing.open),
            FolderVerb::Explore => existing.explore.as_ref(),
        };
        if let Some(existing_verb) = existing_verb {
            let mut previous = existing_verb.previous.clone();
            if !existing_verb.restore_delegate_execute {
                previous.delegate_execute = current.delegate_execute.clone();
            }
            return ExplorerVerbBackup {
                previous,
                restore_delegate_execute: true,
            };
        }
    }
    ExplorerVerbBackup {
        previous: current.clone(),
        restore_delegate_execute: true,
    }
}

fn restore_shell_commands(
    registry: &mut impl ExplorerRegistry,
    open: &ShellCommand,
    explore: &ShellCommand,
) -> Result<(), String> {
    let mut failures = Vec::new();
    if let Err(error) = registry.set_shell_command(FolderVerb::Open, open) {
        failures.push(error);
    }
    if let Err(error) = registry.set_shell_command(FolderVerb::Explore, explore) {
        failures.push(error);
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("；"))
    }
}

fn enable_default_file_manager(
    registry: &mut impl ExplorerRegistry,
    kite_command: &str,
) -> Result<String, String> {
    if kite_command.trim().is_empty() {
        return Err("Kite 打开命令为空".to_string());
    }
    let current_open = registry.shell_command(FolderVerb::Open)?;
    let current_explore = registry.shell_command(FolderVerb::Explore)?;
    let existing_backup = registry.backup()?;
    let kite_state = kite_shell_command(kite_command);
    let current_is_new_kite = current_open == kite_state && current_explore == kite_state;

    if current_is_new_kite
        && existing_backup
            .as_ref()
            .is_some_and(|backup| backup.explore.is_some() && backup.kite_command == kite_command)
    {
        return Ok("Kite 已是默认文件夹打开方式".to_string());
    }
    if current_is_new_kite && existing_backup.is_none() {
        return Ok("Kite 已是默认文件夹打开方式".to_string());
    }

    let backup = ExplorerIntegrationBackup {
        open: make_verb_backup(
            &current_open,
            FolderVerb::Open,
            &kite_state,
            existing_backup.as_ref(),
        ),
        explore: Some(make_verb_backup(
            &current_explore,
            FolderVerb::Explore,
            &kite_state,
            existing_backup.as_ref(),
        )),
        kite_command: kite_command.to_string(),
    };

    if current_open != kite_state
        && let Err(error) = registry.set_shell_command(FolderVerb::Open, &kite_state)
    {
        let rollback = restore_shell_commands(registry, &current_open, &current_explore);
        return Err(with_rollback_error(
            format!("无法启用 Kite 默认文件管理器：{error}"),
            rollback,
        ));
    }
    if current_explore != kite_state
        && let Err(error) = registry.set_shell_command(FolderVerb::Explore, &kite_state)
    {
        let rollback = restore_shell_commands(registry, &current_open, &current_explore);
        return Err(with_rollback_error(
            format!("无法启用 Kite 默认文件管理器：{error}"),
            rollback,
        ));
    }
    if let Err(error) = registry.save_backup(&backup) {
        let rollback = restore_shell_commands(registry, &current_open, &current_explore);
        return Err(with_rollback_error(
            format!("无法保存默认文件管理器设置：{error}"),
            rollback,
        ));
    }

    Ok("Kite 已设为默认文件夹打开方式".to_string())
}

fn with_rollback_error(message: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => message,
        Err(error) => format!("{message}；恢复原设置失败：{error}"),
    }
}

fn restore_default_file_manager(registry: &mut impl ExplorerRegistry) -> Result<String, String> {
    let Some(backup) = registry.backup()? else {
        return Err("Kite 没有可恢复的默认文件管理器设置".to_string());
    };
    let current_open = registry.shell_command(FolderVerb::Open)?;
    let current_explore = registry.shell_command(FolderVerb::Explore)?;
    let expected = kite_shell_command(&backup.kite_command);
    if !current_is_kite(
        &current_open,
        &expected,
        backup.open.restore_delegate_execute,
    ) || backup
        .explore
        .as_ref()
        .is_some_and(|_| current_explore != expected)
    {
        return Err("文件夹默认打开方式已由其他应用更改。Kite 未覆盖当前设置。".to_string());
    }

    let mut previous_open = backup.open.previous.clone();
    if !backup.open.restore_delegate_execute {
        previous_open.delegate_execute = current_open.delegate_execute.clone();
    }
    registry
        .set_shell_command(FolderVerb::Open, &previous_open)
        .map_err(|error| format!("无法恢复文件夹打开方式：{error}"))?;

    if let Some(explore) = backup.explore.as_ref()
        && let Err(error) = registry.set_shell_command(FolderVerb::Explore, &explore.previous)
    {
        let rollback = restore_shell_commands(registry, &current_open, &current_explore);
        return Err(with_rollback_error(
            format!("无法恢复文件夹浏览方式：{error}"),
            rollback,
        ));
    }

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
        ExplorerIntegrationBackup, ExplorerRegistry, ExplorerVerbBackup, FolderVerb, ShellCommand,
        ShellStringValue, create_shell_link, enable_default_file_manager, kite_folder_open_command,
        restore_default_file_manager,
    };
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[derive(Default)]
    struct MemoryRegistry {
        open: ShellCommand,
        explore: ShellCommand,
        backup: Option<ExplorerIntegrationBackup>,
    }

    impl ExplorerRegistry for MemoryRegistry {
        fn shell_command(&self, verb: FolderVerb) -> Result<ShellCommand, String> {
            Ok(match verb {
                FolderVerb::Open => self.open.clone(),
                FolderVerb::Explore => self.explore.clone(),
            })
        }

        fn set_shell_command(
            &mut self,
            verb: FolderVerb,
            state: &ShellCommand,
        ) -> Result<(), String> {
            match verb {
                FolderVerb::Open => self.open = state.clone(),
                FolderVerb::Explore => self.explore = state.clone(),
            }
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
    const KITE_NEW: &str = r#""C:\Apps\Kite 2\Kite.exe" "%1""#;
    const FILES: &str = r#""C:\Apps\Files.exe" "%1""#;
    const FILES_EXPLORE: &str = r#""%LOCALAPPDATA%\Files\Files.exe" "%1""#;

    fn shell_value(text: &str, expand: bool) -> ShellStringValue {
        ShellStringValue {
            text: text.to_string(),
            expand,
        }
    }

    fn files_state(command: &str, expand: bool) -> ShellCommand {
        ShellCommand {
            key_existed: true,
            command: Some(shell_value(command, expand)),
            delegate_execute: Some(shell_value("", false)),
        }
    }

    fn registry_with_files() -> MemoryRegistry {
        MemoryRegistry {
            open: files_state(FILES, false),
            explore: files_state(FILES_EXPLORE, true),
            backup: None,
        }
    }

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
    fn enabling_writes_open_and_explore_with_suppressed_delegate_execute() {
        let mut registry = registry_with_files();

        enable_default_file_manager(&mut registry, KITE).expect("enable Kite");

        for state in [&registry.open, &registry.explore] {
            assert_eq!(
                state.command.as_ref().map(|value| value.text.as_str()),
                Some(KITE)
            );
            assert_eq!(
                state
                    .delegate_execute
                    .as_ref()
                    .map(|value| value.text.as_str()),
                Some("")
            );
            assert!(!state.command.as_ref().expect("command").expand);
        }
    }

    #[test]
    fn enabling_saves_and_restores_both_previous_handlers() {
        let mut registry = registry_with_files();

        enable_default_file_manager(&mut registry, KITE).expect("enable Kite");
        restore_default_file_manager(&mut registry).expect("restore previous handlers");

        assert_eq!(registry.open, files_state(FILES, false));
        assert_eq!(registry.explore, files_state(FILES_EXPLORE, true));
        assert!(registry.backup.is_none());
    }

    #[test]
    fn enabling_twice_keeps_the_original_handlers() {
        let mut registry = registry_with_files();
        enable_default_file_manager(&mut registry, KITE).expect("first enable");
        enable_default_file_manager(&mut registry, KITE).expect("second enable");
        restore_default_file_manager(&mut registry).expect("restore previous handler");

        assert_eq!(
            registry
                .open
                .command
                .as_ref()
                .map(|value| value.text.as_str()),
            Some(FILES)
        );
        assert_eq!(
            registry
                .explore
                .command
                .as_ref()
                .map(|value| value.text.as_str()),
            Some(FILES_EXPLORE)
        );
    }

    #[test]
    fn missing_user_overrides_are_restored_as_missing() {
        let mut registry = MemoryRegistry::default();
        enable_default_file_manager(&mut registry, KITE).expect("enable Kite");
        restore_default_file_manager(&mut registry).expect("restore missing override");

        assert!(!registry.open.key_existed);
        assert!(registry.open.command.is_none());
        assert!(!registry.explore.key_existed);
        assert!(registry.explore.command.is_none());
    }

    #[test]
    fn restore_refuses_to_overwrite_an_external_explore_change() {
        let mut registry = registry_with_files();
        enable_default_file_manager(&mut registry, KITE).expect("enable Kite");
        registry.explore.command = Some(shell_value(r#""C:\Apps\Other.exe" "%1""#, false));

        let error = restore_default_file_manager(&mut registry).expect_err("restore must fail");

        assert!(error.contains("其他应用"));
        assert_eq!(
            registry
                .explore
                .command
                .as_ref()
                .map(|value| value.text.as_str()),
            Some(r#""C:\Apps\Other.exe" "%1""#)
        );
    }

    #[test]
    fn delegate_execute_change_blocks_restore() {
        let mut registry = registry_with_files();
        enable_default_file_manager(&mut registry, KITE).expect("enable Kite");
        registry.open.delegate_execute = None;

        let error = restore_default_file_manager(&mut registry).expect_err("restore must fail");

        assert!(error.contains("其他应用"));
        assert!(registry.open.delegate_execute.is_none());
    }

    #[test]
    fn reenabling_after_executable_change_keeps_original_handlers() {
        let mut registry = registry_with_files();
        enable_default_file_manager(&mut registry, KITE).expect("enable old path");
        enable_default_file_manager(&mut registry, KITE_NEW).expect("enable new path");
        restore_default_file_manager(&mut registry).expect("restore previous handler");

        assert_eq!(
            registry
                .open
                .command
                .as_ref()
                .map(|value| value.text.as_str()),
            Some(FILES)
        );
        assert_eq!(
            registry
                .explore
                .command
                .as_ref()
                .map(|value| value.text.as_str()),
            Some(FILES_EXPLORE)
        );
    }
    #[test]
    fn legacy_backup_restores_open_without_touching_explore() {
        let original_open = files_state(FILES, false);
        let explore = files_state(FILES_EXPLORE, true);
        let mut registry = MemoryRegistry {
            open: ShellCommand {
                key_existed: true,
                command: Some(shell_value(KITE, false)),
                delegate_execute: Some(shell_value("", false)),
            },
            explore: explore.clone(),
            backup: Some(ExplorerIntegrationBackup {
                open: ExplorerVerbBackup {
                    previous: original_open.clone(),
                    restore_delegate_execute: false,
                },
                explore: None,
                kite_command: KITE.to_string(),
            }),
        };

        restore_default_file_manager(&mut registry).expect("restore legacy backup");

        assert_eq!(registry.open, original_open);
        assert_eq!(registry.explore, explore);
        assert!(registry.backup.is_none());
    }
}
