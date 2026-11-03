# Kite macOS

Kite macOS uses SwiftUI for the application shell and AppKit for native file-system integration.

The Rust `kite_core` library provides file listing, sorting, device discovery, VCS state, and typed UniFFI records.
The Swift application does not embed GPUI.
The macOS 26 build uses Liquid Glass for the navigation shell, tab rail, sidebar, selection lens, preview pane, and status bar.
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
- Column multi-selection with range and toggle selection.
- Global toolbar search over the active directory tree.
- Configurable detail columns and persisted column-width presets.
- SwiftUI tab rail with new, close, and switch commands.
- Tab and directory restoration on app relaunch.
- One breadcrumb and path-entry control with Command+L focus and Command+Up parent navigation.
- Sidebar for standard locations, favorites, Rust-enumerated volumes, and settings.
- Toolbar and context-menu favorite controls.
- macOS 26 Liquid Glass surfaces, controls, and selected rows.
- Automatic horizontal positioning of the active directory column.
- macOS 13 through 15 material fallback.
- AppKit workspace actions.
- File-row and column-directory context menus.
- LaunchServices folder-opener selection.
- Folder open requests from Finder or other apps.
- Finder reveal requests for the current user file viewer.
- AppKit sharing service picker.
- Quick Look panel.
- File and folder drag and drop with Finder integration.
- macOS pasteboard.
- macOS Trash API.
- UniFFI-generated Swift bindings.

## Default file viewer

macOS 13 through 15 can set Kite as the `public.folder` handler.
macOS 26 and later keeps Finder as the folder handler.
Kite writes the supported global `NSFileViewer` preference on every supported release.
Kite handles `misc/mvis` and `FNDR/show` Apple Events for reveal requests.

## UI structure

- `ContentView.swift`: window shell, tab rail, breadcrumb toolbar, and status bar.
- `KiteSidebar.swift`: locations, favorites, and volumes.
- `KiteColumnBrowser.swift`: column cards, file rows, and context menus.
- `KitePreviewPane.swift`: Quick Look preview, actions, and metadata.
- `GlassSurface.swift`: Liquid Glass components, fallback materials, and the Kite backdrop.

All filesystem, sorting, VCS, volume, session, and file-operation logic stays in the Rust `kite_core` library.
