# CNshell v0.2.0-beta.7 跨平台 Beta

这是供真实设备测试的未签名预发布版，不是已完成商业代码签名的正式版本。

## 下载选择

- macOS 13 或更高版本、Apple Silicon/Intel：`CNshell_0.2.0-beta.7_universal.dmg`
- Windows 10 22H2（build 19045）或 Windows 11 x64：`CNshell_0.2.0-beta.7_x64-setup.exe`，状态为 **Beta**
- Windows 11 ARM64：`CNshell_0.2.0-beta.7_arm64-setup.exe`，状态为 **Preview**

安装前必须从本 Release 下载 `SHA256SUMS.txt` 并核对 SHA-256。不要从第三方分发站、网盘或聊天附件安装 CNshell。

## 系统安全提示

macOS 包采用 ad-hoc 签名，没有 Developer ID 和 Apple 公证，Gatekeeper 会显示来源或开发者验证提示。确认下载来源和 SHA-256 后，可在 Finder 中对 CNshell 使用右键“打开”；不要执行 `xattr -cr`，也不要关闭 Gatekeeper。

Windows 安装包尚未做 Authenticode，SmartScreen 可能显示“未知发布者”或信誉提示。只有安装包来自本 Release 且 SHA-256 完全一致时才继续；不要关闭 SmartScreen、Defender 或全局降低系统安全设置。

Tauri updater 更新包使用独立 minisign 密钥签名，应用会校验 `.sig`；这项签名用于更新完整性，不能替代 Developer ID、Apple 公证或 Windows Authenticode。

本次 Beta.7 重点验证 Windows 设置页和环境检测完全在后台运行、底部文件面板入口与快捷键一致，以及终端中文右键菜单。

## 希望重点验证

1. Windows 11 打开设置页并切换各设置分区；Kermit、OpenSSH、X11、RDP、MCP 等环境检测不应启动 `OpenConsole.exe`、Windows Terminal 或其他黑色控制台窗口。
2. 在终端工具栏点击文件图标，或在 macOS 按 `⌘J`、Windows 按 `Ctrl+J`；都应显示底部文件管理区，再次操作应隐藏。macOS 原生“显示文件”菜单行为应相同。
3. 在终端右键，确认菜单项为中文的复制、粘贴、全选和清屏；点击外部或按 `Esc` 可关闭菜单，复制在没有选区时应禁用。
4. 回归 SFTP 长时间挂机后重连、目录展开状态、跨服务器并发传输、窗口关闭和浅色主题，确认 Beta.6 的可靠性修复没有退化。
5. Windows 10/11 x64、Windows 11 ARM64 与 macOS 13+ 的安装和覆盖升级；同时巡检中文 IME、100%/125%/150%/200% DPI、高对比与 Narrator/VoiceOver，ARM64 仍为 Preview。

请使用 [Beta 真机反馈](https://github.com/YaphetS0903/CNShell/issues/new?template=beta_report.yml) 提交结果。问题报告中不要包含密码、私钥、令牌、完整主机地址或未经检查的诊断文件。

## 后续正式发布

购买 Apple Developer Program 并取得 Windows 代码签名证书或云签名服务后，将切换到仓库现有的 `Signed Cross-platform Release Candidate` 流程：使用 Developer ID、Apple 公证和 Authenticode 生成正式候选，继续沿用同一 updater 公钥、版本清单、SHA-256 与源码附件门禁。
