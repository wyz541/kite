use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use crate::fs::GlobalNameSearchSnapshot;

#[cfg(target_os = "macos")]
pub fn search_indexed_names(
    roots: &[PathBuf],
    query: &str,
    cancel: &AtomicBool,
) -> Result<Option<GlobalNameSearchSnapshot>, String> {
    macos::search(roots, query, cancel)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn search_indexed_names(
    _roots: &[PathBuf],
    _query: &str,
    _cancel: &AtomicBool,
) -> Result<Option<GlobalNameSearchSnapshot>, String> {
    Ok(None)
}

#[cfg(target_os = "macos")]
mod macos {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::atomic::AtomicBool;

    use super::GlobalNameSearchSnapshot;
    use crate::fs::{
        FileEntry, SEARCH_RESULT_LIMIT, file_entry_from_metadata, has_generated_parent,
        is_generated_search_directory, is_hidden, sort_global_name_entries,
    };

    pub(super) fn search(
        roots: &[PathBuf],
        query: &str,
        cancel: &AtomicBool,
    ) -> Result<Option<GlobalNameSearchSnapshot>, String> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(None);
        }
        let mut entries = Vec::new();
        for root in roots {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return Ok(None);
            }
            let output = match Command::new("mdfind")
                .arg("-0")
                .arg("-onlyin")
                .arg(root)
                .arg("-name")
                .arg(query)
                .output()
            {
                Ok(output) if output.status.success() => output,
                Ok(_) | Err(_) => continue,
            };
            for bytes in output.stdout.split(|value| *value == 0) {
                if bytes.is_empty() {
                    continue;
                }
                let path = PathBuf::from(String::from_utf8_lossy(bytes).into_owned());
                if !path_is_within(&path, root) {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if name.is_empty() || !name.to_lowercase().contains(&query.to_lowercase()) {
                    continue;
                }
                let Ok(metadata) = fs::symlink_metadata(&path) else {
                    continue;
                };
                if is_hidden(&metadata, &name)
                    || (metadata.is_dir() && is_generated_search_directory(&path))
                    || has_generated_parent(&path, root)
                {
                    continue;
                }
                entries.push(file_entry_from_metadata(path, name, &metadata));
                if entries.len() > SEARCH_RESULT_LIMIT {
                    break;
                }
            }
        }
        if entries.is_empty() {
            return Ok(None);
        }
        sort_global_name_entries(&mut entries, query);
        let mut seen = std::collections::HashSet::new();
        entries.retain(|entry| seen.insert(entry.path.to_string_lossy().into_owned()));
        let truncated = entries.len() > SEARCH_RESULT_LIMIT;
        entries.truncate(SEARCH_RESULT_LIMIT);
        Ok(Some(GlobalNameSearchSnapshot {
            roots: roots.to_vec(),
            query: query.to_string(),
            entries,
            truncated,
            partial: true,
        }))
    }

    fn path_is_within(path: &Path, root: &Path) -> bool {
        path == root || path.starts_with(root)
    }
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
        FileEntry, SEARCH_RESULT_LIMIT, file_entry_from_metadata, has_generated_parent,
        is_generated_search_directory, is_hidden, sort_global_name_entries,
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
        if let Some(snapshot) = everything::search(roots, query, cancel) {
            return Ok(Some(snapshot));
        }
        let _com = ComApartment::new();
        let indexed_roots = indexed_file_roots()?;
        let roots = windows_search_roots(roots)
            .into_iter()
            .filter(|root| root_is_indexed(root, &indexed_roots))
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
                if name.is_empty() || !name.to_lowercase().contains(&query_lower) {
                    continue;
                }
                let Some(entry) = build_entry(path, name, &roots) else {
                    continue;
                };
                if entries.len() > SEARCH_RESULT_LIMIT {
                    truncated = true;
                    break 'roots;
                }
                entries.push(entry);
            }
        }

        if entries.is_empty() {
            return Ok(None);
        }
        Ok(Some(finish_entries(roots, query, entries, truncated)))
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

    fn windows_search_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
        let home = dirs::home_dir();
        let mut result = Vec::new();
        for root in roots {
            if is_drive_root(root) {
                if let Some(home) = home.as_ref()
                    && path_is_within(home, root)
                {
                    result.push(home.clone());
                }
            } else {
                result.push(root.clone());
            }
        }
        result.sort();
        result.dedup();
        result
    }

    fn is_drive_root(path: &Path) -> bool {
        path.parent().is_none()
            && path
                .to_string_lossy()
                .trim_end_matches(['\\', '/'])
                .ends_with(':')
    }

    fn build_entry(path: PathBuf, name: String, roots: &[PathBuf]) -> Option<FileEntry> {
        if name.is_empty() || name.starts_with('.') {
            return None;
        }
        let root = roots.iter().find(|root| path_is_within(&path, root))?;
        let metadata = fs::symlink_metadata(&path).ok()?;
        if is_hidden(&metadata, &name)
            || (metadata.is_dir() && is_generated_search_directory(&path))
            || has_generated_parent(&path, root)
        {
            return None;
        }
        Some(file_entry_from_metadata(path, name, &metadata))
    }

    fn finish_entries(
        roots: Vec<PathBuf>,
        query: &str,
        mut entries: Vec<FileEntry>,
        mut truncated: bool,
    ) -> GlobalNameSearchSnapshot {
        sort_global_name_entries(&mut entries, query);
        let mut seen = std::collections::HashSet::new();
        entries.retain(|entry| seen.insert(entry.path.to_string_lossy().to_lowercase()));
        truncated |= entries.len() > SEARCH_RESULT_LIMIT;
        entries.truncate(SEARCH_RESULT_LIMIT);
        GlobalNameSearchSnapshot {
            roots,
            query: query.to_string(),
            entries,
            truncated,
            partial: true,
        }
    }

    mod everything {
        use std::path::PathBuf;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, Ordering};

        use libloading::{Library, Symbol};

        use crate::fs::FileEntry;

        type SetSearchW = unsafe extern "system" fn(*const u16);
        type SetMatchCase = unsafe extern "system" fn(i32);
        type SetMatchPath = unsafe extern "system" fn(i32);
        type SetMatchWholeWord = unsafe extern "system" fn(i32);
        type SetMax = unsafe extern "system" fn(u32);
        type QueryW = unsafe extern "system" fn(i32) -> i32;
        type GetNumResults = unsafe extern "system" fn() -> u32;
        type GetResultFullPathNameW = unsafe extern "system" fn(u32, *mut u16, u32) -> u32;
        type CleanUp = unsafe extern "system" fn();

        static EVERYTHING_LOCK: Mutex<()> = Mutex::new(());

        pub(super) fn search(
            roots: &[PathBuf],
            query: &str,
            cancel: &AtomicBool,
        ) -> Option<super::GlobalNameSearchSnapshot> {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let _guard = EVERYTHING_LOCK.lock().ok()?;
            let library = load_library()?;
            unsafe {
                let set_search: Symbol<SetSearchW> =
                    library.get(b"Everything_SetSearchW\0").ok()?;
                let set_match_case: Symbol<SetMatchCase> =
                    library.get(b"Everything_SetMatchCase\0").ok()?;
                let set_match_path: Symbol<SetMatchPath> =
                    library.get(b"Everything_SetMatchPath\0").ok()?;
                let set_match_whole_word: Symbol<SetMatchWholeWord> =
                    library.get(b"Everything_SetMatchWholeWord\0").ok()?;
                let query_results: Symbol<QueryW> = library.get(b"Everything_QueryW\0").ok()?;
                let get_num_results: Symbol<GetNumResults> =
                    library.get(b"Everything_GetNumResults\0").ok()?;
                let get_full_path: Symbol<GetResultFullPathNameW> =
                    library.get(b"Everything_GetResultFullPathNameW\0").ok()?;
                let cleanup = library.get::<CleanUp>(b"Everything_CleanUp\0").ok();
                let set_max = library.get::<SetMax>(b"Everything_SetMax\0").ok();

                if let Some(cleanup) = cleanup.as_ref() {
                    cleanup();
                }
                let mut wide_query = query.encode_utf16().collect::<Vec<_>>();
                wide_query.push(0);
                set_search(wide_query.as_ptr());
                set_match_case(0);
                set_match_path(0);
                set_match_whole_word(0);
                if let Some(set_max) = set_max.as_ref() {
                    set_max(super::SEARCH_RESULT_LIMIT as u32 + 1);
                }
                if query_results(1) == 0 {
                    if let Some(cleanup) = cleanup.as_ref() {
                        cleanup();
                    }
                    return None;
                }
                let count = get_num_results().min(super::SEARCH_RESULT_LIMIT as u32 + 1);
                let mut entries: Vec<FileEntry> = Vec::new();
                let mut buffer = vec![0u16; 32_768];
                for index in 0..count {
                    if cancel.load(Ordering::Relaxed) {
                        return None;
                    }
                    let length = get_full_path(index, buffer.as_mut_ptr(), buffer.len() as u32);
                    if length == 0 {
                        continue;
                    }
                    let end = buffer
                        .iter()
                        .position(|value| *value == 0)
                        .unwrap_or(length as usize)
                        .min(buffer.len());
                    let path = PathBuf::from(String::from_utf16_lossy(&buffer[..end]));
                    let name = path
                        .file_name()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if name.to_lowercase().contains(&query.to_lowercase())
                        && let Some(entry) = super::build_entry(path, name, roots)
                    {
                        entries.push(entry);
                    }
                }
                if let Some(cleanup) = cleanup.as_ref() {
                    cleanup();
                }
                if entries.is_empty() {
                    return None;
                }
                Some(super::finish_entries(roots.to_vec(), query, entries, false))
            }
        }

        fn load_library() -> Option<Library> {
            let mut candidates = vec![PathBuf::from("Everything64.dll")];
            if let Ok(executable) = std::env::current_exe()
                && let Some(parent) = executable.parent()
            {
                candidates.push(parent.join("Everything64.dll"));
            }
            for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Ok(directory) = std::env::var(variable) {
                    candidates.push(
                        PathBuf::from(directory)
                            .join("Everything")
                            .join("Everything64.dll"),
                    );
                }
            }
            candidates
                .into_iter()
                .find_map(|path| unsafe { Library::new(path).ok() })
        }
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
