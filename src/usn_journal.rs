use std::ffi::{OsString, c_void};
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ExtendedFileIdType, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_ID_128, FILE_ID_DESCRIPTOR, FILE_ID_DESCRIPTOR_0, FILE_SHARE_DELETE, FILE_SHARE_READ,
    FILE_SHARE_WRITE, FILE_TRAVERSE, FileIdType, GetFinalPathNameByHandleW, OPEN_EXISTING,
    OpenFileById, VOLUME_NAME_DOS,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::Ioctl::{
    FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_UNPRIVILEGED_USN_JOURNAL, READ_USN_JOURNAL_DATA_V0,
    USN_JOURNAL_DATA_V0, USN_REASON_BASIC_INFO_CHANGE, USN_REASON_COMPRESSION_CHANGE,
    USN_REASON_DATA_EXTEND, USN_REASON_DATA_OVERWRITE, USN_REASON_DATA_TRUNCATION,
    USN_REASON_DESIRED_STORAGE_CLASS_CHANGE, USN_REASON_EA_CHANGE, USN_REASON_ENCRYPTION_CHANGE,
    USN_REASON_FILE_CREATE, USN_REASON_FILE_DELETE, USN_REASON_HARD_LINK_CHANGE,
    USN_REASON_INTEGRITY_CHANGE, USN_REASON_NAMED_DATA_EXTEND, USN_REASON_NAMED_DATA_OVERWRITE,
    USN_REASON_NAMED_DATA_TRUNCATION, USN_REASON_RENAME_NEW_NAME, USN_REASON_RENAME_OLD_NAME,
    USN_REASON_REPARSE_POINT_CHANGE, USN_REASON_STREAM_CHANGE, USN_REASON_TRANSACTED_CHANGE,
};
use windows::core::HSTRING;

const READ_BUFFER_BYTES: usize = 1024 * 1024;
const ERROR_HANDLE_EOF: i32 = 38;
const ERROR_WRITE_PROTECT: i32 = 19;

const UPSERT_REASONS: u32 = USN_REASON_FILE_CREATE
    | USN_REASON_DATA_OVERWRITE
    | USN_REASON_DATA_EXTEND
    | USN_REASON_DATA_TRUNCATION
    | USN_REASON_NAMED_DATA_OVERWRITE
    | USN_REASON_NAMED_DATA_EXTEND
    | USN_REASON_NAMED_DATA_TRUNCATION
    | USN_REASON_EA_CHANGE
    | USN_REASON_RENAME_NEW_NAME
    | USN_REASON_BASIC_INFO_CHANGE
    | USN_REASON_HARD_LINK_CHANGE
    | USN_REASON_COMPRESSION_CHANGE
    | USN_REASON_ENCRYPTION_CHANGE
    | USN_REASON_REPARSE_POINT_CHANGE
    | USN_REASON_STREAM_CHANGE
    | USN_REASON_TRANSACTED_CHANGE
    | USN_REASON_INTEGRITY_CHANGE
    | USN_REASON_DESIRED_STORAGE_CLASS_CHANGE;

const REMOVE_REASONS: u32 = USN_REASON_FILE_DELETE | USN_REASON_RENAME_OLD_NAME;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JournalCursor {
    pub journal_id: u64,
    pub next_usn: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JournalPreparation {
    pub cursor: JournalCursor,
    pub reusable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsnChangeKind {
    Upsert,
    Remove,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsnChange {
    pub path: PathBuf,
    pub kind: UsnChangeKind,
    pub usn: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsnReadBatch {
    pub changes: Vec<UsnChange>,
    pub next_usn: i64,
}

#[derive(Clone, Copy, Debug)]
struct JournalInfo {
    journal_id: u64,
    first_usn: i64,
    next_usn: i64,
    lowest_valid_usn: i64,
}

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct JournalVolume {
    root: PathBuf,
    handle: OwnedHandle,
}

impl JournalVolume {
    fn open(root: &Path) -> Result<Option<Self>, String> {
        let Some(volume_root) = volume_root(root) else {
            return Ok(None);
        };
        let drive = volume_root
            .to_string_lossy()
            .chars()
            .next()
            .ok_or_else(|| "USN 卷路径没有盘符".to_string())?;
        let volume_path = HSTRING::from(format!(r"\\.\{drive}:"));
        let handle = unsafe {
            CreateFileW(
                &volume_path,
                FILE_TRAVERSE.0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .map_err(|error| format!("无法打开 USN 卷 {drive}:：{error}"))?;
        Ok(Some(Self {
            root: volume_root,
            handle: OwnedHandle(handle),
        }))
    }

    fn query(&self) -> Result<JournalInfo, String> {
        let mut data = USN_JOURNAL_DATA_V0::default();
        let mut returned = 0u32;
        unsafe {
            DeviceIoControl(
                self.handle.0,
                FSCTL_QUERY_USN_JOURNAL,
                None,
                0,
                Some((&mut data as *mut USN_JOURNAL_DATA_V0).cast()),
                size_of::<USN_JOURNAL_DATA_V0>() as u32,
                Some(&mut returned),
                None,
            )
        }
        .map_err(|error| format!("无法查询 USN Journal：{error}"))?;
        if (returned as usize) < size_of::<USN_JOURNAL_DATA_V0>() {
            return Err("USN Journal 查询结果不完整".to_string());
        }
        Ok(JournalInfo {
            journal_id: data.UsnJournalID,
            first_usn: data.FirstUsn,
            next_usn: data.NextUsn,
            lowest_valid_usn: data.LowestValidUsn,
        })
    }

    fn read_available(&self, journal_id: u64, start_usn: i64) -> Result<UsnReadBatch, String> {
        let request = READ_USN_JOURNAL_DATA_V0 {
            StartUsn: start_usn,
            ReasonMask: u32::MAX,
            ReturnOnlyOnClose: 0,
            Timeout: 0,
            BytesToWaitFor: 0,
            UsnJournalID: journal_id,
        };
        let mut buffer = vec![0u8; READ_BUFFER_BYTES];
        let mut returned = 0u32;
        if let Err(error) = unsafe {
            DeviceIoControl(
                self.handle.0,
                FSCTL_READ_UNPRIVILEGED_USN_JOURNAL,
                Some((&request as *const READ_USN_JOURNAL_DATA_V0).cast()),
                size_of::<READ_USN_JOURNAL_DATA_V0>() as u32,
                Some(buffer.as_mut_ptr().cast::<c_void>()),
                buffer.len() as u32,
                Some(&mut returned),
                None,
            )
        } {
            if is_end_of_journal(&error) {
                return Ok(UsnReadBatch {
                    changes: Vec::new(),
                    next_usn: start_usn,
                });
            }
            return Err(format!("无法读取 USN Journal：{error}"));
        }
        let returned = returned as usize;
        if returned < size_of::<i64>() {
            return Ok(UsnReadBatch {
                changes: Vec::new(),
                next_usn: start_usn,
            });
        }
        let next_usn = read_i64(&buffer, 0)?;
        let mut changes = Vec::new();
        let mut offset = size_of::<i64>();
        while offset + size_of::<u32>() <= returned {
            let record_length = read_u32(&buffer, offset)? as usize;
            if record_length < 8 || offset + record_length > returned {
                return Err("USN Journal 记录长度无效".to_string());
            }
            if let Some(record) = ParsedRecord::parse(&buffer[offset..offset + record_length])? {
                let kind = if record.reason & REMOVE_REASONS != 0 {
                    Some(UsnChangeKind::Remove)
                } else if record.reason & UPSERT_REASONS != 0 {
                    Some(UsnChangeKind::Upsert)
                } else {
                    None
                };
                if let Some(kind) = kind {
                    let path = if record.name.is_empty() {
                        if kind == UsnChangeKind::Remove {
                            offset += record_length;
                            continue;
                        }
                        match self.resolve_path(record.file_id) {
                            Ok(path) => path,
                            Err(_) => {
                                offset += record_length;
                                continue;
                            }
                        }
                    } else {
                        match self.resolve_path(record.parent_id) {
                            Ok(parent) => parent.join(&record.name),
                            Err(parent_error) => self
                                .resolve_path(record.file_id)
                                .map_err(|_| parent_error)?,
                        }
                    };
                    if path_is_within(&path, &self.root) {
                        changes.push(UsnChange {
                            path,
                            kind,
                            usn: record.usn,
                        });
                    }
                }
            }
            offset += record_length;
        }
        Ok(UsnReadBatch { changes, next_usn })
    }

    fn resolve_path(&self, file_id: FileId) -> Result<PathBuf, String> {
        let descriptor = file_id.descriptor();
        let handle = unsafe {
            OpenFileById(
                self.handle.0,
                &descriptor,
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                FILE_FLAG_BACKUP_SEMANTICS,
            )
        }
        .map_err(|error| format!("无法按文件 ID 打开路径：{error}"))?;
        let handle = OwnedHandle(handle);
        let mut buffer = vec![0u16; 32_768];
        let length = unsafe { GetFinalPathNameByHandleW(handle.0, &mut buffer, VOLUME_NAME_DOS) };
        if length == 0 {
            return Err("无法读取文件 ID 对应的路径".to_string());
        }
        if length as usize > buffer.len() {
            buffer.resize(length as usize + 1, 0);
            let retry =
                unsafe { GetFinalPathNameByHandleW(handle.0, &mut buffer, VOLUME_NAME_DOS) };
            if retry == 0 || retry as usize > buffer.len() {
                return Err("无法读取文件 ID 对应的长路径".to_string());
            }
        }
        let end = buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(length as usize);
        Ok(normalize_nt_path(OsString::from_wide(&buffer[..end])))
    }
}

pub fn prepare(
    root: &Path,
    existing: Option<JournalCursor>,
) -> Result<Option<JournalPreparation>, String> {
    let Some(volume) = JournalVolume::open(root)? else {
        return Ok(None);
    };
    let journal = volume.query()?;
    let reusable = existing.is_some_and(|cursor| cursor_is_valid(cursor, journal));
    let cursor = existing.filter(|_| reusable).unwrap_or(JournalCursor {
        journal_id: journal.journal_id,
        next_usn: journal.next_usn,
    });
    Ok(Some(JournalPreparation { cursor, reusable }))
}

pub struct JournalReader {
    volume: JournalVolume,
    journal: JournalInfo,
}

impl JournalReader {
    pub fn open(root: &Path) -> Result<Option<Self>, String> {
        let Some(volume) = JournalVolume::open(root)? else {
            return Ok(None);
        };
        let journal = volume.query()?;
        Ok(Some(Self { volume, journal }))
    }

    pub fn cursor(&self) -> JournalCursor {
        JournalCursor {
            journal_id: self.journal.journal_id,
            next_usn: self.journal.next_usn,
        }
    }

    pub fn first_usn(&self) -> i64 {
        self.journal.first_usn
    }

    pub fn lowest_valid_usn(&self) -> i64 {
        self.journal.lowest_valid_usn
    }

    pub fn next_usn(&self) -> i64 {
        self.journal.next_usn
    }

    pub fn read_available(&self, start_usn: i64) -> Result<UsnReadBatch, String> {
        self.volume
            .read_available(self.journal.journal_id, start_usn)
    }
}

#[derive(Clone, Copy)]
enum FileId {
    Standard(u64),
    Extended([u8; 16]),
}

impl FileId {
    fn descriptor(self) -> FILE_ID_DESCRIPTOR {
        let mut descriptor = FILE_ID_DESCRIPTOR {
            dwSize: size_of::<FILE_ID_DESCRIPTOR>() as u32,
            ..Default::default()
        };
        match self {
            Self::Standard(value) => {
                descriptor.Type = FileIdType;
                descriptor.Anonymous = FILE_ID_DESCRIPTOR_0 {
                    FileId: value as i64,
                };
            }
            Self::Extended(value) => {
                descriptor.Type = ExtendedFileIdType;
                descriptor.Anonymous = FILE_ID_DESCRIPTOR_0 {
                    ExtendedFileId: FILE_ID_128 { Identifier: value },
                };
            }
        }
        descriptor
    }
}

struct ParsedRecord {
    usn: i64,
    reason: u32,
    file_id: FileId,
    parent_id: FileId,
    name: OsString,
}

impl ParsedRecord {
    fn parse(record: &[u8]) -> Result<Option<Self>, String> {
        let major = read_u16(record, 4)?;
        let (usn_offset, reason_offset, file_id, parent_id, name_length_offset, name_offset) =
            match major {
                2 if record.len() >= 60 => (
                    24,
                    40,
                    FileId::Standard(read_u64(record, 8)?),
                    FileId::Standard(read_u64(record, 16)?),
                    56,
                    58,
                ),
                3 if record.len() >= 76 => (
                    40,
                    56,
                    FileId::Extended(read_array(record, 8)?),
                    FileId::Extended(read_array(record, 24)?),
                    72,
                    74,
                ),
                _ => return Ok(None),
            };
        let name_length = read_u16(record, name_length_offset)? as usize;
        let name_offset = read_u16(record, name_offset)? as usize;
        if name_offset > record.len()
            || name_offset + name_length > record.len()
            || !name_length.is_multiple_of(2)
        {
            return Ok(None);
        }
        let mut name = Vec::with_capacity(name_length / 2);
        for chunk in record[name_offset..name_offset + name_length]
            .as_chunks::<2>()
            .0
        {
            name.push(u16::from_le_bytes(*chunk));
        }
        Ok(Some(Self {
            usn: read_i64(record, usn_offset)?,
            reason: read_u32(record, reason_offset)?,
            file_id,
            parent_id,
            name: OsString::from_wide(&name),
        }))
    }
}

pub fn volume_root(root: &Path) -> Option<PathBuf> {
    let text = root.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return None;
    }
    Some(PathBuf::from(format!(
        "{}:\\",
        (bytes[0] as char).to_ascii_uppercase()
    )))
}

fn cursor_is_valid(cursor: JournalCursor, journal: JournalInfo) -> bool {
    cursor.journal_id == journal.journal_id
        && cursor.next_usn >= journal.first_usn
        && cursor.next_usn >= journal.lowest_valid_usn
        && cursor.next_usn <= journal.next_usn
}

fn is_end_of_journal(error: &windows::core::Error) -> bool {
    let code = error.code().0 as u32 & 0xffff;
    code == ERROR_HANDLE_EOF as u32 || code == ERROR_WRITE_PROTECT as u32
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    let path_text = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let root_text = root
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    path_text == root_text || path_text.starts_with(&format!("{root_text}\\"))
}

fn normalize_nt_path(path: OsString) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    if let Some(rest) = text.strip_prefix(r"\??\") {
        return PathBuf::from(rest);
    }
    PathBuf::from(path)
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    bytes
        .get(offset..offset + 2)
        .and_then(|value| value.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| "USN Journal 记录字段越界".to_string())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| "USN Journal 记录字段越界".to_string())
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    bytes
        .get(offset..offset + 8)
        .and_then(|value| value.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| "USN Journal 记录字段越界".to_string())
}

fn read_i64(bytes: &[u8], offset: usize) -> Result<i64, String> {
    bytes
        .get(offset..offset + 8)
        .and_then(|value| value.try_into().ok())
        .map(i64::from_le_bytes)
        .ok_or_else(|| "USN Journal 记录字段越界".to_string())
}

fn read_array(bytes: &[u8], offset: usize) -> Result<[u8; 16], String> {
    bytes
        .get(offset..offset + 16)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| "USN Journal 文件 ID 越界".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v2_record(name: &str, reason: u32) -> Vec<u8> {
        let name = name.encode_utf16().collect::<Vec<_>>();
        let name_bytes = name.len() * 2;
        let record_length = (60 + name_bytes + 7) & !7;
        let mut record = vec![0u8; record_length];
        record[0..4].copy_from_slice(&(record_length as u32).to_le_bytes());
        record[4..6].copy_from_slice(&2u16.to_le_bytes());
        record[8..16].copy_from_slice(&42u64.to_le_bytes());
        record[16..24].copy_from_slice(&24u64.to_le_bytes());
        record[24..32].copy_from_slice(&100i64.to_le_bytes());
        record[40..44].copy_from_slice(&reason.to_le_bytes());
        record[56..58].copy_from_slice(&(name_bytes as u16).to_le_bytes());
        record[58..60].copy_from_slice(&60u16.to_le_bytes());
        for (index, value) in name.iter().enumerate() {
            let start = 60 + index * 2;
            record[start..start + 2].copy_from_slice(&value.to_le_bytes());
        }
        record
    }

    fn v3_record(name: &str, reason: u32) -> Vec<u8> {
        let name = name.encode_utf16().collect::<Vec<_>>();
        let name_bytes = name.len() * 2;
        let record_length = (76 + name_bytes + 7) & !7;
        let mut record = vec![0u8; record_length];
        record[0..4].copy_from_slice(&(record_length as u32).to_le_bytes());
        record[4..6].copy_from_slice(&3u16.to_le_bytes());
        record[8..24].copy_from_slice(&[0x42; 16]);
        record[24..40].copy_from_slice(&[0x24; 16]);
        record[40..48].copy_from_slice(&120i64.to_le_bytes());
        record[56..60].copy_from_slice(&reason.to_le_bytes());
        record[72..74].copy_from_slice(&(name_bytes as u16).to_le_bytes());
        record[74..76].copy_from_slice(&76u16.to_le_bytes());
        for (index, value) in name.iter().enumerate() {
            let start = 76 + index * 2;
            record[start..start + 2].copy_from_slice(&value.to_le_bytes());
        }
        record
    }

    #[test]
    fn parses_v2_record_name_reason_and_cursor_fields() {
        let record = ParsedRecord::parse(&v2_record("wooduan.txt", USN_REASON_FILE_CREATE))
            .expect("parse record")
            .expect("supported record");
        assert_eq!(record.usn, 100);
        assert_eq!(record.reason, USN_REASON_FILE_CREATE);
        assert_eq!(record.name, OsString::from("wooduan.txt"));
        assert!(matches!(record.file_id, FileId::Standard(42)));
        assert!(matches!(record.parent_id, FileId::Standard(24)));
    }

    #[test]
    fn parses_v3_record_name_reason_and_extended_ids() {
        let record = ParsedRecord::parse(&v3_record("reports.folder", USN_REASON_RENAME_NEW_NAME))
            .expect("parse record")
            .expect("supported record");
        assert_eq!(record.usn, 120);
        assert_eq!(record.reason, USN_REASON_RENAME_NEW_NAME);
        assert_eq!(record.name, OsString::from("reports.folder"));
        assert!(matches!(record.file_id, FileId::Extended(value) if value == [0x42; 16]));
        assert!(matches!(record.parent_id, FileId::Extended(value) if value == [0x24; 16]));
    }

    #[test]
    fn rejects_cursor_from_other_journal_or_expired_range() {
        let journal = JournalInfo {
            journal_id: 7,
            first_usn: 100,
            next_usn: 300,
            lowest_valid_usn: 80,
        };
        assert!(cursor_is_valid(
            JournalCursor {
                journal_id: 7,
                next_usn: 200,
            },
            journal
        ));
        assert!(!cursor_is_valid(
            JournalCursor {
                journal_id: 8,
                next_usn: 200,
            },
            journal
        ));
        assert!(!cursor_is_valid(
            JournalCursor {
                journal_id: 7,
                next_usn: 50,
            },
            journal
        ));
    }

    #[test]
    fn normalizes_nt_and_unc_paths() {
        assert_eq!(
            normalize_nt_path(OsString::from(r"\\?\C:\Users\Kite")),
            PathBuf::from(r"C:\Users\Kite")
        );
        assert_eq!(
            normalize_nt_path(OsString::from(r"\\?\UNC\server\share\Kite")),
            PathBuf::from(r"\\server\share\Kite")
        );
    }

    #[test]
    fn finds_local_volume_roots_from_drive_paths() {
        assert_eq!(volume_root(Path::new(r"C:\")), Some(PathBuf::from(r"C:\")));
        assert_eq!(
            volume_root(Path::new(r"d:\Projects\Kite")),
            Some(PathBuf::from(r"D:\"))
        );
        assert_eq!(volume_root(Path::new(r"\\server\share")), None);
    }

    #[test]
    #[ignore = "requires a local NTFS or ReFS volume"]
    fn queries_the_system_volume_journal() {
        let preparation = prepare(Path::new(r"C:\"), None)
            .expect("query journal")
            .expect("drive root");
        assert!(preparation.cursor.journal_id > 0);
    }

    #[test]
    #[ignore = "observes the live system USN Journal"]
    fn reads_and_resolves_live_file_creates() {
        use std::fs;
        use std::thread;
        use std::time::{Duration, SystemTime, UNIX_EPOCH};

        let root = Path::new(r"C:\");
        let preparation = prepare(root, None)
            .expect("prepare journal")
            .expect("drive root");
        let reader = JournalReader::open(root)
            .expect("open journal")
            .expect("drive root");
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("kite-usn-live-{}-{unique}.txt", std::process::id()));
        fs::write(&path, b"fixture").expect("write USN fixture");
        let mut cursor = preparation.cursor.next_usn;
        let mut created = false;
        for _ in 0..50 {
            let batch = reader.read_available(cursor).expect("read create record");
            cursor = batch.next_usn;
            if batch
                .changes
                .iter()
                .any(|change| change.path == path && change.kind == UsnChangeKind::Upsert)
            {
                created = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(created, "USN journal did not resolve the created path");
        fs::remove_file(&path).expect("remove USN fixture");
    }
}
