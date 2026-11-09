# Kite v0.6.65

Kite v0.6.65 fixes Windows updates that left Kensaku sidecars missing.

## Windows updates

- Download the Windows ZIP package.
- Verify the ZIP SHA-256 before extraction.
- Stop Kite and Kensaku processes.
- Back up managed files before replacement.
- Install `Kite.exe`, `kensaku-cli.exe`, and `kensaku-daemon.exe`.
- Restore managed files when installation fails.
- Keep the standalone `Kite.exe` asset for migration from v0.6.64.

## Sidecar repair

- Detect missing Kensaku sidecars at search startup.
- Download the current stable Windows ZIP.
- Verify the ZIP SHA-256.
- Extract only `kensaku-cli.exe` and `kensaku-daemon.exe`.
- Replace sidecars through temporary files.

## Build validation

- Require `Kite.exe`, `kensaku-cli.exe`, and `kensaku-daemon.exe` in the Windows ZIP.
- Add a Windows package selection test.
- Add a sidecar extraction test.
- Check the Windows update helper syntax.

## Install

- Windows: use `Kite-v0.6.65-windows-x64.zip`.
- Windows direct executable: use `Kite.exe` only to update from v0.6.64. Extract the ZIP for a new installation.
- macOS: use `Kite-v0.6.65-macOS-arm64.zip` on Apple Silicon.
- Verify downloads with `SHA256SUMS.txt`.

## Platform note

This release has no macOS source change.
macOS parity: not required