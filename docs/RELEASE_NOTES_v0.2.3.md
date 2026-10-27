# Kite v0.2.3

Kite is a native Windows file manager built with Rust and GPUI.

## Changes

- Detect USB storage volumes through the Windows storage bus type.
- Detect removable media even when the bus type is unavailable.
- Detect WPD/MTP portable devices through Windows SetupAPI.
- Show volume names, drive letters, and total capacity in the sidebar.
- Refresh the connected-device list every 2 seconds.
- Browse USB storage volumes directly in Kite.

## Current limitation

WPD/MTP portable devices are recognized and listed. File browsing for these devices is not available in this release.

## Requirements

- Windows 10 or Windows 11 x64.
- Git is optional. Install Git for Git status badges.
- SVN is optional. Install SVN for SVN status badges.

## Install

1. Download `Kite-v0.2.3-windows-x64.zip`.
2. Extract the ZIP file.
3. Run `Kite.exe`.

Kite is not code-signed. Windows SmartScreen may show a warning.



## Verification

- `cargo test`: 20 tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- Apple iPhone WPD detection was checked on the test machine.
- Fixed drives and volume capacities were checked on the test machine.
