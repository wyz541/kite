# Kite v0.6.52

Kite v0.6.52 fixes Windows drag-out for files and folders. Dragged items now reach Chrome, Edge, Explorer, and other OLE drop targets as complete file-system items.

## Windows drag-out

- Post a native `WM_APP` message when the pointer leaves Kite.
- Start OLE after the current GPUI input event returns.
- Call `SHDoDragDrop` with the real Kite window handle.
- Let Shell create the default drag source.
- Use a system Shell data object with complete `CF_HDROP` paths.
- Keep copied source files and folders unchanged.
- Add a controlled drag trace through `KITE_DRAG_TRACE`.

## Browser upload verification

- Drag a real file from Kite to Chrome.
- Drag a real file from Kite to Edge.
- Verify the uploaded file name.
- Verify the uploaded file size.
- Verify the uploaded SHA-256 value.
- Verify Kite remains running after each drop.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows Clippy.
- Windows release build.
- Windows Chrome drag upload E2E.
- Windows Edge drag upload E2E.
- Windows package hash verification.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit build.
- macOS package and signature verification.
- Release parity check.

macOS parity: not required because this release does not change macOS source code.

## Install

- Windows: use Kite-v0.6.52-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.52-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
