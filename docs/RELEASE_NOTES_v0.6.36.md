# Kite v0.6.36

Kite v0.6.36 aligns the Windows GPUI column view with the macOS Kite UI.

## Windows detailed columns

- Add a details button to the active Windows column header.
- Show date, type, and size columns when details are visible.
- Keep detailed columns hidden by default.
- Persist the details preference across Windows launches.
- Format file sizes in MB with two decimal places.
- Show a dash for folder sizes.

## Filename priority

- Expand the active column to fit its longest filename.
- Keep the active column between 416 and 640 logical pixels.
- Preserve the full filename when it fits.
- Use middle ellipsis when a filename exceeds the maximum column width.
- Keep date, type, and size columns visible beside normal long names.

## Layout alignment

- Align the Windows detailed column behavior with macOS.
- Keep the 316-pixel preview pane beside the active column.
- Use an 8-pixel gap between the column strip and preview.
- Keep the header, metadata columns, and file rows aligned.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows Clippy.
- Windows release build.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS native UI boundary check.
- macOS Liquid Glass SDK check.
- macOS SwiftUI and AppKit build.

## Install

- Windows: use `Kite-v0.6.36-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.36-macOS-arm64.zip`.
- Verify all downloaded files with `SHA256SUMS.txt`.
