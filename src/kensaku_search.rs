use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
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

#[cfg(windows)]
use std::fs::OpenOptions;

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
const DAEMON_STATE_TIMEOUT: Duration = Duration::from_millis(250);
const SEARCH_TIMEOUT: Duration = Duration::from_secs(60);
const CLI_SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_COMMAND_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const NAME_CANDIDATE_LIMIT: usize = 5_000;
const CONTENT_CANDIDATE_MULTIPLIER: usize = 8;
const CONTENT_CANDIDATE_LIMIT: usize = 400;
const SNIPPET_SCAN_BYTES: u64 = 8 * 1024 * 1024;
const DAEMON_START_TIMEOUT: Duration = Duration::from_secs(3);
const KENSAKU_PROTOCOL_VERSION: u64 = 1;
const KENSAKU_CONFIG_VERSION: u64 = 7;
const KENSAKU_DEFAULT_EXCLUDED_DIRS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    ".bzr",
    "node_modules",
    "vendor",
    "Pods",
    ".bundle",
    "target",
    "build",
    "dist",
    "out",
    ".output",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".vercel",
    ".netlify",
    ".cache",
    ".npm",
    ".yarn",
    ".pnpm",
    ".cargo",
    ".rustup",
    ".gradle",
    ".m2",
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    ".nox",
    "venv",
    ".venv",
    "env",
    ".env",
    ".vscode",
    ".idea",
    ".vs",
    ".eclipse",
    ".DS_Store",
    "Thumbs.db",
    "desktop.ini",
    ".Trash",
    "lost+found",
];
const KENSAKU_DEFAULT_EXCLUDED_EXTENSIONS: &[&str] = &[
    "exe", "dll", "so", "dylib", "bin", "o", "a", "lib", "class", "pyc", "pyo", "pem", "key",
    "pfx", "p12", "crt", "cer", "db", "sqlite", "sqlite3", "lock", "lockb", "min.js", "min.css",
    "map",
];

static CLI_PATH: OnceLock<PathBuf> = OnceLock::new();
static DAEMON_PATH: OnceLock<PathBuf> = OnceLock::new();
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
    files_indexed: u64,
    #[serde(default)]
    content_indexed: u64,
    #[serde(default)]
    extract_done: u64,
    #[serde(default)]
    extract_total: u64,
    #[serde(default)]
    is_indexing: bool,
    #[serde(default)]
    phase: String,
    #[serde(default)]
    paused: bool,
}

#[derive(Clone, Debug)]
struct CachedState {
    data_dir: PathBuf,
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
    let index_state = load_state(None, None).ok().flatten()?;
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
            Some(indexing_message(&index_state))
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
    let cli = cli_path();
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
        let Some(payload) = search_payload_for_root(
            cli,
            &indexed_query,
            root,
            NAME_CANDIDATE_LIMIT,
            false,
            cancel,
        )?
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
    let cli = cli_path();
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
        let Some(payload) =
            search_payload_for_root(cli, &indexed_query, root, limit, true, cancel)?
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

    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    if !saw_response {
        if state.is_indexing {
            return Ok(Some(ContentSearchSnapshot {
                query: query.to_string(),
                roots,
                files: Vec::new(),
                index_state: ContentIndexState::Indexing,
                partial: true,
                truncated: false,
                scanned_documents: state.content_docs,
                total_matches: 0,
                duration_ms: started.elapsed().as_millis() as u64,
            }));
        }
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
    let guard = DAEMON_LOCK.get_or_init(|| Mutex::new(()));
    let _guard = guard
        .lock()
        .map_err(|_| "Kensaku daemon 锁不可用".to_string())?;
    #[cfg(all(target_os = "windows", not(test)))]
    if cli_path().is_none() || daemon_path().is_none() {
        crate::updater::repair_windows_sidecars()
            .map_err(|error| format!("Kensaku sidecar 缺失，自动安装失败：{error}"))?;
    }
    let _cli = cli_path().ok_or_else(|| "找不到 kensaku-cli".to_string())?;
    let daemon = daemon_path()
        .ok_or_else(|| "找不到 kensaku-daemon；请运行 Kensaku sidecar 构建脚本".to_string())?;
    let data_dir = kensaku_data_dir();
    fs::create_dir_all(&data_dir).map_err(|error| format!("无法创建 Kensaku 数据目录：{error}"))?;
    let config_path = data_dir.join("config.json");
    let configured = config_roots(&config_path);
    let mut merged = configured.clone();
    for root in &roots {
        if !merged.iter().any(|existing| path_is_within(root, existing)) {
            merged.push(root.clone());
        }
    }
    let running = daemon_is_running(&data_dir);
    let config_valid = kensaku_config_is_valid(&config_path);
    if !config_valid {
        if running {
            stop_daemon(&data_dir)?;
        }
        write_kensaku_config(&data_dir, &merged)?;
        return spawn_daemon(daemon, &data_dir);
    }
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

fn kensaku_config_is_valid(path: &Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    let Some(object) = value.as_object() else {
        return false;
    };
    object.get("watched_dirs").is_some_and(Value::is_array)
        && object.get("excluded_dirs").is_some_and(Value::is_array)
        && object
            .get("excluded_extensions")
            .is_some_and(Value::is_array)
        && object.get("shortcut").is_some_and(Value::is_string)
        && object.get("max_results").is_some_and(Value::is_u64)
        && object.get("config_version").is_some_and(Value::is_u64)
        && object
            .get("is_setup_complete")
            .is_some_and(|value| value.as_bool() == Some(true))
}

fn json_string_array(values: &[&str]) -> Value {
    Value::Array(
        values
            .iter()
            .map(|value| Value::String((*value).to_string()))
            .collect(),
    )
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
    if !object.get("excluded_dirs").is_some_and(Value::is_array) {
        object.insert(
            "excluded_dirs".to_string(),
            json_string_array(KENSAKU_DEFAULT_EXCLUDED_DIRS),
        );
    }
    if !object
        .get("excluded_extensions")
        .is_some_and(Value::is_array)
    {
        object.insert(
            "excluded_extensions".to_string(),
            json_string_array(KENSAKU_DEFAULT_EXCLUDED_EXTENSIONS),
        );
    }
    if !object.get("shortcut").is_some_and(Value::is_string) {
        object.insert(
            "shortcut".to_string(),
            Value::String("Alt+Space".to_string()),
        );
    }
    if !object.get("max_results").is_some_and(Value::is_u64) {
        object.insert("max_results".to_string(), Value::Number(50.into()));
    }
    if !object.get("config_version").is_some_and(Value::is_u64) {
        object.insert(
            "config_version".to_string(),
            Value::Number(KENSAKU_CONFIG_VERSION.into()),
        );
    }
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

fn daemon_is_running(data_dir: &Path) -> bool {
    daemon_pid(data_dir).is_some_and(daemon_process_is_running) || daemon_child_is_running()
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
    if let Some(path) = DAEMON_PATH.get() {
        return Some(path.as_path());
    }
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
    // The daemon ignores --help and enters server mode, so discovery must not run it.
    let found = find_daemon_candidate(candidates)?;
    let _ = DAEMON_PATH.set(found);
    DAEMON_PATH.get().map(|path| path.as_path())
}

fn find_daemon_candidate(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|candidate| candidate.is_file())
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
    state
        .content_docs
        .max(state.files)
        .max(state.files_indexed)
        .max(state.content_indexed)
}

fn indexing_message(state: &KensakuIndexState) -> String {
    if state.paused {
        return "Kensaku 索引已暂停".to_string();
    }
    match state.phase.as_str() {
        "starting" => "Kensaku 正在连接索引服务".to_string(),
        "walking" if state_indexed_files(state) == 0 => {
            format!("Kensaku 正在扫描 {} 个位置", state.roots.len().max(1))
        }
        "walking" => format!("Kensaku 正在建立索引：{} 项", state_indexed_files(state)),
        "extracting" if state.extract_total > 0 => format!(
            "Kensaku 正在读取内容：{}/{}",
            state.extract_done, state.extract_total
        ),
        "committing" => format!("Kensaku 正在提交索引：{} 项", state_indexed_files(state)),
        _ => format!("Kensaku 正在建立索引：{} 项", state_indexed_files(state)),
    }
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
    if let Some(path) = CLI_PATH.get() {
        return Some(path.as_path());
    }
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
    let found = candidates
        .into_iter()
        .find(|candidate| command_works(candidate))?;
    let _ = CLI_PATH.set(found);
    CLI_PATH.get().map(|path| path.as_path())
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

trait DaemonReadWrite: Read + Write {}

impl<T: Read + Write> DaemonReadWrite for T {}

#[cfg(windows)]
fn open_daemon_stream(_data_dir: &Path) -> Result<Box<dyn DaemonReadWrite>, String> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(r"\\.\pipe\kensaku")
        .map(|stream| Box::new(stream) as Box<dyn DaemonReadWrite>)
        .map_err(|error| format!("无法连接 Kensaku daemon：{error}"))
}

#[cfg(unix)]
fn open_daemon_stream(data_dir: &Path) -> Result<Box<dyn DaemonReadWrite>, String> {
    use std::os::unix::net::UnixStream;

    let socket_root = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.to_path_buf());
    let stream = UnixStream::connect(socket_root.join("kensaku.sock"))
        .map_err(|error| format!("无法连接 Kensaku daemon：{error}"))?;
    stream
        .set_read_timeout(Some(SEARCH_TIMEOUT))
        .map_err(|error| format!("无法设置 Kensaku daemon 读取超时：{error}"))?;
    stream
        .set_write_timeout(Some(SEARCH_TIMEOUT))
        .map_err(|error| format!("无法设置 Kensaku daemon 写入超时：{error}"))?;
    Ok(Box::new(stream))
}

fn daemon_request(method: &str, params: Value) -> Result<Value, String> {
    let data_dir = kensaku_data_dir();
    let token = fs::read_to_string(data_dir.join("daemon.token"))
        .map_err(|error| format!("无法读取 Kensaku daemon token：{error}"))?;
    let token = token.trim();
    if token.is_empty() {
        return Err("Kensaku daemon token 为空".to_string());
    }
    let Value::Object(mut params) = params else {
        return Err("Kensaku daemon 请求参数不是对象".to_string());
    };
    params.insert("token".to_string(), Value::String(token.to_string()));
    params.insert(
        "protocol_version".to_string(),
        Value::from(KENSAKU_PROTOCOL_VERSION),
    );
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let mut stream = open_daemon_stream(&data_dir)?;
    writeln!(stream, "{request}").map_err(|error| format!("无法写入 Kensaku daemon：{error}"))?;
    stream
        .flush()
        .map_err(|error| format!("无法刷新 Kensaku daemon 请求：{error}"))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("无法读取 Kensaku daemon 响应：{error}"))?;
    if line.len() > MAX_COMMAND_OUTPUT_BYTES {
        return Err("Kensaku daemon 响应超过上限".to_string());
    }
    let response: Value = serde_json::from_str(&line)
        .map_err(|error| format!("无法解析 Kensaku daemon 响应：{error}"))?;
    if let Some(error) = response.get("error") {
        return Err(error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Kensaku daemon 返回错误")
            .to_string());
    }
    Ok(response.get("result").cloned().unwrap_or(Value::Null))
}

fn daemon_request_cancellable(
    method: &str,
    params: Value,
    cancel: &AtomicBool,
    timeout: Duration,
) -> Result<Option<Value>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    let method = method.to_string();
    let (sender, receiver) = std::sync::mpsc::channel();
    let _ = thread::spawn(move || {
        let _ = sender.send(daemon_request(&method, params));
    });
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Kensaku daemon 请求超时".to_string());
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(result) => return result.map(Some),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                return Err("Kensaku daemon 请求线程已结束".to_string());
            }
        }
    }
}

fn search_payload_for_root(
    cli: Option<&Path>,
    query: &str,
    root: &Path,
    limit: usize,
    snippets: bool,
    cancel: &AtomicBool,
) -> Result<Option<KensakuSearchPayload>, String> {
    if cancel.load(Ordering::Relaxed) {
        return Ok(None);
    }
    match daemon_request_cancellable(
        "search",
        json!({
            "query": scoped_query(query, root),
            "limit": limit,
            "offset": 0,
            "snippets": snippets,
        }),
        cancel,
        SEARCH_TIMEOUT,
    ) {
        Ok(Some(value)) => {
            if let Ok(payload) = serde_json::from_value::<KensakuSearchPayload>(value) {
                return Ok(Some(payload));
            }
        }
        Ok(None) => return Ok(None),
        Err(_) if daemon_is_running(&kensaku_data_dir()) => return Ok(None),
        Err(_) => {}
    }
    let Some(cli) = cli else {
        return Ok(None);
    };
    let args = vec![
        OsString::from("search"),
        OsString::from(query),
        OsString::from("--json"),
        OsString::from("--limit"),
        OsString::from(limit.to_string()),
        OsString::from("--path"),
        root.as_os_str().to_os_string(),
    ];
    run_json::<KensakuSearchPayload>(cli, &args, Some(cancel), CLI_SEARCH_TIMEOUT)
}

fn scoped_query(query: &str, root: &Path) -> String {
    format!(
        "{} in:\"{}\"",
        query.trim(),
        escape_query_value(&root.to_string_lossy())
    )
}

fn cli_index_state(cli: Option<&Path>, cancel: Option<&AtomicBool>) -> Option<KensakuIndexState> {
    let cli = cli?;
    let args = [OsString::from("state"), OsString::from("--json")];
    run_json::<KensakuIndexState>(cli, &args, cancel, STATE_TIMEOUT)
        .ok()
        .flatten()
}

fn provisional_index_state(data_dir: &Path) -> KensakuIndexState {
    KensakuIndexState {
        roots: config_roots(&data_dir.join("config.json"))
            .iter()
            .map(|root| root.to_string_lossy().into_owned())
            .collect(),
        files: 0,
        content_docs: 0,
        files_indexed: 0,
        content_indexed: 0,
        extract_done: 0,
        extract_total: 0,
        is_indexing: true,
        phase: "starting".to_string(),
        paused: false,
    }
}

fn load_state(
    cli: Option<&Path>,
    cancel: Option<&AtomicBool>,
) -> Result<Option<KensakuIndexState>, String> {
    if cancel.is_some_and(|cancel| cancel.load(Ordering::Relaxed)) {
        return Ok(None);
    }
    let data_dir = kensaku_data_dir();
    let cache = STATE_CACHE.get_or_init(|| Mutex::new(None));
    let stale_state = cache.lock().ok().and_then(|cache| {
        cache
            .as_ref()
            .filter(|cached| cached.data_dir == data_dir)
            .and_then(|cached| cached.state.clone())
    });
    if let Ok(cache) = cache.lock()
        && let Some(cached) = cache.as_ref()
        && cached.data_dir == data_dir
        && cached.fetched_at.elapsed() < STATE_CACHE_TTL
    {
        return Ok(cached.state.clone());
    }
    let no_cancel = AtomicBool::new(false);
    let request_cancel = cancel.unwrap_or(&no_cancel);
    let daemon_state = match daemon_request_cancellable(
        "index_state",
        json!({}),
        request_cancel,
        DAEMON_STATE_TIMEOUT,
    ) {
        Ok(Some(value)) => serde_json::from_value::<KensakuIndexState>(value).ok(),
        Ok(None) => return Ok(None),
        Err(_) => None,
    };
    let daemon_running = daemon_is_running(&data_dir);
    let state = daemon_state.or_else(|| {
        if daemon_running {
            let mut state = stale_state.unwrap_or_else(|| provisional_index_state(&data_dir));
            state.is_indexing = true;
            if state.phase.is_empty() {
                state.phase = "starting".to_string();
            }
            Some(state)
        } else {
            cli_index_state(cli, cancel).or(stale_state)
        }
    });
    if let Ok(mut cache) = cache.lock() {
        *cache = Some(CachedState {
            data_dir,
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
    fn daemon_scope_query_escapes_path() {
        assert_eq!(
            scoped_query(r#"name:"needle""#, Path::new(r#"C:\A "B""#)),
            r#"name:"needle" in:"C:\\A \"B\"""#
        );
    }

    #[test]
    fn parses_live_daemon_index_progress() {
        let state: KensakuIndexState = serde_json::from_str(
            r#"{
                "files": 0,
                "dirs": 0,
                "content_docs": 0,
                "roots": ["C:\\", "D:\\"],
                "is_indexing": true,
                "phase": "walking",
                "files_indexed": 0,
                "content_indexed": 0,
                "extract_done": 0,
                "extract_total": 0
            }"#,
        )
        .expect("parse live daemon state");
        assert!(state.is_indexing);
        assert_eq!(state.roots.len(), 2);
        assert_eq!(indexing_message(&state), "Kensaku 正在扫描 2 个位置");
    }

    #[test]
    #[ignore = "requires a running Kensaku daemon"]
    fn exercises_live_daemon_contract() {
        let state = daemon_request("index_state", json!({})).expect("read live daemon state");
        assert!(state.get("is_indexing").is_some());
        let root = state["roots"]
            .as_array()
            .and_then(|roots| roots.first())
            .and_then(Value::as_str)
            .expect("daemon root");
        let response = daemon_request(
            "search",
            json!({
                "query": scoped_query("name:\"hello\"", Path::new(root)),
                "limit": 10,
                "offset": 0,
                "snippets": false,
            }),
        )
        .expect("live daemon search");
        assert!(response.get("results").is_some());
        let names = search_names(
            &[PathBuf::from(root)],
            "hello",
            SearchFilters::default(),
            &AtomicBool::new(false),
        )
        .expect("Kite live search")
        .expect("Kite live snapshot");
        assert!(!names.entries.is_empty());
    }

    #[test]
    fn daemon_candidate_discovery_does_not_execute_file() {
        let directory = std::env::temp_dir().join(format!(
            "kite-daemon-discovery-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create daemon discovery test directory");
        let candidate = directory.join(if cfg!(windows) {
            "kensaku-daemon.exe"
        } else {
            "kensaku-daemon"
        });
        fs::write(&candidate, b"fixture").expect("write daemon discovery fixture");
        let found = find_daemon_candidate(vec![candidate.clone()]);
        assert_eq!(found, Some(candidate));
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn repairs_legacy_config_with_required_kensaku_fields() {
        let directory = std::env::temp_dir().join(format!(
            "kite-kensaku-config-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).expect("create config test directory");
        fs::write(
            directory.join("config.json"),
            br#"{"watched_dirs":["C:\\"],"is_setup_complete":true,"auto_update":false}"#,
        )
        .expect("write legacy config");
        assert!(!kensaku_config_is_valid(&directory.join("config.json")));
        write_kensaku_config(&directory, &[PathBuf::from("C:\\")]).expect("repair legacy config");
        let value: Value = serde_json::from_slice(
            &fs::read(directory.join("config.json")).expect("read repaired config"),
        )
        .expect("parse repaired config");
        assert!(
            value["excluded_dirs"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert!(
            value["excluded_extensions"]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        );
        assert_eq!(value["shortcut"], "Alt+Space");
        assert_eq!(value["max_results"], 50);
        assert_eq!(value["config_version"], KENSAKU_CONFIG_VERSION);
        assert_eq!(value["is_setup_complete"], true);
        assert!(kensaku_config_is_valid(&directory.join("config.json")));
        let _ = fs::remove_dir_all(&directory);
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
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline {
            if load_state(None, None)
                .ok()
                .flatten()
                .is_some_and(|state| !state.is_indexing)
            {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
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
