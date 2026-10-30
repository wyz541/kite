# Kite v0.6.30

Kite v0.6.30 fixes opening storage folders on portable devices.

## Portable devices

- Open WPD storage folders from Shell parsing names.
- Fall back to Shell namespace enumeration when direct parsing fails.
- Resolve the target from the nearest reparsable ancestor.
- Support nested WPD paths without a fixed depth limit.
- Keep file and folder enumeration in the selected storage.
- Apply the same resolution to directory checks and parent navigation.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.
- Live WPD device root load test.
- Live WPD storage folder load test.

## Install

- Windows: use `Kite-v0.6.30-windows-x64.zip`.
- macOS: use `Kite-v0.6.30-macOS-arm64.zip`.