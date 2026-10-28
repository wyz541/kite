# Kite v0.6.18

Kite v0.6.18 opens portable devices inside the Kite window.

## Portable devices

- Load WPD/MTP device roots inside Kite.
- Enumerate Windows Shell namespace items with `IShellItem` and `IEnumShellItems`.
- Initialize COM on the background enumeration thread.
- Navigate device folders in list and column views.
- Reuse history, path bar, parent navigation, and tab state.
- Open device files through Windows Shell.
- Use Shell display names in tab labels and path segments.
- Disable unsupported device file operations.
- Disable device-folder search.
- Skip VCS status lookup for Shell namespace paths.
- Add a live ignored MTP enumeration test.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Live Apple iPhone WPD enumeration test.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.

## Install

- Windows: use `Kite-v0.6.18-windows-x64.zip`.
- macOS: use `Kite-v0.6.18-macOS-arm64.zip`.
