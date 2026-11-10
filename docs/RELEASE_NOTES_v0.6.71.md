# Kite v0.6.71

Kite v0.6.71 adds the macOS-aligned Windows settings window.

## Windows settings

- Add a dedicated settings window with a left navigation sidebar.
- Add General settings for session restoration.
- Add Appearance settings for system, light, and dark modes.
- Add Layout settings for the sidebar, preview, and detail columns.
- Add File Handling settings for the default explorer and Start menu shortcut.
- Add Updates settings for update checks and update logs.
- Add About settings for version, product principles, and privacy.
- Open settings from the sidebar and the Kite app menu.
- Persist appearance, layout, and session preferences.

## Cross-platform alignment

- Match the macOS settings window page structure and grouping.
- Use native Windows GPUI switches, buttons, and window controls.
- Keep platform-specific file and Shell actions in the File Handling page.

## Verification

- Windows Clippy check.
- Windows release build.
- Windows Chrome and Edge drag-and-drop E2E.
- Rust Core tests.
- macOS SwiftUI and AppKit release build.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.71-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.71-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release changes the Windows settings interface.
The Windows layout follows the existing macOS settings window.
macOS parity: not required
