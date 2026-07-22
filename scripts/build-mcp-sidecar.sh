#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUTPUT="$ROOT/src-tauri/resources/mcp"
TARGET="$ROOT/src-tauri/target"
TAURI_UNIVERSAL_OUTPUT="$TARGET/universal-apple-darwin/release/cnshell-mcp"
DEPLOYMENT_TARGET="13.0"

mkdir -p "$OUTPUT"
for arch in aarch64-apple-darwin x86_64-apple-darwin; do
  MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET" \
    cargo build --manifest-path "$ROOT/src-tauri/Cargo.toml" --release --bin cnshell-mcp --target "$arch"
done

lipo -create \
  "$TARGET/aarch64-apple-darwin/release/cnshell-mcp" \
  "$TARGET/x86_64-apple-darwin/release/cnshell-mcp" \
  -output "$OUTPUT/cnshell-mcp"
chmod 755 "$OUTPUT/cnshell-mcp"
strip -x "$OUTPUT/cnshell-mcp"
[[ "$(lipo -archs "$OUTPUT/cnshell-mcp")" == *arm64* ]]
[[ "$(lipo -archs "$OUTPUT/cnshell-mcp")" == *x86_64* ]]
[[ $(wc -c < "$ROOT/src-tauri/resources/licenses/rmcp-Apache-2.0.txt") -gt 10000 ]]
"$ROOT/scripts/sign-macos-binary.sh" "$OUTPUT/cnshell-mcp"
mkdir -p "$(dirname "$TAURI_UNIVERSAL_OUTPUT")"
cp "$OUTPUT/cnshell-mcp" "$TAURI_UNIVERSAL_OUTPUT"
chmod 755 "$TAURI_UNIVERSAL_OUTPUT"
echo "CNshell MCP universal sidecar 已生成：$OUTPUT/cnshell-mcp"
