# Kite v0.6.37

Kite v0.6.37 adds column-specific display rules and scoped search.

## Column rules

- Add a search button to every Windows column header.
- Add a column menu with sort, filter, group, and reset rules.
- Sort by name, modified date, type, or size.
- Filter all items, folders, files, media files, or document files.
- Group by type, extension, or modified date.
- Keep file operations in the right-click menu.

## Scoped search

- Open a search bar from a column header.
- Search the selected column directory and its subdirectories.
- Show results in the selected column.
- Clear the search with Escape.
- Show the current result count and truncation state.

## Search performance

- Replace the serial directory walker with `ignore` parallel traversal.
- Use the ripgrep directory traversal stack.
- Cancel stale search tasks.
- Keep the 1000-result limit.
- Keep result order stable after parallel traversal.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- Rust formatting check.
- Windows package hash verification.

## Install

- Windows: use `Kite-v0.6.37-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- Verify all downloaded files with `SHA256SUMS.txt`.