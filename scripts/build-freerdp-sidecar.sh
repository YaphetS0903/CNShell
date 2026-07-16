#!/bin/bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/src-tauri/target/freerdp-sidecar"
DOWNLOADS="$WORK/downloads"
SOURCES="$WORK/sources"
OUTPUT="$ROOT/src-tauri/resources/freerdp"
DEPLOYMENT_TARGET="13.0"
JOBS="$(sysctl -n hw.logicalcpu 2>/dev/null || echo 4)"
export MACOSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET"

CMAKE_VERSION="3.31.10"
FREERDP_VERSION="3.28.0"
OPENSSL_VERSION="3.6.3"
SDL_VERSION="3.4.12"
SDL_TTF_VERSION="3.2.2"
FREETYPE_VERSION="VER-2-13-2-SDL"
FREERDP_BUILD_REVISION="5"

mkdir -p "$DOWNLOADS" "$SOURCES" "$OUTPUT/licenses"
touch "$WORK/.metadata_never_index"

download() {
  local filename="$1"
  local url="$2"
  local checksum="$3"
  local path="$DOWNLOADS/$filename"
  if [[ ! -f "$path" ]] || ! echo "$checksum  $path" | shasum -a 256 -c - >/dev/null 2>&1; then
    rm -f "$path"
    curl --fail --location --retry 5 --retry-all-errors --connect-timeout 20 \
      --output "$path" "$url"
  fi
  echo "$checksum  $path" | shasum -a 256 -c - >/dev/null
}

extract() {
  local archive="$1"
  local directory="$2"
  if [[ ! -d "$directory" ]]; then
    mkdir -p "$directory"
    tar -xzf "$archive" --strip-components=1 -C "$directory"
  fi
}

download "cmake-$CMAKE_VERSION-macos-universal.tar.gz" \
  "https://github.com/Kitware/CMake/releases/download/v$CMAKE_VERSION/cmake-$CMAKE_VERSION-macos-universal.tar.gz" \
  "be9f3faeeaf7921cc2d77cea711dd5e6f72c63af2810cacd9205b3ce8d1593c9"
download "freerdp-$FREERDP_VERSION.tar.gz" \
  "https://github.com/FreeRDP/FreeRDP/archive/refs/tags/$FREERDP_VERSION.tar.gz" \
  "2d6e37cd726163c37c2070a9aa38a4624feb6b2d414f4d9dbecd60600e971142"
download "openssl-$OPENSSL_VERSION.tar.gz" \
  "https://github.com/openssl/openssl/releases/download/openssl-$OPENSSL_VERSION/openssl-$OPENSSL_VERSION.tar.gz" \
  "243a86649cf6f23eeb6a2ff2456e09e5d77dd9018a54d3d96b0c6bdd6ba6c7f1"
download "SDL3-$SDL_VERSION.tar.gz" \
  "https://github.com/libsdl-org/SDL/releases/download/release-$SDL_VERSION/SDL3-$SDL_VERSION.tar.gz" \
  "f07b958a9ac5020fb7a44cadb957f658b2149c3c8abb4f63145fac9303249db7"
download "SDL3_ttf-$SDL_TTF_VERSION.tar.gz" \
  "https://github.com/libsdl-org/SDL_ttf/releases/download/release-$SDL_TTF_VERSION/SDL3_ttf-$SDL_TTF_VERSION.tar.gz" \
  "63547d58d0185c833213885b635a2c0548201cc8f301e6587c0be1a67e1e045d"
download "freetype-$FREETYPE_VERSION.tar.gz" \
  "https://github.com/libsdl-org/freetype/archive/refs/heads/$FREETYPE_VERSION.tar.gz" \
  "35dda4b5bae9c62840cfeb57bc26697dd6f1e7d02da7ef36876ed13f4003a311"

extract "$DOWNLOADS/cmake-$CMAKE_VERSION-macos-universal.tar.gz" "$SOURCES/cmake"
extract "$DOWNLOADS/freerdp-$FREERDP_VERSION.tar.gz" "$SOURCES/freerdp"
extract "$DOWNLOADS/openssl-$OPENSSL_VERSION.tar.gz" "$SOURCES/openssl"
extract "$DOWNLOADS/SDL3-$SDL_VERSION.tar.gz" "$SOURCES/sdl"
extract "$DOWNLOADS/SDL3_ttf-$SDL_TTF_VERSION.tar.gz" "$SOURCES/sdl-ttf"
extract "$DOWNLOADS/freetype-$FREETYPE_VERSION.tar.gz" "$SOURCES/sdl-ttf/external/freetype"

FREERDP_USER_CLOSE_PATCH="$ROOT/scripts/patches/freerdp-sdl-user-close.patch"
if ! grep -q "_userCloseRequested" "$SOURCES/freerdp/client/SDL/SDL3/sdl_context.hpp"; then
  patch -d "$SOURCES/freerdp" -p1 < "$FREERDP_USER_CLOSE_PATCH"
fi
FREERDP_STATE_PATCH="$ROOT/scripts/patches/freerdp-sdl-state-marker.patch"
if ! grep -q "CNSHELL_RDP_STATE=online" "$SOURCES/freerdp/client/SDL/SDL3/sdl_context.cpp"; then
  patch -d "$SOURCES/freerdp" -p1 < "$FREERDP_STATE_PATCH"
fi

CMAKE="$SOURCES/cmake/CMake.app/Contents/bin/cmake"
[[ -x "$CMAKE" ]] || { echo "FreeRDP 构建失败：CMake 不可执行" >&2; exit 1; }

build_architecture() {
  local arch="$1"
  local openssl_target="$2"
  local prefix="$WORK/prefix-$arch"
  local openssl_source="$WORK/openssl-$arch"
  local openssl_stamp="$prefix/.openssl-$OPENSSL_VERSION-macos-$DEPLOYMENT_TARGET"
  local freerdp_stamp="$prefix/.freerdp-$FREERDP_VERSION-r$FREERDP_BUILD_REVISION"

  if [[ ! -f "$openssl_stamp" ]]; then
    rm -f "$prefix/lib/libssl.a" "$prefix/lib/libcrypto.a"
    rm -rf "$openssl_source"
    cp -R "$SOURCES/openssl" "$openssl_source"
    pushd "$openssl_source" >/dev/null
    ./Configure "$openssl_target" \
      no-shared no-tests no-apps no-docs \
      --prefix="$prefix" --openssldir="$prefix/ssl"
    make -s -j"$JOBS" build_sw
    make -s install_sw
    popd >/dev/null
    touch "$openssl_stamp"
  fi

  if [[ ! -f "$prefix/lib/libSDL3.a" ]]; then
    "$CMAKE" -S "$SOURCES/sdl" -B "$WORK/sdl-$arch" \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX="$prefix" \
      -DCMAKE_OSX_ARCHITECTURES="$arch" \
      -DCMAKE_OSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET" \
      -DSDL_SHARED=OFF -DSDL_STATIC=ON -DSDL_TESTS=OFF \
      -DSDL_EXAMPLES=OFF -DSDL_INSTALL=ON -DSDL_INSTALL_DOCS=OFF
    "$CMAKE" --build "$WORK/sdl-$arch" --target install --parallel "$JOBS" -- -s
  fi

  if [[ ! -f "$prefix/lib/libSDL3_ttf.a" ]]; then
    rm -rf "$WORK/sdl-ttf-$arch"
    "$CMAKE" -S "$SOURCES/sdl-ttf" -B "$WORK/sdl-ttf-$arch" \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX="$prefix" \
      -DCMAKE_PREFIX_PATH="$prefix" \
      -DCMAKE_OSX_ARCHITECTURES="$arch" \
      -DCMAKE_OSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET" \
      -DBUILD_SHARED_LIBS=OFF -DSDLTTF_INSTALL=ON -DSDLTTF_VENDORED=ON \
      -DSDLTTF_HARFBUZZ=OFF -DSDLTTF_PLUTOSVG=OFF -DSDLTTF_SAMPLES=OFF
    "$CMAKE" --build "$WORK/sdl-ttf-$arch" --target install --parallel "$JOBS" -- -s
  fi

  if [[ ! -x "$prefix/bin/sdl-freerdp" || ! -f "$freerdp_stamp" ]]; then
    rm -rf "$WORK/freerdp-$arch"
    "$CMAKE" -S "$SOURCES/freerdp" -B "$WORK/freerdp-$arch" \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_INSTALL_PREFIX="$prefix" \
      -DCMAKE_PREFIX_PATH="$prefix" \
      -DCMAKE_OSX_ARCHITECTURES="$arch" \
      -DCMAKE_OSX_DEPLOYMENT_TARGET="$DEPLOYMENT_TARGET" \
      -DOPENSSL_ROOT_DIR="$prefix" -DOPENSSL_USE_STATIC_LIBS=TRUE \
      -DBUILD_SHARED_LIBS=OFF -DBUILD_TESTING=OFF -DBUILD_TESTING_INTERNAL=OFF \
      -DWITH_SERVER=OFF -DWITH_SAMPLE=OFF -DWITH_MANPAGES=OFF \
      -DWITH_X11=OFF -DWITH_FFMPEG=OFF -DWITH_SWSCALE=OFF \
      -DWITH_JSON_DISABLED=ON \
      -DWITH_INTERNAL_MD4=ON -DWITH_INTERNAL_MD5=ON -DWITH_INTERNAL_RC4=ON \
      -DWITH_SMARTCARD_EMULATE=OFF -DWITH_SMARTCARD_PCSC=OFF -DWITH_PCSC=OFF -DWITH_AAD=OFF \
      -DCHANNEL_URBDRC=OFF -DCHANNEL_SMARTCARD=OFF -DCHANNEL_PRINTER=OFF \
      -DCHANNEL_SERIAL=OFF -DCHANNEL_PARALLEL=OFF \
      -DWITH_CLIENT_SDL=ON -DWITH_CLIENT_SDL2=OFF -DWITH_CLIENT_SDL3=ON \
      -DWITH_SDL_LINK_SHARED=OFF -DWITH_SDL_IMAGE_DIALOGS=OFF \
      -DWITH_WEBVIEW=OFF -DWITH_CCACHE=OFF -DWITH_CLANG_FORMAT=OFF \
      -DWITHOUT_FREERDP_3x_DEPRECATED=ON
    "$CMAKE" --build "$WORK/freerdp-$arch" --target sdl3-freerdp --parallel "$JOBS" -- -s
    mkdir -p "$prefix/bin"
    cp "$WORK/freerdp-$arch/client/SDL/SDL3/sdl-freerdp" "$prefix/bin/sdl-freerdp"
    touch "$freerdp_stamp"
  fi

  [[ -x "$prefix/bin/sdl-freerdp" ]] || {
    echo "FreeRDP 构建失败：$arch helper 未生成" >&2
    exit 1
  }
}

build_architecture arm64 darwin64-arm64-cc
build_architecture x86_64 darwin64-x86_64-cc

lipo -create \
  "$WORK/prefix-arm64/bin/sdl-freerdp" \
  "$WORK/prefix-x86_64/bin/sdl-freerdp" \
  -output "$OUTPUT/sdl-freerdp"
chmod 755 "$OUTPUT/sdl-freerdp"
strip -x "$OUTPUT/sdl-freerdp"

cp "$SOURCES/freerdp/LICENSE" "$OUTPUT/licenses/FreeRDP-Apache-2.0.txt"
cp "$SOURCES/openssl/LICENSE.txt" "$OUTPUT/licenses/OpenSSL-Apache-2.0.txt"
cp "$SOURCES/sdl/LICENSE.txt" "$OUTPUT/licenses/SDL3-Zlib.txt"
cp "$SOURCES/sdl-ttf/LICENSE.txt" "$OUTPUT/licenses/SDL3_ttf-Zlib.txt"
cp "$SOURCES/sdl-ttf/external/freetype/LICENSE.TXT" "$OUTPUT/licenses/FreeType-License.txt"
cp "$ROOT/docs/THIRD_PARTY_NOTICES.md" "$OUTPUT/licenses/THIRD_PARTY_NOTICES.md"

architectures="$(lipo -archs "$OUTPUT/sdl-freerdp")"
[[ " $architectures " == *" arm64 "* && " $architectures " == *" x86_64 "* ]] || {
  echo "FreeRDP 构建失败：helper 不是 universal binary：$architectures" >&2
  exit 1
}
if otool -L "$OUTPUT/sdl-freerdp" | awk '/^\t/ && $1 !~ /^\/usr\/lib\// && $1 !~ /^\/System\/Library\// && $1 !~ /^@/ { found=1 } END { exit !found }'; then
  echo "FreeRDP 构建失败：helper 仍依赖非系统动态库" >&2
  otool -L "$OUTPUT/sdl-freerdp" >&2
  exit 1
fi
for arch in arm64 x86_64; do
  xcrun vtool -show-build -arch "$arch" "$OUTPUT/sdl-freerdp" | grep -Eq "minos[[:space:]]+$DEPLOYMENT_TARGET$" || {
    echo "FreeRDP 构建失败：$arch helper 最低 macOS 版本不是 $DEPLOYMENT_TARGET" >&2
    exit 1
  }
done
codesign --force --sign - "$OUTPUT/sdl-freerdp"
codesign --verify --strict --verbose=2 "$OUTPUT/sdl-freerdp"
echo "FreeRDP universal helper 已生成：$OUTPUT/sdl-freerdp ($architectures)"
