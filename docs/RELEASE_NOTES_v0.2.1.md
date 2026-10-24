# Kite v0.2.1

Kite is a native Windows file manager built with Rust and GPUI.

## Fixes

- Path bar navigation reuses loaded column panes.
- Unchanged ancestor panes no longer enter a loading state.
- Path bar navigation keeps the existing column data and truncates deeper panes.

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git status badges.
- SVN is optional. Install SVN for SVN status badges.

## Install

1. Download `Kite-v0.2.1-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Screenshots

![Tabbed workspace](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/tabbed-view.png)

## Verification

- `cargo test`: 18 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Path bar navigation was checked in column view.
