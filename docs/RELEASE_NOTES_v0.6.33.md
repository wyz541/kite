# Kite v0.6.33

Kite v0.6.33 aligns the Windows GPUI interface with the macOS Kite UI and moves the macOS toolbar into the unified titlebar.

## macOS unified titlebar

- Move navigation, breadcrumb, favorite, and file actions into the titlebar.
- Put the sidebar toggle at the leading edge after the traffic lights.
- Remove the unused titlebar spacer.
- Keep the tab rail below the unified toolbar.
- Expand the tab rail and columns when the sidebar is hidden.
- Preserve Liquid Glass surfaces and native AppKit behavior.

## Windows GPUI alignment

- Use a floating rounded tab rail.
- Split the toolbar into navigation, address, search, and action groups.
- Add the current-folder favorite toggle to the toolbar.
- Order the sidebar as Quick Access, Favorites, Devices, and This PC.
- Add a settings entry at the bottom of the sidebar.
- Use rounded column cards with macOS-aligned widths and spacing.
- Use a separate rounded preview card.
- Use a floating rounded status bar.
- Keep native Windows titlebar controls and Shell integration.

## Project constitution

- Define macOS SwiftUI and AppKit as the visual and interaction reference.
- Keep Windows on Rust and GPUI.
- Share the Rust Core and project constitution.
- Do not share UI renderers or UI component code.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows Clippy.
- Windows release build.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS native UI boundary check.
- macOS Liquid Glass SDK check.
- macOS SwiftUI and AppKit build.
- macOS unified titlebar visual smoke test.
- Sidebar collapse and expand smoke test.

## Install

- Windows: use `Kite-v0.6.33-windows-x64.zip`.
- Windows direct executable: use `Kite.exe`.
- macOS: use `Kite-v0.6.33-macOS-arm64.zip`.
- Verify all downloaded files with `SHA256SUMS.txt`.
