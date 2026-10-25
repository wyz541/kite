# Kite v0.3.5

Kite is a native Windows file manager built with Rust and GPUI.

## Changes

- Always show Git and SVN in the first-level context menu.
- Keep both submenus visible outside matching repositories.
- Disable unavailable submenu actions instead of hiding them.
- Add Git diff and log commands.
- Add SVN diff and log commands.
- Open VCS output in the default text editor.
- Add Git unstage, pull, push, fetch, and clean commands.
- Add SVN lock, unlock, and cleanup commands.

## Git submenu

- Add
- Unstage
- Commit
- Pull
- Push
- Fetch
- Show diff
- Show log
- Revert
- Clean working tree

## SVN submenu

- Update
- Add
- Commit
- Lock
- Unlock
- Show diff
- Show log
- Revert
- Clean working copy

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git operations.
- SVN is optional. Install SVN for SVN operations.

## Install

1. Download `Kite-v0.3.5-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 24 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Git diff and log commands were tested.
- SVN diff and log commands were tested.
