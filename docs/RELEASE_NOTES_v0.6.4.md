# Kite v0.6.4

Kite v0.6.4 moves version-control status onto file and folder icons in column view.

## Version-control icons

- Remove trailing version-control text from column rows.
- Show the Git or SVN status code on the item icon.
- Use a circular badge for Git.
- Use a rounded-square badge for SVN.
- Keep status colors for modified, added, deleted, untracked, and conflict states.
- Prefer the most severe status when Git and SVN statuses both exist.
- Keep the version-control column in list view.
- Apply the icon badge in the Windows GPUI frontend.
- Apply the icon badge in the macOS SwiftUI frontend.

## Windows screenshot

![Kite Windows version-control icons](https://github.com/wyz541/kite/releases/download/v0.6.4/Kite-v0.6.4-windows-vcs-icons.png)

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Current boundary

- macOS VCS menus are not yet complete.
- macOS Finder Sync is not yet implemented.
- macOS Universal Binary is not yet implemented.
- macOS notarization is not yet implemented.
- Kite has no built-in MTP file browser.

## Installation notes

- Windows builds are not code-signed. Windows SmartScreen can show a warning.
- The macOS build is arm64 and ad-hoc signed. The app is not notarized. macOS Gatekeeper can block the first launch.

## Install

- Windows: use `Kite-v0.6.4-windows-x64.zip`.
- macOS: use `Kite-v0.6.4-macOS-arm64.zip`.