#!/bin/zsh
set -euo pipefail

cd "${0:A:h}/.."

[[ -f src-tauri/tauri.release.json ]] || {
  echo "拒绝发布：请从 tauri.release.example.json 创建含真实 updater 配置的 tauri.release.json。" >&2
  exit 1
}
rg -q 'REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY|\.example' src-tauri/tauri.release.json && {
  echo "拒绝发布：tauri.release.json 仍含 updater 占位值。" >&2
  exit 1
}

for variable in APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH TAURI_SIGNING_PRIVATE_KEY; do
  [[ -n "${(P)variable:-}" ]] || { echo "缺少发布变量：$variable" >&2; exit 1; }
done

npm run check
npm run test:e2e
npm audit --audit-level=moderate
rustup target add aarch64-apple-darwin x86_64-apple-darwin
npm run tauri build -- --config src-tauri/tauri.release.json --target universal-apple-darwin --bundles app,dmg

BUNDLE_ROOT="src-tauri/target/universal-apple-darwin/release/bundle"
APP_PATH="$BUNDLE_ROOT/macos/CNshell.app"
INFO_PLIST="$APP_PATH/Contents/Info.plist"

[[ -d "$APP_PATH" && -f "$INFO_PLIST" ]] || {
  echo "发布失败：未找到 universal CNshell.app。" >&2
  exit 1
}

EXECUTABLE_NAME="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$INFO_PLIST")"
EXECUTABLE_PATH="$APP_PATH/Contents/MacOS/$EXECUTABLE_NAME"
[[ -x "$EXECUTABLE_PATH" ]] || {
  echo "发布失败：Info.plist 指定的可执行文件不存在：$EXECUTABLE_NAME" >&2
  exit 1
}

MINIMUM_SYSTEM_VERSION="$(/usr/libexec/PlistBuddy -c 'Print :LSMinimumSystemVersion' "$INFO_PLIST")"
awk -v actual="$MINIMUM_SYSTEM_VERSION" 'BEGIN {
  split(actual, parts, ".")
  if ((parts[1] + 0) < 13) exit 1
}' || {
  echo "发布失败：最低 macOS 版本必须为 13.0 或更高，当前为 $MINIMUM_SYSTEM_VERSION。" >&2
  exit 1
}

codesign --verify --deep --strict --verbose=2 "$APP_PATH"
codesign -dv --verbose=4 "$APP_PATH" 2>&1 | rg -Fq "Authority=$APPLE_SIGNING_IDENTITY"
spctl --assess --type execute --verbose "$APP_PATH"

ARCHITECTURES="$(lipo -archs "$EXECUTABLE_PATH")"
[[ " $ARCHITECTURES " == *" arm64 "* && " $ARCHITECTURES " == *" x86_64 "* ]] || {
  echo "发布失败：应用不是 arm64 + x86_64 universal binary：$ARCHITECTURES" >&2
  exit 1
}

FREERDP_HELPER="$APP_PATH/Contents/Resources/freerdp/sdl-freerdp"
[[ -x "$FREERDP_HELPER" ]] || {
  echo "发布失败：应用未包含内置 FreeRDP helper。" >&2
  exit 1
}
FREERDP_ARCHITECTURES="$(lipo -archs "$FREERDP_HELPER")"
[[ " $FREERDP_ARCHITECTURES " == *" arm64 "* && " $FREERDP_ARCHITECTURES " == *" x86_64 "* ]] || {
  echo "发布失败：FreeRDP helper 不是 arm64 + x86_64 universal binary：$FREERDP_ARCHITECTURES" >&2
  exit 1
}
RDP_PREFLIGHT="$(env -i PATH=/usr/bin:/bin:/usr/sbin:/sbin HOME="$HOME" "$EXECUTABLE_PATH" --rdp-preflight)"
printf '%s\n' "$RDP_PREFLIGHT" | rg -Fq '"available":true'
printf '%s\n' "$RDP_PREFLIGHT" | rg -Fq 'Contents/Resources/freerdp/sdl-freerdp'
codesign --verify --strict --verbose=2 "$FREERDP_HELPER"

MOSH_CLIENT="$APP_PATH/Contents/Resources/mosh/mosh-client"
[[ -x "$MOSH_CLIENT" ]] || {
  echo "发布失败：应用未包含内置 Mosh 客户端。" >&2
  exit 1
}
MOSH_ARCHITECTURES="$(lipo -archs "$MOSH_CLIENT")"
[[ " $MOSH_ARCHITECTURES " == *" arm64 "* && " $MOSH_ARCHITECTURES " == *" x86_64 "* ]] || {
  echo "发布失败：Mosh 客户端不是 arm64 + x86_64 universal binary：$MOSH_ARCHITECTURES" >&2
  exit 1
}
[[ -s "$APP_PATH/Contents/Resources/mosh/licenses/Mosh-GPL-3.0-or-later.txt" ]] || {
  echo "发布失败：Mosh GPLv3 许可证缺失。" >&2
  exit 1
}
"$MOSH_CLIENT" -c >/dev/null
codesign --verify --strict --verbose=2 "$MOSH_CLIENT"

KERMIT_HELPER="$APP_PATH/Contents/Resources/kermit/gkermit"
[[ -x "$KERMIT_HELPER" ]] || {
  echo "发布失败：应用未包含内置 G-Kermit helper。" >&2
  exit 1
}
KERMIT_ARCHITECTURES="$(lipo -archs "$KERMIT_HELPER")"
[[ " $KERMIT_ARCHITECTURES " == *" arm64 "* && " $KERMIT_ARCHITECTURES " == *" x86_64 "* ]] || {
  echo "发布失败：G-Kermit helper 不是 arm64 + x86_64 universal binary：$KERMIT_ARCHITECTURES" >&2
  exit 1
}
[[ -s "$APP_PATH/Contents/Resources/kermit/licenses/G-Kermit-GPL-2.0.txt" ]] || {
  echo "发布失败：G-Kermit GPLv2 许可证缺失。" >&2
  exit 1
}
KERMIT_SOURCE="$APP_PATH/Contents/Resources/kermit/source/gku201.tar.gz"
[[ -s "$KERMIT_SOURCE" ]] || { echo "发布失败：G-Kermit 对应源码缺失。" >&2; exit 1; }
echo "19f9ac00d7b230d0a841928a25676269363c2925afc23e62704cde516fc1abbd  $KERMIT_SOURCE" | shasum -a 256 -c - >/dev/null
"$KERMIT_HELPER" -h 2>&1 | rg -Fq "G-Kermit 2.01"
codesign --verify --strict --verbose=2 "$KERMIT_HELPER"

[[ -s "$APP_PATH/Contents/Resources/licenses/serialport-MPL-2.0.txt" ]] || {
  echo "发布失败：serialport-rs MPL-2.0 许可证缺失。" >&2
  exit 1
}

DMG_PATHS=("$BUNDLE_ROOT"/dmg/*.dmg(N))
(( ${#DMG_PATHS[@]} == 1 )) || {
  echo "发布失败：预期生成且仅生成一个 DMG，实际为 ${#DMG_PATHS[@]} 个。" >&2
  exit 1
}
DMG_PATH="${DMG_PATHS[1]}"
hdiutil verify "$DMG_PATH"

UPDATER_ARCHIVES=("$BUNDLE_ROOT"/**/*.app.tar.gz(N))
UPDATER_SIGNATURES=("$BUNDLE_ROOT"/**/*.app.tar.gz.sig(N))
(( ${#UPDATER_ARCHIVES[@]} == 1 && ${#UPDATER_SIGNATURES[@]} == 1 )) || {
  echo "发布失败：缺少唯一的 Tauri updater 归档或签名。" >&2
  exit 1
}
[[ -s "${UPDATER_ARCHIVES[1]}" && -s "${UPDATER_SIGNATURES[1]}" ]] || {
  echo "发布失败：Tauri updater 归档或签名为空。" >&2
  exit 1
}

xcrun stapler validate "$APP_PATH"
xcrun stapler validate "$DMG_PATH"

echo "发布门禁通过：Developer ID 签名、公证票据、universal 架构、DMG 和 updater 产物均已验证。"
