# Kite v0.6.32

Kite v0.6.32 adds native file clipboard support, copy-to-directory actions, and a rebuilt macOS interface.

## macOS Liquid Glass UI

- Rebuild the macOS shell with Liquid Glass navigation surfaces.
- Add a sidebar for locations, favorites, and Rust-enumerated volumes.
- Add breadcrumb navigation and a path editor.
- Add glass-selected rows, preview cards, and a floating status bar.
- Keep the active directory column visible.
- Keep short column sets aligned to the left.
- Use localized macOS display names for tabs, columns, and status text.
- Keep file listing, VCS state, volume discovery, sessions, and file operations in the Rust Core.

## macOS default file viewer

- Use LaunchServices folder association on macOS 13 through 15.
- Use the global `NSFileViewer` preference on macOS 26 and later.
- Handle Finder reveal requests through `misc/mvis` and `FNDR/show` Apple Events.
- Open the parent directory and select the requested file.
- Avoid the macOS error shown when a third-party folder association is rejected.
- Restore Finder behavior without exposing a raw system error.

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
- Add a Liquid Glass macOS application shell.
- Add Finder reveal routing for the default file viewer.
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
- macOS 37 passed, 1 ignored, 0 failed.
- macOS Clippy.
- macOS SwiftUI and AppKit build.
- macOS application launch and file-viewer routing smoke test.
- Windows drop-files clipboard payload test.
- Recursive folder copy test.
- File and folder paste-target tests.

## Install

- Windows: use `Kite-v0.6.32-windows-x64.zip`.
- macOS: use `Kite-v0.6.32-macOS-arm64.zip`.
