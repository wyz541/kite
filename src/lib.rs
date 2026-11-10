uniffi::setup_scaffolding!();

pub mod archive;
pub mod content_search;
pub mod devices;
pub mod favorites;
pub mod ffi;
#[cfg(windows)]
pub mod file_watcher;
pub mod fs;
pub mod global_name_search;
pub mod indexed_search;
pub mod name_index;
pub mod perf_trace;
pub mod platform;
#[cfg(windows)]
pub mod recycle_bin;
pub mod search_filters;
pub mod session;
pub mod text_editor;
pub mod ui_preferences;
pub mod update_notes;
pub mod updater;
#[cfg(windows)]
pub mod usn_journal;
pub mod vcs;
