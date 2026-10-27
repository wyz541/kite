# Kite v0.3.7

Kite is a native Windows file manager built with Rust and GPUI.

## Fix

- Remove the fixed horizontal-only scroll axis.
- Route wheel input from the directory list to the column strip automatically.
- Keep horizontal gestures on the column strip.
- Keep directory scroll positions separate for each column and path.
- Reset list scroll state when a column path changes.

## Behavior

- The directory under the pointer scrolls first.
- At the top or bottom boundary, the remaining wheel delta scrolls the column strip.
- A short or empty directory does not block column scrolling.
- A horizontal trackpad gesture always scrolls the column strip.
- The horizontal scrollbar continues to work.




## Requirements

- Windows 10 or Windows 11 x64.

## Install

1. Download `Kite-v0.3.7-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 29 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Manual checks included directory scrolling, top and bottom handoff, horizontal gestures, and short folders.
