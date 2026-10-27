# Kite v0.5.1

Kite v0.5.1 adds native Liquid Glass to the macOS UI.

## macOS Liquid Glass

- Use Liquid Glass for the macOS 26 toolbar and path controls.
- Group navigation controls and path controls with `GlassEffectContainer`.
- Use `Glass.regular.interactive()` for pointer feedback.
- Add a subtle window backdrop for glass reflection.
- Keep file columns on standard content materials for legibility.
- Use a `regularMaterial` fallback on macOS 13 through macOS 15.
- Target macOS 13 while building with the macOS 26 SDK.
- Keep the macOS application native SwiftUI and AppKit.
- Keep GPUI and egui out of the macOS application.

## Windows

- Windows behavior is unchanged.
- The Windows release continues to use Rust and GPUI.

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

## Installation notes

- Windows builds are not code-signed. Windows SmartScreen can show a warning.
- The macOS build is arm64 and ad-hoc signed. The app is not notarized. macOS Gatekeeper can block the first launch.

## Install

- Windows: use `Kite-v0.5.1-windows-x64.zip`.
- macOS: use `Kite-v0.5.1-macOS-arm64.zip`.