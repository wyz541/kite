# Kite v0.6.10

Kite v0.6.10 adds preview support for large `.log` files.

## Log preview

- Recognize `.log` files as text previews.
- Keep the regular text-file size limit for non-log files.
- Ignore the regular size limit for log files.
- Read only the last 64 KiB of a large log.
- Show the last 6,000 characters.
- Remove the incomplete first line after seeking.
- Reject binary log content.
- Keep log reading in a background task.
- Keep log text selectable and copyable.

## Preview information

- Keep preview property selection.
- Keep `Ctrl+C` and `Command+C` copy behavior.
- Keep the copy-all preview information button.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Large log tail preview test.
- Large log size-limit test.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Install

- Windows: use `Kite-v0.6.10-windows-x64.zip`.
- macOS: use `Kite-v0.6.10-macOS-arm64.zip`.
