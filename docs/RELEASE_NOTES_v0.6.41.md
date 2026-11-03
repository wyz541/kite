# Kite v0.6.41

Kite v0.6.41 adds cross-platform search, batch file actions, portable-device writes, configurable columns, and a column-scroll boundary fix.

## Global search

- Add global toolbar search to macOS.
- Search the active directory and its child directories.
- Show result count and 1000-item truncation state.
- Cancel stale search requests when the query or directory changes.

## Batch selection and file actions

- Add range and toggle selection in column views.
- Add batch copy, cut, and delete actions.
- Add macOS batch share and copy-to-directory actions.
- Keep remaining cut items in the clipboard after a partial failure.
- Refresh the view after partial batch failures.

## Portable devices

- Add Windows WPD/MTP write support through Shell `IFileOperation`.
- Add copy, move, create, rename, and delete support for device items.
- Use Shell attributes for cut, rename, and delete permissions.
- Block destructive actions on device root directories.
- Document permanent device deletion and the internal device clipboard.

## Columns and layout

- Add date, type, and size column toggles.
- Add compact, standard, and wide column presets.
- Persist column visibility and width settings.
- Show details automatically when a detail column is enabled.

## Column scrolling

- Keep vertical wheel input inside the column at top and bottom boundaries.
- Stop the remaining vertical event after the column reaches an edge.
- Keep horizontal input available through Shift+wheel and horizontal trackpads.

## Platform matrix and CI

- Add automated checks for the Windows and macOS feature matrix.
- Track `Cargo.toml` changes as Windows-facing changes.
- Include matrix checks in PR and release parity workflows.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- Windows package hash verification.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS native UI boundary check.
- macOS Liquid Glass SDK check.
- macOS SwiftUI and AppKit build.
- macOS package hash verification.
- Feature matrix, platform parity, and release parity checks.

## Install

- Windows: use Kite-v0.6.41-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.41-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
