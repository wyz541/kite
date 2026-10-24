# Kite v0.2.2

Kite is a native Windows file manager built with Rust and GPUI.

## Changes

- Column count has no fixed upper limit.
- Window width and column width control the visible column count.
- Deep columns remain available in the horizontal scroll area.
- A persistent horizontal scrollbar supports dragging.
- Trackpad horizontal gestures continue to work.

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git status badges.
- SVN is optional. Install SVN for SVN status badges.

## Install

1. Download `Kite-v0.2.2-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Screenshots

![Horizontal column scrolling](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/column-horizontal-scroll.png)

![Tabbed workspace](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/tabbed-view.png)

## Verification

- `cargo test`: 18 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Horizontal scrollbar is visible in release builds.
