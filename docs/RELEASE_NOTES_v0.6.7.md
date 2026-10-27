# Kite v0.6.7

Kite v0.6.7 adds default folder-opener integration on Windows and macOS.


## Windows default file manager

- Open `Kite > 设为默认资源管理器…`.
- Use the toolbar folder settings control.
- Set Kite as the current-user folder opener.
- Store the previous folder-open command.
- Restore the previous command from `Kite > 恢复之前的文件夹打开方式…`.
- Keep the integration in the current-user registry.
- Do not require administrator rights.
- Do not overwrite a later default change during restore.
- Notify Windows Shell after each association change.

## macOS folder opener

- Declare `public.folder` support in `Info.plist`.
- Open `文件夹 > 设为默认文件夹打开方式…`.
- Use LaunchServices `setDefaultApplication`.
- Restore Finder from `文件夹 > 恢复 Finder 为默认…`.
- Receive folder open requests from AppDelegate.
- Open each requested folder in a new tab.

## System boundaries

- Windows Explorer remains the system shell.
- Windows desktop, taskbar, `Win+E`, and system file dialogs remain system-managed.
- Finder remains the macOS system shell.
- Finder windows, Dock, desktop, and system file panels remain system-managed.

## Privacy

- Remove local-path screenshots from the repository.
- Exclude screenshots from Windows release packages.
- Stop CI screenshot capture and artifact upload.
- Remove historical screenshot release assets.

## Verification

- Windows Core tests.
- Windows GPUI tests.
- Windows registry logic tests.
- Windows Clippy.
- Windows release build.
- macOS Core tests.
- macOS Clippy.
- Native UI boundary check.
- macOS 26 Liquid Glass SDK check.
- macOS SwiftUI and AppKit application build.
- macOS app packaging.

## Current boundary

- Windows builds are not code-signed.
- The macOS build is arm64 and ad-hoc signed.
- macOS notarization is not implemented.
- Windows can replace the folder-open command.
- Windows cannot replace the Explorer system shell.
- macOS cannot replace the Finder system shell.

## Install

- Windows: use `Kite-v0.6.7-windows-x64.zip`.
- macOS: use `Kite-v0.6.7-macOS-arm64.zip`.
