# CNshell v0.2.0-beta.3 跨平台 Beta

这是供真实设备测试的未签名预发布版，不是已完成商业代码签名的正式版本。

## 下载选择

- macOS 13 或更高版本、Apple Silicon/Intel：`CNshell_0.2.0-beta.3_universal.dmg`
- Windows 10 22H2（build 19045）或 Windows 11 x64：`CNshell_0.2.0-beta.3_x64-setup.exe`，状态为 **Beta**
- Windows 11 ARM64：`CNshell_0.2.0-beta.3_arm64-setup.exe`，状态为 **Preview**

安装前必须从本 Release 下载 `SHA256SUMS.txt` 并核对 SHA-256。不要从第三方分发站、网盘或聊天附件安装 CNshell。

## 系统安全提示

macOS 包采用 ad-hoc 签名，没有 Developer ID 和 Apple 公证，Gatekeeper 会显示来源或开发者验证提示。确认下载来源和 SHA-256 后，可在 Finder 中对 CNshell 使用右键“打开”；不要执行 `xattr -cr`，也不要关闭 Gatekeeper。

Windows 安装包尚未做 Authenticode，SmartScreen 可能显示“未知发布者”或信誉提示。只有安装包来自本 Release 且 SHA-256 完全一致时才继续；不要关闭 SmartScreen、Defender 或全局降低系统安全设置。

Tauri updater 更新包使用独立 minisign 密钥签名，应用会校验 `.sig`；这项签名用于更新完整性，不能替代 Developer ID、Apple 公证或 Windows Authenticode。

本次 Beta.3 重点验证 Windows 启动不再弹出额外控制台窗口、原生标题为 CNshell、Windows 浅色终端配色正确，以及从 Beta.2 覆盖升级后连接资料与系统凭据保持正确。

## 希望重点验证

1. Windows 10 22H2 x64、Windows 11 x64 与 Windows 11 ARM64 的安装、启动、覆盖升级、卸载和重装。
2. SSH 终端、自动重连、SFTP、目录上传下载、远程编辑、监控和本地 Shell/ConPTY。
3. 真实 Windows RDP 的首帧、键鼠、中文输入、双向文本剪贴板、声音、缩放、多显示器和断网重连。
4. 中文 IME、100%/125%/150%/200% DPI、高对比、Narrator/VoiceOver、睡眠唤醒和真实 Wi-Fi/有线网络切换。
5. 有对应设备时验证 Windows Hello、实体 FIDO2、VcXsrv/Xming、COM 串口、X/Ymodem/Kermit 和 Mosh 漫游。

请使用 [Beta 真机反馈](https://github.com/YaphetS0903/CNShell/issues/new?template=beta_report.yml) 提交结果。问题报告中不要包含密码、私钥、令牌、完整主机地址或未经检查的诊断文件。

## 后续正式发布

购买 Apple Developer Program 并取得 Windows 代码签名证书或云签名服务后，将切换到仓库现有的 `Signed Cross-platform Release Candidate` 流程：使用 Developer ID、Apple 公证和 Authenticode 生成正式候选，继续沿用同一 updater 公钥、版本清单、SHA-256 与源码附件门禁。
