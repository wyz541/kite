# Kite v0.2.4

Kite is a native Windows file manager built with Rust and GPUI.

## Fixes

- Hide protected system junctions such as `C:\Documents and Settings`.
- Read reparse-point attributes before resolving the target.
- Avoid canonicalization failures on protected folder paths.
- Replace raw Windows access errors with a clear permission message.
- Keep the current folder state when a protected folder fails to open.

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git status badges.
- SVN is optional. Install SVN for SVN status badges.

## Install

1. Download `Kite-v0.2.4-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 21 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `C:\Documents and Settings` is absent from the C: root listing.
- Root navigation no longer shows the raw `os error 5` message.
