# Kite v0.6.53

Kite v0.6.53 adds automatic refresh for visible columns on Windows and macOS. File creation, deletion, rename, metadata changes, and version-control status changes now appear without a manual refresh.

## Automatic column refresh

- Watch every visible local directory column with native filesystem events.
- Debounce bursts of filesystem changes.
- Reload changed columns in the background.
- Preserve sorting, filtering, grouping, selection, and scroll position.
- Close child columns when their selected parent directory disappears.
- Refresh the file preview when the selected file changes.
- Keep the manual refresh command for retry use.

## Windows

- Use the native `ReadDirectoryChangesW` backend.
- Skip unsupported shell namespace columns.
- Verify automatic creation refresh with a real Kite window.
- Keep Chrome and Edge drag upload regression coverage.

## macOS

- Use `DispatchSourceFileSystemObject` for directory monitoring.
- Recreate watchers after rename or deletion.
- Add a native directory watcher smoke test.

## Verification

- Windows formatting check.
- Windows Core tests, including directory watcher coverage.
- Windows Clippy.
- Windows release build.
- Windows automatic column refresh E2E.
- Windows Chrome and Edge drag upload E2E.
- Windows package hash verification.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS directory watcher smoke test.
- macOS SwiftUI and AppKit build.
- macOS package and signature verification.
- Release parity check.

## Install

- Windows: use Kite-v0.6.53-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.53-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
