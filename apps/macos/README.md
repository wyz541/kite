# Kite macOS

Kite macOS uses SwiftUI for the application shell and AppKit for native file-system integration.

The Rust `kite_core` library provides file listing, sorting, device discovery, VCS state, and typed UniFFI records.
The Swift application does not embed GPUI.

## Build

```bash
bash apps/macos/build.sh
```

The script creates `apps/macos/build/Kite.app`.

## Current native coverage

- SwiftUI column browser.
- AppKit workspace actions.
- AppKit sharing service picker.
- Quick Look panel.
- macOS pasteboard.
- macOS Trash API.
- UniFFI-generated Swift bindings.
