use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::{Deserialize, Serialize};

use crate::fs::SortSpec;

const UI_PREFERENCES_SCHEMA_VERSION: u32 = 1;
const MAX_UI_PREFERENCES_BYTES: u64 = 64 * 1024;
const MAX_COLUMN_RULES: usize = 512;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppearancePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiPreferences {
    pub restore_session: bool,
    pub appearance: AppearancePreference,
    pub sidebar_visible: bool,
    pub preview_visible: bool,
    pub column_details_visible: bool,
    pub date_column_visible: bool,
    pub kind_column_visible: bool,
    pub size_column_visible: bool,
    pub column_width: u16,
    pub default_column_rule: ColumnViewRule,
    pub column_rules: BTreeMap<String, ColumnViewRule>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            restore_session: true,
            appearance: AppearancePreference::System,
            sidebar_visible: true,
            preview_visible: true,
            column_details_visible: false,
            date_column_visible: true,
            kind_column_visible: true,
            size_column_visible: true,
            column_width: 320,
            default_column_rule: ColumnViewRule::default(),
            column_rules: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnGroupPreference {
    #[default]
    None,
    Kind,
    Extension,
    Modified,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnViewRule {
    #[serde(default)]
    pub sort: SortSpec,
    #[serde(default)]
    pub group: ColumnGroupPreference,
    #[serde(default)]
    pub collapsed_groups: BTreeSet<String>,
}

impl UiPreferences {
    pub fn column_view_rule(&self, path: &Path) -> ColumnViewRule {
        self.column_rules
            .get(&column_rule_key(path))
            .cloned()
            .unwrap_or_else(|| {
                let mut rule = self.default_column_rule.clone();
                rule.collapsed_groups.clear();
                rule
            })
    }
}

#[derive(Serialize, Deserialize)]
struct UiPreferencesFile {
    schema_version: u32,
    #[serde(default = "default_true")]
    restore_session: bool,
    #[serde(default)]
    appearance: AppearancePreference,
    #[serde(default = "default_true")]
    sidebar_visible: bool,
    #[serde(default = "default_true")]
    preview_visible: bool,
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
    #[serde(default)]
    default_column_rule: ColumnViewRule,
    #[serde(default)]
    column_rules: BTreeMap<String, ColumnViewRule>,
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

pub fn save_column_view_rule(path: &Path, rule: ColumnViewRule) -> Result<(), String> {
    let preferences_path =
        ui_preferences_path().ok_or_else(|| "无法确定界面配置目录".to_string())?;
    save_column_view_rule_at(&preferences_path, path, rule)
}

pub fn save_default_column_sort(sort: SortSpec) -> Result<(), String> {
    let path = ui_preferences_path().ok_or_else(|| "无法确定界面配置目录".to_string())?;
    save_default_column_sort_at(&path, sort)
}

fn save_column_view_rule_at(
    preferences_path: &Path,
    rule_path: &Path,
    rule: ColumnViewRule,
) -> Result<(), String> {
    let mut preferences = load_ui_preferences_at(preferences_path).unwrap_or_default();
    let mut default_rule = rule.clone();
    default_rule.collapsed_groups.clear();
    preferences.default_column_rule = default_rule;
    preferences
        .column_rules
        .insert(column_rule_key(rule_path), rule);
    prune_column_rules(&mut preferences.column_rules);
    save_ui_preferences_at(preferences_path, preferences)
}

fn save_default_column_sort_at(path: &Path, sort: SortSpec) -> Result<(), String> {
    let mut preferences = load_ui_preferences_at(path).unwrap_or_default();
    preferences.default_column_rule.sort = sort;
    save_ui_preferences_at(path, preferences)
}

fn ui_preferences_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("KITE_UI_PREFERENCES_PATH") {
        return Some(PathBuf::from(path));
    }
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .map(|path| path.join("Kite").join("ui.json"))
}

fn column_rule_key(path: &Path) -> String {
    let key = path.to_string_lossy().into_owned();
    #[cfg(windows)]
    {
        key.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        key
    }
}

fn load_ui_preferences_at(path: &Path) -> Option<UiPreferences> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_UI_PREFERENCES_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    let file = serde_json::from_slice::<UiPreferencesFile>(&bytes).ok()?;
    if file.schema_version != UI_PREFERENCES_SCHEMA_VERSION {
        return None;
    }
    let mut column_rules = file.column_rules;
    prune_column_rules(&mut column_rules);
    Some(UiPreferences {
        restore_session: file.restore_session,
        appearance: file.appearance,
        sidebar_visible: file.sidebar_visible,
        preview_visible: file.preview_visible,
        column_details_visible: file.column_details_visible,
        date_column_visible: file.date_column_visible,
        kind_column_visible: file.kind_column_visible,
        size_column_visible: file.size_column_visible,
        column_width: file.column_width.clamp(240, 480),
        default_column_rule: file.default_column_rule,
        column_rules,
    })
}

fn save_ui_preferences_at(path: &Path, preferences: UiPreferences) -> Result<(), String> {
    let UiPreferences {
        restore_session,
        appearance,
        sidebar_visible,
        preview_visible,
        column_details_visible,
        date_column_visible,
        kind_column_visible,
        size_column_visible,
        column_width,
        default_column_rule,
        mut column_rules,
    } = preferences;
    prune_column_rules(&mut column_rules);
    let file = UiPreferencesFile {
        schema_version: UI_PREFERENCES_SCHEMA_VERSION,
        restore_session,
        appearance,
        sidebar_visible,
        preview_visible,
        column_details_visible,
        date_column_visible,
        kind_column_visible,
        size_column_visible,
        column_width: column_width.clamp(240, 480),
        default_column_rule,
        column_rules,
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

fn prune_column_rules(rules: &mut BTreeMap<String, ColumnViewRule>) {
    if rules.len() <= MAX_COLUMN_RULES {
        return;
    }
    let overflow = rules.len() - MAX_COLUMN_RULES;
    let keys = rules.keys().take(overflow).cloned().collect::<Vec<_>>();
    for key in keys {
        rules.remove(&key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::SortKey;
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
    fn saves_and_loads_window_preferences() {
        let path = test_path();
        save_ui_preferences_at(
            &path,
            UiPreferences {
                restore_session: false,
                appearance: AppearancePreference::Dark,
                sidebar_visible: false,
                preview_visible: false,
                column_details_visible: true,
                ..UiPreferences::default()
            },
        )
        .expect("save preferences");

        let preferences = load_ui_preferences_at(&path).expect("load preferences");
        assert!(!preferences.restore_session);
        assert_eq!(preferences.appearance, AppearancePreference::Dark);
        assert!(!preferences.sidebar_visible);
        assert!(!preferences.preview_visible);
        assert!(preferences.column_details_visible);
        fs::remove_file(path).expect("remove preferences");
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

    #[test]
    fn saves_and_loads_per_path_column_view_rules() {
        let path = test_path();
        let rule = ColumnViewRule {
            sort: SortSpec {
                key: SortKey::Size,
                ascending: false,
            },
            group: ColumnGroupPreference::Extension,
            collapsed_groups: BTreeSet::from(["PNG".to_string()]),
        };
        let rule_path = PathBuf::from("/repo/assets");

        save_column_view_rule_at(&path, &rule_path, rule.clone()).expect("save column rule");

        let preferences = load_ui_preferences_at(&path).expect("load preferences");
        assert_eq!(preferences.column_view_rule(&rule_path), rule);
        assert_eq!(
            preferences.column_view_rule(Path::new("/repo/other")),
            ColumnViewRule {
                sort: rule.sort,
                group: rule.group,
                collapsed_groups: BTreeSet::new(),
            }
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn saves_and_loads_the_default_sort() {
        let path = test_path();
        let sort = SortSpec {
            key: SortKey::Modified,
            ascending: false,
        };

        save_default_column_sort_at(&path, sort).expect("save default sort");

        assert_eq!(
            load_ui_preferences_at(&path)
                .expect("load preferences")
                .default_column_rule
                .sort,
            sort
        );

        let _ = fs::remove_file(path);
    }
}
