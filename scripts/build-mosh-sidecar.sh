#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/src-tauri/target/mosh-sidecar"
DOWNLOADS="$WORK/downloads"
SOURCES="$WORK/sources"
OUTPUT="$ROOT/src-tauri/resources/mosh"
DEPLOYMENT_TARGET="13.0"
JOBS="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"

CMAKE_VERSION="3.31.10"
PROTOBUF_VERSION="21.12"
MOSH_VERSION="1.4.0"

mkdir -p "$DOWNLOADS" "$SOURCES" "$OUTPUT/licenses"
touch "$WORK/.metadata_never_index"

download() {
  local filename="$1"
  local url="$2"
  local checksum="$3"
  local path="$DOWNLOADS/$filename"
  if [[ ! -f "$path" ]] || ! echo "$checksum  $path" | shasum -a 256 -c - >/dev/null 2>&1; then
    rm -f "$path"
    curl --http1.1 --fail --location --retry 8 --retry-all-errors --connect-timeout 20 \
      --output "$path" "$url"
  fi
  echo "$checksum  $path" | shasum -a 256 -c - >/dev/null
}

extract() {
  local archive="$1" directory="$2"
  if [[ ! -d "$directory" ]]; then
    mkdir -p "$directory"
    tar -xzf "$archive" --strip-components=1 -C "$directory"
  fi
}

download "cmake-$CMAKE_VERSION-macos-universal.tar.gz" \
  "https://github.com/Kitware/CMake/releases/download/v$CMAKE_VERSION/cmake-$CMAKE_VERSION-macos-universal.tar.gz" \
  "be9f3faeeaf7921cc2d77cea711dd5e6f72c63af2810cacd9205b3ce8d1593c9"
download "protobuf-all-$PROTOBUF_VERSION.tar.gz" \
  "https://github.com/protocolbuffers/protobuf/releases/download/v$PROTOBUF_VERSION/protobuf-all-$PROTOBUF_VERSION.tar.gz" \
  "2c6a36c7b5a55accae063667ef3c55f2642e67476d96d355ff0acb13dbb47f09"
download "mosh-$MOSH_VERSION.tar.gz" \
  "https://github.com/mobile-shell/mosh/releases/download/mosh-$MOSH_VERSION/mosh-$MOSH_VERSION.tar.gz" \
  "872e4b134e5df29c8933dff12350785054d2fd2839b5ae6b5587b14db1465ddd"

extract "$DOWNLOADS/cmake-$CMAKE_VERSION-macos-universal.tar.gz" "$SOURCES/cmake"
extract "$DOWNLOADS/protobuf-all-$PROTOBUF_VERSION.tar.gz" "$SOURCES/protobuf"
extract "$DOWNLOADS/mosh-$MOSH_VERSION.tar.gz" "$SOURCES/mosh"

CMAKE="$SOURCES/cmake/CMake.app/Contents/bin/cmake"
[[ -x "$CMAKE" ]] || { echo "CMake 运行时缺失" >&2; exit 1; }

build_arch() {
  local arch="$1"
  local prefix="$WORK/install-$arch"
  local protobuf_build="$WORK/protobuf-build-$arch"
  local mosh_build="$WORK/mosh-build-$arch"
  rm -rf "$prefix" "$protobuf_build" "$mosh_build"
  mkdir -p "$prefix/protobuf" "$prefix/mosh" "$protobuf_build" "$mosh_build"

  "$CMAKE" -S "$SOURCES/protobuf/cmake" -B "$protobuf_build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$prefix/protobuf" \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET" \
    -DCMAKE_OSX_ARCHITECTURES="$arch" \
    -Dprotobuf_BUILD_TESTS=OFF \
    -Dprotobuf_BUILD_SHARED_LIBS=OFF \
    -Dprotobuf_WITH_ZLIB=OFF
  "$CMAKE" --build "$protobuf_build" --target install -j "$JOBS"

  (
    cd "$mosh_build"
    export MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"
    export PATH="$prefix/protobuf/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    export PKG_CONFIG=/usr/bin/false
    export protobuf_CFLAGS="-I$prefix/protobuf/include"
    export protobuf_LIBS="$prefix/protobuf/lib/libprotobuf.a"
    export TINFO_CFLAGS=""
    export TINFO_LIBS="-lncurses"
    export CXXFLAGS="-O2 -DNDEBUG -std=gnu++17 -arch $arch -mmacosx-version-min=$DEPLOYMENT_TARGET"
    export CFLAGS="-O2 -DNDEBUG -arch $arch -mmacosx-version-min=$DEPLOYMENT_TARGET"
    export LDFLAGS="-arch $arch -mmacosx-version-min=$DEPLOYMENT_TARGET -framework Security -framework CoreFoundation"
    "$SOURCES/mosh/configure" \
      --prefix="$prefix/mosh" \
      --disable-server \
      --disable-completion \
      --with-crypto-library=apple-common-crypto
    make -j "$JOBS"
    make install
  )

  strip -x "$prefix/mosh/bin/mosh-client"
  [[ "$(lipo -archs "$prefix/mosh/bin/mosh-client")" == "$arch" ]] || {
    echo "Mosh $arch 架构校验失败" >&2
    exit 1
  }
}

build_arch arm64
build_arch x86_64
lipo -create \
  "$WORK/install-arm64/mosh/bin/mosh-client" \
  "$WORK/install-x86_64/mosh/bin/mosh-client" \
  -output "$OUTPUT/mosh-client"
chmod 755 "$OUTPUT/mosh-client"

ARCHITECTURES="$(lipo -archs "$OUTPUT/mosh-client")"
[[ " $ARCHITECTURES " == *" arm64 "* && " $ARCHITECTURES " == *" x86_64 "* ]] || {
  echo "Mosh sidecar 不是 universal binary：$ARCHITECTURES" >&2
  exit 1
}
for arch in arm64 x86_64; do
  otool -arch "$arch" -L "$OUTPUT/mosh-client" | tail -n +2 | awk '{print $1}' | while read -r library; do
    case "$library" in
      /usr/lib/*|/System/Library/*) ;;
      *) echo "Mosh sidecar 包含非系统动态依赖：$library" >&2; exit 1 ;;
    esac
  done
done
MINIMUM="$(otool -arch arm64 -l "$OUTPUT/mosh-client" | awk '/LC_BUILD_VERSION/{found=1} found && /minos/{print $2; exit}')"
[[ "$MINIMUM" == "$DEPLOYMENT_TARGET" ]] || {
  echo "Mosh 最低系统版本不匹配：$MINIMUM" >&2
  exit 1
}
"$OUTPUT/mosh-client" -c >/dev/null

cp "$SOURCES/mosh/COPYING" "$OUTPUT/licenses/Mosh-GPL-3.0-or-later.txt"
cp "$SOURCES/protobuf/LICENSE" "$OUTPUT/licenses/Protobuf-BSD-3-Clause.txt"
cat > "$OUTPUT/licenses/THIRD_PARTY_NOTICES.md" <<EOF
# Mosh third-party notices

- Mosh $MOSH_VERSION: GPL-3.0-or-later. Source: https://github.com/mobile-shell/mosh/releases/download/mosh-$MOSH_VERSION/mosh-$MOSH_VERSION.tar.gz
- Protocol Buffers $PROTOBUF_VERSION: BSD-3-Clause. Source: https://github.com/protocolbuffers/protobuf/releases/download/v$PROTOBUF_VERSION/protobuf-all-$PROTOBUF_VERSION.tar.gz
- The distributed binary is built by \`scripts/build-mosh-sidecar.sh\` from the checksummed sources above and links only macOS system libraries.
EOF

codesign --force --sign - "$OUTPUT/mosh-client"
codesign --verify --strict "$OUTPUT/mosh-client"
echo "Mosh sidecar 已生成：$OUTPUT/mosh-client ($ARCHITECTURES, macOS $MINIMUM+)"
