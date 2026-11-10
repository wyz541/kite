use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use crate::fs::{GlobalNameSearchSnapshot, SEARCH_RESULT_LIMIT, sort_global_name_entries};
use crate::search_filters::SearchFilters;
use crate::{fs, indexed_search, name_index};

pub fn ensure_roots(roots: &[PathBuf]) {
    if let Ok(service) = name_index::shared_service() {
        service.ensure_roots(roots);
    }
}

pub fn status(roots: &[PathBuf]) -> Vec<name_index::NameIndexStatus> {
    name_index::shared_service()
        .map(|service| service.status(roots))
        .unwrap_or_default()
}

pub fn search_names(
    roots: &[PathBuf],
    query: &str,
    filters: SearchFilters,
    cancel: &AtomicBool,
) -> Result<Option<GlobalNameSearchSnapshot>, String> {
    ensure_roots(roots);
    let service = name_index::shared_service()
        .ok()
        .map(|service| service.as_ref());
    search_names_with_service(roots, query, filters, cancel, service)
}

fn search_names_with_service(
    roots: &[PathBuf],
    query: &str,
    filters: SearchFilters,
    cancel: &AtomicBool,
    service: Option<&name_index::NameIndexService>,
) -> Result<Option<GlobalNameSearchSnapshot>, String> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    let indexed = service.and_then(|service| {
        service
            .search_with_filters(roots, query, filters, cancel)
            .ok()
            .flatten()
    });
    if let Some(snapshot) = indexed.as_ref()
        && !snapshot.partial
    {
        return Ok(Some(snapshot.clone()));
    }

    let accelerated = indexed_search::search_indexed_names(roots, query, filters, cancel)
        .ok()
        .flatten();
    let walked = fs::search_global_names_with_filters(roots, query, filters, cancel)?;
    let Some(walked) = walked else {
        return Ok(None);
    };
    let mut truncated = walked.truncated
        || indexed.as_ref().is_some_and(|snapshot| snapshot.truncated)
        || accelerated
            .as_ref()
            .is_some_and(|snapshot| snapshot.truncated);
    let mut entries = Vec::new();
    if let Some(snapshot) = indexed {
        entries.extend(snapshot.entries);
    }
    if let Some(snapshot) = accelerated {
        entries.extend(snapshot.entries);
    }
    entries.extend(walked.entries);
    sort_global_name_entries(&mut entries, query);
    let mut seen = HashSet::new();
    entries.retain(|entry| seen.insert(path_key(&entry.path)));
    truncated |= entries.len() > SEARCH_RESULT_LIMIT;
    entries.truncate(SEARCH_RESULT_LIMIT);
    Ok(Some(GlobalNameSearchSnapshot {
        roots: walked.roots,
        query: walked.query,
        entries,
        truncated,
        partial: false,
    }))
}

fn path_key(path: &std::path::Path) -> String {
    let value = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        value.replace('/', "\\").to_lowercase()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use std::fs as std_fs;
    use std::path::Path;
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::name_index::{NameIndexService, NameIndexState};

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kite-global-index-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
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

    fn remove_dir_with_retry(path: &Path) {
        for _ in 0..50 {
            if std_fs::remove_dir_all(path).is_ok() || !path.exists() {
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn complete_name_index_returns_without_full_walk() {
        let root = test_path("root");
        std_fs::create_dir_all(&root).expect("create indexed root");
        std_fs::write(root.join("needle.txt"), b"fixture").expect("write indexed file");
        let index_dir = test_path("index");
        let service =
            NameIndexService::open_for_testing(index_dir.clone()).expect("open name index");
        service.ensure_roots(std::slice::from_ref(&root));
        wait_ready(&service, &root);
        let walks_before = fs::GLOBAL_NAME_SEARCH_WALKS.with(std::cell::Cell::get);
        let cancel = AtomicBool::new(false);

        let snapshot = search_names_with_service(
            std::slice::from_ref(&root),
            "needle",
            SearchFilters::default(),
            &cancel,
            Some(&service),
        )
        .expect("search indexed names")
        .expect("indexed name snapshot");

        assert_eq!(snapshot.entries.len(), 1);
        assert!(!snapshot.partial);
        assert_eq!(
            fs::GLOBAL_NAME_SEARCH_WALKS.with(std::cell::Cell::get),
            walks_before
        );
        drop(service);
        remove_dir_with_retry(&root);
        remove_dir_with_retry(&index_dir);
    }
}
