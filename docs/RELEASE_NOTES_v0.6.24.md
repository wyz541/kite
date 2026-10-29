# Kite v0.6.24

Kite v0.6.24 adds file creation to context menus.

## Context menus

- Add “New File” to the Windows Kite context menu.
- Add “New File” to the macOS column context menu.
- Add “New File” to the macOS entry context menu.
- Open a filename prompt from the context menu.
- Create an empty file after confirmation.
- Reject duplicate names.
- Reject invalid filenames.
- Disable file creation while a directory is loading.
- Disable file creation for portable-device namespaces.
- Refresh the current view after creation.
- Clear the filename input after creation.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.
- Context menu model tests.
- Empty-file creation test.

## Install

- Windows: use `Kite-v0.6.24-windows-x64.zip`.
- macOS: use `Kite-v0.6.24-macOS-arm64.zip`.
