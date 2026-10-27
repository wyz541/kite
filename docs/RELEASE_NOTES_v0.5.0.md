# Kite v0.5.0

Kite is a cross-platform file manager with native frontends.






## Architecture

- Split shared Rust Core from platform UI code.
- Keep Windows UI in Rust and GPUI.
- Add macOS UI in SwiftUI and AppKit.
- Add UniFFI records and a Swift-callable Core object.
- Move file system, VCS, device, and path rules into shared Core modules.
- Add platform-specific file name rules.
- Add macOS volume discovery.
- Add cross-platform CI build gates.

## Windows

- Keep the GPUI file manager.
- Keep GPUI tabs and column scrolling.
- Keep the Windows Shell `IContextMenu` bridge.
- Keep Git and SVN menus at the first level.

## macOS

- Add a SwiftUI application shell.
- Add a SwiftUI column browser.
- Add AppKit `NSWorkspace` open and Finder reveal actions.
- Add Quick Look.
- Add native sharing picker.
- Add native pasteboard support.
- Add macOS Trash support.
- Link the macOS app to the shared Rust Core through UniFFI.
- Do not link GPUI into the macOS application.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS UniFFI binding generation.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Current boundary

- macOS VCS menus are not yet complete.
- macOS Finder Sync is not yet implemented.
- macOS Universal Binary is not yet implemented.
- macOS notarization is not yet implemented.

## Architecture documents

- [Platform architecture](https://github.com/wyz541/kite/blob/main/docs/architecture/platforms.md)
- [Context menu architecture](https://github.com/wyz541/kite/blob/main/docs/architecture/context-menu.md)

## Installation notes

- Windows builds are not code-signed. Windows SmartScreen can show a warning.
- The macOS build is arm64 and ad-hoc signed. The app is not notarized. macOS Gatekeeper can block the first launch.

## Install

- Windows: use `Kite-v0.5.0-windows-x64.zip`.
- macOS: use `Kite-v0.5.0-macOS-arm64.zip`.
