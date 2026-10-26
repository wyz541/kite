# Kite v0.4.0

Kite is a native Windows file manager built with Rust and GPUI.

## Context menu architecture

- Add one context menu model for list and column views.
- Add ordered menu providers for core actions, VCS actions, Windows Shell, and refresh.
- Add a typed action envelope with the target path.
- Remove duplicated Git and SVN menu builders.
- Add model tests for Git, SVN, capability gating, and native menu placement.

## Windows Shell integration

- Add a native Windows context menu bridge.
- Load Shell items with `IShellItem` and `IShellItemArray`.
- Bind Shell menus through `BHID_SFUIObject` and `IContextMenu`.
- Forward menu messages to `IContextMenu2` and `IContextMenu3`.
- Invoke selected commands with `CMINVOKECOMMANDINFOEX`.
- Keep menu handles and window subclass data in RAII guards.

## New behavior

- Select `Windows 原生菜单…` to open the full Windows menu.
- Use installed Shell extensions from Kite.
- Use `Open with`, `Properties`, `Send to`, compression, sync, and VCS extensions.
- Keep Kite Git and SVN menus at the first level.
- Keep the selected target path in each menu action.

## Current boundary

- The current menu target is one path.
- Folder background menus are not included yet.
- Windows 11 modern `IExplorerCommand` menus are not merged yet.
- The architecture document defines the next phases for both areas.

## Screenshots

![Context menu](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/context-menu.png)

![Git and SVN context menu](https://raw.githubusercontent.com/wyz541/kite/main/docs/screenshots/vcs-context-menu.png)

## Requirements

- Windows 10 or Windows 11 x64.

## Install

1. Download `Kite-v0.4.0-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.

## Verification

- `cargo test`: 33 tests passed.
- `cargo test`: 1 native menu smoke test ignored by default.
- Native menu smoke test: passed manually.
- `cargo clippy --all-targets -- -D warnings`: passed.

## Architecture

- [Kite context menu architecture](https://github.com/wyz541/kite/blob/main/docs/architecture/context-menu.md)
