use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::platform::paths_equal;

pub const FAVORITES_SCHEMA_VERSION: u32 = 1;
const MAX_FAVORITES: usize = 128;
const MAX_FAVORITES_BYTES: u64 = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, uniffi::Record)]
pub struct FavoriteEntryDto {
    pub path: String,
    pub name: String,
    pub is_directory: bool,
}

#[derive(Serialize, Deserialize)]
struct FavoritesFile {
    schema_version: u32,
    entries: Vec<FavoriteEntryDto>,
}

pub fn load_favorites() -> Vec<FavoriteEntryDto> {
    favorites_path()
        .and_then(|path| load_favorites_at(&path))
        .unwrap_or_default()
}

pub fn add_favorite(path: &Path, name: &str, is_directory: bool) -> Result<bool, String> {
    let config_path = favorites_path().ok_or_else(|| "无法确定收藏配置目录".to_string())?;
    add_favorite_at(&config_path, path, name, is_directory)
}

pub fn remove_favorite(target: &Path) -> Result<bool, String> {
    let config_path = favorites_path().ok_or_else(|| "无法确定收藏配置目录".to_string())?;
    remove_favorite_at(&config_path, target)
}

fn favorites_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("KITE_FAVORITES_PATH") {
        return Some(PathBuf::from(path));
    }
    dirs::config_dir()
        .or_else(dirs::data_local_dir)
        .map(|path| path.join("Kite").join("favorites.json"))
}

fn normalize_entry(path: &Path, name: &str, is_directory: bool) -> Option<FavoriteEntryDto> {
    let path = path.to_string_lossy().trim().to_string();
    if path.is_empty() {
        return None;
    }
    let fallback = Path::new(&path)
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.clone());
    let name = {
        let value = name.trim();
        if value.is_empty() {
            fallback
        } else {
            value.to_string()
        }
    };
    Some(FavoriteEntryDto {
        path,
        name,
        is_directory,
    })
}

fn normalize_entries(entries: Vec<FavoriteEntryDto>) -> Vec<FavoriteEntryDto> {
    let mut normalized = Vec::new();
    for entry in entries {
        let Some(entry) = normalize_entry(Path::new(&entry.path), &entry.name, entry.is_directory)
        else {
            continue;
        };
        if normalized.iter().any(|existing: &FavoriteEntryDto| {
            paths_equal(Path::new(&existing.path), Path::new(&entry.path))
        }) {
            continue;
        }
        normalized.push(entry);
        if normalized.len() == MAX_FAVORITES {
            break;
        }
    }
    normalized
}

fn load_favorites_at(path: &Path) -> Option<Vec<FavoriteEntryDto>> {
    for candidate in [path.to_path_buf(), backup_path(path)] {
        let Ok(metadata) = fs::metadata(&candidate) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() > MAX_FAVORITES_BYTES {
            continue;
        }
        let Ok(bytes) = fs::read(&candidate) else {
            continue;
        };
        let Ok(file) = serde_json::from_slice::<FavoritesFile>(&bytes) else {
            continue;
        };
        if file.schema_version != FAVORITES_SCHEMA_VERSION {
            continue;
        }
        return Some(normalize_entries(file.entries));
    }
    None
}

fn save_favorites_at(
    path: &Path,
    entries: Vec<FavoriteEntryDto>,
) -> Result<Vec<FavoriteEntryDto>, String> {
    let entries = normalize_entries(entries);
    let file = FavoritesFile {
        schema_version: FAVORITES_SCHEMA_VERSION,
        entries: entries.clone(),
    };
    let bytes =
        serde_json::to_vec_pretty(&file).map_err(|error| format!("无法序列化收藏配置：{error}"))?;
    if bytes.len() as u64 > MAX_FAVORITES_BYTES {
        return Err("收藏配置过大".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建收藏配置目录：{error}"))?;
    }

    let temporary = temporary_path(path);
    {
        let mut output = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| format!("无法创建收藏临时文件：{error}"))?;
        output
            .write_all(&bytes)
            .map_err(|error| format!("无法写入收藏临时文件：{error}"))?;
        output
            .sync_all()
            .map_err(|error| format!("无法同步收藏临时文件：{error}"))?;
    }

    let backup = backup_path(path);
    if path.exists() {
        if backup.exists() {
            fs::remove_file(&backup).map_err(|error| format!("无法替换收藏备份：{error}"))?;
        }
        fs::rename(path, &backup).map_err(|error| format!("无法轮换收藏备份：{error}"))?;
    }
    if let Err(error) = fs::rename(&temporary, path) {
        if backup.exists() && !path.exists() {
            let _ = fs::rename(&backup, path);
        }
        let _ = fs::remove_file(&temporary);
        return Err(format!("无法替换收藏文件：{error}"));
    }
    Ok(entries)
}

fn add_favorite_at(
    config_path: &Path,
    target: &Path,
    name: &str,
    is_directory: bool,
) -> Result<bool, String> {
    let Some(entry) = normalize_entry(target, name, is_directory) else {
        return Err("收藏路径不能为空".to_string());
    };
    let mut entries = load_favorites_at(config_path).unwrap_or_default();
    if entries
        .iter()
        .any(|existing| paths_equal(Path::new(&existing.path), Path::new(&entry.path)))
    {
        return Ok(false);
    }
    entries.push(entry);
    save_favorites_at(config_path, entries)?;
    Ok(true)
}

fn remove_favorite_at(config_path: &Path, target: &Path) -> Result<bool, String> {
    let mut entries = load_favorites_at(config_path).unwrap_or_default();
    let original_len = entries.len();
    entries.retain(|entry| !paths_equal(Path::new(&entry.path), target));
    if entries.len() == original_len {
        return Ok(false);
    }
    save_favorites_at(config_path, entries)?;
    Ok(true)
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("json.tmp")
}

fn backup_path(path: &Path) -> PathBuf {
    path.with_extension("json.bak")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "kite-favorites-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn loads_the_backup_when_the_primary_file_is_corrupt() {
        let path = test_path();
        let entries = vec![FavoriteEntryDto {
            path: "C:/backup".to_string(),
            name: "backup".to_string(),
            is_directory: true,
        }];
        save_favorites_at(&path, entries).expect("save favorites");
        fs::copy(&path, backup_path(&path)).expect("copy backup");
        fs::write(&path, b"not json").expect("corrupt primary");

        let favorites = load_favorites_at(&path).expect("load backup");
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].path, "C:/backup");

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(backup_path(&path));
    }

    #[test]
    fn adds_deduplicates_and_removes_favorites() {
        let path = test_path();
        assert!(add_favorite_at(&path, Path::new("C:/work"), "work", true).unwrap());
        assert!(!add_favorite_at(&path, Path::new("C:/work"), "work-again", true).unwrap());
        assert!(
            add_favorite_at(&path, Path::new("C:/work/readme.md"), "readme.md", false).unwrap()
        );

        let favorites = load_favorites_at(&path).expect("load favorites");
        assert_eq!(favorites.len(), 2);
        assert_eq!(favorites[0].name, "work");
        assert!(!favorites[1].is_directory);

        assert!(remove_favorite_at(&path, Path::new("C:/work")).unwrap());
        let favorites = load_favorites_at(&path).expect("load favorites after remove");
        assert_eq!(favorites.len(), 1);
        assert_eq!(favorites[0].name, "readme.md");

        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(backup_path(&path));
    }
}
