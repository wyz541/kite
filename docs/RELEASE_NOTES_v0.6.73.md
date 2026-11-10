# Kite v0.6.73

Kite v0.6.73 replaces the Windows Markdown update prompt with a product update window.

## Update window

- Show current version, new version, release date, and package size.
- Split release notes into a summary and feature sections.
- Remove Markdown headings, list markers, code markers, and link targets.
- Hide verification, build, install, and platform-note sections.
- Keep long release notes inside a fixed-height scrolling area.
- Keep the window header and action area visible.
- Show download percentage, byte counts, and a progress bar.
- Allow the user to hide the window while the download continues.
- Show a retry action when download or installation fails.
- Open the complete GitHub Release notes from the update window.
- Show a dedicated completion state when Kite is current.

## Release notes parser

- Add a shared Rust parser for GitHub Release Markdown.
- Keep visible link text and remove the URL target.
- Remove fenced code blocks and HTML comments.
- Fall back to all sections when a release has no product sections.
- Add parser tests with shipped release notes.

## Verification

- Rust formatting check.
- Rust Core tests.
- Windows target Clippy check.
- Windows release build.
- Windows update window build check.
- macOS SwiftUI and AppKit release build.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.73-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.73-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release changes the Windows update window.
macOS receives the shared-core version bump and release package.
macOS parity: not required
