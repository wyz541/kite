# Kite v0.6.47

Kite v0.6.47 fixes a Windows crash when dragging files to web upload targets.

macOS parity: not required

## Windows drag fix

- Build Shell drag data from full desktop-rooted file identifiers.
- Support files from different source folders in one drag.
- Keep Explorer compatibility for real files and folders.
- Prevent malformed parent-relative drag data from reaching browsers.
- Keep the existing copy drag effect.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows GPUI build.
- Windows Clippy.
- Windows Shell drag module cross-check.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS native UI boundary check.
- macOS Liquid Glass SDK check.
- macOS SwiftUI and AppKit build.
- Release parity check.
- Windows and macOS package hash verification.

## Install

- Windows: use Kite-v0.6.47-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.47-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
