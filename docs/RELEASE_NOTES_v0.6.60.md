# Kite v0.6.60

Kite v0.6.60 improves Windows storage navigation and Android MTP directory speed.

## Windows storage sidebar

- Show the drive letter in a fixed badge.
- Show the volume label as a separate field.
- Move free and total capacity to a dedicated line.
- Add a free-space progress bar for each storage volume.
- Show a warning below 10% free space.
- Show a red warning below 5% free space.
- Use a network icon for network drives.
- Keep existing navigation and context-menu actions.

## Android MTP directories

- Build child paths from parent-relative Shell names.
- Resolve child items directly before fallback enumeration.
- Cache Shell display names and item capabilities.
- Clear cached Shell metadata when the device list changes.
- Reduce repeated Android MTP directory reads.

## Platform parity

- macOS parity: not required
- The Windows storage-sidebar change has no macOS source change.
- The shared Rust Core remains compatible with both platforms.

## Verification

- Rust formatting check.
- Rust Core tests.
- Windows Clippy.
- Windows release build.
- Windows Chrome and Edge drag-upload E2E.
- Storage capacity and warning-threshold tests.
- macOS SwiftUI and AppKit release build.
- Feature matrix check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.60-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.60-macOS-arm64.zip`.
- Verify downloaded files with `SHA256SUMS.txt`.
