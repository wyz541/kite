# Kite v0.6.59

Kite v0.6.59 corrects the macOS global-search scope set.

## macOS search scopes

- Replace the macOS “网络卷” scope with “根磁盘”.
- Search the root filesystem `/` for the root-disk scope.
- Keep current-folder, favorites, and local-disk scopes.
- Keep root-disk search on the bounded low-priority global-search path.

## Windows search scopes

- Keep the network-volume scope on Windows.
- Preserve UNC and mapped-remote-drive detection.
- Preserve network favorites and current-network-folder fallback.

## Platform-specific scope sets

- macOS: current folder, favorites, local disks, root disk.
- Windows: current folder, favorites, local disks, network volumes.

## Verification

- Rust formatting check.
- Rust Core tests.
- Rust Clippy.
- Feature matrix check.
- Windows Clippy and release build.
- Windows Chrome and Edge regression tests.
- macOS SwiftUI and AppKit release build.
- macOS scope-menu smoke test.
- macOS root-disk search smoke test.
- Platform parity check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.59-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.59-macOS-arm64.zip`.
- Verify downloaded files with `SHA256SUMS.txt`.
