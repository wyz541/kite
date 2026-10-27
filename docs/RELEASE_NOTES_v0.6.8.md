# Kite v0.6.8

Kite v0.6.8 adds Windows application discovery registration.

## Why portable Kite is not in global search

The ZIP package does not install Kite.
Windows Search and PowerToys Run do not scan every `.exe` on disk.
They search installed applications, Start Menu entries, and App Paths.
A portable `Kite.exe` has none of these entries after extraction.

## Windows discovery registration

- Open `Kite > 创建开始菜单快捷方式…`.
- Create `App Paths\Kite.exe` under the current-user registry.
- Create `Kite.lnk` in the current-user Start Menu.
- Point the shortcut to the running `Kite.exe`.
- Create the same entries when Kite becomes the default file manager.
- Restart PowerToys Run or reload its application list.

## Scope

- Registration uses the current user.
- Registration does not require administrator rights.
- Moving `Kite.exe` invalidates the shortcut and App Paths target.
- Run the command again after moving the executable.

## Privacy

- Keep screenshots out of the repository.
- Keep screenshots out of Windows release packages.
- Keep screenshots out of CI artifacts.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Install

- Windows: use `Kite-v0.6.8-windows-x64.zip`.
- macOS: use `Kite-v0.6.8-macOS-arm64.zip`.
