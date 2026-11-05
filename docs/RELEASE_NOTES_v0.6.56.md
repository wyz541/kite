# Kite v0.6.56

Kite v0.6.56 refines the global search window introduced in v0.6.55. Name and content search now share visible scopes, relevance ordering, duplicate grouping, and clearer result presentation on macOS and Windows.

## Search scope

- Show the scope selector in name mode and content mode.
- Search the current folder, favorite folders, or all mounted volumes.
- Include fixed, USB, removable, and network volumes in the all-volume scope.
- Apply the same scope behavior on macOS and Windows.

## Search speed and system load

- Search names through the shared Rust Core.
- Use at most two traversal workers for each search root.
- Skip hidden directories and generated directories during global name search.
- Start a live search after two typed characters and a 280 ms pause.
- Keep searches cancellable when the query or scope changes.

## Ranking and grouping

- Rank exact name and exact file-stem matches first.
- Rank prefix and token-boundary matches above substring-only matches.
- Prefer `trunk`, `main`, `master`, `develop`, and similar development paths.
- Rank publish, release, build, cache, and archive copies later.
- Group same-name results of the same kind.
- Keep the best location visible and let users expand other locations.

## Result readability

- Reduce the visual weight of file-type icons.
- Increase path contrast.
- Show full parent path, type, size, and modified time for name results.
- Keep line numbers and context in content results.
- Highlight the matching text in each content snippet.

## Windows

- Add multi-root name search to the native GPUI search window.
- Show the shared scope selector in both search modes.
- Support duplicate grouping and content-term highlighting.
- Include network volumes in the all-volume scope.

## macOS

- Add multi-root name search through the shared UniFFI API.
- Show the shared scope selector in both search modes.
- Support duplicate grouping and content-term highlighting.
- Include network volumes in the all-volume scope.

## Verification

- Rust formatting check.
- Rust Core tests.
- Rust Clippy.
- Feature matrix check.
- Windows Clippy and release build.
- Windows Chrome and Edge regression tests.
- macOS SwiftUI and AppKit release build.
- macOS name/content search smoke test.
- Platform parity check.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.56-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.56-macOS-arm64.zip`.
- Verify downloaded files with `SHA256SUMS.txt`.
