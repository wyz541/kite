# Kite v0.2.0

Kite is a native Windows file manager built with Rust and GPUI.

## Highlights

- Multiple tabs in one window.
- Independent path, history, column, search, and view state for each tab.
- Shared file clipboard across all tabs.
- Tab bar with new, select, and close controls.
- Keyboard shortcuts for tab creation, closing, and switching.
- Last remaining tab protection.

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git status badges.
- SVN is optional. Install SVN for SVN status badges.

## Install

1. Download `Kite-v0.2.0-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Shortcuts

| Shortcut | Action |
| --- | --- |
| `Ctrl+T` | Create a tab |
| `Ctrl+W` | Close the active tab |
| `Ctrl+Tab` | Select the next tab |
| `Ctrl+Shift+Tab` | Select the previous tab |

## Screenshots

![Tabbed workspace](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/tabbed-view.png)

![Column view](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/column-view.png)

![Context menu](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/context-menu.png)

## Verification

- `cargo test`: 17 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Release executable starts and opens a folder.
