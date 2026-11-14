# Kite v0.6.75

Kite v0.6.75 improves Windows column recognition and makes update feedback easier to understand.

## Windows columns

- Replace hollow folder icons with solid orange folder icons.
- Give image, audio, video, archive, code, text, and generic files distinct silhouettes.
- Increase column row icons to 18 px for clearer type recognition.
- Tighten icon-title spacing and increase file-name emphasis.
- Strengthen the visual contrast between column containers and the workspace background.
- Fix issue #92.

## Settings and updates

- Clarify update progress and diagnostic log terminology.
- Keep update status visible across the settings experience.
- Handle GitHub API rate limits without blocking normal app use.
- Keep an actionable retry path when an update check fails.
- Remove internal implementation details from product-facing copy.

## Verification

- Rust formatting check.
- Rust Core tests.
- Windows Clippy check.
- Windows GPUI tests.
- Windows debug build.
- Windows release build and package verification.
- Light and dark column rendering.
- Linux checks.
- Platform parity.
- Startup performance.
- macOS SwiftUI and AppKit release build.

## Install

- Windows: use `Kite-v0.6.75-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.75-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release updates Windows column visuals and shared update/settings behavior.
macOS receives the shared version bump and release package.
macOS parity: not required
