#!/usr/bin/env bash
set -euo pipefail

version="${1:-}"
if [[ -z "$version" ]]; then
  echo "Usage: $0 vX.Y.Z" >&2
  exit 2
fi

if git rev-parse --verify --quiet "refs/tags/$version" >/dev/null; then
  target="$version"
else
  target="HEAD"
fi

previous_tag="$(git tag --sort=-version:refname | grep -E '^v[0-9]' | grep -vx "$version" | head -n 1 || true)"
if [[ -z "$previous_tag" ]]; then
  echo "No previous release tag found." >&2
  exit 2
fi

files="$(git diff --name-only "$previous_tag" "$target")"
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
    apps/macos/Sources/*)
      macos_changed=true
      ;;
  esac
done <<< "$files"

notes="docs/RELEASE_NOTES_${version}.md"
if [[ ! -f "$notes" ]]; then
  echo "::error::Missing release notes: $notes"
  exit 1
fi

if [[ "$windows_changed" == true && "$macos_changed" == false ]]; then
  if ! grep -Fq "macOS parity: not required" "$notes"; then
    echo "::error::Windows changes since $previous_tag have no macOS parity change."
    echo "Add macOS parity or record 'macOS parity: not required' in $notes."
    exit 1
  fi
fi

if ! grep -Eq '^- Windows:' "$notes"; then
  echo "::error::$notes has no Windows install entry."
  exit 1
fi

if ! grep -Eq '^- macOS:' "$notes"; then
  echo "::error::$notes has no macOS install entry."
  exit 1
fi

if command -v gh >/dev/null 2>&1 && [[ -n "${GH_TOKEN:-}" ]]; then
  pending="$(gh pr list --state open --label parity:required --json number --jq 'length')"
  if [[ "$pending" != "0" ]]; then
    echo "::error::$pending open PR(s) still carry parity:required."
    exit 1
  fi
fi

{
  echo "## Release parity"
  echo
  echo "- Version: $version"
  echo "- Previous tag: $previous_tag"
  echo "- Windows changed: $windows_changed"
  echo "- macOS changed: $macos_changed"
  echo "- Release notes: $notes"
} >> "${GITHUB_STEP_SUMMARY:-/dev/null}"

echo "Release parity check passed."
