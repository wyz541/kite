use std::path::PathBuf;

use crate::fs::EntryKind;
use crate::kensaku_search;
use crate::search_filters::SearchFilters;

pub const CONTENT_SEARCH_MAX_FILES: usize = 50;

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

pub fn prepare_roots(roots: &[PathBuf]) -> Result<(), String> {
    kensaku_search::prepare_content(roots)
}

pub fn status() -> ContentIndexStatus {
    match kensaku_search::content_status() {
        Some(status) => ContentIndexStatus {
            state: status.state,
            indexed_files: status.indexed_files,
            message: status.message,
        },
        None => ContentIndexStatus {
            state: ContentIndexState::Error,
            indexed_files: 0,
            message: Some("Kensaku 索引尚不可用".to_string()),
        },
    }
}

pub fn note_interaction() {}

pub fn set_indexing_paused(_paused: bool) {}

pub fn cancel_search() {
    kensaku_search::cancel_content_search();
}

pub fn clear() -> Result<(), String> {
    Err("Kensaku 索引由 Kensaku daemon 管理，Kite 不删除该索引".to_string())
}

pub fn search_with_filters(
    roots: &[PathBuf],
    query: &str,
    regex: bool,
    max_files: usize,
    filters: SearchFilters,
) -> Result<Option<ContentSearchSnapshot>, String> {
    kensaku_search::prepare_content(roots)?;
    match kensaku_search::search_content(roots, query, regex, max_files, filters)? {
        Some(snapshot) => Ok(Some(snapshot)),
        None if kensaku_search::content_search_cancelled() => Ok(None),
        None => Err("Kensaku 索引尚未就绪".to_string()),
    }
}
