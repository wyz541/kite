# Kite v0.6.3

Kite v0.6.3 removes the sidebar privacy footer.

## Sidebar cleanup

- Remove the “本地文件，不上传” label.
- Remove the shield icon beside the label.
- Let sidebar content use the full vertical space.
- Keep quick links, devices, and drive entries unchanged.
- Keep the clickable WPD/MTP device behavior unchanged.

## Windows screenshot

![Kite Windows sidebar](https://github.com/wyz541/kite/releases/download/v0.6.3/Kite-v0.6.3-windows-sidebar.png)

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
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

- Windows: use `Kite-v0.6.3-windows-x64.zip`.
- macOS: use `Kite-v0.6.3-macOS-arm64.zip`.