use std::cmp::Ordering;
use std::fs::{self, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::SystemTime;

use chrono::{DateTime, Local};
use walkdir::WalkDir;

pub const SEARCH_RESULT_LIMIT: usize = 1_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Directory,
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VcsStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Ignored,
    Conflicted,
    PropertyModified,
    Missing,
    External,
}

impl VcsStatus {
    pub fn rank(self) -> u8 {
        match self {
            Self::Ignored => 1,
            Self::External => 2,
            Self::Untracked => 3,
            Self::Modified => 4,
            Self::PropertyModified => 5,
            Self::Added => 6,
            Self::Renamed => 7,
            Self::Deleted => 8,
            Self::Missing => 9,
            Self::Conflicted => 10,
        }
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "?",
            Self::Ignored => "!",
            Self::Conflicted => "C",
            Self::PropertyModified => "P",
            Self::Missing => "!",
            Self::External => "X",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Modified => "已修改",
            Self::Added => "已新增",
            Self::Deleted => "已删除",
            Self::Renamed => "已重命名",
            Self::Untracked => "未跟踪",
            Self::Ignored => "已忽略",
            Self::Conflicted => "存在冲突",
            Self::PropertyModified => "属性已修改",
            Self::Missing => "缺失",
            Self::External => "外部项目",
        }
    }
}

pub fn worse_status(left: Option<VcsStatus>, right: Option<VcsStatus>) -> Option<VcsStatus> {
    match (left, right) {
        (Some(left), Some(right)) => Some(if left.rank() >= right.rank() {
            left
        } else {
            right
        }),
        (Some(status), None) | (None, Some(status)) => Some(status),
        (None, None) => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Vcs,
    Modified,
    Kind,
    Size,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortSpec {
    pub key: SortKey,
    pub ascending: bool,
}

impl Default for SortSpec {
    fn default() -> Self {
        Self {
            key: SortKey::Name,
            ascending: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub extension: String,
    pub git_status: Option<VcsStatus>,
    pub svn_status: Option<VcsStatus>,
}

impl FileEntry {
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Directory
    }

    pub fn size_label(&self) -> String {
        if self.is_dir() {
            String::new()
        } else {
            format_size(self.size)
        }
    }

    pub fn modified_label(&self) -> String {
        self.modified
            .map(|value| {
                let value: DateTime<Local> = value.into();
                value.format("%Y/%m/%d %H:%M").to_string()
            })
            .unwrap_or_else(|| "未知".to_string())
    }

    pub fn type_label(&self) -> String {
        type_label(self.kind, &self.extension)
    }

    pub fn vcs_rank(&self) -> u8 {
        worse_status(self.git_status, self.svn_status).map_or(0, VcsStatus::rank)
    }
}

#[derive(Debug)]
pub struct DirectorySnapshot {
    pub path: PathBuf,
    pub entries: Vec<FileEntry>,
    pub skipped: usize,
}

#[derive(Debug)]
pub struct SearchSnapshot {
    pub root: PathBuf,
    pub query: String,
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
}

pub fn load_directory(path: &Path, sort: SortSpec) -> Result<DirectorySnapshot, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("无法确定当前文件夹：{error}"))?
            .join(path)
    };
    let reader = fs::read_dir(&path).map_err(|error| folder_error("读取", &error))?;
    let mut entries = Vec::new();
    let mut skipped = 0usize;

    for item in reader {
        let Ok(item) = item else {
            skipped += 1;
            continue;
        };
        let entry_path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        let file_type = match item.file_type() {
            Ok(file_type) => file_type,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let link_metadata = fs::symlink_metadata(&entry_path);
        let Ok(link_metadata) = link_metadata else {
            skipped += 1;
            continue;
        };
        if is_hidden(&link_metadata, &name) {
            continue;
        }
        let metadata = if file_type.is_symlink() {
            fs::metadata(&entry_path)
        } else {
            Ok(link_metadata)
        };
        let Ok(metadata) = metadata else {
            skipped += 1;
            continue;
        };
        entries.push(file_entry_from_metadata(entry_path, name, &metadata));
    }

    sort_entries(&mut entries, sort);
    Ok(DirectorySnapshot {
        path,
        entries,
        skipped,
    })
}

fn folder_error(action: &str, error: &io::Error) -> String {
    let reason = match error.kind() {
        io::ErrorKind::PermissionDenied => "拒绝访问。此文件夹可能受系统保护",
        io::ErrorKind::NotFound => "路径不存在",
        _ => return format!("无法{action}文件夹：{error}"),
    };
    format!("无法{action}文件夹：{reason}。")
}

pub fn search_directory(
    root: &Path,
    query: &str,
    sort: SortSpec,
    cancel: &AtomicBool,
) -> Result<Option<SearchSnapshot>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    let query_lower = query.to_lowercase();
    let mut entries = Vec::new();
    let mut truncated = false;

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if entry.depth() == 0 {
                return true;
            }
            let name = entry.file_name().to_string_lossy();
            if is_hidden_name(&name) {
                return false;
            }
            entry
                .metadata()
                .map(|metadata| !is_hidden(&metadata, &name))
                .unwrap_or(false)
        });
    for item in walker {
        if cancel.load(AtomicOrdering::Relaxed) {
            return Ok(None);
        }
        let Ok(item) = item else {
            continue;
        };
        if item.depth() == 0 {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        if is_hidden_name(&name) {
            if item.file_type().is_dir() {
                // WalkDir cannot prune from this loop form. Hidden folders are
                // rare and preserving correctness is safer than a second walk.
            }
            continue;
        }
        if !name.to_lowercase().contains(&query_lower) {
            continue;
        }
        let Ok(metadata) = item.metadata() else {
            continue;
        };
        if is_hidden(&metadata, &name) {
            continue;
        }
        entries.push(file_entry_from_metadata(
            item.path().to_path_buf(),
            name,
            &metadata,
        ));
        if entries.len() >= SEARCH_RESULT_LIMIT {
            truncated = true;
            break;
        }
    }

    sort_entries(&mut entries, sort);
    Ok(Some(SearchSnapshot {
        root: root.to_path_buf(),
        query: query.to_string(),
        entries,
        truncated,
    }))
}

fn file_entry_from_metadata(path: PathBuf, name: String, metadata: &Metadata) -> FileEntry {
    let kind = if metadata.is_dir() {
        EntryKind::Directory
    } else {
        EntryKind::File
    };
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    FileEntry {
        path,
        name,
        kind,
        size: metadata.len(),
        modified: metadata.modified().ok(),
        extension,
        git_status: None,
        svn_status: None,
    }
}

pub fn sort_entries(entries: &mut [FileEntry], spec: SortSpec) {
    entries.sort_by(|left, right| {
        group_order(left, right).then_with(|| {
            let order = match spec.key {
                SortKey::Name => compare_names(left, right),
                SortKey::Vcs => left.vcs_rank().cmp(&right.vcs_rank()),
                SortKey::Modified => left.modified.cmp(&right.modified),
                SortKey::Kind => left
                    .extension
                    .cmp(&right.extension)
                    .then_with(|| compare_names(left, right)),
                SortKey::Size => left.size.cmp(&right.size),
            };
            if spec.ascending {
                order
            } else {
                order.reverse()
            }
        })
    });
}

fn group_order(left: &FileEntry, right: &FileEntry) -> Ordering {
    match (left.is_dir(), right.is_dir()) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}

fn compare_names(left: &FileEntry, right: &FileEntry) -> Ordering {
    left.name
        .to_lowercase()
        .cmp(&right.name.to_lowercase())
        .then_with(|| left.name.cmp(&right.name))
}

pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f < KB {
        format!("{bytes} B")
    } else if bytes_f < MB {
        format!("{:.1} KB", bytes_f / KB)
    } else if bytes_f < GB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f < TB {
        format!("{:.1} GB", bytes_f / GB)
    } else {
        format!("{:.1} TB", bytes_f / TB)
    }
}

pub fn type_label(kind: EntryKind, extension: &str) -> String {
    if kind == EntryKind::Directory {
        return "文件夹".to_string();
    }
    let category = match extension {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" => "图像",
        "txt" | "md" | "log" | "ini" | "toml" | "yaml" | "yml" | "json" => "文本文档",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" => "文档",
        "zip" | "7z" | "rar" | "tar" | "gz" => "压缩文件",
        "exe" | "msi" => "应用",
        "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "html" | "css" | "c" | "cpp" | "h" | "cs"
        | "java" | "go" | "kt" => "源代码",
        "mp3" | "wav" | "flac" | "aac" | "ogg" => "音频",
        "mp4" | "mkv" | "mov" | "avi" | "webm" => "视频",
        _ => return "文件".to_string(),
    };
    if extension.is_empty() {
        category.to_string()
    } else {
        format!("{} {}", extension.to_uppercase(), category)
    }
}

pub fn create_folder(parent: &Path, name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let target = parent.join(name.trim());
    if target.exists() {
        return Err("同名项目已经存在".to_string());
    }
    fs::create_dir(&target).map_err(|error| format!("新建文件夹失败：{error}"))?;
    Ok(target)
}

pub fn create_file(parent: &Path, name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let target = parent.join(name.trim());
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .map_err(|error| {
            if error.kind() == io::ErrorKind::AlreadyExists {
                "同名项目已经存在".to_string()
            } else {
                format!("新建文件失败：{error}")
            }
        })?;
    Ok(target)
}

pub fn rename_entry(path: &Path, name: &str) -> Result<PathBuf, String> {
    validate_name(name)?;
    let Some(parent) = path.parent() else {
        return Err("不能重命名根目录".to_string());
    };
    let target = parent.join(name.trim());
    if target == path {
        return Ok(target);
    }
    if target.exists() {
        return Err("同名项目已经存在".to_string());
    }
    fs::rename(path, &target).map_err(|error| format!("重命名失败：{error}"))?;
    Ok(target)
}

pub fn recycle_entry(path: &Path) -> Result<(), String> {
    trash::delete(path).map_err(|error| format!("移到回收站失败：{error}"))
}

#[cfg(target_os = "windows")]
pub fn open_file(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    use windows::core::PCWSTR;

    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let file = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let directory = directory
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR(directory.as_ptr()),
            SW_SHOWNORMAL,
        )
    };
    let code = result.0 as isize;
    if code <= 32 {
        Err(format!("打开文件失败：Windows 错误 {code}"))
    } else {
        Ok(())
    }
}

#[cfg(not(target_os = "windows"))]
pub fn open_file(path: &Path) -> Result<(), String> {
    open::that(path).map_err(|error| format!("打开文件失败：{error}"))
}

pub fn validate_name(name: &str) -> Result<(), String> {
    crate::platform::validate_file_name(name)
}

#[cfg(target_os = "windows")]
fn is_hidden(metadata: &Metadata, _name: &str) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    metadata.file_attributes() & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0
}

#[cfg(not(target_os = "windows"))]
fn is_hidden(_metadata: &Metadata, name: &str) -> bool {
    is_hidden_name(name)
}

fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn entry(name: &str, kind: EntryKind, size: u64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(name),
            name: name.to_string(),
            kind,
            size,
            modified: None,
            extension: Path::new(name)
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
            git_status: None,
            svn_status: None,
        }
    }

    #[test]
    fn sorts_directories_before_files() {
        let mut entries = vec![
            entry("alpha.txt", EntryKind::File, 20),
            entry("zeta", EntryKind::Directory, 0),
            entry("beta.txt", EntryKind::File, 10),
        ];
        sort_entries(&mut entries, SortSpec::default());
        assert_eq!(entries[0].name, "zeta");
        assert_eq!(entries[1].name, "alpha.txt");
        assert_eq!(entries[2].name, "beta.txt");
    }

    #[test]
    fn sorts_by_size_without_mixing_file_types() {
        let mut entries = vec![
            entry("small.txt", EntryKind::File, 1),
            entry("large.txt", EntryKind::File, 100),
            entry("folder", EntryKind::Directory, 0),
        ];
        sort_entries(
            &mut entries,
            SortSpec {
                key: SortKey::Size,
                ascending: false,
            },
        );
        assert_eq!(entries[0].name, "folder");
        assert_eq!(entries[1].name, "large.txt");
        assert_eq!(entries[2].name, "small.txt");
    }

    #[test]
    fn formats_sizes_with_binary_units() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(2 * 1024 * 1024), "2.0 MB");
    }

    #[cfg(target_os = "windows")]
    #[test]
    #[ignore = "launches a visible batch process"]
    fn opens_batch_files_in_their_parent_directory() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("kite-batch-cwd-{}-{unique}", std::process::id()));
        fs::create_dir_all(&root).expect("create batch directory");
        let result_path = root.join("result.txt");
        let batch_path = root.join("capture-cwd.bat");
        let batch = format!("@echo off\r\ncd > \"{}\"\r\n", result_path.display());
        fs::write(&batch_path, batch).expect("write batch file");

        open_file(&batch_path).expect("launch batch file");
        let mut actual = String::new();
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            if let Ok(value) = fs::read_to_string(&result_path) {
                actual = value.trim().to_string();
                break;
            }
        }

        assert!(!actual.is_empty(), "batch file did not run");
        assert!(crate::platform::paths_equal(Path::new(&actual), &root));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn identifies_known_file_types() {
        assert_eq!(type_label(EntryKind::Directory, ""), "文件夹");
        assert_eq!(type_label(EntryKind::File, "rs"), "RS 源代码");
        assert_eq!(type_label(EntryKind::File, "xyz"), "文件");
    }

    #[test]
    fn rejects_unsafe_names() {
        assert!(validate_name("").is_err());
        assert!(validate_name("..").is_err());
        assert!(validate_name("bad/name").is_err());
        assert!(validate_name("valid-name.txt").is_ok());
        #[cfg(target_os = "windows")]
        {
            assert!(validate_name("bad:name").is_err());
            assert!(validate_name("bad.").is_err());
        }
    }

    #[test]
    fn creates_an_empty_file_and_rejects_duplicate_names() {
        let root = std::env::temp_dir().join(format!(
            "kite-create-file-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create test folder");

        let created = create_file(&root, "notes.txt").expect("create file");
        assert!(created.is_file());
        assert_eq!(fs::metadata(&created).expect("file metadata").len(), 0);
        assert_eq!(
            create_file(&root, "notes.txt").unwrap_err(),
            "同名项目已经存在"
        );

        fs::remove_dir_all(root).expect("clean test folder");
    }

    #[test]
    fn loads_and_searches_a_temporary_directory() {
        let root = std::env::temp_dir().join(format!(
            "kite-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_nanos()
        ));
        fs::create_dir_all(root.join("nested")).expect("create nested test folder");
        fs::write(root.join("hello.txt"), b"hello").expect("write test file");
        fs::write(root.join("nested").join("target.log"), b"log").expect("write nested test file");

        let snapshot = load_directory(&root, SortSpec::default()).expect("load directory");
        assert!(snapshot.entries.iter().any(|item| item.name == "hello.txt"));
        assert!(snapshot.entries.iter().any(|item| item.name == "nested"));

        let cancel = AtomicBool::new(false);
        let result = search_directory(&root, "target", SortSpec::default(), &cancel)
            .expect("search directory")
            .expect("non-empty search");
        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].name, "target.log");

        let cancelled = AtomicBool::new(true);
        let cancelled_result = search_directory(&root, "target", SortSpec::default(), &cancelled)
            .expect("cancelled search should not fail");
        assert!(cancelled_result.is_none());

        fs::remove_dir_all(root).expect("clean test directory");
    }
}
pub fn transfer_entry(
    source: &Path,
    destination_dir: &Path,
    move_source: bool,
) -> Result<PathBuf, String> {
    if !destination_dir.is_dir() {
        return Err("目标文件夹不存在".to_string());
    }
    let source_parent = source.parent();
    if move_source && source_parent.is_some_and(|parent| parent == destination_dir) {
        return Ok(source.to_path_buf());
    }
    let name = source
        .file_name()
        .ok_or_else(|| "不能复制或移动根目录".to_string())?;
    let destination = unique_destination(destination_dir, name);
    if source.is_dir() && destination.starts_with(source) {
        return Err("不能把文件夹复制或移动到自身内部".to_string());
    }

    if move_source {
        match fs::rename(source, &destination) {
            Ok(()) => return Ok(destination),
            Err(error) if error.kind() == std::io::ErrorKind::CrossesDevices => {
                copy_entry(source, &destination)?;
                fs::remove_dir_all(source)
                    .or_else(|remove_error| {
                        if source.is_dir() {
                            Err(remove_error)
                        } else {
                            fs::remove_file(source)
                        }
                    })
                    .map_err(|remove_error| {
                        format!("复制完成，但移除源项目失败：{remove_error}")
                    })?;
            }
            Err(error) => return Err(format!("移动失败：{error}")),
        }
    } else {
        if let Err(error) = copy_entry(source, &destination) {
            cleanup_partial_destination(&destination);
            return Err(error);
        }
    }
    Ok(destination)
}

fn cleanup_partial_destination(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = fs::remove_file(path);
    }
}

pub fn unique_destination(directory: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let original = directory.join(name);
    if !original.exists() {
        return original;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "项目".to_string());
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().into_owned());
    for index in 1..10_000 {
        let suffix = if index == 1 {
            " - 副本".to_string()
        } else {
            format!(" - 副本 ({index})")
        };
        let filename = match &extension {
            Some(extension) => format!("{stem}{suffix}.{extension}"),
            None => format!("{stem}{suffix}"),
        };
        let candidate = directory.join(filename);
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem} - 副本"))
}

fn copy_entry(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata =
        fs::symlink_metadata(source).map_err(|error| format!("无法读取源项目：{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("暂不支持复制或移动符号链接".to_string());
    }
    if metadata.is_dir() {
        fs::create_dir(destination).map_err(|error| format!("无法创建目标文件夹：{error}"))?;
        let entries = fs::read_dir(source).map_err(|error| format!("无法读取源文件夹：{error}"))?;
        for entry in entries {
            let entry = entry.map_err(|error| format!("读取源文件夹失败：{error}"))?;
            copy_entry(&entry.path(), &destination.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| format!("复制文件失败：{error}"))
    }
}
#[cfg(test)]
mod transfer_tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kite-transfer-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn copies_file_without_removing_source() {
        let root = test_root("copy");
        let destination = root.join("destination");
        fs::create_dir_all(&destination).expect("create destination");
        let source = root.join("note.txt");
        fs::write(&source, b"copy me").expect("write source");

        let copied = transfer_entry(&source, &destination, false).expect("copy file");
        assert_eq!(fs::read(&copied).expect("read copy"), b"copy me");
        assert!(source.exists());
        fs::remove_dir_all(root).expect("clean test root");
    }

    #[test]
    fn copies_directory_recursively() {
        let root = test_root("copy-directory");
        let source = root.join("source");
        let nested = source.join("nested");
        let destination = root.join("destination");
        fs::create_dir_all(&nested).expect("create source directory");
        fs::create_dir_all(&destination).expect("create destination");
        fs::write(nested.join("note.txt"), b"nested copy").expect("write source file");

        let copied = transfer_entry(&source, &destination, false).expect("copy directory");
        assert_eq!(
            fs::read(copied.join("nested").join("note.txt")).expect("read copied file"),
            b"nested copy"
        );
        assert!(source.exists());
        fs::remove_dir_all(root).expect("clean test root");
    }

    #[test]
    fn renames_conflicting_copy_without_overwrite() {
        let root = test_root("conflict");
        let destination = root.join("destination");
        fs::create_dir_all(&destination).expect("create destination");
        let source = root.join("report.txt");
        fs::write(&source, b"source").expect("write source");
        fs::write(destination.join("report.txt"), b"existing").expect("write existing");

        let copied = transfer_entry(&source, &destination, false).expect("copy file");
        assert_eq!(copied.file_name().unwrap(), "report - 副本.txt");
        assert_eq!(
            fs::read(destination.join("report.txt")).expect("read existing"),
            b"existing"
        );
        assert_eq!(fs::read(copied).expect("read copy"), b"source");
        fs::remove_dir_all(root).expect("clean test root");
    }

    #[test]
    fn rejects_copying_directory_into_itself() {
        let root = test_root("self");
        let source = root.join("folder");
        let destination = source.join("inside");
        fs::create_dir_all(&source).expect("create source");

        let result = transfer_entry(&source, &destination, false);
        assert!(result.is_err());
        fs::remove_dir_all(root).expect("clean test root");
    }

    #[test]
    fn explains_permission_denied_without_raw_windows_error() {
        let error = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let message = super::folder_error("打开", &error);

        assert!(message.contains("拒绝访问"));
        assert!(message.contains("系统保护"));
        assert!(!message.contains("os error 5"));
    }
}
