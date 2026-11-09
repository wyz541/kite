use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use crate::fs::GlobalNameSearchSnapshot;
use crate::kensaku_search::{self, KensakuContentStatus};
use crate::search_filters::SearchFilters;

pub fn ensure_roots(roots: &[PathBuf]) {
    let _ = kensaku_search::prepare_names(roots);
}

pub fn status(_roots: &[PathBuf]) -> Vec<KensakuContentStatus> {
    kensaku_search::content_status().into_iter().collect()
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
    kensaku_search::prepare_names(roots)?;
    match kensaku_search::search_names(roots, query, filters, cancel)? {
        Some(snapshot) => Ok(Some(snapshot)),
        None if cancel.load(std::sync::atomic::Ordering::Relaxed) => Ok(None),
        None => Ok(Some(GlobalNameSearchSnapshot {
            roots: roots.to_vec(),
            query: query.to_string(),
            entries: Vec::new(),
            truncated: false,
            partial: true,
        })),
    }
}
