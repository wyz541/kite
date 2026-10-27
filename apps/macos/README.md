# Kite macOS

Kite macOS uses SwiftUI for the application shell and AppKit for native file-system integration.

The Rust `kite_core` library provides file listing, sorting, device discovery, VCS state, and typed UniFFI records.
The Swift application does not embed GPUI.
The macOS 26 build uses Liquid Glass in the toolbar and path controls.
macOS 13 through 15 use a native material fallback.

## Build

macOS needs stable Rust, Xcode 26 or newer, and Xcode Command Line Tools.
The deployment target is macOS 13.

```bash
bash apps/macos/build.sh
```

The script creates `apps/macos/build/Kite.app`.

## Current native coverage

- SwiftUI column browser.
- SwiftUI tab bar with new, close, and switch commands.
- Tab and directory restoration on app relaunch.
- One path entry with Command+L focus and Command+Up parent navigation.
- macOS 26 Liquid Glass toolbar and path controls.
- macOS 13 through 15 material fallback.
- AppKit workspace actions.
- File-row and column-directory context menus.
- AppKit sharing service picker.
- Quick Look panel.
- macOS pasteboard.
- macOS Trash API.
- UniFFI-generated Swift bindings.
