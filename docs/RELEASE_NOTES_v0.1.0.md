# Kite v0.1.0

Kite is a native Windows file manager built with Rust and GPUI.

## Highlights

- Column view with clickable path bar.
- List view with Git and SVN status badges.
- Native Windows file operations.
- Internal copy, cut, and paste.
- Context menu actions.
- Light and dark themes.
- Custom glass-style application icon.

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git status badges.
- SVN is optional. Install SVN for SVN status badges.

## Install

1. Download `Kite-v0.1.0-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Screenshots

![Column view](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/column-view.png)

![Wide hierarchy](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/column-view-wide.png)

![Context menu](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/context-menu.png)

## Verification

- `cargo test`: 14 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Release executable starts and opens a folder.