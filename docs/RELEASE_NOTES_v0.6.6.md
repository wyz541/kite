# Kite v0.6.6

Kite v0.6.6 adds a directory context menu to every column.

## Column directory menu

- Right-click a column header.
- Right-click empty space in a column.
- Open the edit menu for that column directory.
- Keep row menus for files and child folders.
- Use the column directory as the menu target.
- Use the same directory as the working directory.
- Create new folders in that column directory.
- Paste items into that column directory.
- Rename the column directory itself.
- Move the column directory to the Trash.
- Run Git and SVN actions on the column directory.
- Open the Windows native Shell menu for the column directory.

## Path-safe actions

- Send the target path and working directory in every context-menu action.
- Keep target actions separate from directory-scoped actions.
- Rewrite the current path after the rename of an ancestor directory.
- Move the view to the parent after removal of an ancestor directory.
- Disable rename and deletion for a drive root.
- Keep list-view context menus on the current working directory.

## macOS

- Add native SwiftUI context menus to every column.
- Add open, reveal, Quick Look, copy path, share, and Trash actions.
- Add folder creation and rename dialogs with AppKit.
- Keep the Liquid Glass toolbar and path controls.
- Keep the SwiftUI and AppKit native UI boundary.

![Kite column directory context menu](https://github.com/wyz541/kite/blob/main/docs/screenshots/column-context-menu.png?raw=true)

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Current boundary

- Windows builds are not code-signed.
- The macOS build is arm64 and ad-hoc signed.
- macOS notarization is not implemented.
- Kite has no built-in MTP file browser.
- Finder Sync is not implemented.

## Install

- Windows: use `Kite-v0.6.6-windows-x64.zip`.
- macOS: use `Kite-v0.6.6-macOS-arm64.zip`.
