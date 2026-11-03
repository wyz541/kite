# Kite v0.6.42

Kite v0.6.42 fixes blocked Windows updates caused by running Kite processes.

## Windows update reliability

- Wait up to 10 seconds for the initiating Kite process.
- Stop all remaining Kite processes before replacing the executable.
- Retry process cleanup when the target file remains locked.
- Write the PowerShell helper as UTF-8 with BOM for Windows PowerShell 5.1.
- Restore Chinese error text in the update dialog.
- Add process-cleanup and PowerShell syntax regression tests.

## Upgrade note

Versions with the previous updater need one manual install of Kite v0.6.42.
Later updates use the corrected helper.

macOS parity: not required

## Verification

- Windows updater tests.
- PowerShell helper syntax test.
- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- Release parity check.
- Windows and macOS package hash verification.

## Install

- Windows: use Kite-v0.6.42-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.42-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
