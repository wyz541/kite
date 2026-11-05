# Kite v0.6.58

Kite v0.6.58 fixes the unavailable network-volume state in global search.

## Search scope availability

- Disable empty scopes in the global-search dropdown and menu.
- Prevent selection of “网络卷” when no usable network root exists.
- Detect Windows UNC paths and mapped remote drives as network locations.
- Add directory favorites on network locations to the network scope.
- Fall back to the current network folder when volume enumeration returns no network root.
- Apply the equivalent non-local-volume fallback on macOS.

## Global search stability

- Keep local-disk and network-volume scopes separate.
- Keep broad scopes on Enter or Search only.
- Avoid the red empty-root error for scopes with no usable root.
- Preserve the bounded low-priority global name search from v0.6.57.

## Verification

- Rust formatting check.
- Rust Core tests.
- Rust Clippy.
- Feature matrix check.
- Windows Clippy and release build.
- Windows Chrome and Edge regression tests.
- macOS SwiftUI and AppKit release build.
- macOS unavailable-network-scope smoke test.
- Platform parity check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.58-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.58-macOS-arm64.zip`.
- Verify downloaded files with `SHA256SUMS.txt`.
