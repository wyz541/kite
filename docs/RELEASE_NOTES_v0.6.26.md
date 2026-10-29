# Kite v0.6.26

Kite v0.6.26 separates right-click context menus from column selection.

## Context menu behavior

- Right-click a file without selecting it.
- Right-click a folder without selecting it.
- Do not load the next column after right-click.
- Do not load file preview after right-click.
- Show the context menu for the clicked file or folder.
- Keep left-click selection and navigation unchanged.
- Keep blank-area context menus unchanged.
- Update the context menu architecture notes.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.
- Context menu model tests.
- List-background context menu test.

## Install

- Windows: use `Kite-v0.6.26-windows-x64.zip`.
- macOS: use `Kite-v0.6.26-macOS-arm64.zip`.
