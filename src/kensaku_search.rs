use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atomicwrites::{AllowOverwrite, AtomicFile};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::content_search::{
    ContentFileMatch, ContentIndexState, ContentLineMatch, ContentSearchSnapshot,
};
use crate::fs::{
    EntryKind, FileEntry, GlobalNameSearchSnapshot, SEARCH_RESULT_LIMIT, file_entry_from_metadata,
    has_generated_parent, is_generated_search_directory, is_hidden, sort_global_name_entries,
};
use crate::search_filters::SearchFilters;

const STATE_CACHE_TTL: Duration = Duration::from_secs(2);
const STATE_TIMEOUT: Duration = Duration::from_secs(5);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_COMMAND_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const NAME_CANDIDATE_LIMIT: usize = 5_000;
const CONTENT_CANDIDATE_MULTIPLIER: usize = 8;
const CONTENT_CANDIDATE_LIMIT: usize = 400;
const SNIPPET_SCAN_BYTES: u64 = 8 * 1024 * 1024;
const DAEMON_START_TIMEOUT: Duration = Duration::from_secs(3);

static CLI_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static DAEMON_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static STATE_CACHE: OnceLock<Mutex<Option<CachedState>>> = OnceLock::new();
static CONTENT_CANCEL: OnceLock<AtomicBool> = OnceLock::new();
static DAEMON_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static DAEMON_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();

#[derive(Clone, Debug, Deserialize)]
struct KensakuIndexState {
    #[serde(default)]
    roots: Vec<String>,
    #[serde(default)]
    files: u64,
    #[serde(default)]
    content_docs: u64,
    #[serde(default)]
    is_indexing: bool,
    #[serde(default)]
    phase: String,
}

#[derive(Clone, Debug)]
struct CachedState {
    cli: PathBuf,
    fetched_at: Instant,
    state: Option<KensakuIndexState>,
}

#[derive(Debug, Deserialize)]
struct KensakuSearchPayload {
    #[serde(default)]
    results: Vec<KensakuSearchItem>,
    #[serde(default)]
    total: u64,
    #[serde(default)]
    returned: u64,
}

#[derive(Debug, Deserialize)]
struct KensakuSearchItem {
    path: String,
    #[serde(default)]
    score: f64,
    #[serde(default)]
    snippet: Option<String>,
    #[serde(default)]
    match_type: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    modified: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KensakuContentStatus {
    pub state: ContentIndexState,
    pub indexed_files: u64,
    pub message: Option<String>,
}

pub fn prepare_names(roots: &[PathBuf]) -> Result<(), String> {
    ensure_daemon(roots)
}

pub fn prepare_content(roots: &[PathBuf]) -> Result<(), String> {
    ensure_daemon(roots)
}

pub fn content_status() -> Option<KensakuContentStatus> {
    let cli = cli_path()?;
    let index_state = load_state(cli, None).ok().flatten()?;
    let state = if index_state.is_indexing {
        ContentIndexState::Indexing
    } else if index_state.phase == "error" {
        ContentIndexState::Error
    } else {
        ContentIndexState::Ready
    };
    Some(KensakuContentStatus {
        state,
        indexed_files: state_indexed_files(&index_state),
        message: if index_state.is_indexing {
            Some(format!(
                "Kensaku 正在建立索引：{} 项",
                state_indexed_files(&index_state)
            ))
        } else if index_state.phase == "error" {
            Some("Kensaku 索引处于错误状态".to_string())
        } else {
            None
        },
    })
}

pub fn search_names(
    roots: &[PathBuf],
    query: &str,
    filters: SearchFilters,
    cancel: &AtomicBool,
) -> Result<Option<GlobalNameSearchSnapshot>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    let roots = normalize_roots(roots);
    if roots.is_empty() {
        return Ok(None);
    }
    let Some(cli) = cli_path() else {
        return Ok(None);
    };
    let Some(state) = load_state(cli, Some(cancel))? else {
        return Ok(None);
    };
    let state_roots = state_roots(&state);
    let covered = roots_are_covered_by(&roots, &state_roots);
    let indexed_query = name_query(query);
    let mut entries = Vec::new();
    let mut saw_response = false;
    let mut raw_returned = 0usize;
    let mut raw_total = 0u64;

    for root in &roots {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let args = vec![
            OsString::from("search"),
            OsString::from(&indexed_query),
            OsString::from("--json"),
            OsString::from("--limit"),
            OsString::from(NAME_CANDIDATE_LIMIT.to_string()),
            OsString::from("--path"),
            root.as_os_str().to_os_string(),
        ];
        let Some(payload) =
            run_json::<KensakuSearchPayload>(cli, &args, Some(cancel), SEARCH_TIMEOUT)?
        else {
            continue;
        };
        saw_response = true;
        raw_returned = raw_returned.saturating_add(payload.results.len());
        raw_total = raw_total.saturating_add(payload.total.max(payload.returned));
        for item in payload.results {
            if let Some(entry) = name_entry(root, item, filters) {
                entries.push(entry);
            }
        }
        if entries.len() > SEARCH_RESULT_LIMIT {
            break;
        }
    }

    if !saw_response {
        return Ok(None);
    }
    sort_global_name_entries(&mut entries, query);
    let mut seen = std::collections::HashSet::new();
    entries.retain(|entry| seen.insert(path_key(&entry.path)));
    let truncated = raw_total > raw_returned as u64 || entries.len() > SEARCH_RESULT_LIMIT;
    entries.truncate(SEARCH_RESULT_LIMIT);
    Ok(Some(GlobalNameSearchSnapshot {
        roots,
        query: query.to_string(),
        entries,
        truncated,
        partial: !covered || state_partial(&state),
    }))
}

pub fn search_content(
    roots: &[PathBuf],
    query: &str,
    regex: bool,
    max_files: usize,
    filters: SearchFilters,
) -> Result<Option<ContentSearchSnapshot>, String> {
    let started = Instant::now();
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    let roots = normalize_roots(roots);
    if roots.is_empty() {
        return Ok(None);
    }
    let max_files = max_files.clamp(1, crate::content_search::CONTENT_SEARCH_MAX_FILES);
    let cancel = content_cancel();
    cancel.store(false, Ordering::Relaxed);
    let Some(cli) = cli_path() else {
        return Ok(None);
    };
    let Some(state) = load_state(cli, Some(cancel))? else {
        return Ok(None);
    };
    let state_roots = state_roots(&state);
    let covered = roots_are_covered_by(&roots, &state_roots);
    let indexed_query = if regex {
        format!("re:\"{}\"", escape_query_value(query))
    } else {
        query.to_string()
    };
    let limit = max_files
        .saturating_mul(CONTENT_CANDIDATE_MULTIPLIER)
        .clamp(max_files, CONTENT_CANDIDATE_LIMIT);
    let mut files = Vec::new();
    let mut saw_response = false;
    let mut raw_returned = 0usize;
    let mut raw_total = 0u64;

    for root in &roots {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let args = vec![
            OsString::from("search"),
            OsString::from(&indexed_query),
            OsString::from("--json"),
            OsString::from("--limit"),
            OsString::from(limit.to_string()),
            OsString::from("--path"),
            root.as_os_str().to_os_string(),
        ];
        let Some(payload) =
            run_json::<KensakuSearchPayload>(cli, &args, Some(cancel), SEARCH_TIMEOUT)?
        else {
            continue;
        };
        saw_response = true;
        raw_returned = raw_returned.saturating_add(payload.results.len());
        raw_total = raw_total.saturating_add(payload.total.max(payload.returned));
        for item in payload.results {
            if let Some(file) = content_file(root, item, filters, query) {
                files.push(file);
            }
        }
        if files.len() >= max_files {
            break;
        }
    }

    if !saw_response || cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    files.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| right.modified_unix_ms.cmp(&left.modified_unix_ms))
            .then_with(|| left.path.cmp(&right.path))
    });
    let mut seen = std::collections::HashSet::new();
    files.retain(|file| seen.insert(path_key(&file.path)));
    let truncated = raw_total > raw_returned as u64 || files.len() > max_files;
    files.truncate(max_files);
    Ok(Some(ContentSearchSnapshot {
        query: query.to_string(),
        roots,
        files,
        index_state: content_state(&state),
        partial: !covered || state_partial(&state),
        truncated,
        scanned_documents: state.content_docs,
        total_matches: raw_total,
        duration_ms: started.elapsed().as_millis() as u64,
    }))
}

fn state_partial(state: &KensakuIndexState) -> bool {
    state.is_indexing || state.phase == "error"
}

fn content_state(state: &KensakuIndexState) -> ContentIndexState {
    if state.is_indexing {
        ContentIndexState::Indexing
    } else if state.phase == "error" {
        ContentIndexState::Error
    } else {
        ContentIndexState::Ready
    }
}

pub fn cancel_content_search() {
    content_cancel().store(true, Ordering::Relaxed);
}

pub fn content_search_cancelled() -> bool {
    content_cancel().load(Ordering::Relaxed)
}

fn content_cancel() -> &'static AtomicBool {
    CONTENT_CANCEL.get_or_init(|| AtomicBool::new(false))
}

fn ensure_daemon(roots: &[PathBuf]) -> Result<(), String> {
    let roots = normalize_roots(roots);
    if roots.is_empty() {
        return Err("没有可用的 Kensaku 搜索根目录".to_string());
    }
    let _cli = cli_path().ok_or_else(|| "找不到 kensaku-cli".to_string())?;
    let daemon = daemon_path()
        .ok_or_else(|| "找不到 kensaku-daemon；请运行 Kensaku sidecar 构建脚本".to_string())?;
    let data_dir = kensaku_data_dir();
    fs::create_dir_all(&data_dir).map_err(|error| format!("无法创建 Kensaku 数据目录：{error}"))?;
    let guard = DAEMON_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = guard
        .lock()
        .map_err(|_| "Kensaku daemon 锁不可用".to_string())?;
    let config_path = data_dir.join("config.json");
    let configured = config_roots(&config_path);
    let mut merged = configured.clone();
    for root in &roots {
        if !merged.iter().any(|existing| path_is_within(root, existing)) {
            merged.push(root.clone());
        }
    }
    let running = daemon_pid(&data_dir)
        .filter(|pid| daemon_process_is_running(*pid))
        .is_some()
        || daemon_child_is_running();
    if running && roots_are_covered_by(&roots, &configured) {
        return Ok(());
    }
    if running {
        stop_daemon(&data_dir)?;
    }
    write_kensaku_config(&data_dir, &merged)?;
    spawn_daemon(daemon, &data_dir)
}

fn config_roots(config_path: &Path) -> Vec<PathBuf> {
    let Ok(bytes) = fs::read(config_path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return Vec::new();
    };
    value
        .get("watched_dirs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(PathBuf::from)
        .collect()
}

fn write_kensaku_config(data_dir: &Path, roots: &[PathBuf]) -> Result<(), String> {
    fs::create_dir_all(data_dir).map_err(|error| format!("无法创建 Kensaku 数据目录：{error}"))?;
    let path = data_dir.join("config.json");
    let mut value = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let object = value
        .as_object_mut()
        .ok_or_else(|| "Kensaku 配置不是对象".to_string())?;
    object.insert(
        "watched_dirs".to_string(),
        Value::Array(
            roots
                .iter()
                .map(|root| Value::String(root.to_string_lossy().into_owned()))
                .collect(),
        ),
    );
    object.insert("is_setup_complete".to_string(), Value::Bool(true));
    object.insert("auto_update".to_string(), Value::Bool(false));
    let bytes = serde_json::to_vec_pretty(&value)
        .map_err(|error| format!("无法编码 Kensaku 配置：{error}"))?;
    AtomicFile::new(&path, AllowOverwrite)
        .write(|file| file.write_all(&bytes))
        .map_err(|error| format!("无法写入 Kensaku 配置：{error}"))
}

fn daemon_child_is_running() -> bool {
    let Some(slot) = DAEMON_CHILD.get() else {
        return false;
    };
    let Ok(mut slot) = slot.lock() else {
        return false;
    };
    let Some(child) = slot.as_mut() else {
        return false;
    };
    match child.try_wait() {
        Ok(None) => true,
        Ok(Some(_)) | Err(_) => {
            *slot = None;
            false
        }
    }
}

fn daemon_pid(data_dir: &Path) -> Option<u32> {
    fs::read_to_string(data_dir.join("daemon.pid"))
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
}

fn daemon_process_is_running(pid: u32) -> bool {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    system.process(pid).is_some_and(|process| {
        let name = process.name().to_string_lossy().to_lowercase();
        name.contains("kensaku-daemon")
            || process
                .exe()
                .and_then(Path::file_name)
                .map(|name| {
                    name.to_string_lossy()
                        .to_lowercase()
                        .contains("kensaku-daemon")
                })
                .unwrap_or(false)
    })
}

fn stop_daemon(data_dir: &Path) -> Result<(), String> {
    let pid = daemon_pid(data_dir);
    if let Some(child_slot) = DAEMON_CHILD.get()
        && let Ok(mut child) = child_slot.lock()
        && let Some(mut child) = child.take()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
    if let Some(pid) = pid
        && daemon_process_is_running(pid)
    {
        let process_id = Pid::from_u32(pid);
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::Some(&[process_id]), true);
        if let Some(process) = system.process(process_id) {
            let _ = process.kill();
        }
    }
    let deadline = Instant::now() + DAEMON_START_TIMEOUT;
    while Instant::now() < deadline {
        if pid.is_none_or(|pid| !daemon_process_is_running(pid)) {
            let _ = fs::remove_file(data_dir.join("daemon.pid"));
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err("无法停止旧的 Kensaku daemon".to_string())
}

fn spawn_daemon(daemon: &Path, data_dir: &Path) -> Result<(), String> {
    let stdout_path = data_dir.join("daemon.stdout.log");
    let stderr_path = data_dir.join("daemon.stderr.log");
    let stdout = fs::File::create(&stdout_path)
        .map_err(|error| format!("无法创建 Kensaku daemon 日志：{error}"))?;
    let stderr = fs::File::create(&stderr_path)
        .map_err(|error| format!("无法创建 Kensaku daemon 日志：{error}"))?;
    let mut command = make_base_command(daemon);
    command
        .env("KENSAKU_DATA_DIR", data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    let mut child = command
        .spawn()
        .map_err(|error| format!("无法启动 Kensaku daemon：{error}"))?;
    thread::sleep(Duration::from_millis(200));
    match child.try_wait() {
        Ok(Some(status)) if !status.success() => {
            let error = fs::read_to_string(&stderr_path).unwrap_or_default();
            Err(format!("Kensaku daemon 启动失败：{}", error.trim()))
        }
        Ok(Some(_)) => Ok(()),
        Ok(None) => {
            if let Ok(mut slot) = DAEMON_CHILD.get_or_init(|| Mutex::new(None)).lock() {
                *slot = Some(child);
            }
            Ok(())
        }
        Err(error) => Err(format!("无法检查 Kensaku daemon：{error}")),
    }
}

fn daemon_path() -> Option<&'static Path> {
    DAEMON_PATH
        .get_or_init(|| {
            let mut candidates = Vec::new();
            if let Some(path) = std::env::var_os("KITE_KENSAKU_DAEMON") {
                candidates.push(PathBuf::from(path));
            }
            if let Ok(executable) = std::env::current_exe()
                && let Some(parent) = executable.parent()
            {
                for name in ["kensaku-daemon.exe", "kensaku-daemon"] {
                    candidates.push(parent.join(name));
                }
            }
            candidates.push(PathBuf::from("kensaku-daemon.exe"));
            candidates.push(PathBuf::from("kensaku-daemon"));
            candidates
                .into_iter()
                .find(|candidate| command_works(candidate))
        })
        .as_deref()
}

fn kensaku_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("KITE_KENSAKU_DATA_DIR") {
        return PathBuf::from(path);
    }
    if let Some(path) = std::env::var_os("KENSAKU_DATA_DIR") {
        return PathBuf::from(path);
    }
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("Kite")
        .join("kensaku")
}

fn roots_are_covered_by(roots: &[PathBuf], indexed_roots: &[PathBuf]) -> bool {
    roots.iter().all(|root| {
        indexed_roots
            .iter()
            .any(|indexed| path_is_within(root, indexed))
    })
}

fn state_roots(state: &KensakuIndexState) -> Vec<PathBuf> {
    state.roots.iter().map(PathBuf::from).collect()
}

fn state_indexed_files(state: &KensakuIndexState) -> u64 {
    state.content_docs.max(state.files)
}

fn name_query(query: &str) -> String {
    format!("name:\"{}\"", escape_query_value(query))
}

fn escape_query_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn name_entry(root: &Path, item: KensakuSearchItem, filters: SearchFilters) -> Option<FileEntry> {
    let path = PathBuf::from(item.path);
    if !path_is_within(&path, root) {
        return None;
    }
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let metadata = fs::symlink_metadata(&path).ok()?;
    if name.is_empty()
        || is_hidden(&metadata, &name)
        || (metadata.is_dir() && is_generated_search_directory(&path))
        || has_generated_parent(&path, root)
        || !filters.matches(&path, metadata.is_dir(), metadata.modified().ok())
    {
        return None;
    }
    let mut entry = file_entry_from_metadata(path, name, &metadata);
    if let Some(size) = item.size {
        entry.size = size;
    }
    if let Some(modified) = timestamp_from_unix_seconds(item.modified) {
        entry.modified = Some(modified);
    }
    Some(entry)
}

fn content_file(
    root: &Path,
    item: KensakuSearchItem,
    filters: SearchFilters,
    query: &str,
) -> Option<ContentFileMatch> {
    let path = PathBuf::from(item.path);
    if !path_is_within(&path, root) {
        return None;
    }
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let metadata = fs::symlink_metadata(&path).ok()?;
    if name.is_empty()
        || is_hidden(&metadata, &name)
        || (metadata.is_dir() && is_generated_search_directory(&path))
        || has_generated_parent(&path, root)
        || !filters.matches(&path, metadata.is_dir(), metadata.modified().ok())
    {
        return None;
    }
    let is_content_match = item
        .match_type
        .as_deref()
        .map(|kind| !kind.eq_ignore_ascii_case("FileName"))
        .unwrap_or_else(|| item.snippet.is_some());
    if !is_content_match {
        return None;
    }
    let entry = file_entry_from_metadata(path.clone(), name.clone(), &metadata);
    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(&path)
        .to_string_lossy()
        .into_owned();
    let size = item.size.unwrap_or(metadata.len());
    let modified_unix_ms = item
        .modified
        .and_then(|seconds| seconds.checked_mul(1_000))
        .map(|value| value.min(i64::MAX as u64) as i64)
        .or_else(|| {
            metadata
                .modified()
                .ok()
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
        });
    let matches = item
        .snippet
        .as_deref()
        .map(|snippet| vec![best_effort_line_match(&path, snippet, query)])
        .unwrap_or_default();
    if matches.is_empty() {
        return None;
    }
    Some(ContentFileMatch {
        path,
        root: root.to_path_buf(),
        relative_path,
        name,
        kind: if metadata.is_dir() {
            EntryKind::Directory
        } else {
            EntryKind::File
        },
        kind_label: entry.type_label(),
        size,
        modified_unix_ms,
        score: item.score as f32,
        match_count: 1,
        matches,
    })
}

fn best_effort_line_match(path: &Path, snippet: &str, query: &str) -> ContentLineMatch {
    let snippet = snippet.trim();
    let fragment = snippet_fragment(snippet, query);
    if let Some(metadata) = fs::symlink_metadata(path).ok()
        && metadata.is_file()
        && metadata.len() <= SNIPPET_SCAN_BYTES
        && let Ok(bytes) = fs::read(path)
    {
        let text = String::from_utf8_lossy(&bytes);
        for (index, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if !fragment.is_empty() && lower.contains(&fragment) {
                let column = lower
                    .find(&fragment)
                    .map(|byte| lower[..byte].chars().count() as u64 + 1);
                return ContentLineMatch {
                    line_number: index as u64 + 1,
                    column_start: column,
                    column_end: column.map(|start| start + fragment.chars().count() as u64),
                    text: line.to_string(),
                };
            }
        }
    }
    ContentLineMatch {
        line_number: 1,
        column_start: None,
        column_end: None,
        text: snippet.to_string(),
    }
}

fn snippet_fragment(snippet: &str, query: &str) -> String {
    let candidate = snippet
        .trim()
        .trim_matches(|character: char| matches!(character, '.' | '…' | '·' | ' '))
        .to_lowercase();
    if !candidate.is_empty() {
        let fragment = candidate
            .split_whitespace()
            .max_by_key(|part| part.chars().count())
            .unwrap_or(candidate.as_str());
        if fragment.chars().count() >= 3 {
            return fragment.to_string();
        }
    }
    query
        .split_whitespace()
        .map(|part| part.trim_matches(':').to_lowercase())
        .max_by_key(|part| part.chars().count())
        .unwrap_or_default()
}

fn timestamp_from_unix_seconds(value: Option<u64>) -> Option<SystemTime> {
    value.and_then(|seconds| UNIX_EPOCH.checked_add(Duration::from_secs(seconds)))
}

fn normalize_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = roots
        .iter()
        .filter(|root| root.is_dir())
        .cloned()
        .collect::<Vec<_>>();
    roots.sort_by_key(|root| root.components().count());
    let mut result = Vec::<PathBuf>::new();
    let mut seen = std::collections::HashSet::new();
    for root in roots {
        let key = path_key(&root);
        if !seen.insert(key) || result.iter().any(|parent| path_is_within(&root, parent)) {
            continue;
        }
        result.push(root);
    }
    result
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    let path = path_key(path);
    let root = path_key(root);
    if path == root {
        return true;
    }
    let prefix = if root.ends_with(std::path::MAIN_SEPARATOR) {
        root
    } else {
        root + std::path::MAIN_SEPARATOR_STR
    };
    path.starts_with(&prefix)
}

fn path_key(path: &Path) -> String {
    let mut value = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        value = value.replace('/', "\\").to_lowercase();
        let length = value.trim_end_matches('\\').len();
        value.truncate(length);
        if value.ends_with(':') {
            value.push('\\');
        }
    } else if value != std::path::MAIN_SEPARATOR_STR {
        let length = value.trim_end_matches('/').len();
        value.truncate(length);
    }
    value
}

fn cli_path() -> Option<&'static Path> {
    CLI_PATH
        .get_or_init(|| {
            let mut candidates = Vec::new();
            if let Some(path) = std::env::var_os("KITE_KENSAKU_CLI") {
                candidates.push(PathBuf::from(path));
            }
            if let Ok(executable) = std::env::current_exe()
                && let Some(parent) = executable.parent()
            {
                for name in ["kensaku-cli.exe", "kensaku-cli"] {
                    candidates.push(parent.join(name));
                }
            }
            candidates.push(PathBuf::from("kensaku-cli.exe"));
            candidates.push(PathBuf::from("kensaku-cli"));
            candidates
                .into_iter()
                .find(|candidate| command_works(candidate))
        })
        .as_deref()
}

fn command_works(path: &Path) -> bool {
    make_base_command(path)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn make_base_command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

fn make_cli_command(path: &Path) -> Command {
    let mut command = make_base_command(path);
    command.env("KENSAKU_DATA_DIR", kensaku_data_dir());
    command
}

fn load_state(
    cli: &Path,
    cancel: Option<&AtomicBool>,
) -> Result<Option<KensakuIndexState>, String> {
    let cache = STATE_CACHE.get_or_init(|| Mutex::new(None));
    if let Ok(cache) = cache.lock()
        && let Some(cached) = cache.as_ref()
        && cached.cli == cli
        && cached.fetched_at.elapsed() < STATE_CACHE_TTL
    {
        return Ok(cached.state.clone());
    }
    let args = [OsString::from("state"), OsString::from("--json")];
    let state = run_json::<KensakuIndexState>(cli, &args, cancel, STATE_TIMEOUT)?;
    if let Ok(mut cache) = cache.lock() {
        *cache = Some(CachedState {
            cli: cli.to_path_buf(),
            fetched_at: Instant::now(),
            state: state.clone(),
        });
    }
    Ok(state)
}

fn run_json<T: DeserializeOwned>(
    cli: &Path,
    args: &[OsString],
    cancel: Option<&AtomicBool>,
    timeout: Duration,
) -> Result<Option<T>, String> {
    let mut child = make_cli_command(cli)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法启动 Kensaku CLI：{error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Kensaku CLI 标准输出不可用".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Kensaku CLI 标准错误不可用".to_string())?;
    let stdout_thread = thread::spawn(move || read_limited(stdout));
    let stderr_thread = thread::spawn(move || read_limited(stderr));
    let deadline = Instant::now() + timeout;
    let mut cancelled = false;
    let mut timed_out = false;
    let status = loop {
        if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
            cancelled = true;
            let _ = child.kill();
            break child.wait().ok();
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() >= deadline => {
                timed_out = true;
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(format!("无法等待 Kensaku CLI：{error}"));
            }
        }
    };
    let stdout = stdout_thread.join().unwrap_or_default();
    let stderr = stderr_thread.join().unwrap_or_default();
    if stdout.len() > MAX_COMMAND_OUTPUT_BYTES || stderr.len() > MAX_COMMAND_OUTPUT_BYTES {
        return Err("Kensaku CLI 输出超过上限".to_string());
    }
    if cancelled {
        return Ok(None);
    }
    if timed_out {
        return Err("Kensaku CLI 超时".to_string());
    }
    let status = status.ok_or_else(|| "Kensaku CLI 没有退出状态".to_string())?;
    if status.code() == Some(3) {
        return Ok(None);
    }
    if !status.success() {
        return Err(format!(
            "Kensaku CLI 失败：{}",
            String::from_utf8_lossy(&stderr).trim()
        ));
    }
    if stdout.is_empty() {
        return Ok(None);
    }
    serde_json::from_slice(&stdout)
        .map(Some)
        .map_err(|error| format!("无法解析 Kensaku CLI 输出：{error}"))
}

fn read_limited(mut reader: impl Read) -> Vec<u8> {
    let mut output = Vec::new();
    let _ = reader
        .by_ref()
        .take(MAX_COMMAND_OUTPUT_BYTES as u64 + 1)
        .read_to_end(&mut output);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_query_escapes_quotes_and_backslashes() {
        assert_eq!(name_query(r#"a"b\c"#), r#"name:"a\"b\\c""#);
    }

    #[test]
    fn coverage_accepts_requested_descendants() {
        let indexed = vec![PathBuf::from("workspace")];
        let roots = vec![indexed[0].join("projects")];
        assert!(roots_are_covered_by(&roots, &indexed));
    }

    #[test]
    fn coverage_rejects_unindexed_root() {
        let roots = vec![PathBuf::from("projects")];
        let indexed = vec![PathBuf::from("workspace")];
        assert!(!roots_are_covered_by(&roots, &indexed));
    }

    #[test]
    #[ignore = "requires KITE_KENSAKU_CLI, KITE_KENSAKU_DAEMON, KITE_KENSAKU_DATA_DIR, and KITE_KENSAKU_SMOKE_ROOT"]
    fn exercises_external_cli_contract() {
        let root = PathBuf::from(
            std::env::var("KITE_KENSAKU_SMOKE_ROOT").expect("smoke root environment"),
        );
        prepare_names(std::slice::from_ref(&root)).expect("start fake daemon");
        let config = kensaku_data_dir().join("config.json");
        let value: Value =
            serde_json::from_slice(&fs::read(config).expect("read config")).expect("parse config");
        assert_eq!(value["is_setup_complete"], true);
        let cancel = AtomicBool::new(false);
        let names = search_names(
            std::slice::from_ref(&root),
            "needle",
            SearchFilters::default(),
            &cancel,
        )
        .expect("name search")
        .expect("name snapshot");
        assert_eq!(names.entries.len(), 1);
        assert!(!names.partial);

        let content = search_content(
            std::slice::from_ref(&root),
            "needle",
            false,
            10,
            SearchFilters::default(),
        )
        .expect("content search")
        .expect("content snapshot");
        assert_eq!(content.files.len(), 1);
        assert_eq!(content.files[0].matches[0].line_number, 2);
        stop_daemon(&kensaku_data_dir()).expect("stop fake daemon");
    }

    #[test]
    fn parses_kensaku_search_payload() {
        let payload: KensakuSearchPayload = serde_json::from_str(
            r#"{
                "results": [{
                    "path": "C:\\Projects\\invoice.pdf",
                    "score": 0.9,
                    "snippet": "invoice total",
                    "match_type": "Both",
                    "size": 42,
                    "modified": 1700000000
                }],
                "total": 3,
                "returned": 1
            }"#,
        )
        .expect("parse payload");
        assert_eq!(payload.results.len(), 1);
        assert_eq!(payload.total, 3);
        assert_eq!(payload.results[0].size, Some(42));
    }
}
