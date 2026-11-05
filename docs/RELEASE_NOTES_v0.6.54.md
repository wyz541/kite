# Kite v0.6.54

Kite v0.6.54 adds native ZIP and TAR.GZ compression actions to the right-click menu on Windows and macOS. The refresh command is now the first action in file and directory context menus.

## Context menus

- Add a first-level Compress menu.
- Add ZIP and TAR.GZ choices as the second-level menu.
- Add a first-level Extract menu for ZIP, TAR.GZ, and TGZ files.
- Add Extract Here and Extract to... actions.
- Move Refresh to the first position in every right-click operation menu.
- Keep multi-selection safe by disabling single-item archive actions for a multi-selection.

## Archive safety

- Write each archive to a temporary file.
- Rename the temporary file only after a successful write.
- Avoid overwriting existing files during extraction.
- Reject unsafe archive paths.
- Reject archives that contain symbolic links.

## Windows

- Use the shared Rust archive core.
- Show archive actions in list and column context menus.
- Keep the existing Windows Shell menu for third-party archive tools.

## macOS

- Use UniFFI to call the shared Rust archive core.
- Add archive actions to file and directory context menus.
- Refresh visible columns after compression or extraction.

## Verification

- Rust formatting check.
- Rust Core tests for ZIP and TAR.GZ round trips.
- Rust Core test for overwrite protection.
- Rust Core test for unsafe symbolic-link rejection.
- Rust Clippy.
- macOS Swift syntax check.
- macOS SwiftUI and AppKit release build.
- macOS app package and ad-hoc signature verification.
- Windows release build through Windows CI.
- Release parity check.

## Install

- Windows: use Kite-v0.6.54-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.54-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
