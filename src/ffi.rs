use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::UNIX_EPOCH;

use crate::devices::{self, PortableDevice, StorageVolume, VolumeKind};
use crate::favorites::{self as favorites_store, FavoriteEntryDto};
use crate::fs::{EntryKind, FileEntry, SortSpec, VcsStatus, load_directory, transfer_entry};
use crate::platform::{HostPlatform, default_root_path, host_platform};
use crate::session::{self as session_store, SessionStateDto};
use crate::text_editor::{self, TextDocument, TextLineEnding};
use crate::updater::{self, UpdateInfo};
use crate::vcs::{apply_statuses, load_vcs_snapshot};

#[derive(Clone, Debug, uniffi::Record)]
pub struct DirectoryEntryDto {
    pub path: String,
    pub name: String,
    pub is_directory: bool,
    pub size: u64,
    pub modified_unix_ms: Option<i64>,
    pub kind_label: String,
    pub git_status: Option<String>,
    pub svn_status: Option<String>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DirectoryListingDto {
    pub path: String,
    pub entries: Vec<DirectoryEntryDto>,
    pub skipped: u64,
    pub git_available: bool,
    pub svn_available: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DirectorySearchDto {
    pub path: String,
    pub query: String,
    pub entries: Vec<DirectoryEntryDto>,
    pub truncated: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct VolumeDto {
    pub path: String,
    pub name: Option<String>,
    pub device_name: Option<String>,
    pub kind: String,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct PortableDeviceDto {
    pub name: String,
    pub shell_path: String,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct DeviceSnapshotDto {
    pub volumes: Vec<VolumeDto>,
    pub portable_devices: Vec<PortableDeviceDto>,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UpdateInfoDto {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_name: String,
    pub release_url: String,
    pub release_notes: String,
    pub published_at: Option<String>,
    pub asset_name: String,
    pub asset_url: String,
    pub asset_sha256: String,
    pub asset_size: u64,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct UpdateDownloadProgressDto {
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub active: bool,
}

#[derive(Clone, Debug, uniffi::Record)]
pub struct TextDocumentDto {
    pub path: String,
    pub text: String,
    pub language: String,
    pub line_ending: String,
    pub has_bom: bool,
    pub content_sha256: String,
    pub byte_len: u64,
}

#[derive(Clone, Debug, thiserror::Error, uniffi::Error)]
pub enum KiteError {
    #[error("{message}")]
    OperationFailed { message: String },
}

impl KiteError {
    fn operation(message: impl Into<String>) -> Self {
        Self::OperationFailed {
            message: message.into(),
        }
    }
}

#[derive(Debug, uniffi::Object)]
pub struct KiteCore {
    search_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

#[uniffi::export]
impl KiteCore {
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            search_cancel: Mutex::new(None),
        })
    }

    pub fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    pub fn check_for_update(&self) -> Result<UpdateInfoDto, KiteError> {
        updater::check_for_update()
            .map(update_info_dto)
            .map_err(KiteError::operation)
    }

    pub fn download_progress(&self) -> UpdateDownloadProgressDto {
        update_download_progress_dto(updater::download_progress())
    }

    pub fn download_update(&self, update: UpdateInfoDto) -> Result<String, KiteError> {
        if !update.update_available {
            return Err(KiteError::operation("当前版本已是最新版本"));
        }
        updater::download_update(&update_info_from_dto(update))
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(KiteError::operation)
    }

    pub fn platform(&self) -> String {
        match host_platform() {
            HostPlatform::Windows => "windows",
            HostPlatform::MacOS => "macos",
            HostPlatform::Other => "other",
        }
        .to_string()
    }

    pub fn default_directory(&self) -> String {
        dirs::home_dir()
            .unwrap_or_else(default_root_path)
            .to_string_lossy()
            .into_owned()
    }

    pub fn list_directory(&self, path: String) -> Result<DirectoryListingDto, KiteError> {
        let snapshot = load_directory(&PathBuf::from(&path), SortSpec::default())
            .map_err(KiteError::operation)?;
        let vcs = load_vcs_snapshot(&snapshot.path);
        let mut entries = snapshot.entries;
        apply_statuses(&mut entries, &snapshot.path, &vcs);
        Ok(DirectoryListingDto {
            path: snapshot.path.to_string_lossy().into_owned(),
            entries: entries.iter().map(entry_dto).collect(),
            skipped: snapshot.skipped as u64,
            git_available: vcs.git_available(),
            svn_available: vcs.svn_available(),
        })
    }

    pub fn search_directory(
        &self,
        path: String,
        query: String,
    ) -> Result<Option<DirectorySearchDto>, KiteError> {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = self
                .search_cancel
                .lock()
                .map_err(|_| KiteError::operation("搜索状态锁不可用"))?;
            *active = Some(cancel.clone());
        }

        let result = crate::fs::search_directory(
            &PathBuf::from(&path),
            &query,
            SortSpec::default(),
            cancel.as_ref(),
        );

        if let Ok(mut active) = self.search_cancel.lock()
            && active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &cancel))
        {
            *active = None;
        }

        let Some(snapshot) = result.map_err(KiteError::operation)? else {
            return Ok(None);
        };
        let vcs = load_vcs_snapshot(&snapshot.root);
        let mut entries = snapshot.entries;
        apply_statuses(&mut entries, &snapshot.root, &vcs);
        Ok(Some(DirectorySearchDto {
            path: snapshot.root.to_string_lossy().into_owned(),
            query: snapshot.query,
            entries: entries.iter().map(entry_dto).collect(),
            truncated: snapshot.truncated,
        }))
    }

    pub fn cancel_directory_search(&self) {
        if let Ok(active) = self.search_cancel.lock()
            && let Some(cancel) = active.as_ref()
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn devices(&self) -> DeviceSnapshotDto {
        let snapshot = devices::snapshot();
        DeviceSnapshotDto {
            volumes: snapshot.volumes.iter().map(volume_dto).collect(),
            portable_devices: snapshot.portable_devices.iter().map(device_dto).collect(),
        }
    }

    pub fn favorites(&self) -> Vec<FavoriteEntryDto> {
        favorites_store::load_favorites()
    }

    pub fn add_favorite(
        &self,
        path: String,
        name: String,
        is_directory: bool,
    ) -> Result<bool, KiteError> {
        favorites_store::add_favorite(&PathBuf::from(path), &name, is_directory)
            .map_err(KiteError::operation)
    }

    pub fn remove_favorite(&self, path: String) -> Result<bool, KiteError> {
        favorites_store::remove_favorite(&PathBuf::from(path)).map_err(KiteError::operation)
    }

    pub fn load_session(&self) -> Option<SessionStateDto> {
        session_store::load_session()
    }

    pub fn save_session(&self, state: SessionStateDto) -> Result<(), KiteError> {
        session_store::save_session(&state).map_err(KiteError::operation)
    }

    pub fn open_text_document(&self, path: String) -> Result<TextDocumentDto, KiteError> {
        text_editor::TextDocument::open(&PathBuf::from(path))
            .map(text_document_dto)
            .map_err(KiteError::operation)
    }

    pub fn copy_path_to(
        &self,
        source: String,
        destination_directory: String,
    ) -> Result<String, KiteError> {
        transfer_entry(
            &PathBuf::from(source),
            &PathBuf::from(destination_directory),
            false,
        )
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(KiteError::operation)
    }

    pub fn move_path_to(
        &self,
        source: String,
        destination_directory: String,
    ) -> Result<String, KiteError> {
        transfer_entry(
            &PathBuf::from(source),
            &PathBuf::from(destination_directory),
            true,
        )
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(KiteError::operation)
    }

    pub fn save_text_document(
        &self,
        path: String,
        text: String,
        line_ending: String,
        has_bom: bool,
        expected_sha256: String,
    ) -> Result<TextDocumentDto, KiteError> {
        let line_ending = TextLineEnding::parse(&line_ending)
            .ok_or_else(|| KiteError::operation("不支持的换行格式"))?;
        text_editor::save_text_document(
            &PathBuf::from(path),
            &text,
            line_ending,
            has_bom,
            &expected_sha256,
        )
        .map(text_document_dto)
        .map_err(KiteError::operation)
    }
}

fn text_document_dto(document: TextDocument) -> TextDocumentDto {
    TextDocumentDto {
        path: document.path.to_string_lossy().into_owned(),
        text: document.text,
        language: document.language,
        line_ending: document.line_ending.as_str().to_string(),
        has_bom: document.has_bom,
        content_sha256: document.content_sha256,
        byte_len: document.byte_len,
    }
}

fn update_info_dto(info: UpdateInfo) -> UpdateInfoDto {
    UpdateInfoDto {
        current_version: info.current_version,
        latest_version: info.latest_version,
        update_available: info.update_available,
        release_name: info.release_name,
        release_url: info.release_url,
        release_notes: info.release_notes,
        published_at: info.published_at,
        asset_name: info.asset_name,
        asset_url: info.asset_url,
        asset_sha256: info.asset_sha256,
        asset_size: info.asset_size,
    }
}

fn update_download_progress_dto(
    progress: updater::UpdateDownloadProgress,
) -> UpdateDownloadProgressDto {
    UpdateDownloadProgressDto {
        downloaded_bytes: progress.downloaded_bytes,
        total_bytes: progress.total_bytes,
        active: progress.active,
    }
}

fn update_info_from_dto(info: UpdateInfoDto) -> UpdateInfo {
    UpdateInfo {
        current_version: info.current_version,
        latest_version: info.latest_version,
        update_available: info.update_available,
        release_name: info.release_name,
        release_url: info.release_url,
        release_notes: info.release_notes,
        published_at: info.published_at,
        asset_name: info.asset_name,
        asset_url: info.asset_url,
        asset_sha256: info.asset_sha256,
        asset_size: info.asset_size,
    }
}

fn entry_dto(entry: &FileEntry) -> DirectoryEntryDto {
    DirectoryEntryDto {
        path: entry.path.to_string_lossy().into_owned(),
        name: entry.name.clone(),
        is_directory: entry.kind == EntryKind::Directory,
        size: entry.size,
        modified_unix_ms: entry
            .modified
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64),
        kind_label: entry.type_label(),
        git_status: entry.git_status.map(vcs_status_text),
        svn_status: entry.svn_status.map(vcs_status_text),
    }
}

fn vcs_status_text(status: VcsStatus) -> String {
    format!("{} ({})", status.label(), status.code())
}

fn volume_dto(volume: &StorageVolume) -> VolumeDto {
    VolumeDto {
        path: volume.path.to_string_lossy().into_owned(),
        name: volume.volume_name.clone(),
        device_name: volume.device_name.clone(),
        kind: match volume.kind {
            VolumeKind::Usb => "usb",
            VolumeKind::Removable => "removable",
            VolumeKind::Fixed => "fixed",
            VolumeKind::Optical => "optical",
            VolumeKind::Network => "network",
            VolumeKind::Other => "other",
        }
        .to_string(),
        free_bytes: volume.free_bytes,
        total_bytes: volume.total_bytes,
    }
}

fn device_dto(device: &PortableDevice) -> PortableDeviceDto {
    PortableDeviceDto {
        name: device.name.clone(),
        shell_path: device.shell_path.clone(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_record_round_trips_asset_fields() {
        let update = UpdateInfo {
            current_version: "0.6.12".to_string(),
            latest_version: "0.6.13".to_string(),
            update_available: true,
            release_name: "Kite v0.6.13".to_string(),
            release_url: "https://github.com/wyz541/kite/releases/tag/v0.6.13".to_string(),
            release_notes: "notes".to_string(),
            published_at: Some("2026-10-27T10:00:00Z".to_string()),
            asset_name: "Kite.exe".to_string(),
            asset_url: "https://github.com/wyz541/kite/releases/download/v0.6.13/Kite.exe"
                .to_string(),
            asset_sha256: "a".repeat(64),
            asset_size: 1024,
        };

        let dto = update_info_dto(update.clone());
        let restored = update_info_from_dto(dto);

        assert_eq!(restored, update);
    }

    #[test]
    fn core_searches_nested_entries() {
        let directory = std::env::temp_dir().join(format!(
            "kite-core-search-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let nested = directory.join("nested");
        std::fs::create_dir_all(&nested).expect("create search fixture");
        std::fs::write(nested.join("target.txt"), b"search target").expect("write search fixture");

        let core = KiteCore::new();
        let result = core
            .search_directory(
                directory.to_string_lossy().into_owned(),
                "target".to_string(),
            )
            .expect("search succeeds")
            .expect("search returns a snapshot");
        let _ = std::fs::remove_dir_all(&directory);

        assert_eq!(result.query, "target");
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].name, "target.txt");
        assert!(!result.truncated);
    }

    #[test]
    fn core_exposes_platform_and_version() {
        let core = KiteCore::new();
        assert!(!core.version().is_empty());
        assert!(matches!(
            core.platform().as_str(),
            "windows" | "macos" | "other"
        ));
    }

    #[test]
    fn core_exposes_download_progress() {
        let progress = KiteCore::new().download_progress();

        assert_eq!(progress.downloaded_bytes, 0);
        assert_eq!(progress.total_bytes, 0);
        assert!(!progress.active);
    }
}
