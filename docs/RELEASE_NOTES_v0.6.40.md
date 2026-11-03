# Kite v0.6.40

Kite v0.6.40 completes macOS column parity and adds automated parity checks.

## macOS column display rules

- Add per-column sort fields: name, modified date, type, and size.
- Add ascending and descending order for every field.
- Add filters for folders, files, media, and documents.
- Add grouping by kind, extension, and modified date.
- Add grouped section rows and reset controls.

## macOS column search

- Add a search button and search bar to every column header.
- Search the selected directory and its child directories.
- Show results inside the originating column.
- Apply active sorting, filtering, and grouping rules to results.
- Show result count and 1000-item truncation state.
- Cancel stale search requests on query, tab, or search-state changes.

## Release parity

- Add platform labels for Windows and macOS work.
- Require parity review for Windows UI changes without macOS changes.
- Add a release gate against the previous version.
- Require Windows and macOS install entries in release notes.
- Block release preparation while open parity work remains.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS native UI boundary check.
- macOS Liquid Glass SDK check.
- macOS SwiftUI and AppKit build.
- Platform parity check.
- Windows and macOS package hash verification.

## Install

- Windows: use Kite-v0.6.40-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.40-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
