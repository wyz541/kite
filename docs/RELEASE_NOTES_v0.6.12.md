# Kite v0.6.12

Kite v0.6.12 fixes file-path opening and native Shell menu stability.

## File paths

- Open a file path in Kite.
- Switch to the file's parent folder.
- Select the file and start its preview.
- Open the file with the system default application.
- Support the same behavior in startup arguments.

## Show more options

- Add `显示更多选项 (Show more options)` to file and folder menus.
- Open the Windows Shell menu from that entry.
- Include normal verbs and extended Shell verbs.
- Show installed third-party Shell extensions.

## Native menu stability

- Open the native Shell menu outside the GPUI update call stack.
- Dispatch the menu through a private Windows window message.
- Prevent command selection from re-entering a GPUI update.
- Remove the crash with `0xc0000409` and `FAST_FAIL_FATAL_APP_EXIT`.
- Refresh files and version control status after the menu closes.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- File-path target tests.
- Native Shell menu bridge test.
- WinDbg native command crash regression.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Install

- Windows: use `Kite-v0.6.12-windows-x64.zip`.
- macOS: use `Kite-v0.6.12-macOS-arm64.zip`.
