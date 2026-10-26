# Kite v0.3.8

Kite is a native Windows file manager built with Rust and GPUI.

## Fix

- Open Git log with the TortoiseGit Log Messages window.
- Open SVN log with the TortoiseSVN log window.
- Find TortoiseGit and TortoiseSVN in `PATH` or the default install folder.
- Use `gitk` when `TortoiseGitProc.exe` is not available.
- Use a text file when no Git log GUI is available.
- Keep Git and SVN operations in background tasks.

## Behavior

- Select a Git item and open the Git submenu.
- Select `显示日志` to open the repository log window.
- Select an SVN item and open the SVN submenu.
- Select `显示日志` to open the working-copy log window.
- GUI startup does not block the Kite window.

## Screenshots

![Git and SVN context menu](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/vcs-context-menu.png)

## Requirements

- Windows 10 or Windows 11 x64.
- Git is required for Git status and Git operations.
- TortoiseGit is recommended for the Git log window.
- SVN and TortoiseSVN are optional.

## Install

1. Download `Kite-v0.3.8-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 29 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- The TortoiseGit log command opened the Kite repository log window.
