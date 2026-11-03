#!/usr/bin/env bash
set -euo pipefail

base_sha="${1:-}"
head_sha="${2:-}"
pr_number="${3:-}"

if [[ -z "$base_sha" || -z "$head_sha" || -z "$pr_number" ]]; then
  echo "Usage: $0 <base-sha> <head-sha> <pr-number>" >&2
  exit 2
fi

files="$(git diff --name-only "$base_sha" "$head_sha")"
labels="$(gh pr view "$pr_number" --json labels --jq '.labels[].name')"

windows_changed=false
macos_changed=false
while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  case "$file" in
    Cargo.toml|src/app.rs|src/actions.rs|src/table.rs|src/tabs.rs|src/preview_window.rs|src/main.rs|src/explorer_integration.rs|src/file_clipboard.rs|src/shell_fs.rs|src/ui_preferences.rs|build.ps1|src/context_menu/*)
      windows_changed=true
      ;;
  esac
  case "$file" in
    apps/macos/*)
      macos_changed=true
      ;;
  esac
done <<< "$files"

has_label() {
  grep -qx "$1" <<< "$labels"
}

if [[ "$windows_changed" == true ]] && ! has_label "platform:windows"; then
  echo "::error::Windows UI files changed without the platform:windows label."
  exit 1
fi

if [[ "$macos_changed" == true ]] && ! has_label "platform:macos"; then
  echo "::error::macOS source files changed without the platform:macos label."
  exit 1
fi

if [[ "$windows_changed" == true && "$macos_changed" == false ]] && ! has_label "parity:required"; then
  echo "::error::Windows UI changes need macOS parity work or the parity:required label."
  exit 1
fi

{
  echo "## Platform parity"
  echo
  echo "- Windows changed: $windows_changed"
  echo "- macOS changed: $macos_changed"
  echo "- PR labels checked"
} >> "${GITHUB_STEP_SUMMARY:-/dev/null}"

echo "Platform parity check passed."
