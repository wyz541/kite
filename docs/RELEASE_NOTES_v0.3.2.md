# Kite v0.3.2

Kite is a native Windows file manager built with Rust and GPUI.

## Changes

- Always show Git and SVN in the first-level context menu.
- Keep both submenus visible when a repository is not detected.
- Disable unavailable repository actions without hiding them.
- Add Git unstage, pull, push, fetch, and clean operations.
- Add SVN lock, unlock, and cleanup operations.
- Add confirmation for destructive Git clean.

## Git submenu

- Add
- Unstage
- Commit
- Pull
- Push
- Fetch
- Revert
- Clean working tree

## SVN submenu

- Update
- Add
- Commit
- Lock
- Unlock
- Revert
- Clean working copy

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git operations.
- SVN is optional. Install SVN for SVN operations.

## Install

1. Download `Kite-v0.3.2-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 23 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Git unstage, clean, and local operations were tested.
- SVN cleanup and local operations were tested.
