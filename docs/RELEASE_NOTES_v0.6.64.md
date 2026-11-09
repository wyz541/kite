# Kite v0.6.64

Kite v0.6.64 fixes Windows folder handoff and Windows update replacement.

## Windows folder handoff

- Register both `Folder\shell\open` and `Folder\shell\explore`.
- Write an empty `DelegateExecute` value to both command keys.
- Prevent Shell from using the inherited Explorer COM handler.
- Back up both command values and both `DelegateExecute` values.
- Restore both commands and both `DelegateExecute` values.
- Keep `REG_SZ` and `REG_EXPAND_SZ` value types during restore.
- Restore legacy v1 backups through the `open` command only.

## Windows update replacement

- Rename the target EXE directly before copying the new EXE.
- Retry the rename while Windows holds a delete-sharing loader lock.
- Stop remaining Kite processes between retries.
- Verify the installed EXE SHA-256 after replacement.
- Add a loader-lock replacement test.

## Verification

- Rust formatting check.
- Rust Core tests.
- Windows UI tests.
- Clippy with warnings denied.
- Windows release build.
- macOS SwiftUI and AppKit build.
- Release parity check.

## Install

- Windows: use `Kite-v0.6.64-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.64-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release has no macOS source change.
macOS parity: not required
