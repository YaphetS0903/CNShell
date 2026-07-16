#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/src-tauri/target/kermit-sidecar"
DOWNLOADS="$WORK/downloads"
SOURCE="$WORK/source"
OUTPUT="$ROOT/src-tauri/resources/kermit"
VERSION="2.01"
DEPLOYMENT_TARGET="13.0"
ARCHIVE="gku201.tar.gz"
ARCHIVE_SHA256="19f9ac00d7b230d0a841928a25676269363c2925afc23e62704cde516fc1abbd"
ARCHIVE_URL="https://www.kermitproject.org/ftp/kermit/archives/$ARCHIVE"

mkdir -p "$DOWNLOADS" "$OUTPUT/licenses" "$OUTPUT/source"
touch "$WORK/.metadata_never_index"
if [[ ! -f "$DOWNLOADS/$ARCHIVE" ]] || ! echo "$ARCHIVE_SHA256  $DOWNLOADS/$ARCHIVE" | shasum -a 256 -c - >/dev/null 2>&1; then
  rm -f "$DOWNLOADS/$ARCHIVE"
  curl --fail --location --retry 5 --retry-all-errors --connect-timeout 20 \
    --output "$DOWNLOADS/$ARCHIVE" "$ARCHIVE_URL"
fi
echo "$ARCHIVE_SHA256  $DOWNLOADS/$ARCHIVE" | shasum -a 256 -c - >/dev/null

rm -rf "$SOURCE"
mkdir -p "$SOURCE"
tar -xzf "$DOWNLOADS/$ARCHIVE" -C "$SOURCE"

build_arch() {
  local arch="$1"
  local build="$WORK/build-$arch"
  rm -rf "$build"
  cp -R "$SOURCE" "$build"
  (
    cd "$build"
    for source in gproto.c gkermit.c gunixio.c gcmdline.c; do
      clang -arch "$arch" -mmacosx-version-min="$DEPLOYMENT_TARGET" \
        -DPOSIX -DNODEBUG -O2 -c "$source"
    done
    clang -arch "$arch" -mmacosx-version-min="$DEPLOYMENT_TARGET" \
      -o gkermit gproto.o gkermit.o gunixio.o gcmdline.o
  )
  strip -x "$build/gkermit"
  [[ "$(lipo -archs "$build/gkermit")" == "$arch" ]] || {
    echo "G-Kermit $arch 架构校验失败" >&2
    exit 1
  }
}

build_arch arm64
build_arch x86_64
lipo -create "$WORK/build-arm64/gkermit" "$WORK/build-x86_64/gkermit" \
  -output "$OUTPUT/gkermit"
chmod 755 "$OUTPUT/gkermit"

ARCHITECTURES="$(lipo -archs "$OUTPUT/gkermit")"
[[ " $ARCHITECTURES " == *" arm64 "* && " $ARCHITECTURES " == *" x86_64 "* ]] || {
  echo "G-Kermit sidecar 不是 universal binary：$ARCHITECTURES" >&2
  exit 1
}
for arch in arm64 x86_64; do
  otool -arch "$arch" -L "$OUTPUT/gkermit" | tail -n +2 | awk '{print $1}' | while read -r library; do
    case "$library" in
      /usr/lib/*|/System/Library/*) ;;
      *) echo "G-Kermit sidecar 包含非系统动态依赖：$library" >&2; exit 1 ;;
    esac
  done
done
MINIMUM="$(otool -arch arm64 -l "$OUTPUT/gkermit" | awk '/LC_BUILD_VERSION/{found=1} found && /minos/{print $2; exit}')"
[[ "$MINIMUM" == "$DEPLOYMENT_TARGET" ]] || {
  echo "G-Kermit 最低系统版本不匹配：$MINIMUM" >&2
  exit 1
}
"$OUTPUT/gkermit" -h 2>&1 | rg -Fq "G-Kermit $VERSION"

cp "$SOURCE/COPYING" "$OUTPUT/licenses/G-Kermit-GPL-2.0.txt"
cp "$DOWNLOADS/$ARCHIVE" "$OUTPUT/source/$ARCHIVE"
cat > "$OUTPUT/THIRD_PARTY_NOTICES.md" <<EOF
# G-Kermit third-party notice

- G-Kermit $VERSION: GPL-2.0. Project: https://www.kermitproject.org/gkermit.html
- Corresponding source: source/$ARCHIVE (SHA-256: $ARCHIVE_SHA256).
- The distributed binary is built without source changes by scripts/build-kermit-sidecar.sh and links only macOS system libraries.
EOF

"$ROOT/scripts/sign-macos-binary.sh" "$OUTPUT/gkermit"
echo "G-Kermit sidecar 已生成：$OUTPUT/gkermit ($ARCHITECTURES, macOS $MINIMUM+)"
