# Kite v0.6.31

Kite v0.6.31 upgrades the text preview into a professional text editor.

## Text editor

- Open editable text files in a dedicated editor window.
- Add line numbers and syntax highlighting.
- Add undo and redo.
- Add search and replace.
- Add save and dirty-state protection.
- Add word-wrap, whitespace, and line-number controls.
- Keep large logs in the read-only tail preview.

## Platform support

### Windows

- Use the GPUI Kit Rope editor.
- Use the lighter syntect highlighter for broad TextMate syntax support.
- Use the native GPUI input, selection, clipboard, and accessibility systems.

### macOS

- Use a native AppKit `NSTextView`.
- Keep the SwiftUI Liquid Glass window shell.
- Use the system find bar and replace bar.
- Keep the native undo manager, text services, and accessibility.
- Add a native line-number ruler.

## File safety

- Open UTF-8 text files up to 8 MB.
- Detect binary and non-UTF-8 files.
- Preserve UTF-8 BOM.
- Preserve LF and CRLF line endings.
- Detect file changes made outside Kite.
- Reject conflicting saves.
- Replace file contents with atomic writes.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- macOS SwiftUI and AppKit application build.
- UTF-8 BOM and CRLF round-trip test.
- External file-change conflict test.
- Binary and oversized-file rejection test.

## Research

See `docs/TEXT_EDITOR_RESEARCH.md` for the editor survey and architecture decision.

## Install

- Windows: use `Kite-v0.6.31-windows-x64.zip`.
- macOS: use `Kite-v0.6.31-macOS-arm64.zip`.