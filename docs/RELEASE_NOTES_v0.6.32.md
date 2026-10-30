# Kite v0.6.32

Kite v0.6.32 adds native file clipboard support and copy-to-directory actions.

## File clipboard

- Copy files and folders with standard keyboard shortcuts.
- Cut files and folders with standard keyboard shortcuts.
- Paste one or multiple files.
- Share the Windows file clipboard with Explorer through `CF_HDROP`.
- Share the macOS file clipboard with Finder.
- Keep clipboard state across Kite tabs.
- Keep cut mode in the system clipboard metadata.
- Prevent silent overwrites.
- Generate unique copy names when the destination contains a conflict.

## Right-click menu

- Add “拷贝到…” to file and folder menus.
- Add “拷贝到…” to directory background menus.
- Open a native directory picker for the copy destination.
- Paste into the right-clicked folder.
- Paste files into the containing folder for file targets.
- Paste files into the folder itself for folder targets.

## Platform support

### Windows

- Add native `CF_HDROP` clipboard read and write support.
- Add multi-file paste from Windows Explorer.
- Add the native Windows folder picker.

### macOS

- Add Finder-compatible `NSPasteboard` URL support.
- Add copy, cut, paste, and “拷贝到…” context menu commands.
- Add copy, cut, and paste toolbar buttons.
- Use Rust Core for file transfer and conflict naming.

## Safety

- Copy folders recursively.
- Keep the source after a copy operation.
- Move the source only after a cut operation.
- Reject copying a folder into itself.
- Reject unsupported symbolic-link copies.
- Keep filesystem errors visible in the Kite window.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit build.
- Windows drop-files clipboard payload test.
- Recursive folder copy test.
- File and folder paste-target tests.

## Install

- Windows: use `Kite-v0.6.32-windows-x64.zip`.
- macOS: use `Kite-v0.6.32-macOS-arm64.zip`.