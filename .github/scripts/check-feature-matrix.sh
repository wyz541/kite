#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
matrix="$root/README.md"

if ! grep -Fq "| 能力 | Windows | macOS |" "$matrix"; then
  echo "::error::README feature matrix header is missing."
  exit 1
fi

required_features=(
  "全局工具栏搜索"
  "分栏批量选择"
  "批量复制、剪切和删除"
  "自定义详细信息列"
  "列宽预设与布局恢复"
  "设备浏览与写入"
)

failed=false
for feature in "${required_features[@]}"; do
  row_count="$(grep -Fc "| $feature |" "$matrix" || true)"
  if [[ "$row_count" -eq 0 ]]; then
    echo "::error::Feature matrix is missing: $feature"
    failed=true
    continue
  fi
  if [[ "$row_count" -gt 1 ]]; then
    echo "::error::Feature matrix contains duplicate rows: $feature"
    failed=true
    continue
  fi
  row="$(grep -F "| $feature |" "$matrix")"
  IFS='|' read -r _ windows_cell macos_cell _ <<< "$row"
  windows_cell="$(xargs <<< "$windows_cell")"
  macos_cell="$(xargs <<< "$macos_cell")"
  for platform in Windows macOS; do
    if [[ "$platform" == "Windows" ]]; then
      cell="$windows_cell"
    else
      cell="$macos_cell"
    fi
    if [[ -z "$cell" || "$cell" =~ (开发中|未支持|不支持|不适用) ]]; then
      echo "::error::Feature matrix is not complete for $platform: $feature"
      failed=true
    fi
  done
done

if [[ "$failed" == true ]]; then
  exit 1
fi

echo "Feature matrix check passed."
