# CNshell v0.2.0-beta.5 跨平台 Beta

这是供真实设备测试的未签名预发布版，不是已完成商业代码签名的正式版本。

## 下载选择

- macOS 13 或更高版本、Apple Silicon/Intel：`CNshell_0.2.0-beta.5_universal.dmg`
- Windows 10 22H2（build 19045）或 Windows 11 x64：`CNshell_0.2.0-beta.5_x64-setup.exe`，状态为 **Beta**
- Windows 11 ARM64：`CNshell_0.2.0-beta.5_arm64-setup.exe`，状态为 **Preview**

安装前必须从本 Release 下载 `SHA256SUMS.txt` 并核对 SHA-256。不要从第三方分发站、网盘或聊天附件安装 CNshell。

## 系统安全提示

macOS 包采用 ad-hoc 签名，没有 Developer ID 和 Apple 公证，Gatekeeper 会显示来源或开发者验证提示。确认下载来源和 SHA-256 后，可在 Finder 中对 CNshell 使用右键“打开”；不要执行 `xattr -cr`，也不要关闭 Gatekeeper。

Windows 安装包尚未做 Authenticode，SmartScreen 可能显示“未知发布者”或信誉提示。只有安装包来自本 Release 且 SHA-256 完全一致时才继续；不要关闭 SmartScreen、Defender 或全局降低系统安全设置。

Tauri updater 更新包使用独立 minisign 密钥签名，应用会校验 `.sig`；这项签名用于更新完整性，不能替代 Developer ID、Apple 公证或 Windows Authenticode。

本次 Beta.5 重点验证 SFTP 文件夹在监控、终端和 MCP 并行使用时仍能快速展开，系统文件可以拖入远端目录，以及本机 MCP Server 的授权、审批、撤销和安装自检流程。

## 希望重点验证

1. 保持监控刷新并连续展开 `/`、`/home`、`/etc`、`/dev` 及多层目录；关闭连接、挂机后重连，再次展开目录不应持续转圈、超时或变红。
2. 从 Finder/资源管理器把单个文件和目录拖入文件管理区，确认传输队列、冲突策略、取消/重试和远端结果正确。
3. 在“设置 → MCP 服务”启用 MCP、创建隔离客户端并执行自检；验证只读工具、命令审批、精确规则、一次性/持久本地授权、撤销和 5 秒完成提示。
4. Windows 10 22H2 x64、Windows 11 x64 与 Windows 11 ARM64 的安装、启动、覆盖升级、卸载和重装，以及 SSH/SFTP/监控、本地 Shell/ConPTY 和真实 RDP。
5. 中文 IME、100%/125%/150%/200% DPI、高对比、Narrator/VoiceOver、睡眠唤醒、Wi-Fi/有线网络切换，以及有设备时的 Windows Hello、FIDO2、VcXsrv/Xming、COM 串口和 Mosh。

请使用 [Beta 真机反馈](https://github.com/YaphetS0903/CNShell/issues/new?template=beta_report.yml) 提交结果。问题报告中不要包含密码、私钥、令牌、完整主机地址或未经检查的诊断文件。

## 后续正式发布

购买 Apple Developer Program 并取得 Windows 代码签名证书或云签名服务后，将切换到仓库现有的 `Signed Cross-platform Release Candidate` 流程：使用 Developer ID、Apple 公证和 Authenticode 生成正式候选，继续沿用同一 updater 公钥、版本清单、SHA-256 与源码附件门禁。
