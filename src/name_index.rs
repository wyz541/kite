use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use atomicwrites::{AllowOverwrite, AtomicFile};
use ignore::WalkBuilder;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tantivy::collector::{DocSetCollector, TopDocs};
use tantivy::query::{BooleanQuery, Occur, Query, RegexQuery};
use tantivy::schema::{Field, STORED, STRING, Schema, TantivyDocument, Value};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};
use thread_priority::{ThreadBuilderExt, ThreadPriority};

use crate::fs::{
    EntryKind, FileEntry, GlobalNameSearchSnapshot, SEARCH_RESULT_LIMIT, has_generated_parent,
    is_generated_search_directory, is_hidden, sort_global_name_entries,
};
#[cfg(windows)]
use crate::usn_journal;

const INDEX_VERSION: u32 = 2;
const INDEX_WRITER_MEMORY_BYTES: usize = 64 * 1024 * 1024;
const COMMIT_DOCUMENT_INTERVAL: usize = 25_000;
const COMMIT_TIME_INTERVAL: Duration = Duration::from_secs(5);
const QUERY_CANDIDATE_LIMIT: usize = 5_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameIndexState {
    Idle,
    Indexing,
    Ready,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NameIndexProvider {
    #[default]
    Notify,
    UsnJournal,
}

impl NameIndexProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Notify => "notify",
            Self::UsnJournal => "usn_journal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NameIndexCoverage {
    Complete,
    Partial,
    Unavailable,
}

impl NameIndexCoverage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::Unavailable => "unavailable",
        }
    }
}

impl NameIndexState {
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
pub struct NameIndexStatus {
    pub root: PathBuf,
    pub state: NameIndexState,
    pub usable: bool,
    pub indexed_entries: u64,
    pub message: Option<String>,
    pub provider: NameIndexProvider,
    pub coverage: NameIndexCoverage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct JournalCursor {
    journal_id: u64,
    next_usn: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct JournalPreparation {
    cursor: JournalCursor,
    reusable: bool,
}

#[cfg(windows)]
impl From<usn_journal::JournalCursor> for JournalCursor {
    fn from(cursor: usn_journal::JournalCursor) -> Self {
        Self {
            journal_id: cursor.journal_id,
            next_usn: cursor.next_usn,
        }
    }
}

#[cfg(windows)]
impl From<JournalCursor> for usn_journal::JournalCursor {
    fn from(cursor: JournalCursor) -> Self {
        Self {
            journal_id: cursor.journal_id,
            next_usn: cursor.next_usn,
        }
    }
}

#[derive(Clone, Debug)]
struct RootStatus {
    root: PathBuf,
    root_key: String,
    state: NameIndexState,
    usable: bool,
    indexed_entries: u64,
    message: Option<String>,
    reconciled: bool,
    watchable: bool,
    watch_error: Option<String>,
    provider: NameIndexProvider,
    journal_id: Option<u64>,
    journal_next_usn: Option<i64>,
}

impl RootStatus {
    fn coverage(&self) -> NameIndexCoverage {
        if self.usable && self.reconciled && self.state == NameIndexState::Ready {
            NameIndexCoverage::Complete
        } else if self.usable || self.indexed_entries > 0 {
            NameIndexCoverage::Partial
        } else {
            NameIndexCoverage::Unavailable
        }
    }

    #[cfg(windows)]
    fn journal_cursor(&self) -> Option<JournalCursor> {
        Some(JournalCursor {
            journal_id: self.journal_id?,
            next_usn: self.journal_next_usn?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Manifest {
    version: u32,
    roots: Vec<ManifestRoot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ManifestRoot {
    path: String,
    indexed_entries: u64,
    completed_unix_ms: u64,
    #[serde(default)]
    provider: NameIndexProvider,
    #[serde(default)]
    journal_id: Option<u64>,
    #[serde(default)]
    journal_next_usn: Option<i64>,
}

#[derive(Clone)]
struct NameFields {
    path_key: Field,
    path: Field,
    root_key: Field,
    name_key: Field,
}

enum IndexCommand {
    EnsureRoot(PathBuf),
    PathChanged {
        path: PathBuf,
        root: PathBuf,
    },
    JournalAdvanced {
        root: PathBuf,
        journal_id: u64,
        next_usn: i64,
    },
    RemoveRoot(PathBuf),
    Clear,
}

#[cfg(windows)]
struct UsnWatcher {
    root: PathBuf,
    cancel: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

#[cfg(windows)]
impl Drop for UsnWatcher {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Relaxed);
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub struct NameIndexService {
    index_dir: PathBuf,
    schema: NameFields,
    reader: IndexReader,
    command_tx: mpsc::Sender<IndexCommand>,
    statuses: Arc<Mutex<HashMap<String, RootStatus>>>,
    pending: Arc<Mutex<HashSet<String>>>,
    watched_roots: Arc<Mutex<BTreeSet<PathBuf>>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    #[cfg(windows)]
    usn_watchers: Mutex<HashMap<String, UsnWatcher>>,
}

impl NameIndexService {
    pub fn new() -> Result<Self, String> {
        Self::open_at(default_index_dir())
    }

    #[cfg(test)]
    pub(crate) fn open_for_testing(index_dir: PathBuf) -> Result<Self, String> {
        Self::open_at(index_dir)
    }

    fn open_at(index_dir: PathBuf) -> Result<Self, String> {
        let (index, schema) = open_or_create_index(&index_dir)?;
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()
            .map_err(|error| format!("无法打开名称索引：{error}"))?;
        let statuses = Arc::new(Mutex::new(load_manifest(&index_dir)));
        let pending = Arc::new(Mutex::new(HashSet::new()));
        let (command_tx, command_rx) = mpsc::channel();
        {
            let statuses = Arc::clone(&statuses);
            let pending = Arc::clone(&pending);
            let reader = reader.clone();
            let schema = schema.clone();
            let worker_index_dir = index_dir.clone();
            thread::Builder::new()
                .name("kite-name-index".to_string())
                .spawn_with_priority(ThreadPriority::Min, move |_priority| {
                    index_worker(
                        index,
                        schema,
                        reader,
                        command_rx,
                        statuses,
                        pending,
                        worker_index_dir,
                    );
                })
                .map_err(|error| format!("无法启动名称索引线程：{error}"))?;
        }
        Ok(Self {
            index_dir,
            schema,
            reader,
            command_tx,
            statuses,
            pending,
            watched_roots: Arc::new(Mutex::new(BTreeSet::new())),
            watcher: Mutex::new(None),
            #[cfg(windows)]
            usn_watchers: Mutex::new(HashMap::new()),
        })
    }

    pub fn ensure_roots(&self, roots: &[PathBuf]) {
        let roots = normalize_roots(roots);
        if roots.is_empty() {
            return;
        }
        for root in roots {
            let root_key = path_key(&root);
            let existing = {
                let statuses = match self.statuses.lock() {
                    Ok(statuses) => statuses,
                    Err(_) => return,
                };
                statuses
                    .values()
                    .filter(|status| path_is_within(&root, &status.root))
                    .min_by_key(|status| status.root.components().count())
                    .cloned()
            };
            if let Some(status) = existing.as_ref()
                && status.provider == NameIndexProvider::UsnJournal
                && self.has_active_usn_watcher(&status.root)
            {
                continue;
            }
            #[cfg(windows)]
            let usn_preparation = usn_journal::prepare(
                &root,
                existing
                    .as_ref()
                    .and_then(RootStatus::journal_cursor)
                    .map(Into::into),
            )
            .map(|preparation| {
                preparation.map(|preparation| JournalPreparation {
                    cursor: preparation.cursor.into(),
                    reusable: preparation.reusable,
                })
            });
            #[cfg(not(windows))]
            let usn_preparation: Result<Option<JournalPreparation>, String> = Ok(None);
            let (provider, journal_cursor, reusable, watch_error) = match usn_preparation {
                Ok(Some(preparation)) => match self.ensure_usn_watcher(&root, preparation.cursor) {
                    Ok(()) => {
                        let _ = self.ensure_watcher(&root);
                        (
                            NameIndexProvider::UsnJournal,
                            Some(preparation.cursor),
                            preparation.reusable,
                            None,
                        )
                    }
                    Err(_) => {
                        let watch_error = self.ensure_watcher(&root).err();
                        (NameIndexProvider::Notify, None, false, watch_error)
                    }
                },
                Ok(None) | Err(_) => {
                    let watch_error = self.ensure_watcher(&root).err();
                    (NameIndexProvider::Notify, None, false, watch_error)
                }
            };
            if let Some(status) = existing {
                if let Some(error) = watch_error.as_deref() {
                    set_watch_failure(&self.statuses, &status.root_key, error);
                    continue;
                }
                let source_changed = status.provider != provider
                    || (provider == NameIndexProvider::UsnJournal && !reusable);
                let restore_complete = provider == NameIndexProvider::UsnJournal && reusable;
                let needs_reconcile = !restore_complete
                    && (source_changed || !status.reconciled || !status.watchable);
                set_watch_available(
                    &self.statuses,
                    &status.root_key,
                    provider,
                    journal_cursor,
                    restore_complete,
                );
                if needs_reconcile && self.mark_pending(&status.root_key) {
                    set_status_indexing(
                        &self.statuses,
                        &status.root_key,
                        "正在校准名称索引",
                        false,
                    );
                    let _ = save_manifest(&self.index_dir, &self.statuses);
                    let _ = self.command_tx.send(IndexCommand::EnsureRoot(status.root));
                }
                continue;
            }
            if !self.mark_pending(&root_key) {
                continue;
            }
            if let Ok(mut statuses) = self.statuses.lock() {
                statuses.retain(|_, status| !path_is_within(&status.root, &root));
                statuses.insert(
                    root_key.clone(),
                    RootStatus {
                        root: root.clone(),
                        root_key,
                        state: NameIndexState::Indexing,
                        usable: false,
                        indexed_entries: 0,
                        message: Some(match watch_error.as_deref() {
                            Some(error) => format!("正在建立名称索引；{error}"),
                            None => "正在建立名称索引".to_string(),
                        }),
                        reconciled: false,
                        watchable: watch_error.is_none(),
                        watch_error,
                        provider,
                        journal_id: journal_cursor.map(|cursor| cursor.journal_id),
                        journal_next_usn: journal_cursor.map(|cursor| cursor.next_usn),
                    },
                );
            }
            let _ = self.command_tx.send(IndexCommand::EnsureRoot(root));
        }
    }

    pub fn remove_root(&self, root: PathBuf) {
        let _ = self.command_tx.send(IndexCommand::RemoveRoot(root));
    }

    pub fn clear(&self) {
        let _ = self.command_tx.send(IndexCommand::Clear);
    }

    pub fn status(&self, roots: &[PathBuf]) -> Vec<NameIndexStatus> {
        let roots = normalize_roots(roots);
        let Ok(statuses) = self.statuses.lock() else {
            return Vec::new();
        };
        roots
            .into_iter()
            .filter_map(|root| {
                statuses
                    .values()
                    .filter(|status| path_is_within(&root, &status.root))
                    .min_by_key(|status| status.root.components().count())
                    .map(|status| NameIndexStatus {
                        root: status.root.clone(),
                        state: status.state,
                        usable: status.usable,
                        indexed_entries: status.indexed_entries,
                        message: status.message.clone(),
                        provider: status.provider,
                        coverage: status.coverage(),
                    })
            })
            .collect()
    }

    pub fn search(
        &self,
        roots: &[PathBuf],
        query: &str,
        cancel: &AtomicBool,
    ) -> Result<Option<GlobalNameSearchSnapshot>, String> {
        let query_text = query.trim();
        if query_text.is_empty() {
            return Ok(None);
        }
        let roots = normalize_roots(roots);
        if roots.is_empty() {
            return Ok(None);
        }
        let (covered_any, all_usable) = self.coverage(&roots);
        if !covered_any {
            return Ok(None);
        }
        if cancel.load(Ordering::Relaxed) {
            return Ok(None);
        }
        let _ = self.reader.reload();
        let searcher = self.reader.searcher();
        let name_query = RegexQuery::from_pattern(
            &format!(".*{}.*", regex::escape(&query_text.to_lowercase())),
            self.schema.name_key,
        )
        .map_err(|error| format!("无法生成名称索引查询：{error}"))?;
        let root_queries = roots
            .iter()
            .map(|root| {
                RegexQuery::from_pattern(&path_scope_pattern(&path_key(root)), self.schema.path_key)
                    .map(|query| Box::new(query) as Box<dyn Query>)
                    .map_err(|error| format!("无法生成名称范围查询：{error}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let scope_query = BooleanQuery::new(
            root_queries
                .into_iter()
                .map(|query| (Occur::Should, query))
                .collect(),
        );
        let index_query = BooleanQuery::new(vec![
            (Occur::Must, Box::new(name_query) as Box<dyn Query>),
            (Occur::Must, Box::new(scope_query) as Box<dyn Query>),
        ]);
        let top_docs = searcher
            .search(
                &index_query,
                &TopDocs::with_limit(QUERY_CANDIDATE_LIMIT).order_by_score(),
            )
            .map_err(|error| format!("无法查询名称索引：{error}"))?;
        let mut entries = Vec::new();
        for (_, address) in top_docs.iter() {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let document = searcher
                .doc::<TantivyDocument>(*address)
                .map_err(|error| format!("无法读取名称索引文档：{error}"))?;
            let Some(path) = text_value(&document, self.schema.path) else {
                continue;
            };
            let path = PathBuf::from(path);
            let Some(root) = roots.iter().find(|root| path_is_within(&path, root)) else {
                continue;
            };
            let name = path
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_default();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                let _ = self.command_tx.send(IndexCommand::PathChanged {
                    path: path.clone(),
                    root: root.clone(),
                });
                continue;
            };
            if is_hidden(&metadata, &name)
                || (metadata.is_dir() && is_generated_search_directory(&path))
                || has_generated_parent(&path, root)
            {
                let _ = self.command_tx.send(IndexCommand::PathChanged {
                    path: path.clone(),
                    root: root.clone(),
                });
                continue;
            }
            let kind = if metadata.is_dir() {
                EntryKind::Directory
            } else {
                EntryKind::File
            };
            let extension = path
                .extension()
                .map(|value| value.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            entries.push(FileEntry {
                path,
                name,
                kind,
                size: metadata.len(),
                modified: metadata.modified().ok(),
                extension,
                git_status: None,
                svn_status: None,
            });
        }
        sort_global_name_entries(&mut entries, query_text);
        let mut seen = HashSet::new();
        entries.retain(|entry| seen.insert(path_key(&entry.path)));
        let truncated =
            top_docs.len() >= QUERY_CANDIDATE_LIMIT || entries.len() > SEARCH_RESULT_LIMIT;
        entries.truncate(SEARCH_RESULT_LIMIT);
        Ok(Some(GlobalNameSearchSnapshot {
            roots,
            query: query_text.to_string(),
            entries,
            truncated,
            partial: !all_usable,
        }))
    }

    fn coverage(&self, roots: &[PathBuf]) -> (bool, bool) {
        let Ok(statuses) = self.statuses.lock() else {
            return (false, false);
        };
        let mut covered_any = false;
        let mut all_usable = true;
        for root in roots {
            let covering = statuses
                .values()
                .filter(|status| path_is_within(root, &status.root))
                .min_by_key(|status| status.root.components().count());
            if let Some(status) = covering {
                covered_any = true;
                all_usable &= status.usable;
            } else {
                all_usable = false;
            }
        }
        (covered_any, all_usable)
    }

    fn mark_pending(&self, root_key: &str) -> bool {
        self.pending
            .lock()
            .map(|mut pending| pending.insert(root_key.to_string()))
            .unwrap_or(false)
    }

    #[cfg(windows)]
    fn has_active_usn_watcher(&self, root: &Path) -> bool {
        self.usn_watchers
            .lock()
            .map(|watchers| {
                watchers.values().any(|watcher| {
                    watcher.alive.load(Ordering::Relaxed) && path_is_within(root, &watcher.root)
                })
            })
            .unwrap_or(false)
    }

    #[cfg(not(windows))]
    fn has_active_usn_watcher(&self, _root: &Path) -> bool {
        false
    }

    #[cfg(windows)]
    fn ensure_usn_watcher(&self, root: &Path, start: JournalCursor) -> Result<(), String> {
        let root_key = path_key(root);
        let mut watchers = self
            .usn_watchers
            .lock()
            .map_err(|_| "USN 监听锁不可用".to_string())?;
        if watchers
            .get(&root_key)
            .is_some_and(|watcher| watcher.alive.load(Ordering::Relaxed))
        {
            return Ok(());
        }
        watchers.remove(&root_key);
        let cancel = Arc::new(AtomicBool::new(false));
        let alive = Arc::new(AtomicBool::new(true));
        let command_tx = self.command_tx.clone();
        let statuses = Arc::clone(&self.statuses);
        let pending = Arc::clone(&self.pending);
        let watcher_root = root.to_path_buf();
        let thread_cancel = Arc::clone(&cancel);
        let thread_alive = Arc::clone(&alive);
        let handle = thread::Builder::new()
            .name(format!("kite-usn-{}", root.display()))
            .spawn(move || {
                usn_watch_loop(
                    watcher_root,
                    start,
                    command_tx,
                    statuses,
                    pending,
                    thread_cancel,
                    thread_alive,
                );
            })
            .map_err(|error| format!("无法启动 USN 监听线程：{error}"))?;
        watchers.insert(
            root_key,
            UsnWatcher {
                root: root.to_path_buf(),
                cancel,
                alive,
                handle: Some(handle),
            },
        );
        Ok(())
    }

    #[cfg(not(windows))]
    fn ensure_usn_watcher(&self, _root: &Path, _start: JournalCursor) -> Result<(), String> {
        Err("USN Journal 仅支持 Windows".to_string())
    }

    fn ensure_watcher(&self, root: &Path) -> Result<(), String> {
        let mut watcher_guard = self
            .watcher
            .lock()
            .map_err(|_| "名称索引监听锁不可用".to_string())?;
        if watcher_guard.is_none() {
            let command_tx = self.command_tx.clone();
            let watched_roots = Arc::clone(&self.watched_roots);
            let index_dir = self.index_dir.clone();
            let watcher =
                notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
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
                        if path_is_within(&path, &index_dir) {
                            continue;
                        }
                        if let Some(root) = best_root(&path, &roots) {
                            if has_generated_parent(&path, &root)
                                || (path_key(&path) != path_key(&root)
                                    && path.is_dir()
                                    && is_generated_search_directory(&path))
                            {
                                continue;
                            }
                            let _ = command_tx.send(IndexCommand::PathChanged { path, root });
                        }
                    }
                })
                .map_err(|error| format!("无法启动名称索引监听：{error}"))?;
            *watcher_guard = Some(watcher);
        }
        let Some(watcher) = watcher_guard.as_mut() else {
            return Ok(());
        };
        let watched_snapshot = self
            .watched_roots
            .lock()
            .map(|roots| roots.iter().cloned().collect::<Vec<_>>())
            .map_err(|_| "名称索引监听范围锁不可用".to_string())?;
        if watched_snapshot
            .iter()
            .any(|watched| path_is_within(root, watched))
        {
            return Ok(());
        }
        for descendant in watched_snapshot
            .iter()
            .filter(|watched| path_is_within(watched, root))
        {
            if watcher.unwatch(descendant).is_ok()
                && let Ok(mut watched) = self.watched_roots.lock()
            {
                watched.remove(descendant);
            }
        }
        watcher
            .watch(root, RecursiveMode::Recursive)
            .map_err(|error| format!("无法监听 {}：{error}", root.display()))?;
        if let Ok(mut watched) = self.watched_roots.lock() {
            watched.insert(root.to_path_buf());
        }
        Ok(())
    }
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn usn_watch_loop(
    root: PathBuf,
    mut cursor: JournalCursor,
    command_tx: mpsc::Sender<IndexCommand>,
    statuses: Arc<Mutex<HashMap<String, RootStatus>>>,
    pending: Arc<Mutex<HashSet<String>>>,
    cancel: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
) {
    let root_key = path_key(&root);
    let mut reader = None;
    while !cancel.load(Ordering::Relaxed) {
        if !root_status_is_ready(&statuses, &root_key) {
            thread::sleep(Duration::from_millis(100));
            continue;
        }
        if reader.is_none() {
            match usn_journal::JournalReader::open(&root) {
                Ok(Some(opened)) => {
                    let current: JournalCursor = opened.cursor().into();
                    if current.journal_id != cursor.journal_id
                        || cursor.next_usn < opened.first_usn()
                        || cursor.next_usn < opened.lowest_valid_usn()
                        || cursor.next_usn > opened.next_usn()
                    {
                        cursor = current;
                        set_journal_recovery(&statuses, &root_key, cursor);
                        if mark_pending(&pending, &root_key)
                            && command_tx
                                .send(IndexCommand::EnsureRoot(root.clone()))
                                .is_err()
                        {
                            break;
                        }
                        continue;
                    }
                    reader = Some(opened);
                }
                Ok(None) => break,
                Err(error) => {
                    set_usn_watcher_failure(&statuses, &root_key, &error);
                    break;
                }
            }
        }
        let Some(opened) = reader.as_ref() else {
            continue;
        };
        if let Some(persisted) = status_journal_cursor(&statuses, &root_key)
            && persisted.journal_id == cursor.journal_id
            && persisted.next_usn < cursor.next_usn
        {
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        match opened.read_available(cursor.next_usn) {
            Ok(batch) => {
                for change in batch.changes {
                    if command_tx
                        .send(IndexCommand::PathChanged {
                            path: change.path,
                            root: root.clone(),
                        })
                        .is_err()
                    {
                        alive.store(false, Ordering::Relaxed);
                        return;
                    }
                }
                if batch.next_usn > cursor.next_usn {
                    cursor.next_usn = batch.next_usn;
                    if command_tx
                        .send(IndexCommand::JournalAdvanced {
                            root: root.clone(),
                            journal_id: cursor.journal_id,
                            next_usn: cursor.next_usn,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
            }
            Err(error) => {
                set_usn_watcher_failure(&statuses, &root_key, &error);
                break;
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    alive.store(false, Ordering::Relaxed);
}

#[cfg(windows)]
fn root_status_is_ready(
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    root_key: &str,
) -> bool {
    statuses
        .lock()
        .ok()
        .and_then(|statuses| {
            statuses.get(root_key).map(|status| {
                status.usable && status.reconciled && status.state == NameIndexState::Ready
            })
        })
        .unwrap_or(false)
}

#[cfg(windows)]
fn status_journal_cursor(
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    root_key: &str,
) -> Option<JournalCursor> {
    statuses.lock().ok()?.get(root_key)?.journal_cursor()
}

#[cfg(windows)]
fn mark_pending(pending: &Arc<Mutex<HashSet<String>>>, root_key: &str) -> bool {
    pending
        .lock()
        .map(|mut pending| pending.insert(root_key.to_string()))
        .unwrap_or(false)
}

#[cfg(windows)]
fn set_journal_recovery(
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    root_key: &str,
    cursor: JournalCursor,
) {
    if let Ok(mut statuses) = statuses.lock()
        && let Some(status) = statuses.get_mut(root_key)
    {
        status.state = NameIndexState::Indexing;
        status.usable = false;
        status.reconciled = false;
        status.provider = NameIndexProvider::UsnJournal;
        status.journal_id = Some(cursor.journal_id);
        status.journal_next_usn = Some(cursor.next_usn);
        status.message = Some("USN Journal 已回退，正在重建名称索引".to_string());
    }
}

#[cfg(windows)]
fn set_usn_watcher_failure(
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    root_key: &str,
    error: &str,
) {
    if let Ok(mut statuses) = statuses.lock()
        && let Some(status) = statuses.get_mut(root_key)
    {
        status.state = NameIndexState::Error;
        status.usable = false;
        status.reconciled = true;
        status.watchable = false;
        status.watch_error = Some(error.to_string());
        status.message = Some(format!("USN Journal 监听失败：{error}"));
    }
}

#[cfg(not(test))]
pub fn shared_service() -> Result<&'static Arc<NameIndexService>, String> {
    static SERVICE: OnceLock<Result<Arc<NameIndexService>, String>> = OnceLock::new();
    SERVICE
        .get_or_init(|| NameIndexService::new().map(Arc::new))
        .as_ref()
        .map_err(Clone::clone)
}

#[cfg(test)]
pub fn shared_service() -> Result<&'static Arc<NameIndexService>, String> {
    Err("单元测试禁用共享名称索引".to_string())
}

fn default_index_dir() -> PathBuf {
    if let Ok(path) = std::env::var("KITE_NAME_INDEX_DIR") {
        return PathBuf::from(path);
    }
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("Kite")
        .join("name-index-v2")
}

fn open_or_create_index(index_dir: &Path) -> Result<(Index, NameFields), String> {
    let schema = name_schema();
    if index_dir.join("meta.json").is_file() {
        let index =
            Index::open_in_dir(index_dir).map_err(|error| format!("无法打开名称索引：{error}"))?;
        let fields = NameFields::from_schema(&index.schema())?;
        return Ok((index, fields));
    }
    fs::create_dir_all(index_dir).map_err(|error| format!("无法创建名称索引目录：{error}"))?;
    let index = Index::create_in_dir(index_dir, schema)
        .map_err(|error| format!("无法创建名称索引：{error}"))?;
    let fields = NameFields::from_schema(&index.schema())?;
    Ok((index, fields))
}

fn name_schema() -> Schema {
    let mut builder = Schema::builder();
    builder.add_text_field("path_key", STRING);
    builder.add_text_field("root_key", STRING);
    builder.add_text_field("name_key", STRING);
    builder.add_text_field("path", STORED);
    builder.build()
}

impl NameFields {
    fn from_schema(schema: &Schema) -> Result<Self, String> {
        Ok(Self {
            path_key: required_field(schema, "path_key")?,
            path: required_field(schema, "path")?,
            root_key: required_field(schema, "root_key")?,
            name_key: required_field(schema, "name_key")?,
        })
    }
}

fn required_field(schema: &Schema, name: &str) -> Result<Field, String> {
    schema
        .get_field(name)
        .map_err(|_| format!("名称索引缺少 {name} 字段"))
}

fn index_worker(
    index: Index,
    schema: NameFields,
    reader: IndexReader,
    command_rx: mpsc::Receiver<IndexCommand>,
    statuses: Arc<Mutex<HashMap<String, RootStatus>>>,
    pending: Arc<Mutex<HashSet<String>>>,
    index_dir: PathBuf,
) {
    let mut writer = match index.writer_with_num_threads(1, INDEX_WRITER_MEMORY_BYTES) {
        Ok(writer) => writer,
        Err(error) => {
            set_global_error(&statuses, format!("无法打开名称索引写入器：{error}"));
            return;
        }
    };
    while let Ok(command) = command_rx.recv() {
        match command {
            IndexCommand::EnsureRoot(root) => {
                let root_key = path_key(&root);
                set_status_indexing(&statuses, &root_key, "正在建立名称索引", false);
                let _ = save_manifest(&index_dir, &statuses);
                let result = index_root(
                    &mut writer,
                    &schema,
                    &reader,
                    &root,
                    &root_key,
                    &statuses,
                    &index_dir,
                );
                finish_command(
                    &mut writer,
                    &reader,
                    &statuses,
                    &pending,
                    &index_dir,
                    &root_key,
                    "正在建立名称索引",
                    result,
                );
            }
            IndexCommand::PathChanged { path, root } => {
                let root_key = path_key(&root);
                set_status_indexing(&statuses, &root_key, "正在更新名称索引", true);
                let result = update_path(&mut writer, &schema, &reader, &path, &root, &index_dir);
                finish_command(
                    &mut writer,
                    &reader,
                    &statuses,
                    &pending,
                    &index_dir,
                    &root_key,
                    "正在更新名称索引",
                    result,
                );
            }
            IndexCommand::JournalAdvanced {
                root,
                journal_id,
                next_usn,
            } => {
                let root_key = path_key(&root);
                if let Ok(mut statuses) = statuses.lock()
                    && let Some(status) = statuses.get_mut(&root_key)
                    && (status.journal_id.is_none()
                        || (status.journal_id == Some(journal_id)
                            && status
                                .journal_next_usn
                                .is_none_or(|current| next_usn > current)))
                {
                    status.provider = NameIndexProvider::UsnJournal;
                    status.journal_id = Some(journal_id);
                    status.journal_next_usn = Some(next_usn);
                }
                let _ = save_manifest(&index_dir, &statuses);
            }
            IndexCommand::RemoveRoot(root) => {
                let root_key = path_key(&root);
                if delete_path_prefix(&mut writer, &reader, &schema, &root).is_ok() {
                    if let Ok(mut statuses) = statuses.lock() {
                        statuses.remove(&root_key);
                    }
                    let _ = writer.commit();
                    let _ = reader.reload();
                    let _ = save_manifest(&index_dir, &statuses);
                }
            }
            IndexCommand::Clear => {
                let _ = writer.delete_all_documents();
                let _ = writer.commit();
                let _ = reader.reload();
                if let Ok(mut statuses) = statuses.lock() {
                    statuses.clear();
                }
                let _ = save_manifest(&index_dir, &statuses);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_command(
    writer: &mut IndexWriter,
    reader: &IndexReader,
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    pending: &Arc<Mutex<HashSet<String>>>,
    index_dir: &Path,
    root_key: &str,
    ready_message: &str,
    result: Result<u64, String>,
) {
    let succeeded = result.is_ok();
    match result {
        Ok(entries) => {
            if let Ok(mut statuses) = statuses.lock()
                && let Some(status) = statuses.get_mut(root_key)
            {
                if entries > 0 || status.indexed_entries == 0 {
                    status.indexed_entries = entries;
                }
                status.usable = status.watchable;
                status.message = status
                    .watch_error
                    .as_ref()
                    .map(|error| format!("名称索引已建立，但无法监听增量更新：{error}"));
                status.reconciled = true;
            }
        }
        Err(error) => {
            if let Ok(mut statuses) = statuses.lock()
                && let Some(status) = statuses.get_mut(root_key)
            {
                status.state = NameIndexState::Error;
                status.usable = false;
                status.message = Some(format!("{ready_message}失败：{error}"));
                status.reconciled = false;
            }
        }
    }
    let _ = writer.commit();
    let _ = reader.reload();
    let _ = save_manifest(index_dir, statuses);
    if succeeded
        && let Ok(mut statuses) = statuses.lock()
        && let Some(status) = statuses.get_mut(root_key)
    {
        status.state = NameIndexState::Ready;
    }
    if let Ok(mut pending) = pending.lock() {
        pending.remove(root_key);
    }
}

fn index_root(
    writer: &mut IndexWriter,
    schema: &NameFields,
    reader: &IndexReader,
    root: &Path,
    root_key: &str,
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    index_dir: &Path,
) -> Result<u64, String> {
    delete_path_prefix(writer, reader, schema, root)?;
    let mut indexed_entries = 0u64;
    let mut dirty = 0usize;
    let mut last_commit = Instant::now();
    let mut builder = WalkBuilder::new(root);
    builder
        .standard_filters(false)
        .hidden(true)
        .follow_links(false)
        .same_file_system(true)
        .threads(1);
    let index_dir = index_dir.to_path_buf();
    builder.filter_entry(move |entry| {
        entry.depth() == 0
            || (!path_is_within(entry.path(), &index_dir)
                && (!entry.file_type().is_some_and(|kind| kind.is_dir())
                    || !is_generated_search_directory(entry.path())))
    });
    for result in builder.build() {
        let Ok(entry) = result else {
            continue;
        };
        let Some(file_type) = entry.file_type() else {
            continue;
        };
        if entry.depth() == 0 || file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        if index_entry(writer, schema, path, root_key, Some(file_type))? {
            dirty += 1;
            indexed_entries += 1;
        }
        if dirty >= COMMIT_DOCUMENT_INTERVAL || last_commit.elapsed() >= COMMIT_TIME_INTERVAL {
            writer
                .commit()
                .map_err(|error| format!("无法提交名称索引：{error}"))?;
            update_index_progress(statuses, root_key, indexed_entries);
            dirty = 0;
            last_commit = Instant::now();
        }
    }
    writer
        .commit()
        .map_err(|error| format!("无法提交名称索引：{error}"))?;
    reader
        .reload()
        .map_err(|error| format!("无法刷新名称索引：{error}"))?;
    Ok(indexed_entries)
}

fn index_entry(
    writer: &mut IndexWriter,
    schema: &NameFields,
    path: &Path,
    root_key: &str,
    file_type: Option<fs::FileType>,
) -> Result<bool, String> {
    let file_type = match file_type {
        Some(file_type) => file_type,
        None => match fs::symlink_metadata(path) {
            Ok(metadata) => metadata.file_type(),
            Err(_) => return Ok(false),
        },
    };
    if file_type.is_symlink() {
        return Ok(false);
    }
    let path_text = path.to_string_lossy().into_owned();
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| path_text.clone());
    let key = path_key(path);
    let mut document = TantivyDocument::default();
    document.add_text(schema.path_key, &key);
    document.add_text(schema.path, &path_text);
    document.add_text(schema.root_key, root_key);
    document.add_text(schema.name_key, name.to_lowercase());
    writer
        .add_document(document)
        .map_err(|error| format!("无法写入名称索引：{error}"))?;
    Ok(true)
}

fn update_path(
    writer: &mut IndexWriter,
    schema: &NameFields,
    reader: &IndexReader,
    path: &Path,
    root: &Path,
    index_dir: &Path,
) -> Result<u64, String> {
    let root_key = path_key(root);
    if path_is_within(path, index_dir) {
        return Ok(0);
    }
    if let Ok(metadata) = fs::symlink_metadata(path) {
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        if (path_key(path) != path_key(root) && is_hidden(&metadata, &name))
            || (path_key(path) != path_key(root)
                && metadata.is_dir()
                && is_generated_search_directory(path))
            || has_generated_parent(path, root)
        {
            delete_path_prefix(writer, reader, schema, path)?;
            writer
                .commit()
                .map_err(|error| format!("无法提交名称索引：{error}"))?;
            reader
                .reload()
                .map_err(|error| format!("无法刷新名称索引：{error}"))?;
            return Ok(0);
        }
    }
    if !path.exists() {
        delete_path_prefix(writer, reader, schema, path)?;
        writer
            .commit()
            .map_err(|error| format!("无法提交名称索引：{error}"))?;
        reader
            .reload()
            .map_err(|error| format!("无法刷新名称索引：{error}"))?;
        return Ok(0);
    }
    if path.is_dir() {
        delete_path_prefix(writer, reader, schema, path)?;
        index_entry(writer, schema, path, &root_key, None)?;
        let mut builder = WalkBuilder::new(path);
        builder
            .standard_filters(false)
            .hidden(true)
            .follow_links(false)
            .same_file_system(true);
        let index_dir = index_dir.to_path_buf();
        builder.filter_entry(move |entry| {
            entry.depth() == 0
                || (!path_is_within(entry.path(), &index_dir)
                    && (!entry.file_type().is_some_and(|kind| kind.is_dir())
                        || !is_generated_search_directory(entry.path())))
        });
        for result in builder.build() {
            if let Ok(entry) = result
                && entry.depth() > 0
                && !entry.file_name().to_string_lossy().starts_with('.')
            {
                let _ = index_entry(writer, schema, entry.path(), &root_key, entry.file_type())?;
            }
        }
    } else if path.is_file() {
        writer.delete_term(Term::from_field_text(schema.path_key, &path_key(path)));
        index_entry(writer, schema, path, &root_key, None)?;
    }
    writer
        .commit()
        .map_err(|error| format!("无法提交名称索引：{error}"))?;
    reader
        .reload()
        .map_err(|error| format!("无法刷新名称索引：{error}"))?;
    Ok(0)
}

fn delete_path_prefix(
    writer: &mut IndexWriter,
    reader: &IndexReader,
    schema: &NameFields,
    path: &Path,
) -> Result<(), String> {
    let prefix = path_key(path);
    let searcher = reader.searcher();
    let query = RegexQuery::from_pattern(&path_scope_pattern(&prefix), schema.path_key)
        .map_err(|error| format!("无法生成名称删除查询：{error}"))?;
    let addresses = searcher
        .search(&query, &DocSetCollector)
        .map_err(|error| format!("无法查询名称索引：{error}"))?;
    for address in addresses {
        let document = searcher
            .doc::<TantivyDocument>(address)
            .map_err(|error| format!("无法读取名称索引文档：{error}"))?;
        if let Some(key) = text_value(&document, schema.path_key) {
            writer.delete_term(Term::from_field_text(schema.path_key, &key));
        }
    }
    Ok(())
}

fn path_scope_pattern(prefix: &str) -> String {
    let escaped = regex::escape(prefix);
    if prefix.ends_with(std::path::MAIN_SEPARATOR) {
        format!("({escaped}|{escaped}.*)")
    } else {
        let separator = regex::escape(std::path::MAIN_SEPARATOR_STR);
        format!("({escaped}|{escaped}{separator}.*)")
    }
}

fn text_value(document: &TantivyDocument, field: Field) -> Option<String> {
    document
        .get_first(field)
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn update_index_progress(
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    root_key: &str,
    indexed_entries: u64,
) {
    if let Ok(mut statuses) = statuses.lock()
        && let Some(status) = statuses.get_mut(root_key)
    {
        status.indexed_entries = indexed_entries;
        status.message = Some(format!("正在建立名称索引：{indexed_entries} 项"));
    }
}

fn set_status_indexing(
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    root_key: &str,
    message: &str,
    preserve_usable: bool,
) {
    if let Ok(mut statuses) = statuses.lock()
        && let Some(status) = statuses.get_mut(root_key)
    {
        status.state = NameIndexState::Indexing;
        if !preserve_usable {
            status.usable = false;
        }
        status.message = Some(message.to_string());
    }
}

fn set_watch_failure(
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    root_key: &str,
    error: &str,
) {
    if let Ok(mut statuses) = statuses.lock()
        && let Some(status) = statuses.get_mut(root_key)
    {
        status.state = NameIndexState::Error;
        status.usable = false;
        status.reconciled = true;
        status.watchable = false;
        status.watch_error = Some(error.to_string());
        status.message = Some(format!("无法监听增量更新：{error}"));
    }
}

fn set_watch_available(
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
    root_key: &str,
    provider: NameIndexProvider,
    journal_cursor: Option<JournalCursor>,
    restore_complete: bool,
) {
    if let Ok(mut statuses) = statuses.lock()
        && let Some(status) = statuses.get_mut(root_key)
    {
        if restore_complete {
            status.usable = true;
            status.reconciled = true;
            status.state = NameIndexState::Ready;
        } else if !status.watchable || status.provider != provider {
            status.reconciled = false;
        }
        status.watchable = true;
        status.watch_error = None;
        status.provider = provider;
        status.journal_id = journal_cursor.map(|cursor| cursor.journal_id);
        status.journal_next_usn = journal_cursor.map(|cursor| cursor.next_usn);
        if status.reconciled {
            status.message = None;
        }
    }
}

fn set_global_error(statuses: &Arc<Mutex<HashMap<String, RootStatus>>>, message: String) {
    if let Ok(mut statuses) = statuses.lock() {
        for status in statuses.values_mut() {
            status.state = NameIndexState::Error;
            status.usable = false;
            status.message = Some(message.clone());
            status.reconciled = false;
        }
    }
}

fn load_manifest(index_dir: &Path) -> HashMap<String, RootStatus> {
    let path = index_dir.join("manifest.json");
    let Ok(bytes) = fs::read(path) else {
        return HashMap::new();
    };
    let Ok(manifest) = serde_json::from_slice::<Manifest>(&bytes) else {
        return HashMap::new();
    };
    if manifest.version != INDEX_VERSION {
        return HashMap::new();
    }
    manifest
        .roots
        .into_iter()
        .map(|root| {
            let path = PathBuf::from(root.path);
            let root_key = path_key(&path);
            (
                root_key.clone(),
                RootStatus {
                    root: path,
                    root_key,
                    state: NameIndexState::Ready,
                    usable: true,
                    indexed_entries: root.indexed_entries,
                    message: None,
                    reconciled: root.provider == NameIndexProvider::UsnJournal
                        && root.journal_id.is_some()
                        && root.journal_next_usn.is_some(),
                    watchable: true,
                    watch_error: None,
                    provider: root.provider,
                    journal_id: root.journal_id,
                    journal_next_usn: root.journal_next_usn,
                },
            )
        })
        .collect()
}

fn save_manifest(
    index_dir: &Path,
    statuses: &Arc<Mutex<HashMap<String, RootStatus>>>,
) -> Result<(), String> {
    let roots = match statuses.lock() {
        Ok(statuses) => statuses
            .values()
            .filter(|status| status.usable)
            .map(|status| ManifestRoot {
                path: status.root.to_string_lossy().into_owned(),
                indexed_entries: status.indexed_entries,
                completed_unix_ms: now_unix_ms(),
                provider: status.provider,
                journal_id: status.journal_id,
                journal_next_usn: status.journal_next_usn,
            })
            .collect(),
        Err(_) => return Err("名称索引状态锁不可用".to_string()),
    };
    let manifest = Manifest {
        version: INDEX_VERSION,
        roots,
    };
    let path = index_dir.join("manifest.json");
    let bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| format!("无法编码名称索引清单：{error}"))?;
    AtomicFile::new(&path, AllowOverwrite)
        .write(|file| file.write_all(&bytes))
        .map_err(|error| format!("无法写入名称索引清单：{error}"))?;
    Ok(())
}

fn normalize_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = roots
        .iter()
        .filter(|root| root.is_dir())
        .cloned()
        .collect::<Vec<_>>();
    roots.sort_by_key(|root| root.components().count());
    let mut result = Vec::<PathBuf>::new();
    let mut seen = HashSet::new();
    for root in roots {
        let key = path_key(&root);
        if !seen.insert(key) {
            continue;
        }
        if result.iter().any(|parent| path_is_within(&root, parent)) {
            continue;
        }
        result.push(root);
    }
    result
}

fn covering_root(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .filter(|root| path_is_within(path, root))
        .max_by_key(|root| root.components().count())
        .cloned()
}

fn best_root(path: &Path, roots: &[PathBuf]) -> Option<PathBuf> {
    covering_root(path, roots)
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    let path = path_key(path);
    let root = path_key(root);
    if path == root {
        return true;
    }
    let child_prefix = if root.ends_with(std::path::MAIN_SEPARATOR) {
        root
    } else {
        root + std::path::MAIN_SEPARATOR_STR
    };
    path.starts_with(&child_prefix)
}

fn path_key(path: &Path) -> String {
    let mut value = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        value = value.replace('/', "\\").to_lowercase();
        let trimmed_len = value.trim_end_matches('\\').len();
        value.truncate(trimmed_len);
        if value.ends_with(':') {
            value.push('\\');
        }
    } else {
        if value != std::path::MAIN_SEPARATOR_STR {
            let trimmed_len = value.trim_end_matches('/').len();
            value.truncate(trimmed_len);
        }
    }
    value
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kite-name-index-{name}-{}-{}",
            std::process::id(),
            now_unix_ms()
        ))
    }

    fn remove_dir_with_retry(path: &Path) {
        for _ in 0..50 {
            if fs::remove_dir_all(path).is_ok() || !path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_ready(service: &NameIndexService, root: &Path) {
        for _ in 0..150 {
            if service
                .status(&[root.to_path_buf()])
                .iter()
                .any(|status| status.state == NameIndexState::Ready && status.usable)
            {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        panic!("name index did not become ready");
    }

    #[test]
    fn scopes_filesystem_root_children() {
        let root = if cfg!(windows) {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        };
        let child = root.join("Users");
        assert!(path_is_within(&child, &root));

        let pattern = path_scope_pattern(&path_key(&root));
        let regex = regex::Regex::new(&pattern).expect("compile root scope pattern");
        assert!(regex.is_match(&path_key(&child)));
    }

    #[test]
    fn updates_name_index_after_notify_events() {
        let root = test_root("updates");
        fs::create_dir_all(&root).expect("create update fixture");
        let index_dir = test_root("update-index");
        let service =
            NameIndexService::open_for_testing(index_dir.clone()).expect("open update index");
        service.ensure_roots(std::slice::from_ref(&root));
        wait_ready(&service, &root);
        let cancel = AtomicBool::new(false);

        let new_file = root.join("wooduan-new.txt");
        fs::write(&new_file, b"fixture").expect("write updated fixture");
        let mut found = false;
        for _ in 0..150 {
            if service
                .search(std::slice::from_ref(&root), "wooduan", &cancel)
                .expect("search updated index")
                .is_some_and(|snapshot| snapshot.entries.iter().any(|entry| entry.path == new_file))
            {
                found = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(found, "notify event did not add the new file");

        fs::remove_file(&new_file).expect("remove updated fixture");
        let mut removed = false;
        for _ in 0..150 {
            let still_present = service
                .search(std::slice::from_ref(&root), "wooduan", &cancel)
                .expect("search removed index")
                .is_some_and(|snapshot| {
                    snapshot.entries.iter().any(|entry| entry.path == new_file)
                });
            if !still_present {
                removed = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(removed, "notify event did not remove the deleted file");
        drop(service);
        fs::remove_dir_all(root).expect("clean update fixture");
        remove_dir_with_retry(&index_dir);
    }

    #[test]
    fn indexes_files_and_directories_and_persists_results() {
        let root = test_root("basic");
        let nested = root.join("wooduan-soc");
        fs::create_dir_all(&nested).expect("create name index fixture");
        fs::write(nested.join("notes.txt"), b"fixture").expect("write name index fixture");
        let index_dir = test_root("index");
        let service = NameIndexService::open_for_testing(index_dir.clone()).expect("open index");
        service.ensure_roots(std::slice::from_ref(&root));
        wait_ready(&service, &root);
        let cancel = AtomicBool::new(false);
        let status = service
            .status(std::slice::from_ref(&root))
            .into_iter()
            .next()
            .expect("index status");
        assert_eq!(status.provider, NameIndexProvider::Notify);
        assert_eq!(status.coverage, NameIndexCoverage::Complete);
        let snapshot = service
            .search(std::slice::from_ref(&root), "wooduan", &cancel)
            .expect("search index")
            .expect("indexed results");
        assert_eq!(snapshot.entries.len(), 1);
        assert!(!snapshot.partial);
        drop(service);

        let service = NameIndexService::open_for_testing(index_dir.clone()).expect("reopen index");
        assert!(
            service.reader.searcher().num_docs() > 0,
            "persisted index is empty"
        );
        let snapshot = service
            .search(std::slice::from_ref(&root), "wooduan", &cancel)
            .expect("search persisted index")
            .expect("persisted results");
        assert_eq!(snapshot.entries.len(), 1);
        assert!(!snapshot.partial);
        drop(service);
        fs::remove_dir_all(root).expect("clean name index fixture");
        remove_dir_with_retry(&index_dir);
    }

    #[test]
    fn does_not_index_its_own_index_directory() {
        let root = test_root("self");
        fs::create_dir_all(&root).expect("create self-index fixture");
        let index_dir = root.join("name-index-v2");
        let service =
            NameIndexService::open_for_testing(index_dir.clone()).expect("open self index");
        service.ensure_roots(std::slice::from_ref(&root));
        wait_ready(&service, &root);
        thread::sleep(Duration::from_millis(100));

        let cancel = AtomicBool::new(false);
        let snapshot = service
            .search(std::slice::from_ref(&root), "meta", &cancel)
            .expect("search self index")
            .expect("covered self index");
        assert!(
            snapshot
                .entries
                .iter()
                .all(|entry| !path_is_within(&entry.path, &index_dir)),
            "name index stored its own files"
        );

        drop(service);
        fs::remove_dir_all(root).expect("clean self-index fixture");
    }

    #[test]
    fn allows_generated_directory_as_explicit_search_root() {
        let base = test_root("generated-root");
        let root = base.join("target");
        fs::create_dir_all(&root).expect("create generated root fixture");
        fs::write(root.join("wooduan-explicit.txt"), b"fixture")
            .expect("write generated root file");
        let index_dir = test_root("generated-root-index");
        let service = NameIndexService::open_for_testing(index_dir.clone())
            .expect("open generated root index");
        service.ensure_roots(std::slice::from_ref(&root));
        wait_ready(&service, &root);

        let cancel = AtomicBool::new(false);
        let snapshot = service
            .search(std::slice::from_ref(&root), "wooduan", &cancel)
            .expect("search generated root")
            .expect("covered generated root");
        assert!(
            snapshot
                .entries
                .iter()
                .any(|entry| entry.path == root.join("wooduan-explicit.txt"))
        );

        drop(service);
        fs::remove_dir_all(base).expect("clean generated root fixture");
        remove_dir_with_retry(&index_dir);
    }

    #[test]
    #[cfg(windows)]
    fn restores_usn_cursor_as_complete_manifest_state() {
        let index_dir = test_root("manifest");
        fs::create_dir_all(&index_dir).expect("create manifest fixture");
        let root_path = test_root("manifest-root");
        let manifest = Manifest {
            version: INDEX_VERSION,
            roots: vec![ManifestRoot {
                path: root_path.to_string_lossy().into_owned(),
                indexed_entries: 42,
                completed_unix_ms: 1,
                provider: NameIndexProvider::UsnJournal,
                journal_id: Some(7),
                journal_next_usn: Some(100),
            }],
        };
        fs::write(
            index_dir.join("manifest.json"),
            serde_json::to_vec(&manifest).expect("encode manifest"),
        )
        .expect("write manifest");

        let statuses = load_manifest(&index_dir);
        let status = statuses
            .get(&path_key(&root_path))
            .expect("restored status");
        assert_eq!(status.provider, NameIndexProvider::UsnJournal);
        assert!(status.reconciled);
        assert!(status.usable);
        assert_eq!(
            status.journal_cursor(),
            Some(JournalCursor {
                journal_id: 7,
                next_usn: 100,
            })
        );
        remove_dir_with_retry(&index_dir);
    }
}
