# Kite v0.3.0

Kite is a native Windows file manager built with Rust and GPUI.

## Highlights

- Context-menu Git operations: add, commit, and revert.
- Context-menu SVN operations: update, add, commit, and revert.
- Repository-aware menu visibility.
- Commit message dialog for Git and SVN.
- Confirmation dialog before revert.
- Background command execution with success and error reporting.
- Correct SVN working-copy detection through `svn info`.

## Safety

- Git commit applies to the selected path only.
- Git commit includes staged changes for that path.
- Git revert restores the selected path from `HEAD`.
- SVN revert applies to the selected path and its children.
- Revert never deletes unversioned files.
- Network authentication uses cached credentials.

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git operations.
- SVN is optional. Install SVN for SVN operations.

## Install

1. Download `Kite-v0.3.0-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.



## Verification

- `cargo test`: 23 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Git add, commit, and revert were checked in a temporary repository.
- SVN add and revert were checked in a temporary working copy.
- Git-only context menus were checked in the Kite repository.
