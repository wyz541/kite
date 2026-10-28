# Kite v0.6.17

Kite v0.6.17 adds a More menu to the column preview.

## Preview actions

- Add a More button to the preview title bar.
- Add Open file.
- Add Copy.
- Add Copy full path.
- Add File properties.
- Add Show native menu.
- Reuse the shared context-menu action envelope and dispatcher.
- Add the CopyPath command.
- Add the ShowProperties command.
- Open system properties with `SHObjectProperties` and `SHOP_FILEPATH`.
- Add Copy full path and Properties to the first-level menu.
- Document the preview menu architecture.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.

## Install

- Windows: use `Kite-v0.6.17-windows-x64.zip`.
- macOS: use `Kite-v0.6.17-macOS-arm64.zip`.