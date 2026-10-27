# Kite v0.6.5

Kite v0.6.5 adds a preview and properties column for selected files.

## Preview and properties

- Show a preview column when a file is selected in the last column.
- Show the preview beside the column stack.
- Scroll the column strip to reveal the preview.
- Show file name, type, size, modified date, and full path.
- Show Git and SVN status when available.
- Close the preview when the selection or directory changes.
- Do not show the preview for folders or files in older columns.

## Windows

- Preview common image formats with the GPUI image loader.
- Preview text files with a bounded background read.
- Limit text reads to the first 64 KiB.
- Limit text excerpts to 6,000 characters.
- Reject files that contain binary content.
- Show a type icon for formats without an internal preview.
- Keep image loading and text loading off the UI thread.

## macOS

- Use native `QLPreviewView` for Quick Look.
- Show native Quick Look content in the preview column.
- Use the same properties and Git/SVN status fields.
- Keep Liquid Glass behavior unchanged.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- Windows text preview test.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Current boundary

- macOS VCS menus are not yet complete.
- macOS Finder Sync is not yet implemented.
- macOS Universal Binary is not yet implemented.
- macOS notarization is not yet implemented.
- Kite has no built-in MTP file browser.

## Installation notes

- Windows builds are not code-signed. Windows SmartScreen can show a warning.
- The macOS build is arm64 and ad-hoc signed. The app is not notarized. macOS Gatekeeper can block the first launch.

## Install

- Windows: use `Kite-v0.6.5-windows-x64.zip`.
- macOS: use `Kite-v0.6.5-macOS-arm64.zip`.