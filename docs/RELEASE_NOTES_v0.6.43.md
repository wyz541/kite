# Kite v0.6.43

Kite v0.6.43 adds live column search on Windows and macOS. It also fixes macOS preview layout.

## Live column search

- Filter columns as the query changes on Windows.
- Filter columns as the query changes on macOS.
- Wait 180 milliseconds after the last input before searching.
- Keep explicit submit for an immediate search.
- Clear search results when the query becomes empty.
- Cancel stale searches when the query changes.

## macOS preview layout

- Keep the column browser inside the available content width.
- Keep the preview pane from overlapping visible columns.
- Show an empty preview pane when preview is enabled without a selection.
- Keep column scrolling stable when the viewport width changes.

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
- Release parity check.
- Windows and macOS package hash verification.

## Install

- Windows: use Kite-v0.6.43-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.43-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
