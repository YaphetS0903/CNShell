# CNshell v0.2.0-beta.12 跨平台 Beta

这是供真实设备测试的未签名预发布版，不是已完成商业代码签名的正式版本。

## 下载选择

- macOS 13 或更高版本、Apple Silicon/Intel：`CNshell_0.2.0-beta.12_universal.dmg`
- Windows 10 22H2（build 19045）或 Windows 11 x64：`CNshell_0.2.0-beta.12_x64-setup.exe`，状态为 **Beta**
- Windows 11 ARM64：`CNshell_0.2.0-beta.12_arm64-setup.exe`，状态为 **Preview**

安装前必须从本 Release 下载 `SHA256SUMS.txt` 并核对 SHA-256。不要从第三方分发站、网盘或聊天附件安装 CNshell。

## 系统安全提示

macOS 包采用 ad-hoc 签名，没有 Developer ID 和 Apple 公证，Gatekeeper 会显示来源或开发者验证提示。确认下载来源和 SHA-256 后，可在 Finder 中对 CNshell 使用右键“打开”；不要执行 `xattr -cr`，也不要关闭 Gatekeeper。

Windows 安装包尚未做 Authenticode，SmartScreen 可能显示“未知发布者”或信誉提示。只有安装包来自本 Release 且 SHA-256 完全一致时才继续；不要关闭 SmartScreen、Defender 或全局降低系统安全设置。

Tauri updater 更新包使用独立 minisign 密钥签名，应用会校验 `.sig`；这项签名用于更新完整性，不能替代 Developer ID、Apple 公证或 Windows Authenticode。

本次 Beta.12 重点验证工作区布局、连接编辑、文件路径栏、自动化中心、AI 辅助入口、设置保存语义、传输队列与主题对比度。Beta.11 的传输、编辑器、系统信息与 Windows 私钥认证可靠性继续纳入回归。

## 希望重点验证

1. 在 90%、100%、110% 和 125% 界面缩放下检查紧凑/宽窗口，确认工作区布局预设、单面板放大、标题栏连接信息和固定操作区没有遮挡或溢出。
2. 在五种协议间往返切换，确认各协议草稿独立，本地身份与系统默认值正确，固定保存区及“保存并连接”可用。
3. 检查文件路径栏的面包屑、编辑、根目录、用户主目录和无效路径恢复；确认传输队列的状态/连接筛选和失败恢复提示正确。
4. 创建并运行自动化任务，检查自然语言计划、时区、下次运行和持久历史；检查 AI Provider 与终端 AI 辅助入口职责分离及发送前脱敏确认。
5. 切换深色、浅色和高对比主题，检查辅助文字、表单控件、弹窗初始焦点、Escape 关闭和焦点返回；回归 Windows 100%/125%/150%/200% DPI 与中文 IME。
6. 从 Beta.11 使用“设置 → 软件更新”检查并安装 Beta.12，确认 Tauri 签名验证、版本更新、连接资料、凭据引用和自动化运行记录保持正常；ARM64 仍为 Preview。

请使用 [Beta 真机反馈](https://github.com/YaphetS0903/CNShell/issues/new?template=beta_report.yml) 提交结果。问题报告中不要包含密码、私钥、令牌、完整主机地址或未经检查的诊断文件。

## 可选的系统代码签名

GitHub Releases 与 Tauri updater 签名已经提供应用内一键更新，不依赖 Apple Developer Program。以后若取得 Developer ID 或 Windows 代码签名服务，可以启用仓库现有的 `Signed Cross-platform Release Candidate` 流程，改善 Gatekeeper 和 SmartScreen 首次安装体验；届时继续沿用同一 updater 公钥、版本清单、SHA-256 与源码附件门禁。
