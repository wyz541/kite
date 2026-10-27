# Kite v0.6.13

Kite v0.6.13 adds GitHub Release updates.

## Update check

- Add `检查更新…` to the Kite menu.
- Add `检查更新…` to the Windows settings menu.
- Add `检查更新…` to the macOS application menu.
- Read the latest stable GitHub Release.
- Compare versions with semantic version rules.
- Show the current version and latest version.
- Show Release notes before installation.

## Rate-limit fallback

- Use the GitHub Release API first.
- Fall back to the GitHub latest release redirect.
- Read asset hashes from `SHA256SUMS.txt`.
- Keep update checks available when the anonymous API is limited.

## Download integrity

- Select the Windows `Kite.exe` asset.
- Select the macOS ARM64 ZIP asset.
- Restrict update URLs to the Kite Release path.
- Limit Release metadata to 1 MiB.
- Limit update assets to 256 MiB.
- Require a SHA-256 digest.
- Calculate SHA-256 while downloading.
- Reject size or hash mismatches.

## Windows installation

- Wait for the current Kite process to exit.
- Back up the current executable.
- Replace the executable with the verified file.
- Verify the installed executable SHA-256.
- Start the updated Kite.
- Restore the previous executable on failure.
- Write update diagnostics to `%LOCALAPPDATA%\Kite\updates\update.log`.

## macOS installation

- Download and verify the macOS ZIP in the shared Rust Core.
- Wait for the current Kite process to exit.
- Extract the new application with `ditto`.
- Replace `Kite.app`.
- Start the updated application.
- Restore the previous application on failure.
- Write update diagnostics to `~/Library/Application Support/Kite/updates/update.log`.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Version comparison tests.
- Asset selection tests.
- SHA-256 requirement tests.
- Release URL allowlist tests.
- Checksum file parsing tests.
- Live GitHub Release resolution test.
- Live Windows asset download and digest test.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Install

- Windows: use `Kite-v0.6.13-windows-x64.zip`.
- macOS: use `Kite-v0.6.13-macOS-arm64.zip`.
