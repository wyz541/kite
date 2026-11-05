# Kite v0.6.57

Kite v0.6.57 fixes broad global-search stalls and changes the Windows scope selector to a dropdown menu.

## Global search stability

- Split local disks and network volumes into separate scopes.
- Require Enter or the Search button for local-disk and network-volume searches.
- Stop automatic searching while typing in broad scopes.
- Reject repeated searches while a broad search is still running.
- Use one low-priority traversal worker for global name search.
- Limit a global name search to 5 seconds or 250000 scanned entries.
- Read metadata only for entries whose names match the query.
- Remove `canonicalize` from global search root preparation.
- Keep a root scan within one filesystem.
- Mark result limits and scan limits as incomplete results.

## Scope dropdown

- Replace the Windows scope cycle button with a dropdown menu.
- Show current folder, favorites, local disks, and network volumes as separate choices.
- Match the macOS scope menu on both platforms.
- Show the selected scope in the search toolbar and result header.

## Verification

- Rust formatting check.
- Rust Core tests.
- Rust Clippy.
- Feature matrix check.
- Windows Clippy and release build.
- Windows Chrome and Edge regression tests.
- macOS SwiftUI and AppKit release build.
- macOS local-disk search smoke test.
- Platform parity check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.57-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.57-macOS-arm64.zip`.
- Verify downloaded files with `SHA256SUMS.txt`.
