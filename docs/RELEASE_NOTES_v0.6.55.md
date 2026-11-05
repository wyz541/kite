# Kite v0.6.55

Kite v0.6.55 adds a separate global search window on Windows and macOS. Search names or indexed file contents across the current folder, favorites, and local disks without replacing the main file browser.

## Global search window

- Open a separate global search window.
- Use `Ctrl+Shift+F` on Windows.
- Use `Command+Shift+F` on macOS.
- Keep the main browser open and usable while search runs.
- Switch between name and content search.
- Select the current folder, favorites, or local disks as the search scope.
- Show file metadata, match counts, line numbers, and matching snippets.
- Open search results with a double-click.

## Content search

- Add a shared Rust content-search engine.
- Build a persistent local Tantivy index.
- Update the index through filesystem events.
- Verify indexed candidates with `grep-searcher`.
- Support case-insensitive literal searches.
- Support a regex fallback through the Core API.
- Apply bounded result, file-size, and snippet limits.
- Return partial results while the background index is building.

## System responsiveness

- Run indexing on a low-priority worker.
- Pause indexing during active search interaction.
- Pause indexing in low-power or thermal-pressure states.
- Pause indexing when available memory or disk space is low.
- Bound the Tantivy writer buffer and result queues.
- Exclude hidden items, generated directories, system directories, cloud placeholders, symlinks, binary files, and oversized files by default.

## Windows

- Add a native GPUI global search window.
- Bind `Ctrl+Shift+F` to the new window.
- Add the global search entry to the Navigation menu.
- Share indexing and search behavior through the Rust Core.

## macOS

- Add an AppKit-hosted SwiftUI global search window.
- Bind `Command+Shift+F` to the new window.
- Add the global search toolbar action and menu command.
- Reuse the shared Rust index and UniFFI result records.

## Verification

- Rust formatting check.
- Rust Core tests.
- Rust Clippy.
- macOS directory watcher smoke test.
- macOS file drag provider smoke test.
- macOS SwiftUI and AppKit release build.
- Windows Clippy.
- Windows release build.
- Windows Chrome and Edge regression tests.
- Linux checks.
- Platform parity check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.55-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.55-macOS-arm64.zip`.
- Verify all downloaded files with `SHA256SUMS.txt`.
