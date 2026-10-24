# Kite v0.3.1

Kite is a native Windows file manager built with Rust and GPUI.

## Changes

- Move Git operations into a Git submenu.
- Move SVN operations into an SVN submenu.
- Keep the top-level context menu compact.
- Show a submenu caret for both VCS entries.
- Preserve repository-aware Git and SVN menu visibility.

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git operations.
- SVN is optional. Install SVN for SVN operations.

## Install

1. Download `Kite-v0.3.1-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Screenshots

![Git submenu entry](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/vcs-context-menu.png)

## Verification

- `cargo test`: 23 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Top-level Git entry and submenu caret were checked in list view.
