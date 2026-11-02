# Kite v0.6.34

Kite v0.6.34 improves update feedback and completes the latest Windows UI alignment with macOS.

## Update progress

- Show download percentage, downloaded size, and total size.
- Add a determinate download progress bar on Windows.
- Add a determinate download progress bar and size summary on macOS.
- Read `Content-Length` when a Release asset size is unavailable.
- Keep update progress in the shared Rust Core.

## Windows update safety

- Check that the running `Kite.exe` and its directory are writable before download.
- Stop the update when Kite runs from a read-only location.
- Tell the user to copy `Kite.exe` to a writable directory.
- Do not silently restart the old executable after a failed replacement.
- Show a Windows error dialog when the update helper fails.
- Write source and target paths to the update log.

## macOS layout

- Place the tab rail above the unified toolbar.
- Preserve the traffic-light safe area in the tab rail.

## Windows layout

- Merge the breadcrumb path into the toolbar.
- Switch the address control between breadcrumbs and path editing.
- Show the search field only when search is active.
- Move the preview toggle to the tab rail.
- Remove low-frequency view and path-bar controls from the toolbar.
- Keep update progress in the rounded status bar.

## Windows sidebar

- Remove the expandable directory tree.
- Remove directory expansion arrows.
- Make locations, favorites, devices, and drives flat navigation items.
- Navigate a directory by loading it in the right-side columns.
- Open favorite files with the system default application.
- Match the flat sidebar behavior used by macOS.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows Clippy.
- Windows release build.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit build.
- PowerShell update-helper syntax check.

## Install

- Windows: use `Kite-v0.6.34-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.34-macOS-arm64.zip`.
- Verify downloaded files with `SHA256SUMS.txt`.
