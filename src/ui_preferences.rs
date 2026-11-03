use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::{Deserialize, Serialize};

const UI_PREFERENCES_SCHEMA_VERSION: u32 = 1;
const MAX_UI_PREFERENCES_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiPreferences {
    pub column_details_visible: bool,
    pub date_column_visible: bool,
    pub kind_column_visible: bool,
    pub size_column_visible: bool,
    pub column_width: u16,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            column_details_visible: false,
            date_column_visible: true,
            kind_column_visible: true,
            size_column_visible: true,
            column_width: 320,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct UiPreferencesFile {
    schema_version: u32,
    #[serde(default)]
    column_details_visible: bool,
    #[serde(default = "default_true")]
    date_column_visible: bool,
    #[serde(default = "default_true")]
    kind_column_visible: bool,
    #[serde(default = "default_true")]
    size_column_visible: bool,
    #[serde(default = "default_column_width")]
    column_width: u16,
}

pub fn load_ui_preferences() -> UiPreferences {
    ui_preferences_path()
        .and_then(|path| load_ui_preferences_at(&path))
        .unwrap_or_default()
}

pub fn save_ui_preferences(preferences: UiPreferences) -> Result<(), String> {
    let path = ui_preferences_path().ok_or_else(|| "无法确定界面配置目录".to_string())?;
    save_ui_preferences_at(&path, preferences)
}

fn ui_preferences_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("KITE_UI_PREFERENCES_PATH") {
        return Some(PathBuf::from(path));
    }
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .map(|path| path.join("Kite").join("ui.json"))
}

fn load_ui_preferences_at(path: &Path) -> Option<UiPreferences> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_UI_PREFERENCES_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let file = serde_json::from_slice::<UiPreferencesFile>(&bytes).ok()?;
    (file.schema_version == UI_PREFERENCES_SCHEMA_VERSION).then_some(UiPreferences {
        column_details_visible: file.column_details_visible,
        date_column_visible: file.date_column_visible,
        kind_column_visible: file.kind_column_visible,
        size_column_visible: file.size_column_visible,
        column_width: file.column_width.clamp(240, 480),
    })
}

fn save_ui_preferences_at(path: &Path, preferences: UiPreferences) -> Result<(), String> {
    let file = UiPreferencesFile {
        schema_version: UI_PREFERENCES_SCHEMA_VERSION,
        column_details_visible: preferences.column_details_visible,
        date_column_visible: preferences.date_column_visible,
        kind_column_visible: preferences.kind_column_visible,
        size_column_visible: preferences.size_column_visible,
        column_width: preferences.column_width.clamp(240, 480),
    };
    let bytes =
        serde_json::to_vec_pretty(&file).map_err(|error| format!("无法序列化界面配置：{error}"))?;
    if bytes.len() as u64 > MAX_UI_PREFERENCES_BYTES {
        return Err("界面配置过大".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建界面配置目录：{error}"))?;
    }
    AtomicFile::new(path, AllowOverwrite)
        .write(|file| file.write_all(&bytes))
        .map_err(|error| format!("无法保存界面配置：{error}"))
}

fn default_true() -> bool {
    true
}

fn default_column_width() -> u16 {
    320
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(0);

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "kite-ui-preferences-{}-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn saves_and_loads_column_details_visibility() {
        let path = test_path();
        save_ui_preferences_at(
            &path,
            UiPreferences {
                column_details_visible: true,
                ..UiPreferences::default()
            },
        )
        .expect("save preferences");

        assert_eq!(
            load_ui_preferences_at(&path),
            Some(UiPreferences {
                column_details_visible: true,
                ..UiPreferences::default()
            })
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_corrupt_preferences() {
        let path = test_path();
        fs::write(&path, b"not json").expect("write corrupt preferences");

        assert_eq!(load_ui_preferences_at(&path), None);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn loads_legacy_preferences_with_default_columns() {
        let path = test_path();
        fs::write(
            &path,
            br#"{"schema_version":1,"column_details_visible":true}"#,
        )
        .expect("write legacy preferences");

        assert_eq!(
            load_ui_preferences_at(&path),
            Some(UiPreferences {
                column_details_visible: true,
                ..UiPreferences::default()
            })
        );

        let _ = fs::remove_file(path);
    }
}
