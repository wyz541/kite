# Kite v0.3.4

Kite is a native Windows file manager built with Rust and GPUI.

## Changes

- Remove the repeated application icon from the toolbar.
- Remove the repeated application name from the toolbar.
- Give the released width to address and navigation controls.

## Requirements

- Windows 10 or Windows 11 x64.

## Install

1. Download `Kite-v0.3.4-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 24 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Toolbar starts with navigation controls.
