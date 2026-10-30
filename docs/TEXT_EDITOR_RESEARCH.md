# Kite Text Editor Research

Date: 2026-10-30

## Decision

Kite uses the editor engine already shipped with `gpui-kit 0.8.0` on Windows.
Kite uses the native AppKit `NSTextView` text system on macOS.
A shared Rust module owns text-file validation and safe save operations.

## Requirements

- Native GPUI integration on Windows.
- Native SwiftUI and AppKit integration on macOS.
- Search and replace.
- Undo and redo.
- Syntax highlighting.
- Large-file behavior with a clear edit limit.
- Stable file save behavior.
- No second UI framework.

## Candidate review

| Candidate | Strength | Integration risk | Decision |
| --- | --- | --- | --- |
| `gpui-kit` editor | Rope buffer, tree-sitter highlighting, search panel, replace, undo, LSP hooks | Already used by Kite | Use on Windows |
| Zed editor | Mature editor and GPUI based | Full application and GPL components; not a small reusable dependency | Do not embed |
| Lapce | Rust native editor | Full application architecture and large dependency surface | Do not embed |
| Helix | Fast editor core and terminal UI | Terminal-focused architecture; no GPUI component | Do not embed |
| `ropey` | Proven rope text storage | Data structure only; no editor UI or input handling | Use through `gpui-kit` |
| `syntect` | Mature TextMate syntax engine | Separate highlight model; no editor behavior | Do not add |
| `tree-sitter` | Incremental parsing and language grammars | Needs UI integration and grammar selection | Use through `gpui-kit` |
| AppKit `NSTextView` | Native find bar, undo, selection, accessibility, services | macOS only | Use on macOS |

## Why not embed another editor

Zed, Lapce, and Helix are applications.
Their editor cores are tightly coupled to application state, rendering, configuration, and service layers.
Embedding one of these applications would add a second architecture to Kite.
The result would increase startup cost, binary size, update risk, and platform-specific maintenance.

`gpui-kit` is the existing Kite UI dependency.
Its editor uses Rope storage, a search matcher, tree-sitter languages, and GPUI-native input handling.
The editor keeps one rendering stack on Windows.

## Shared Core boundary

`src/text_editor.rs` provides these services on all platforms:

- UTF-8 validation.
- UTF-8 BOM detection.
- LF and CRLF detection.
- Maximum editable size enforcement.
- Language selection.
- SHA-256 content revision.
- External-change conflict detection.
- Atomic file replacement.

The UI owns only transient editing state.
The Core owns file encoding, file identity, and save safety.

## Platform behavior

### Windows

`src/preview_window.rs` uses the GPUI Kit `Editor` component.

Features:

- Line numbers.
- Tree-sitter syntax highlighting.
- Undo and redo.
- Search and replace.
- Save shortcut.
- Word-wrap toggle.
- Whitespace toggle.
- Dirty-state close guard.

### macOS

`apps/macos/Sources/TextEditorWindow.swift` uses SwiftUI for the window shell and AppKit for the text surface.

Features:

- Liquid Glass window shell.
- Native `NSTextView`.
- Native find bar and replace bar.
- Native undo manager.
- Line-number ruler.
- Native accessibility and text services.
- Rust Core file open and save.

## Limits

- Editable files must be UTF-8.
- Editable files are limited to 8 MB.
- Encoding conversion is not implemented.
- LSP is not enabled in Kite yet.
- Large logs remain read-only and use the tail preview.

## Sources

- GPUI Kit: https://github.com/longbridge/gpui-kit
- GPUI Kit editor documentation: https://gpui-kit.com/component/editor
- Ropey: https://github.com/cessen/ropey
- Tree-sitter: https://github.com/tree-sitter/tree-sitter
- Zed: https://github.com/zed-industries/zed
- Lapce: https://github.com/lapce/lapce
- Helix: https://github.com/helix-editor/helix
- Apple NSTextView: https://developer.apple.com/documentation/appkit/nstextview