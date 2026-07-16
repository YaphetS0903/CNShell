#!/bin/bash
set -euo pipefail

[[ $# -eq 1 && -f "$1" ]] || {
  echo "签名失败：需要一个现有二进制文件。" >&2
  exit 1
}

binary="$1"
identity="${APPLE_SIGNING_IDENTITY:--}"

if [[ "$identity" == "-" ]]; then
  codesign --force --sign - "$binary"
else
  codesign --force --options runtime --timestamp --sign "$identity" "$binary"
fi

codesign --verify --strict --verbose=2 "$binary"

if [[ "$identity" != "-" ]]; then
  details="$(codesign -dv --verbose=4 "$binary" 2>&1)"
  grep -Fq "Authority=$identity" <<<"$details" || {
    echo "签名失败：二进制未使用指定 Developer ID：$binary" >&2
    exit 1
  }
  grep -Eq 'flags=.*\(runtime\)' <<<"$details" || {
    echo "签名失败：二进制未启用 Hardened Runtime：$binary" >&2
    exit 1
  }
fi
