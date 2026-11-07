use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use crate::fs::GlobalNameSearchSnapshot;

#[cfg(not(windows))]
pub fn search_indexed_names(
    _roots: &[PathBuf],
    _query: &str,
    _cancel: &AtomicBool,
) -> Result<Option<GlobalNameSearchSnapshot>, String> {
    Ok(None)
}

#[cfg(windows)]
pub fn search_indexed_names(
    roots: &[PathBuf],
    query: &str,
    cancel: &AtomicBool,
) -> Result<Option<GlobalNameSearchSnapshot>, String> {
    windows::search(roots, query, cancel).map_err(|error| error.to_string())
}

#[cfg(windows)]
mod windows {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};

    use windows::Win32::Foundation::E_UNEXPECTED;
    use windows::Win32::System::Com::{
        CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
        CoUninitialize, IEnumUnknown,
    };
    use windows::Win32::System::Search::{
        CSearchManager, IQueryParser, IQueryParserManager, ISearchManager, QueryParserManager,
    };
    use windows::Win32::UI::Shell::{
        BHID_EnumItems, IEnumShellItems, ISearchFolderItemFactory, IShellItem, IShellItemArray,
        PathCreateFromUrlAlloc, SHCreateItemFromParsingName, SHCreateShellItemArrayFromShellItem,
        SIGDN_DESKTOPABSOLUTEPARSING, SearchFolderItemFactory,
    };
    use windows::core::{HSTRING, PWSTR, Result as WindowsResult, w};

    use super::GlobalNameSearchSnapshot;
    use crate::fs::{
        SEARCH_RESULT_LIMIT, file_entry_from_metadata, is_generated_search_directory, is_hidden,
        sort_global_name_entries,
    };

    struct ComApartment {
        owned: bool,
    }

    impl ComApartment {
        fn new() -> Self {
            let result = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            Self {
                owned: result.is_ok(),
            }
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            if self.owned {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub(super) fn search(
        roots: &[PathBuf],
        query: &str,
        cancel: &AtomicBool,
    ) -> WindowsResult<Option<GlobalNameSearchSnapshot>> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(None);
        }
        let _com = ComApartment::new();
        let indexed_roots = indexed_file_roots()?;
        let roots = roots
            .iter()
            .filter(|root| root_is_indexed(root, &indexed_roots))
            .cloned()
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return Ok(None);
        }

        let manager: IQueryParserManager =
            unsafe { CoCreateInstance(&QueryParserManager, None, CLSCTX_ALL)? };
        let parser: IQueryParser =
            unsafe { manager.CreateLoadedParser(w!("SystemIndex"), 0x0409)? };
        unsafe { manager.InitializeOptions(true, true, &parser)? };
        let indexed_query = format!("System.FileName:\"{}\"", query.replace('"', "\\\""));
        let solution =
            unsafe { parser.Parse(&HSTRING::from(indexed_query), None::<&IEnumUnknown>)? };
        let mut condition = None;
        unsafe { solution.GetQuery(Some(&mut condition), None)? };
        let condition = condition.ok_or_else(|| windows::core::Error::from(E_UNEXPECTED))?;
        let query_lower = query.to_lowercase();
        let mut entries = Vec::new();
        let mut truncated = false;
        'roots: for root in &roots {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let root_text = HSTRING::from(root.as_os_str());
            let root_item: IShellItem = unsafe {
                SHCreateItemFromParsingName(
                    &root_text,
                    None::<&windows::Win32::System::Com::IBindCtx>,
                )?
            };
            let scope: IShellItemArray =
                unsafe { SHCreateShellItemArrayFromShellItem(&root_item)? };
            let factory: ISearchFolderItemFactory =
                unsafe { CoCreateInstance(&SearchFolderItemFactory, None, CLSCTX_ALL)? };
            unsafe {
                factory.SetDisplayName(w!("Kite global name search"))?;
                factory.SetScope(&scope)?;
                factory.SetCondition(&condition)?;
            }
            let search_item: IShellItem = unsafe { factory.GetShellItem()? };
            let items: IEnumShellItems = unsafe {
                search_item.BindToHandler(
                    None::<&windows::Win32::System::Com::IBindCtx>,
                    &BHID_EnumItems,
                )?
            };

            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(None);
                }
                let mut fetched = 0u32;
                let mut batch = [None];
                unsafe { items.Next(&mut batch, Some(&mut fetched))? };
                if fetched == 0 {
                    break;
                }
                let Some(item) = batch[0].as_ref() else {
                    continue;
                };
                let path = unsafe { item.GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING)? };
                let path_text = unsafe { path.to_string() }.unwrap_or_default();
                let path = PathBuf::from(path_text);
                if !path_is_within(&path, root) {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.is_empty()
                    || name.starts_with('.')
                    || !name.to_lowercase().contains(&query_lower)
                {
                    continue;
                }
                let Ok(metadata) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if is_hidden(&metadata, &name)
                    || (metadata.is_dir() && is_generated_search_directory(&path))
                {
                    continue;
                }
                if entries.len() > SEARCH_RESULT_LIMIT {
                    truncated = true;
                    break 'roots;
                }
                entries.push(file_entry_from_metadata(path, name, &metadata));
            }
        }

        if entries.is_empty() {
            return Ok(None);
        }
        sort_global_name_entries(&mut entries, query);
        let mut seen = std::collections::HashSet::new();
        entries.retain(|entry| seen.insert(entry.path.to_string_lossy().to_lowercase()));
        truncated |= entries.len() > SEARCH_RESULT_LIMIT;
        entries.truncate(SEARCH_RESULT_LIMIT);
        Ok(Some(GlobalNameSearchSnapshot {
            roots,
            query: query.to_string(),
            entries,
            truncated,
            partial: true,
        }))
    }

    fn indexed_file_roots() -> WindowsResult<Vec<PathBuf>> {
        unsafe {
            let manager: ISearchManager = CoCreateInstance(&CSearchManager, None, CLSCTX_ALL)?;
            let catalog = manager.GetCatalog(w!("SystemIndex"))?;
            let crawl = catalog.GetCrawlScopeManager()?;
            let roots = crawl.EnumerateRoots()?;
            let mut result = Vec::new();
            loop {
                let mut fetched = 0u32;
                let mut batch = [None];
                roots.Next(&mut batch, &mut fetched)?;
                if fetched == 0 {
                    break;
                }
                let Some(root) = batch[0].as_ref() else {
                    continue;
                };
                let url = root.RootURL()?;
                let url = url.to_string().unwrap_or_default();
                if let Some(path) = file_url_to_path(&url) {
                    result.push(path);
                }
            }
            Ok(result)
        }
    }

    fn file_url_to_path(url: &str) -> Option<PathBuf> {
        if !url.to_ascii_lowercase().starts_with("file:") {
            return None;
        }
        let url = HSTRING::from(url);
        let mut path = PWSTR::null();
        unsafe { PathCreateFromUrlAlloc(&url, &mut path, 0).ok()? };
        let text = unsafe { path.to_string() }.ok()?;
        unsafe { CoTaskMemFree(Some(path.0.cast())) };
        Some(PathBuf::from(text))
    }

    fn root_is_indexed(root: &Path, indexed_roots: &[PathBuf]) -> bool {
        indexed_roots
            .iter()
            .any(|indexed| path_is_within(root, indexed))
    }

    fn path_is_within(path: &Path, root: &Path) -> bool {
        let path = normalized_path_key(path);
        let root = normalized_path_key(root);
        path == root || path.starts_with(&(root + "\\"))
    }

    fn normalized_path_key(path: &Path) -> String {
        path.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn detects_indexed_roots_with_windows_path_boundaries() {
            let roots = vec![PathBuf::from("C:\\")];
            assert!(root_is_indexed(Path::new("C:\\Users"), &roots));
            assert!(root_is_indexed(Path::new("C:\\"), &roots));
            assert!(!root_is_indexed(Path::new("D:\\"), &roots));
        }
    }
}
