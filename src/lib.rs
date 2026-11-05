uniffi::setup_scaffolding!();

pub mod archive;
pub mod devices;
pub mod favorites;
pub mod ffi;
#[cfg(windows)]
pub mod file_watcher;
pub mod fs;
pub mod platform;
pub mod session;
pub mod text_editor;
pub mod ui_preferences;
pub mod updater;
pub mod vcs;
