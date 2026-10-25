# Kite v0.3.3

Kite is a native Windows file manager built with Rust and GPUI.

## Fixes

- Reuse loaded column panes during navigation.
- Stop resetting every column to a loading state.
- Load only missing levels after a branch change.
- Reuse the final pane when navigating to a parent directory.
- Update only the current pane on refresh.
- Load intermediate panes in the background when a path skips levels.

## Behavior

- Entering a child folder keeps all parent columns visible.
- Direct child navigation appends one ready pane.
- Parent navigation truncates deeper panes without reloading the common prefix.
- Branch navigation keeps the common prefix and loads only new levels.
- Final path entries stay available while missing intermediate levels load.

## Requirements

- Windows 10 or Windows 11 x64.

## Install

1. Download `Kite-v0.3.3-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 24 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Loaded-prefix calculation has a regression test.
