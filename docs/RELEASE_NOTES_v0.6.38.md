# Kite v0.6.38

Kite v0.6.38 adds column sort controls and fixes column scrolling and file-size display.

## Column sorting

- Add a sort-direction button to every Windows column header.
- Keep the current sort field.
- Toggle between ascending and descending order.

## Column scrolling

- Stop vertical column scrolling at the top and bottom boundaries.
- Keep vertical wheel input inside the pointer's column.
- Keep `Shift+wheel` and horizontal gestures for column-strip scrolling.
- Apply the same boundary behavior to short and empty columns.

## File sizes

- Show detailed file sizes with automatic units.
- Use B for small files.
- Use KB, MB, GB, or TB for larger files.
- Keep folder size placeholders as a dash.
- Match the detailed size format on Windows and macOS.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- Windows package hash verification.

## Install

- Windows: use `Kite-v0.6.38-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.38-macOS-arm64.zip`.
- Verify all downloaded files with `SHA256SUMS.txt`.