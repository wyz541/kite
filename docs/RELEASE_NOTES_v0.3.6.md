# Kite v0.3.6

Kite is a native Windows file manager built with Rust and GPUI.

## Fix

- Restrict the column strip to horizontal scrolling.
- Stop vertical wheel input from changing the column scroll offset.
- Keep vertical wheel input inside the directory list.
- Preserve horizontal trackpad gestures and scrollbar dragging.

## Behavior

- Vertical wheel over a directory scrolls only that directory.
- Horizontal trackpad gesture over a column scrolls the column strip.
- Dragging the horizontal scrollbar still changes the column offset.
- Directory vertical position no longer changes during horizontal scrolling.

## Requirements

- Windows 10 or Windows 11 x64.

## Install

1. Download `Kite-v0.3.6-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 24 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Scroll-axis restriction compiles with the current GPUI version.
