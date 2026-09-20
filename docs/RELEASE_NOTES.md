# CNshell v0.2.0

这是 CNshell 首个跨平台正式版，通过 GitHub Releases 分发，并使用独立的 Tauri updater 签名保护应用内更新包。

## 下载选择

- macOS 13 或更高版本、Apple Silicon/Intel：`CNshell_0.2.0_universal.dmg`
- Windows 10 22H2（build 19045）或 Windows 11 x64：`CNshell_0.2.0_x64-setup.exe`
- Windows 11 ARM64：`CNshell_0.2.0_arm64-setup.exe`；该架构尚缺少原生设备验收

安装前必须从本 Release 下载 `SHA256SUMS.txt` 并核对 SHA-256。不要从第三方分发站、网盘或聊天附件安装 CNshell。

## 系统安全提示

macOS 包采用 ad-hoc 签名，没有 Developer ID 和 Apple 公证，Gatekeeper 会显示来源或开发者验证提示。确认下载来源和 SHA-256 后，可在 Finder 中对 CNshell 使用右键“打开”；不要执行 `xattr -cr`，也不要关闭 Gatekeeper。

Windows 安装包尚未做 Authenticode，SmartScreen 可能显示“未知发布者”或信誉提示。只有安装包来自本 Release 且 SHA-256 完全一致时才继续；不要关闭 SmartScreen、Defender 或全局降低系统安全设置。

Tauri updater 更新包使用独立 minisign 密钥签名，应用会校验 `.sig`；这项签名用于更新完整性，不能替代 Developer ID、Apple 公证或 Windows Authenticode。

本次正式版重点修复 macOS 加密导出时按连接数量重复请求钥匙串授权，以及部分 Windows 网络无法访问 GitHub raw 域名时更新检查失败的问题。macOS 会把旧版逐连接凭据迁移到一个 CNshell 钥匙串条目；首次迁移旧条目时系统仍可能逐项询问一次，迁移完成后再次导出只访问一个共享条目。更新器优先读取 GitHub Release 上的固定正式版清单，并保留 raw 清单作为备用地址。

## 希望重点验证

1. 在 macOS 选择“设置 → 连接与安全 → 连接库备份 → 导出含凭据的加密备份”。首次迁移旧凭据时允许系统完成旧条目授权，再次导出应不再按连接数量重复弹窗。
2. 将加密备份导入 Windows，确认使用同一备份口令即可解密，密码凭据写入 Windows 凭据管理器并能正常认证；错误口令不得写入部分连接。
3. 在 Windows v0.2.0 中点击“检查更新”，确认 `raw.githubusercontent.com` 不可用时可通过 GitHub Release 固定清单完成检查；当前已是最新版时不应显示网络错误。
4. 在 90%、100%、110% 和 125% 界面缩放下检查口令表单与设置页，确认输入框、显隐按钮、错误提示及操作按钮没有遮挡或溢出，并可完全使用键盘操作。
5. 在五种协议间往返切换，确认各协议草稿独立，本地身份与系统默认值正确，固定保存区及“保存并连接”可用。
6. 检查文件路径栏、传输队列和自动化任务；切换深色、浅色与高对比主题，并回归 Windows 100%/125%/150%/200% DPI 与中文 IME；ARM64 仍为 Preview。

请使用 [真机反馈](https://github.com/YaphetS0903/CNShell/issues/new?template=release_report.yml) 提交结果。问题报告中不要包含密码、私钥、令牌、完整主机地址或未经检查的诊断文件。

## 可选的系统代码签名

GitHub Releases 与 Tauri updater 签名已经提供应用内一键更新，不依赖 Apple Developer Program。以后若取得 Developer ID 或 Windows 代码签名服务，可以启用仓库现有的 `Signed Cross-platform Release Candidate` 流程，改善 Gatekeeper 和 SmartScreen 首次安装体验；届时继续沿用同一 updater 公钥、版本清单、SHA-256 与源码附件门禁。
