#!/bin/zsh
set -u
set -o pipefail

ROOT="${0:A:h}/.."
OUTPUT=""
REQUIRE_READY=false
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
export PATH

usage() {
  cat <<'EOF'
Usage: ./scripts/external-acceptance-preflight.sh [--output REPORT.md] [--require-ready]

Read-only preflight for CNshell external acceptance environments. The report never
prints certificate names, keys, URLs, hostnames, account names, or device paths.

  --output PATH     Atomically write a private (0600) Markdown report.
  --require-ready   Exit 2 when any prerequisite is missing.
  --help            Show this help.
EOF
}

while (( $# > 0 )); do
  case "$1" in
    --output)
      (( $# >= 2 )) || { echo "--output requires a path" >&2; exit 64; }
      OUTPUT="$2"
      shift 2
      ;;
    --require-ready)
      REQUIRE_READY=true
      shift
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

typeset -a CHECK_NAMES CHECK_STATUSES CHECK_DETAILS

add_check() {
  CHECK_NAMES+=("$1")
  CHECK_STATUSES+=("$2")
  CHECK_DETAILS+=("$3")
}

is_present() {
  [[ -n "${(P)1:-}" ]]
}

is_safe_https() {
  [[ "$1" == https://* && "$1" != *"@"* && "$1" != *"#"* ]]
}

if [[ "$(uname -s 2>/dev/null)" == "Darwin" ]]; then
  os_version="$(sw_vers -productVersion 2>/dev/null || echo unknown)"
  architecture="$(uname -m 2>/dev/null || echo unknown)"
  add_check "macOS 基线" "PASS" "macOS ${os_version//|/ }，架构 ${architecture//|/ }"
else
  os_version="not-macos"
  architecture="$(uname -m 2>/dev/null || echo unknown)"
  add_check "macOS 基线" "MISSING" "正式桌面验收必须在 macOS 13 或更高版本执行"
fi

if [[ "$architecture" == "x86_64" ]]; then
  add_check "Intel 真机" "READY" "当前环境是 Intel，可执行 Intel 运行验收"
else
  add_check "Intel 真机" "MISSING" "当前环境不是 Intel，仍需 Intel Mac 真机"
fi
add_check "多 macOS / 干净 Mac" "MANUAL" "需在无开发环境的目标系统执行安装、升级、卸载和数据保留"

identity_output="$(security find-identity -v -p codesigning 2>/dev/null || true)"
identity_count="$(printf '%s\n' "$identity_output" | grep -c 'Developer ID Application:' || true)"
if (( identity_count > 0 )) && is_present APPLE_SIGNING_IDENTITY \
  && [[ "$identity_output" == *"\"${APPLE_SIGNING_IDENTITY}\""* ]]; then
  add_check "Developer ID" "READY" "检测到并匹配指定 Developer ID Application 身份，报告不包含名称"
elif (( identity_count > 0 )); then
  add_check "Developer ID" "MISSING" "检测到 ${identity_count} 个身份，但未提供或未匹配精确签名身份"
else
  add_check "Developer ID" "MISSING" "未检测到 Developer ID Application 身份"
fi
unset identity_output

release_config="$ROOT/src-tauri/tauri.release.json"
if [[ -f "$release_config" ]] && ! grep -Eq 'REPLACE_WITH_TAURI_UPDATER_PUBLIC_KEY|\.example' "$release_config"; then
  add_check "正式 updater 配置" "READY" "检测到不含占位值的私有 release 配置"
else
  add_check "正式 updater 配置" "MISSING" "缺少私有 release 配置或配置仍含占位值"
fi

api_key_path="${APPLE_API_KEY_PATH:-}"
api_key_mode=""
if [[ -n "$api_key_path" && -f "$api_key_path" && ! -L "$api_key_path" ]]; then
  api_key_mode="$(stat -f '%Lp' "$api_key_path" 2>/dev/null || true)"
fi
if is_present APPLE_API_ISSUER && is_present APPLE_API_KEY \
  && [[ "$api_key_mode" == "400" || "$api_key_mode" == "600" ]]; then
  add_check "Apple 公证凭据" "READY" "公证变量和 API 私钥文件均已提供，报告不包含值或路径"
else
  add_check "Apple 公证凭据" "MISSING" "需要 issuer、key ID 和权限为 0400/0600 的普通 API 私钥文件"
fi
unset api_key_path api_key_mode

updater_base_url="${UPDATER_DOWNLOAD_BASE_URL:-}"
if is_present TAURI_SIGNING_PRIVATE_KEY \
  && is_safe_https "$updater_base_url"; then
  add_check "签名更新服务" "READY" "检测到 updater 私钥和 HTTPS 下载基址，报告不包含内容"
else
  add_check "签名更新服务" "MISSING" "需要 updater 私钥和正式 HTTPS 下载基址"
fi
unset updater_base_url

display_value="${DISPLAY:-}"
if [[ -x /opt/X11/bin/xauth \
  && ( "$display_value" == :* || "$display_value" == localhost:* || "$display_value" == /private/tmp/* ) \
  && "$display_value" != *[[:space:]]* ]] \
  && /opt/X11/bin/xauth list "$display_value" >/dev/null 2>&1; then
  add_check "XQuartz / X11" "READY" "XQuartz、DISPLAY 和 MIT cookie 已就绪，报告不包含 cookie"
else
  add_check "XQuartz / X11" "MISSING" "需要运行中的 XQuartz、DISPLAY 和有效 MIT cookie"
fi
unset display_value

fido_count=0
if [[ -S "${SSH_AUTH_SOCK:-}" ]] && command -v ssh-add >/dev/null 2>&1; then
  fido_count="$(ssh-add -L 2>/dev/null | awk '
    $1 ~ /^sk-(ssh-ed25519|ecdsa-sha2-nistp256)(-cert-v01)?@openssh\.com$/ { count++ }
    END { print count + 0 }
  ' || true)"
fi
if (( fido_count > 0 )); then
  add_check "FIDO2 身份" "READY" "Agent 中检测到 ${fido_count} 个硬件身份，报告不包含公钥"
else
  add_check "FIDO2 身份" "MISSING" "Agent 中未检测到 OpenSSH sk-* 硬件身份"
fi
add_check "FIDO2 交互" "MANUAL" "需人工验证触摸、PIN、取消和使用中拔出"
add_check "Touch ID 交互" "MANUAL" "需在已安装应用中验证保存、解锁、取消和指纹集合变化"

setopt local_options null_glob
serial_count=0
for device in /dev/cu.*; do
  case "${device:t}" in
    cu.Bluetooth-Incoming-Port|cu.debug-console|cu.wlan-debug) ;;
    *) (( serial_count++ )) ;;
  esac
done
if (( serial_count > 0 )); then
  add_check "实体串口" "READY" "检测到 ${serial_count} 个候选串口，报告不包含设备路径"
else
  add_check "实体串口" "MISSING" "未检测到可用于互操作验收的实体串口"
fi

rdp_targets=0
for variable in CNSHELL_ACCEPTANCE_RDP_WINDOWS_10 CNSHELL_ACCEPTANCE_RDP_WINDOWS_11 CNSHELL_ACCEPTANCE_RDP_WINDOWS_SERVER; do
  is_present "$variable" && (( rdp_targets++ ))
done
if (( rdp_targets == 3 )); then
  add_check "Windows RDP 矩阵" "READY" "三类目标资料均已提供，报告不包含主机信息"
else
  add_check "Windows RDP 矩阵" "MISSING" "需要 Windows 10、Windows 11 和 Windows Server 三类目标"
fi
add_check "RDP 画面与交互" "MANUAL" "需人工验证首帧、中文 IME、键鼠、剪贴板、音频、缩放和重连"

if is_present CNSHELL_ACCEPTANCE_MOSH_TARGET; then
  add_check "Mosh 漫游目标" "READY" "已提供测试目标，报告不包含目标信息"
else
  add_check "Mosh 漫游目标" "MISSING" "需要允许 UDP 的真实 Mosh 目标"
fi
add_check "Mosh 网络切换" "MANUAL" "需人工执行 IP/Wi-Fi 切换和 30 秒断网恢复，不由预检修改网络"

webdav_url="${CNSHELL_ACCEPTANCE_WEBDAV_URL:-}"
if is_safe_https "$webdav_url" && is_present CNSHELL_ACCEPTANCE_SECOND_DEVICE; then
  add_check "WebDAV 双设备" "READY" "已提供真实 WebDAV 环境和第二设备标记，报告不包含 URL"
else
  add_check "WebDAV 双设备" "MISSING" "需要无内嵌凭据的 HTTPS WebDAV 环境和第二台设备"
fi
unset webdav_url

relay_url="${CNSHELL_ACCEPTANCE_RELAY_URL:-}"
if is_safe_https "$relay_url" \
  && is_present CNSHELL_RELAY_AGE_RECIPIENT \
  && is_present CNSHELL_ACCEPTANCE_RELAY_BACKUP_TARGET \
  && is_present CNSHELL_ACCEPTANCE_SECOND_DEVICE; then
  add_check "生产 Relay" "READY" "TLS、备份 recipient、异地目标和第二设备标记均已提供"
else
  add_check "生产 Relay" "MISSING" "需要正式 TLS 服务、生产 age recipient、异地备份目标和第二台设备"
fi
unset relay_url
add_check "生产 Relay 演练" "MANUAL" "需在生产边界验证 WSS、邮件、限速、监控、恢复和跨网络控制"

render_report() {
  local ready_count=0
  local missing_count=0
  local manual_count=0
  local index row_status
  print -r -- "# CNshell 外部验收预检"
  print -r -- ""
  print -r -- "- 生成时间：$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
  print -r -- "- 数据边界：不包含证书名称、密钥、公钥、URL、主机、账号、cookie 或设备路径"
  print -r -- ""
  print -r -- "| 检查项 | 状态 | 说明 |"
  print -r -- "| --- | --- | --- |"
  for (( index = 1; index <= ${#CHECK_NAMES}; index++ )); do
    row_status="${CHECK_STATUSES[$index]}"
    case "$row_status" in
      PASS|READY) (( ready_count++ )) ;;
      MISSING) (( missing_count++ )) ;;
      MANUAL) (( manual_count++ )) ;;
    esac
    print -r -- "| ${CHECK_NAMES[$index]} | $row_status | ${CHECK_DETAILS[$index]} |"
  done
  print -r -- ""
  print -r -- "汇总：READY/PASS $ready_count，MISSING $missing_count，MANUAL $manual_count。"
  print -r -- "READY 只表示前置条件已检测到；对应场景实际执行并留证后才能在验收矩阵标记通过。"
}

if [[ -n "$OUTPUT" ]]; then
  output_parent="${OUTPUT:h}"
  [[ -d "$output_parent" ]] || { echo "Output directory does not exist" >&2; exit 73; }
  [[ ! -L "$OUTPUT" ]] || { echo "Refusing to replace a symbolic link" >&2; exit 73; }
  umask 077
  temporary="$(mktemp "$output_parent/.cnshell-external-acceptance.XXXXXX")" || exit 73
  trap 'rm -f "$temporary"' EXIT
  render_report > "$temporary"
  chmod 600 "$temporary"
  mv -f -- "$temporary" "$OUTPUT"
  trap - EXIT
  print -r -- "External acceptance report written with mode 0600."
else
  render_report
fi

if [[ "$REQUIRE_READY" == true ]] && (( ${CHECK_STATUSES[(I)MISSING]} > 0 )); then
  exit 2
fi
