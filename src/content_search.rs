use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dunce::canonicalize;
use grep_matcher::Matcher;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::{BinaryDetection, SearcherBuilder, Sink, SinkFinish, SinkMatch};
use ignore::{WalkBuilder, WalkState};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use sysinfo::{Disks, System};
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, PhraseQuery, Query, RegexQuery};
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TEXT, TantivyDocument, TextFieldIndexing,
    TextOptions, Value,
};
use tantivy::tokenizer::{SimpleTokenizer, TokenStream, Tokenizer};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};
use thread_priority::{ThreadBuilderExt, ThreadPriority};

use crate::fs::{EntryKind, search_path_quality_rank, type_label};

pub const CONTENT_SEARCH_MAX_FILES: usize = 50;
pub const CONTENT_SEARCH_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
pub const CONTENT_SEARCH_MAX_INDEXED_FILES: usize = 250_000;

const INDEX_CANDIDATE_LIMIT: usize = 200;
const MATCH_SNIPPETS_PER_FILE: usize = 3;
const MAX_MATCHES_PER_FILE: u64 = 10_000;
const INDEX_WRITER_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const COMMIT_DOCUMENT_INTERVAL: usize = 100;
const COMMIT_TIME_INTERVAL: Duration = Duration::from_secs(1);
const LIVE_PREVIEW_BUDGET: Duration = Duration::from_millis(350);
const REGEX_PREVIEW_BUDGET: Duration = Duration::from_millis(500);
const INTERACTION_PAUSE: Duration = Duration::from_millis(650);
const RESOURCE_CHECK_INTERVAL: Duration = Duration::from_secs(15);
const MIN_AVAILABLE_MEMORY_BYTES: u64 = 128 * 1024 * 1024;
const MIN_AVAILABLE_DISK_BYTES: u64 = 256 * 1024 * 1024;
const TEXT_PREVIEW_CHARS: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentIndexState {
    Idle,
    Indexing,
    Ready,
    Error,
}

impl ContentIndexState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Indexing => "indexing",
            Self::Ready => "ready",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContentIndexStatus {
    pub state: ContentIndexState,
    pub indexed_files: u64,
    pub message: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContentLineMatch {
    pub line_number: u64,
    pub column_start: Option<u64>,
    pub column_end: Option<u64>,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct ContentFileMatch {
    pub path: PathBuf,
    pub root: PathBuf,
    pub relative_path: String,
    pub name: String,
    pub kind: EntryKind,
    pub kind_label: String,
    pub size: u64,
    pub modified_unix_ms: Option<i64>,
    pub score: f32,
    pub match_count: u64,
    pub matches: Vec<ContentLineMatch>,
}

#[derive(Clone, Debug)]
pub struct ContentSearchSnapshot {
    pub query: String,
    pub roots: Vec<PathBuf>,
    pub files: Vec<ContentFileMatch>,
    pub index_state: ContentIndexState,
    pub partial: bool,
    pub truncated: bool,
    pub scanned_documents: u64,
    pub total_matches: u64,
    pub duration_ms: u64,
}

#[derive(Clone)]
struct ContentFields {
    path_key: Field,
    path: Field,
    root_key: Field,
    root: Field,
    name: Field,
    size: Field,
    modified_ms: Field,
    body: Field,
}

#[derive(Clone, Debug)]
struct IndexedMeta {
    path: PathBuf,
    root_key: String,
    size: u64,
    modified_ms: i64,
}

enum IndexCommand {
    EnsureRoot(PathBuf),
    PathChanged { path: PathBuf, root: PathBuf },
    Clear,
}

struct IndexResourceGuard {
    system: System,
    disks: Disks,
    last_check: Instant,
    blocked: bool,
}

impl IndexResourceGuard {
    fn new(index_dir: &Path) -> Self {
        let mut guard = Self {
            system: System::new(),
            disks: Disks::new_with_refreshed_list(),
            last_check: Instant::now() - RESOURCE_CHECK_INTERVAL,
            blocked: false,
        };
        guard.refresh(index_dir);
        guard
    }

    fn should_pause(&mut self, index_dir: &Path) -> bool {
        if self.last_check.elapsed() >= RESOURCE_CHECK_INTERVAL {
            self.refresh(index_dir);
        }
        self.blocked
    }

    fn refresh(&mut self, index_dir: &Path) {
        self.system.refresh_memory();
        self.disks.refresh(true);
        let low_memory = self.system.available_memory() < MIN_AVAILABLE_MEMORY_BYTES;
        let index_path = index_dir
            .canonicalize()
            .unwrap_or_else(|_| index_dir.to_path_buf());
        let low_disk = self
            .disks
            .list()
            .iter()
            .filter(|disk| index_path.starts_with(disk.mount_point()))
            .max_by_key(|disk| disk.mount_point().components().count())
            .is_some_and(|disk| disk.available_space() < MIN_AVAILABLE_DISK_BYTES);
        self.blocked = low_memory || low_disk;
        self.last_check = Instant::now();
    }
}

pub struct ContentSearchService {
    index_dir: PathBuf,
    schema: ContentFields,
    reader: IndexReader,
    command_tx: mpsc::SyncSender<IndexCommand>,
    status: Arc<Mutex<ContentIndexStatus>>,
    watched_roots: Arc<Mutex<BTreeSet<PathBuf>>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    interactive_until_ms: Arc<AtomicU64>,
    indexing_paused: Arc<AtomicBool>,
    search_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl ContentSearchService {
    pub fn new() -> Result<Self, String> {
        Self::open_at(default_index_dir())
    }

    #[cfg(feature = "performance")]
    pub fn open_for_performance(index_dir: PathBuf) -> Result<Self, String> {
        Self::open_at(index_dir)
    }

    fn open_at(index_dir: PathBuf) -> Result<Self, String> {
        let (index, schema) = open_or_create_index(&index_dir)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|error| format!("无法打开内容搜索索引：{error}"))?;
        let initial_count = reader.searcher().num_docs();
        let status = Arc::new(Mutex::new(ContentIndexStatus {
            state: ContentIndexState::Idle,
            indexed_files: initial_count,
            message: None,
        }));
        let interactive_until_ms = Arc::new(AtomicU64::new(0));
        let indexing_paused = Arc::new(AtomicBool::new(false));
        let (command_tx, command_rx) = mpsc::sync_channel(1024);

        {
            let index = index.clone();
            let schema = schema.clone();
            let reader = reader.clone();
            let status = Arc::clone(&status);
            let interactive_until_ms = Arc::clone(&interactive_until_ms);
            let indexing_paused = Arc::clone(&indexing_paused);
            let index_dir = index_dir.clone();
            thread::Builder::new()
                .name("kite-content-index".to_string())
                .spawn_with_priority(ThreadPriority::Min, move |_priority| {
                    index_worker(
                        index,
                        schema,
                        reader,
                        command_rx,
                        status,
                        interactive_until_ms,
                        indexing_paused,
                        index_dir,
                    );
                })
                .map_err(|error| format!("无法启动内容索引线程：{error}"))?;
        }

        let watched_roots = Arc::new(Mutex::new(BTreeSet::new()));
        let watcher = {
            let command_tx = command_tx.clone();
            let watched_roots = Arc::clone(&watched_roots);
            match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else {
                    return;
                };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                let roots = watched_roots
                    .lock()
                    .map(|roots| roots.iter().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                for path in event.paths {
                    let root = find_root_for_path(&path, &roots).unwrap_or_else(|| path.clone());
                    let _ = command_tx.try_send(IndexCommand::PathChanged { path, root });
                }
            }) {
                Ok(watcher) => Some(watcher),
                Err(error) => {
                    eprintln!("content search watcher unavailable: {error}");
                    None
                }
            }
        };

        Ok(Self {
            index_dir,
            schema,
            reader,
            command_tx,
            status,
            watched_roots,
            watcher: Mutex::new(watcher),
            interactive_until_ms,
            indexing_paused,
            search_cancel: Mutex::new(None),
        })
    }

    pub fn ensure_roots(&self, roots: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
        let roots = normalize_roots(roots)?;
        let mut watched = self
            .watched_roots
            .lock()
            .map_err(|_| "内容搜索范围锁不可用".to_string())?;
        let mut watcher = self
            .watcher
            .lock()
            .map_err(|_| "内容搜索监听器锁不可用".to_string())?;
        for root in &roots {
            if watched.insert(root.clone()) {
                if let Some(watcher) = watcher.as_mut() {
                    let _ = watcher.watch(root, RecursiveMode::Recursive);
                }
                let _ = self.command_tx.send(IndexCommand::EnsureRoot(root.clone()));
            }
        }
        Ok(roots)
    }

    pub fn status(&self) -> ContentIndexStatus {
        let mut status =
            self.status
                .lock()
                .map(|status| status.clone())
                .unwrap_or(ContentIndexStatus {
                    state: ContentIndexState::Error,
                    indexed_files: 0,
                    message: Some("内容索引状态不可用".to_string()),
                });
        status.indexed_files = self.reader.searcher().num_docs();
        status
    }

    pub fn note_interaction(&self) {
        self.interactive_until_ms.store(
            now_millis() + INTERACTION_PAUSE.as_millis() as u64,
            Ordering::Relaxed,
        );
    }

    pub fn set_indexing_paused(&self, paused: bool) {
        self.indexing_paused.store(paused, Ordering::Relaxed);
    }

    pub fn clear(&self) -> Result<(), String> {
        self.command_tx
            .send(IndexCommand::Clear)
            .map_err(|_| "内容索引任务不可用".to_string())
    }

    pub fn cancel_search(&self) {
        if let Ok(active) = self.search_cancel.lock()
            && let Some(cancel) = active.as_ref()
        {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn search(
        &self,
        roots: &[PathBuf],
        query: &str,
        regex: bool,
        max_files: usize,
    ) -> Result<Option<ContentSearchSnapshot>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(None);
        }
        self.note_interaction();
        let roots = self.ensure_roots(roots)?;
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = self
                .search_cancel
                .lock()
                .map_err(|_| "内容搜索状态锁不可用".to_string())?;
            *active = Some(Arc::clone(&cancel));
        }

        let started = Instant::now();
        let result = self.search_inner(&roots, query, regex, max_files, cancel.as_ref(), started);
        if let Ok(mut active) = self.search_cancel.lock()
            && active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &cancel))
        {
            *active = None;
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        result.map(Some)
    }

    fn search_inner(
        &self,
        roots: &[PathBuf],
        query: &str,
        regex: bool,
        max_files: usize,
        cancel: &AtomicBool,
        started: Instant,
    ) -> Result<ContentSearchSnapshot, String> {
        let max_files = max_files.clamp(1, CONTENT_SEARCH_MAX_FILES);
        let status = self.status();
        let mut files = Vec::new();
        let mut partial = status.state != ContentIndexState::Ready;
        let mut truncated = false;

        if regex {
            validate_query(query, true)?;
        } else if query.chars().count() >= 2 {
            files = self.search_index(roots, query, max_files, cancel)?;
        }

        let needs_preview = regex || files.is_empty();
        if needs_preview {
            let budget = if regex {
                REGEX_PREVIEW_BUDGET
            } else {
                LIVE_PREVIEW_BUDGET
            };
            let preview = live_search(
                roots,
                query,
                regex,
                max_files,
                started + budget,
                cancel,
                &self.index_dir,
            )?;
            partial |= preview.partial;
            truncated |= preview.truncated;
            merge_files(&mut files, preview.files);
        }

        let mut hydrated = Vec::new();
        let hydrate_limit = (max_files * 2).min(files.len());
        truncated |= files.len() > hydrate_limit;
        for mut file in files.into_iter().take(hydrate_limit) {
            if cancel.load(Ordering::Relaxed) {
                return Err("内容搜索已取消".to_string());
            }
            if file.matches.is_empty() {
                let Ok(metadata) = fs::metadata(&file.path) else {
                    let _ = self.command_tx.try_send(IndexCommand::PathChanged {
                        path: file.path.clone(),
                        root: file.root.clone(),
                    });
                    continue;
                };
                if !metadata.is_file() || metadata.len() > CONTENT_SEARCH_MAX_FILE_BYTES {
                    continue;
                }
                let Ok((count, matches)) = search_file_matches(&file.path, query, regex, cancel)
                else {
                    continue;
                };
                if count == 0 {
                    let _ = self.command_tx.try_send(IndexCommand::PathChanged {
                        path: file.path.clone(),
                        root: file.root.clone(),
                    });
                    continue;
                }
                file.match_count = count;
                file.matches = matches;
            }
            if file.match_count > 0 && !file.matches.is_empty() {
                hydrated.push(file);
            }
            if hydrated.len() >= max_files {
                truncated = true;
                break;
            }
        }

        hydrated.sort_by(compare_content_files);
        let total_matches = hydrated.iter().map(|file| file.match_count).sum();
        Ok(ContentSearchSnapshot {
            query: query.to_string(),
            roots: roots.to_vec(),
            files: hydrated,
            index_state: status.state,
            partial,
            truncated,
            scanned_documents: status.indexed_files,
            total_matches,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    fn search_index(
        &self,
        roots: &[PathBuf],
        query: &str,
        max_files: usize,
        cancel: &AtomicBool,
    ) -> Result<Vec<ContentFileMatch>, String> {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }
        let searcher = self.reader.searcher();
        let content_query: Box<dyn Query> = content_query(query, self.schema.body)?;
        let mut clauses: Vec<(Occur, Box<dyn Query>)> = vec![(Occur::Must, content_query)];
        let root_queries = roots
            .iter()
            .map(|root| {
                let prefix = regex::escape(&path_key(root));
                let separator = regex::escape(std::path::MAIN_SEPARATOR_STR);
                RegexQuery::from_pattern(&format!("{prefix}{separator}.*"), self.schema.path_key)
            })
            .map(|query| {
                query
                    .map(|query| Box::new(query) as Box<dyn Query>)
                    .map_err(|error| format!("无法生成搜索范围查询：{error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !root_queries.is_empty() {
            clauses.push((
                Occur::Must,
                Box::new(BooleanQuery::new(
                    root_queries
                        .into_iter()
                        .map(|query| (Occur::Should, query))
                        .collect(),
                )),
            ));
        }
        let query = BooleanQuery::new(clauses);
        let limit = (max_files * 4).min(INDEX_CANDIDATE_LIMIT);
        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limit).order_by_score())
            .map_err(|error| format!("无法查询内容索引：{error}"))?;

        let mut files = Vec::new();
        for (score, address) in top_docs {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let document = searcher
                .doc::<TantivyDocument>(address)
                .map_err(|error| format!("无法读取内容索引文档：{error}"))?;
            let Some(path) = text_value(&document, self.schema.path) else {
                continue;
            };
            let Some(root) = text_value(&document, self.schema.root) else {
                continue;
            };
            let path = PathBuf::from(path);
            let root = PathBuf::from(root);
            if !roots
                .iter()
                .any(|candidate| is_path_within(&path, candidate))
            {
                continue;
            }
            let name = text_value(&document, self.schema.name)
                .unwrap_or_else(|| path.display().to_string());
            let size = document
                .get_first(self.schema.size)
                .and_then(|value| value.as_u64())
                .unwrap_or_default();
            let modified_unix_ms = document
                .get_first(self.schema.modified_ms)
                .and_then(|value| value.as_i64())
                .filter(|value| *value > 0);
            let extension = path
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            files.push(ContentFileMatch {
                relative_path: relative_path(&path, &root),
                path,
                root,
                name,
                kind: EntryKind::File,
                kind_label: type_label(EntryKind::File, &extension),
                size,
                modified_unix_ms,
                score,
                match_count: 0,
                matches: Vec::new(),
            });
        }
        Ok(files)
    }
}

pub fn shared_service() -> Result<&'static Arc<ContentSearchService>, String> {
    static SERVICE: OnceLock<Result<Arc<ContentSearchService>, String>> = OnceLock::new();
    SERVICE
        .get_or_init(|| ContentSearchService::new().map(Arc::new))
        .as_ref()
        .map_err(Clone::clone)
}

fn default_index_dir() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("Kite")
        .join("search-index-v1")
}

fn open_or_create_index(index_dir: &Path) -> Result<(Index, ContentFields), String> {
    let schema = content_schema();
    let fields = ContentFields::from_schema(&schema)?;
    if index_dir.join("meta.json").is_file() {
        let index =
            Index::open_in_dir(index_dir).map_err(|error| format!("无法打开内容索引：{error}"))?;
        let fields = ContentFields::from_schema(&index.schema())?;
        return Ok((index, fields));
    }
    fs::create_dir_all(index_dir).map_err(|error| format!("无法创建内容索引目录：{error}"))?;
    let index = Index::create_in_dir(index_dir, schema)
        .map_err(|error| format!("无法创建内容索引：{error}"))?;
    Ok((index, fields))
}

fn content_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("path_key", STRING | STORED);
    builder.add_text_field("path", STRING | STORED);
    builder.add_text_field("root_key", STRING | STORED);
    builder.add_text_field("root", STRING | STORED);
    builder.add_text_field("name", TEXT | STORED);
    builder.add_u64_field("size", STORED);
    builder.add_i64_field("modified_ms", STORED);
    let body_options = TextOptions::default().set_indexing_options(
        TextFieldIndexing::default()
            .set_tokenizer("default")
            .set_index_option(IndexRecordOption::WithFreqsAndPositions),
    );
    builder.add_text_field("body", body_options);
    builder.build()
}

impl ContentFields {
    fn from_schema(schema: &Schema) -> Result<Self, String> {
        Ok(Self {
            path_key: schema
                .get_field("path_key")
                .map_err(|_| "内容索引缺少 path_key 字段".to_string())?,
            path: schema
                .get_field("path")
                .map_err(|_| "内容索引缺少 path 字段".to_string())?,
            root_key: schema
                .get_field("root_key")
                .map_err(|_| "内容索引缺少 root_key 字段".to_string())?,
            root: schema
                .get_field("root")
                .map_err(|_| "内容索引缺少 root 字段".to_string())?,
            name: schema
                .get_field("name")
                .map_err(|_| "内容索引缺少 name 字段".to_string())?,
            size: schema
                .get_field("size")
                .map_err(|_| "内容索引缺少 size 字段".to_string())?,
            modified_ms: schema
                .get_field("modified_ms")
                .map_err(|_| "内容索引缺少 modified_ms 字段".to_string())?,
            body: schema
                .get_field("body")
                .map_err(|_| "内容索引缺少 body 字段".to_string())?,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn index_worker(
    index: Index,
    schema: ContentFields,
    reader: IndexReader,
    command_rx: mpsc::Receiver<IndexCommand>,
    status: Arc<Mutex<ContentIndexStatus>>,
    interactive_until_ms: Arc<AtomicU64>,
    indexing_paused: Arc<AtomicBool>,
    index_dir: PathBuf,
) {
    let mut writer = match index.writer_with_num_threads(1, INDEX_WRITER_MEMORY_BYTES) {
        Ok(writer) => writer,
        Err(error) => {
            set_index_error(&status, format!("无法启动内容索引写入器：{error}"));
            return;
        }
    };
    let mut indexed = match load_indexed_meta(&reader, &schema) {
        Ok(indexed) => indexed,
        Err(error) => {
            set_index_error(&status, error);
            return;
        }
    };
    let _ = reader.reload();
    let mut dirty_documents = 0usize;
    let mut last_commit = Instant::now();
    let mut resource_guard = IndexResourceGuard::new(&index_dir);

    loop {
        let command = match command_rx.recv_timeout(Duration::from_millis(250)) {
            Ok(command) => command,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if dirty_documents > 0
                    && (dirty_documents >= COMMIT_DOCUMENT_INTERVAL
                        || last_commit.elapsed() >= COMMIT_TIME_INTERVAL)
                {
                    commit_index(
                        &mut writer,
                        &reader,
                        &status,
                        &mut dirty_documents,
                        &mut last_commit,
                        indexed.len(),
                    );
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };

        match command {
            IndexCommand::EnsureRoot(root) => {
                set_index_state(
                    &status,
                    ContentIndexState::Indexing,
                    indexed.len(),
                    Some(format!("正在索引 {}", root.display())),
                );
                let mut seen = HashSet::new();
                index_root(
                    &mut writer,
                    &schema,
                    &root,
                    &index_dir,
                    &interactive_until_ms,
                    &indexing_paused,
                    &mut resource_guard,
                    &index_dir,
                    &mut indexed,
                    &mut seen,
                    &mut dirty_documents,
                    &mut last_commit,
                    &reader,
                    &status,
                );
                remove_stale_for_root(
                    &root,
                    schema.path_key,
                    &mut writer,
                    &mut indexed,
                    &seen,
                    &mut dirty_documents,
                );
                commit_index(
                    &mut writer,
                    &reader,
                    &status,
                    &mut dirty_documents,
                    &mut last_commit,
                    indexed.len(),
                );
                set_index_state(&status, ContentIndexState::Ready, indexed.len(), None);
            }
            IndexCommand::PathChanged { path, root } => {
                set_index_state(
                    &status,
                    ContentIndexState::Indexing,
                    indexed.len(),
                    Some("正在更新内容索引".to_string()),
                );
                if path.is_dir() {
                    let mut seen = HashSet::new();
                    index_root(
                        &mut writer,
                        &schema,
                        &root,
                        &index_dir,
                        &interactive_until_ms,
                        &indexing_paused,
                        &mut resource_guard,
                        &index_dir,
                        &mut indexed,
                        &mut seen,
                        &mut dirty_documents,
                        &mut last_commit,
                        &reader,
                        &status,
                    );
                } else if path.is_file() {
                    wait_for_index_permission(
                        &interactive_until_ms,
                        &indexing_paused,
                        &mut resource_guard,
                        &index_dir,
                    );
                    let _ = index_file(
                        &mut writer,
                        &schema,
                        &path,
                        &root,
                        &mut indexed,
                        &mut dirty_documents,
                    );
                } else {
                    remove_path_from_index(
                        &path,
                        schema.path_key,
                        &mut writer,
                        &mut indexed,
                        &mut dirty_documents,
                    );
                }
                if dirty_documents >= COMMIT_DOCUMENT_INTERVAL
                    || last_commit.elapsed() >= COMMIT_TIME_INTERVAL
                {
                    commit_index(
                        &mut writer,
                        &reader,
                        &status,
                        &mut dirty_documents,
                        &mut last_commit,
                        indexed.len(),
                    );
                }
                set_index_state(&status, ContentIndexState::Ready, indexed.len(), None);
            }
            IndexCommand::Clear => {
                if let Err(error) = writer.delete_all_documents() {
                    set_index_error(&status, format!("无法清空内容索引：{error}"));
                    continue;
                }
                indexed.clear();
                commit_index(
                    &mut writer,
                    &reader,
                    &status,
                    &mut dirty_documents,
                    &mut last_commit,
                    0,
                );
                set_index_state(&status, ContentIndexState::Idle, 0, None);
            }
        }
    }
    if dirty_documents > 0 {
        commit_index(
            &mut writer,
            &reader,
            &status,
            &mut dirty_documents,
            &mut last_commit,
            indexed.len(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn index_root(
    writer: &mut IndexWriter,
    schema: &ContentFields,
    root: &Path,
    index_dir: &Path,
    interactive_until_ms: &AtomicU64,
    indexing_paused: &AtomicBool,
    resource_guard: &mut IndexResourceGuard,
    index_dir_for_guard: &Path,
    indexed: &mut HashMap<String, IndexedMeta>,
    seen: &mut HashSet<String>,
    dirty_documents: &mut usize,
    last_commit: &mut Instant,
    reader: &IndexReader,
    status: &Arc<Mutex<ContentIndexStatus>>,
) {
    let index_dir = index_dir.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(true)
        .hidden(true)
        .follow_links(false);
    builder
        .filter_entry(move |entry| entry.depth() == 0 || should_descend(entry.path(), &index_dir));

    let mut visited_files = 0usize;
    for item in builder.build() {
        wait_for_index_permission(
            interactive_until_ms,
            indexing_paused,
            resource_guard,
            index_dir_for_guard,
        );
        let Ok(entry) = item else {
            continue;
        };
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        visited_files += 1;
        if visited_files > CONTENT_SEARCH_MAX_INDEXED_FILES {
            set_index_message(status, "内容索引达到文件数量上限");
            break;
        }
        let path = entry.path();
        let key = path_key(path);
        seen.insert(key);
        if index_file(writer, schema, path, root, indexed, dirty_documents).unwrap_or(false)
            && (*dirty_documents >= COMMIT_DOCUMENT_INTERVAL
                || last_commit.elapsed() >= COMMIT_TIME_INTERVAL)
        {
            commit_index(
                writer,
                reader,
                status,
                dirty_documents,
                last_commit,
                indexed.len(),
            );
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn index_file(
    writer: &mut IndexWriter,
    schema: &ContentFields,
    path: &Path,
    root: &Path,
    indexed: &mut HashMap<String, IndexedMeta>,
    dirty_documents: &mut usize,
) -> Result<bool, String> {
    let key = path_key(path);
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => {
            remove_indexed_key(&key, schema.path_key, writer, indexed, dirty_documents);
            return Ok(false);
        }
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        remove_indexed_key(&key, schema.path_key, writer, indexed, dirty_documents);
        return Ok(false);
    }
    if metadata.len() > CONTENT_SEARCH_MAX_FILE_BYTES || is_cloud_placeholder(path, &metadata) {
        remove_indexed_key(&key, schema.path_key, writer, indexed, dirty_documents);
        return Ok(false);
    }
    let modified_ms = modified_ms(&metadata);
    if indexed
        .get(&key)
        .is_some_and(|current| current.size == metadata.len() && current.modified_ms == modified_ms)
    {
        return Ok(false);
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => {
            remove_indexed_key(&key, schema.path_key, writer, indexed, dirty_documents);
            return Ok(false);
        }
    };
    if bytes.contains(&0) {
        remove_indexed_key(&key, schema.path_key, writer, indexed, dirty_documents);
        return Ok(false);
    }
    let Ok(text) = String::from_utf8(bytes) else {
        remove_indexed_key(&key, schema.path_key, writer, indexed, dirty_documents);
        return Ok(false);
    };
    let path_text = path.to_string_lossy().into_owned();
    let root_text = root.to_string_lossy().into_owned();
    let root_key = path_key(root);
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path_text.clone());

    writer.delete_term(Term::from_field_text(schema.path_key, &key));
    let mut document = TantivyDocument::default();
    document.add_text(schema.path_key, &key);
    document.add_text(schema.path, &path_text);
    document.add_text(schema.root_key, &root_key);
    document.add_text(schema.root, &root_text);
    document.add_text(schema.name, &name);
    document.add_u64(schema.size, metadata.len());
    document.add_i64(schema.modified_ms, modified_ms);
    document.add_text(schema.body, text.to_lowercase());
    writer
        .add_document(document)
        .map_err(|error| format!("无法写入内容索引：{error}"))?;
    indexed.insert(
        key,
        IndexedMeta {
            path: path.to_path_buf(),
            root_key,
            size: metadata.len(),
            modified_ms,
        },
    );
    *dirty_documents += 1;
    Ok(true)
}

fn remove_indexed_key(
    key: &str,
    path_key_field: Field,
    writer: &mut IndexWriter,
    indexed: &mut HashMap<String, IndexedMeta>,
    dirty_documents: &mut usize,
) {
    if indexed.remove(key).is_some() {
        writer.delete_term(Term::from_field_text(path_key_field, key));
        *dirty_documents += 1;
    }
}

fn remove_stale_for_root(
    root: &Path,
    path_key_field: Field,
    writer: &mut IndexWriter,
    indexed: &mut HashMap<String, IndexedMeta>,
    seen: &HashSet<String>,
    dirty_documents: &mut usize,
) {
    let root_key = path_key(root);
    let stale = indexed
        .iter()
        .filter(|(key, meta)| {
            meta.root_key == root_key && is_path_within(&meta.path, root) && !seen.contains(*key)
        })
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in stale {
        if indexed.remove(&key).is_some() {
            writer.delete_term(Term::from_field_text(path_key_field, &key));
            *dirty_documents += 1;
        }
    }
}

fn remove_path_from_index(
    path: &Path,
    path_key_field: Field,
    writer: &mut IndexWriter,
    indexed: &mut HashMap<String, IndexedMeta>,
    dirty_documents: &mut usize,
) {
    let keys = indexed
        .iter()
        .filter(|(_, meta)| is_path_within(&meta.path, path))
        .map(|(key, _)| key.clone())
        .collect::<Vec<_>>();
    for key in keys {
        if indexed.remove(&key).is_some() {
            writer.delete_term(Term::from_field_text(path_key_field, &key));
            *dirty_documents += 1;
        }
    }
}

fn commit_index(
    writer: &mut IndexWriter,
    reader: &IndexReader,
    status: &Arc<Mutex<ContentIndexStatus>>,
    dirty_documents: &mut usize,
    last_commit: &mut Instant,
    indexed_files: usize,
) {
    if *dirty_documents == 0 && last_commit.elapsed() < COMMIT_TIME_INTERVAL {
        return;
    }
    match writer.commit() {
        Ok(_) => {
            let _ = reader.reload();
            *dirty_documents = 0;
            *last_commit = Instant::now();
            set_index_state(status, ContentIndexState::Ready, indexed_files, None);
        }
        Err(error) => {
            set_index_error(status, format!("无法提交内容索引：{error}"));
        }
    }
}

fn load_indexed_meta(
    reader: &IndexReader,
    schema: &ContentFields,
) -> Result<HashMap<String, IndexedMeta>, String> {
    let searcher = reader.searcher();
    let mut indexed = HashMap::new();
    for segment in searcher.segment_readers() {
        let store = segment
            .get_store_reader(8)
            .map_err(|error| format!("无法读取内容索引：{error}"))?;
        for document in store.iter::<TantivyDocument>(segment.alive_bitset()) {
            let document = document.map_err(|error| format!("无法读取内容索引文档：{error}"))?;
            let Some(key) = text_value(&document, schema.path_key) else {
                continue;
            };
            let Some(path) = text_value(&document, schema.path) else {
                continue;
            };
            let Some(root_key) = text_value(&document, schema.root_key) else {
                continue;
            };
            indexed.insert(
                key,
                IndexedMeta {
                    path: PathBuf::from(path),
                    root_key,
                    size: document
                        .get_first(schema.size)
                        .and_then(|value| value.as_u64())
                        .unwrap_or_default(),
                    modified_ms: document
                        .get_first(schema.modified_ms)
                        .and_then(|value| value.as_i64())
                        .unwrap_or_default(),
                },
            );
        }
    }
    Ok(indexed)
}

fn content_query(query: &str, field: Field) -> Result<Box<dyn Query>, String> {
    let lower = query.to_lowercase();
    let mut tokenizer = SimpleTokenizer::default();
    let mut stream = tokenizer.token_stream(&lower);
    let mut query_terms: Vec<String> = Vec::new();
    while let Some(token) = stream.next() {
        query_terms.push(token.text.clone());
    }
    if query_terms.len() > 1 {
        let terms = query_terms
            .into_iter()
            .map(|term| Term::from_field_text(field, term.as_str()))
            .collect::<Vec<_>>();
        return Ok(Box::new(PhraseQuery::new(terms)));
    }
    let literal = query_terms.first().map(String::as_str).unwrap_or(&lower);
    let pattern = format!(".*{}.*", regex::escape(literal));
    RegexQuery::from_pattern(&pattern, field)
        .map(|query| Box::new(query) as Box<dyn Query>)
        .map_err(|error| format!("无法生成内容索引查询：{error}"))
}

#[derive(Default)]
struct LiveSearchOutcome {
    files: Vec<ContentFileMatch>,
    partial: bool,
    truncated: bool,
}

fn live_search(
    roots: &[PathBuf],
    query: &str,
    regex: bool,
    max_files: usize,
    deadline: Instant,
    cancel: &AtomicBool,
    index_dir: &Path,
) -> Result<LiveSearchOutcome, String> {
    let files = Arc::new(Mutex::new(Vec::<ContentFileMatch>::new()));
    let scanned = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(AtomicBool::new(false));
    let index_dir = index_dir.to_path_buf();

    for root in roots {
        let mut builder = WalkBuilder::new(root);
        builder
            .standard_filters(true)
            .hidden(true)
            .follow_links(false)
            .threads(2);
        let filter_index_dir = index_dir.clone();
        builder.filter_entry(move |entry| {
            entry.depth() == 0 || should_descend(entry.path(), &filter_index_dir)
        });
        builder.build_parallel().run(|| {
            let files = Arc::clone(&files);
            let scanned = Arc::clone(&scanned);
            let stopped = Arc::clone(&stopped);
            Box::new(move |entry| {
                if cancel.load(Ordering::Relaxed)
                    || Instant::now() >= deadline
                    || files
                        .lock()
                        .map(|files| files.len() >= max_files)
                        .unwrap_or(true)
                {
                    stopped.store(true, Ordering::Relaxed);
                    return WalkState::Quit;
                }
                let Ok(entry) = entry else {
                    return WalkState::Continue;
                };
                if !entry.file_type().is_some_and(|kind| kind.is_file()) {
                    return WalkState::Continue;
                }
                scanned.fetch_add(1, Ordering::Relaxed);
                let Ok(metadata) = entry.metadata() else {
                    return WalkState::Continue;
                };
                if metadata.len() > CONTENT_SEARCH_MAX_FILE_BYTES
                    || is_cloud_placeholder(entry.path(), &metadata)
                {
                    return WalkState::Continue;
                }
                let Ok((count, matches)) = search_file_matches(entry.path(), query, regex, cancel)
                else {
                    return WalkState::Continue;
                };
                if count == 0 || matches.is_empty() {
                    return WalkState::Continue;
                }
                let path = entry.path().to_path_buf();
                let extension = path
                    .extension()
                    .map(|value| value.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let name = path
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                let Ok(mut files) = files.lock() else {
                    return WalkState::Quit;
                };
                if files.len() >= max_files {
                    stopped.store(true, Ordering::Relaxed);
                    return WalkState::Quit;
                }
                files.push(ContentFileMatch {
                    relative_path: relative_path(&path, root),
                    path,
                    root: root.clone(),
                    name,
                    kind: EntryKind::File,
                    kind_label: type_label(EntryKind::File, &extension),
                    size: metadata.len(),
                    modified_unix_ms: Some(modified_ms(&metadata)),
                    score: 0.0,
                    match_count: count,
                    matches,
                });
                WalkState::Continue
            })
        });
        if cancel.load(Ordering::Relaxed) || stopped.load(Ordering::Relaxed) {
            break;
        }
    }

    let mut files = Arc::try_unwrap(files)
        .map_err(|_| "内容搜索任务未正确结束".to_string())?
        .into_inner()
        .map_err(|_| "内容搜索结果锁损坏".to_string())?;
    files.sort_by(compare_content_files);
    Ok(LiveSearchOutcome {
        partial: stopped.load(Ordering::Relaxed),
        truncated: scanned.load(Ordering::Relaxed) >= CONTENT_SEARCH_MAX_INDEXED_FILES,
        files,
    })
}

fn validate_query(query: &str, regex: bool) -> Result<(), String> {
    let mut builder = RegexMatcherBuilder::new();
    builder.case_insensitive(true).fixed_strings(!regex);
    builder
        .build(query)
        .map(|_| ())
        .map_err(|error| format!("无法解析搜索内容：{error}"))
}

fn search_file_matches(
    path: &Path,
    query: &str,
    regex: bool,
    cancel: &AtomicBool,
) -> Result<(u64, Vec<ContentLineMatch>), String> {
    let mut matcher_builder = RegexMatcherBuilder::new();
    matcher_builder.case_insensitive(true).fixed_strings(!regex);
    let matcher = matcher_builder
        .build(query)
        .map_err(|error| format!("无法解析搜索内容：{error}"))?;
    let mut searcher = SearcherBuilder::new()
        .line_number(true)
        .binary_detection(BinaryDetection::quit(0))
        .max_matches(Some(MAX_MATCHES_PER_FILE))
        .build();
    let mut sink = LineSink {
        matcher: &matcher,
        matches: Vec::new(),
        count: 0,
        cancel,
    };
    searcher
        .search_path(&matcher, path, &mut sink)
        .map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
    Ok((sink.count, sink.matches))
}

struct LineSink<'a> {
    matcher: &'a grep_regex::RegexMatcher,
    matches: Vec<ContentLineMatch>,
    count: u64,
    cancel: &'a AtomicBool,
}

impl Sink for LineSink<'_> {
    type Error = io::Error;

    fn matched(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        mat: &SinkMatch<'_>,
    ) -> io::Result<bool> {
        if self.cancel.load(Ordering::Relaxed) {
            return Ok(false);
        }
        self.count = self.count.saturating_add(1);
        if self.matches.len() < MATCH_SNIPPETS_PER_FILE {
            let line = String::from_utf8_lossy(mat.bytes());
            let line = line.trim_end_matches(['\r', '\n']);
            let found = self
                .matcher
                .find(line.as_bytes())
                .map_err(|error| io::Error::other(error.to_string()))?;
            let (text, column_start, column_end) = match found {
                Some(found) => clip_line(line, found.start(), found.end()),
                None => (line.chars().take(TEXT_PREVIEW_CHARS).collect(), None, None),
            };
            self.matches.push(ContentLineMatch {
                line_number: mat.line_number().unwrap_or_default(),
                column_start,
                column_end,
                text,
            });
        }
        Ok(self.count < MAX_MATCHES_PER_FILE)
    }

    fn finish(
        &mut self,
        _searcher: &grep_searcher::Searcher,
        _finish: &SinkFinish,
    ) -> io::Result<()> {
        Ok(())
    }
}

fn clip_line(
    line: &str,
    match_start: usize,
    match_end: usize,
) -> (String, Option<u64>, Option<u64>) {
    let total_chars = line.chars().count();
    let match_start_chars = line
        .get(..match_start)
        .map(str::chars)
        .map(Iterator::count)
        .unwrap_or(0);
    let match_end_chars = line
        .get(..match_end)
        .map(str::chars)
        .map(Iterator::count)
        .unwrap_or(match_start_chars);
    let clip_start = match_start_chars.saturating_sub(TEXT_PREVIEW_CHARS / 5);
    let clip_end = (clip_start + TEXT_PREVIEW_CHARS).min(total_chars);
    let mut text = line
        .chars()
        .skip(clip_start)
        .take(clip_end.saturating_sub(clip_start))
        .collect::<String>();
    let prefix = clip_start > 0;
    let suffix = clip_end < total_chars;
    if prefix {
        text.insert(0, '…');
    }
    if suffix {
        text.push('…');
    }
    let offset = usize::from(prefix);
    (
        text,
        Some((match_start_chars.saturating_sub(clip_start) + offset + 1) as u64),
        Some((match_end_chars.saturating_sub(clip_start) + offset + 1) as u64),
    )
}

fn merge_files(target: &mut Vec<ContentFileMatch>, incoming: Vec<ContentFileMatch>) {
    let mut existing = target
        .iter()
        .map(|file| path_key(&file.path))
        .collect::<HashSet<_>>();
    for file in incoming {
        if existing.insert(path_key(&file.path)) {
            target.push(file);
        }
    }
}

fn compare_content_files(left: &ContentFileMatch, right: &ContentFileMatch) -> std::cmp::Ordering {
    right
        .match_count
        .cmp(&left.match_count)
        .then_with(|| {
            search_path_quality_rank(&right.path).cmp(&search_path_quality_rank(&left.path))
        })
        .then_with(|| right.modified_unix_ms.cmp(&left.modified_unix_ms))
        .then_with(|| left.relative_path.cmp(&right.relative_path))
}

fn normalize_roots(roots: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut normalized = roots
        .iter()
        .filter_map(|root| canonicalize(root).ok())
        .filter(|root| root.is_dir())
        .collect::<Vec<_>>();
    normalized.sort_by_key(|root| root.components().count());
    normalized.dedup();
    let mut result = Vec::<PathBuf>::new();
    for root in normalized {
        if !result.iter().any(|parent| is_path_within(&root, parent)) {
            result.push(root);
        }
    }
    if result.is_empty() {
        return Err("请选择至少一个可搜索的文件夹".to_string());
    }
    Ok(result)
}

fn find_root_for_path(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .filter(|root| is_path_within(path, root))
        .max_by_key(|root| root.components().count())
        .cloned()
}

fn is_path_within(path: &Path, root: &Path) -> bool {
    let path = path_key(path);
    let root = path_key(root);
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|suffix| suffix.starts_with(std::path::MAIN_SEPARATOR))
}

fn should_descend(path: &Path, index_dir: &Path) -> bool {
    if path.starts_with(index_dir) || is_internal_app_data_path(path) {
        return false;
    }
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    let name = name.to_lowercase();
    !matches!(
        name.as_str(),
        ".git"
            | ".svn"
            | ".hg"
            | "node_modules"
            | "target"
            | "build"
            | "dist"
            | ".cache"
            | "library"
            | "$recycle.bin"
            | "system volume information"
            | "windows"
            | "program files"
            | "program files (x86)"
            | "programdata"
    )
}

fn is_internal_app_data_path(path: &Path) -> bool {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .is_some_and(|base| path.starts_with(base.join("Kite")))
}

fn path_key(path: &Path) -> String {
    let value = path
        .to_string_lossy()
        .replace('/', std::path::MAIN_SEPARATOR_STR);
    if cfg!(windows) {
        value.to_lowercase()
    } else {
        value
    }
}

fn relative_path(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .map(|value| {
            if value.as_os_str().is_empty() {
                ".".to_string()
            } else {
                value.to_string_lossy().into_owned()
            }
        })
        .unwrap_or_else(|_| path.display().to_string())
}

fn modified_ms(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}

fn wait_for_index_permission(
    interactive_until_ms: &AtomicU64,
    indexing_paused: &AtomicBool,
    resource_guard: &mut IndexResourceGuard,
    index_dir: &Path,
) {
    loop {
        if resource_guard.should_pause(index_dir) {
            thread::sleep(Duration::from_secs(2));
            continue;
        }
        if indexing_paused.load(Ordering::Relaxed)
            || now_millis() < interactive_until_ms.load(Ordering::Relaxed)
        {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        break;
    }
}

fn text_value(document: &TantivyDocument, field: Field) -> Option<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn set_index_state(
    status: &Arc<Mutex<ContentIndexStatus>>,
    state: ContentIndexState,
    indexed_files: usize,
    message: Option<String>,
) {
    if let Ok(mut status) = status.lock() {
        status.state = state;
        status.indexed_files = indexed_files as u64;
        status.message = message;
    }
}

fn set_index_message(status: &Arc<Mutex<ContentIndexStatus>>, message: &str) {
    if let Ok(mut status) = status.lock() {
        status.message = Some(message.to_string());
    }
}

fn set_index_error(status: &Arc<Mutex<ContentIndexStatus>>, message: String) {
    if let Ok(mut status) = status.lock() {
        status.state = ContentIndexState::Error;
        status.message = Some(message);
    }
}

#[cfg(target_os = "windows")]
fn is_cloud_placeholder(_path: &Path, metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x0004_0000;
    let attributes = metadata.file_attributes();
    attributes & (FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS | FILE_ATTRIBUTE_RECALL_ON_OPEN) != 0
}

#[cfg(not(target_os = "windows"))]
fn is_cloud_placeholder(path: &Path, _metadata: &fs::Metadata) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("icloud"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "kite-content-{name}-{}-{unique}",
            std::process::id()
        ))
    }

    fn wait_until_ready(service: &ContentSearchService, expected: u64) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            let status = service.status();
            if status.state == ContentIndexState::Ready && status.indexed_files >= expected {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("content index did not become ready: {:?}", service.status());
    }

    #[test]
    fn searches_indexed_content_and_returns_lines() {
        let root = test_root("indexed");
        let index_dir = test_root("index");
        fs::create_dir_all(&root).expect("create root");
        fs::write(
            root.join("notes.txt"),
            "first line\nNeedle in the haystack\nlast line\n",
        )
        .expect("write notes");
        let service = ContentSearchService::open_at(index_dir).expect("open service");
        service
            .ensure_roots(std::slice::from_ref(&root))
            .expect("ensure root");
        wait_until_ready(&service, 1);

        let result = service
            .search(std::slice::from_ref(&root), "NEEDLE", false, 20)
            .expect("search succeeds")
            .expect("search returns result");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].name, "notes.txt");
        assert_eq!(result.files[0].matches[0].line_number, 2);
        assert!(result.files[0].matches[0].text.contains("Needle"));
        assert_eq!(result.index_state, ContentIndexState::Ready);

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(service.index_dir.clone());
    }

    #[test]
    fn searches_chinese_without_index_delay() {
        let root = test_root("chinese");
        let index_dir = test_root("index-cn");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("说明.md"), "第一行\n这里包含搜索目标\n第三行\n").expect("write notes");
        let service = ContentSearchService::open_at(index_dir).expect("open service");

        let result = service
            .search(std::slice::from_ref(&root), "搜索目标", false, 20)
            .expect("search succeeds")
            .expect("search returns result");
        assert_eq!(result.files.len(), 1);
        assert!(result.files[0].matches[0].text.contains("搜索目标"));

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(service.index_dir.clone());
    }

    #[test]
    fn watches_changed_files_and_refreshes_index() {
        let root = test_root("watch");
        fs::create_dir_all(&root).expect("create root");
        let file = root.join("live.txt");
        fs::write(&file, "old content").expect("write old content");
        let service =
            ContentSearchService::open_at(test_root("index-watch")).expect("open service");
        service
            .ensure_roots(std::slice::from_ref(&root))
            .expect("ensure root");
        wait_until_ready(&service, 1);

        fs::write(&file, "fresh searchable content").expect("write new content");
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut found = false;
        while Instant::now() < deadline {
            let result = service
                .search(std::slice::from_ref(&root), "fresh searchable", false, 20)
                .expect("search succeeds")
                .expect("search returns result");
            if result.files.iter().any(|file| file.name == "live.txt") {
                found = true;
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(found, "watcher did not refresh changed file");

        let _ = fs::remove_dir_all(root);
        service.set_indexing_paused(true);
        let _ = fs::remove_dir_all(service.index_dir.clone());
    }

    #[test]
    fn regex_search_uses_live_fallback() {
        let root = test_root("regex");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("notes.txt"), "alpha 123\nbeta 456\n").expect("write notes");
        let service =
            ContentSearchService::open_at(test_root("index-regex")).expect("open service");

        let result = service
            .search(std::slice::from_ref(&root), r"\d{3}", true, 20)
            .expect("search succeeds")
            .expect("search returns result");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].match_count, 2);

        let _ = fs::remove_dir_all(root);
        service.set_indexing_paused(true);
        let _ = fs::remove_dir_all(service.index_dir.clone());
    }

    #[test]
    fn filters_results_to_requested_nested_scope() {
        let root = test_root("nested-scope");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create nested root");
        fs::write(root.join("root.txt"), "needle root").expect("write root file");
        fs::write(nested.join("nested.txt"), "needle nested").expect("write nested file");
        let service =
            ContentSearchService::open_at(test_root("index-nested-scope")).expect("open service");
        service
            .ensure_roots(std::slice::from_ref(&root))
            .expect("ensure root");
        wait_until_ready(&service, 2);

        let nested_result = service
            .search(std::slice::from_ref(&nested), "needle", false, 20)
            .expect("search succeeds")
            .expect("search returns result");
        assert_eq!(nested_result.files.len(), 1);
        assert_eq!(nested_result.files[0].name, "nested.txt");

        let root_result = service
            .search(std::slice::from_ref(&root), "needle", false, 20)
            .expect("search succeeds")
            .expect("search returns result");
        assert_eq!(root_result.files.len(), 2);

        let _ = fs::remove_dir_all(root);
        service.set_indexing_paused(true);
        let _ = fs::remove_dir_all(service.index_dir.clone());
    }

    #[test]
    fn deduplicates_nested_roots() {
        let root = test_root("roots");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create root");
        let roots = normalize_roots(&[nested, root.clone()]).expect("normalize roots");
        let expected = root.canonicalize().expect("canonical root");
        assert_eq!(roots, vec![expected]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skips_binary_and_oversized_files() {
        let root = test_root("skip");
        fs::create_dir_all(&root).expect("create root");
        fs::write(root.join("binary.txt"), [0, 1, 2, 3]).expect("write binary");
        fs::write(root.join("text.txt"), "searchable").expect("write text");
        let service = ContentSearchService::open_at(test_root("index-skip")).expect("open service");
        service
            .ensure_roots(std::slice::from_ref(&root))
            .expect("ensure root");
        wait_until_ready(&service, 1);

        let result = service
            .search(std::slice::from_ref(&root), "searchable", false, 20)
            .expect("search succeeds")
            .expect("search returns result");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].name, "text.txt");

        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(service.index_dir.clone());
    }
}
