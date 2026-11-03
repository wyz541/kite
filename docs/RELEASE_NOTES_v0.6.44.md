# Kite v0.6.44

Kite v0.6.44 adds persistent column sorting and grouping on Windows and macOS. Group sections can now be collapsed and expanded.

## Persistent column rules

- Save sort field, sort direction, grouping, and collapsed groups for each directory.
- Restore directory-specific column rules after relaunch.
- Apply the latest sort and grouping rule to directories without a saved rule.
- Keep collapsed groups specific to their directory.
- Persist the Windows list-view sort field and direction.
- Keep existing `ui.json` files compatible.

## Collapsible groups

- Click a group header to collapse or expand the group.
- Show a disclosure indicator and item count in each group header.
- Hide rows inside collapsed groups.
- Clear selections that become hidden after a group is collapsed.
- Keep both Windows and macOS behavior aligned.

## Verification

- Windows formatting check.
- Windows Core tests.
- Windows GPUI tests.
- Windows Clippy.
- Windows release build.
- macOS formatting check.
- macOS Core tests.
- macOS Clippy.
- macOS native UI boundary check.
- macOS Liquid Glass SDK check.
- macOS SwiftUI and AppKit build.
- macOS persistence round-trip check.
- Release parity check.
- Windows and macOS package hash verification.

## Install

- Windows: use Kite-v0.6.44-windows-x64.zip.
- Windows direct executable: use Kite.exe.
- macOS: use Kite-v0.6.44-macOS-arm64.zip.
- Verify all downloaded files with SHA256SUMS.txt.
