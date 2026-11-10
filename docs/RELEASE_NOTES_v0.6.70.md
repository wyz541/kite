# Kite v0.6.70

Kite v0.6.70 adds a native macOS settings window.

## macOS settings window

- Add a standard macOS Settings scene with a sidebar and detail panes.
- Add General settings for restoring tabs and directories on launch.
- Add Appearance settings for system, light, and dark modes.
- Add Layout settings for the sidebar, preview pane, and detail columns.
- Add File Handling settings for the default file viewer and Finder restoration.
- Add Updates settings for update checks, progress, and update logs.
- Add About settings with version, product principles, and privacy information.
- Open the settings window from the sidebar settings button and the standard app menu.
- Keep native switches, pickers, buttons, window controls, and accessibility labels.

## Repository

- Bump the shared version to 0.6.70.
- Keep the Windows release package on the same shared-core version.
- Add no Windows UI changes.

## Verification

- Rust formatting check.
- Rust Core tests.
- macOS SwiftUI and AppKit release build.
- Windows GPUI release build.
- Windows release package check.
- macOS release package check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.70-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.70-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

The settings window is a macOS UI feature.
Windows receives the shared-core version bump and a new release package.
macOS parity: required
