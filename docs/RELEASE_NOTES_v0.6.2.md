# Kite v0.6.2

Kite v0.6.2 makes WPD and MTP device entries clickable.

## Portable devices

- Enumerate the Windows WPD device instance ID.
- Build the matching Windows Shell namespace path.
- Make each portable-device sidebar row clickable.
- Open the selected device root through Windows Shell.
- Add pointer, hover, MTP label, and disclosure indicator.
- Show an inline error when Windows cannot open the device.
- Keep automatic device refresh every 2 seconds.

Kite still does not provide built-in WPD file browsing.
MTP file operations inside Kite are not implemented.

## Windows screenshot


## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- WPD shell namespace path test.
- WPD Shell open smoke test.
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

- Windows: use `Kite-v0.6.2-windows-x64.zip`.
- macOS: use `Kite-v0.6.2-macOS-arm64.zip`.
