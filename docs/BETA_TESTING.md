# CNshell v0.2.0-beta.9 跨平台 Beta

这是供真实设备测试的未签名预发布版，不是已完成商业代码签名的正式版本。

## 下载选择

- macOS 13 或更高版本、Apple Silicon/Intel：`CNshell_0.2.0-beta.9_universal.dmg`
- Windows 10 22H2（build 19045）或 Windows 11 x64：`CNshell_0.2.0-beta.9_x64-setup.exe`，状态为 **Beta**
- Windows 11 ARM64：`CNshell_0.2.0-beta.9_arm64-setup.exe`，状态为 **Preview**

安装前必须从本 Release 下载 `SHA256SUMS.txt` 并核对 SHA-256。不要从第三方分发站、网盘或聊天附件安装 CNshell。

## 系统安全提示

macOS 包采用 ad-hoc 签名，没有 Developer ID 和 Apple 公证，Gatekeeper 会显示来源或开发者验证提示。确认下载来源和 SHA-256 后，可在 Finder 中对 CNshell 使用右键“打开”；不要执行 `xattr -cr`，也不要关闭 Gatekeeper。

Windows 安装包尚未做 Authenticode，SmartScreen 可能显示“未知发布者”或信誉提示。只有安装包来自本 Release 且 SHA-256 完全一致时才继续；不要关闭 SmartScreen、Defender 或全局降低系统安全设置。

Tauri updater 更新包使用独立 minisign 密钥签名，应用会校验 `.sig`；这项签名用于更新完整性，不能替代 Developer ID、Apple 公证或 Windows Authenticode。

本次 Beta.9 重点验证 Windows 现代 OpenSSH、PEM、PKCS#8、Ed25519 及加密私钥认证，并回归中文/空格路径、SSH Certificate、SSH Jump 和错误口令提示。Beta.8 的五分类设置中心、固定保存栏、设置搜索、高级模块折叠，以及 macOS VoiceOver 和 Windows Narrator 设置页可访问性继续纳入回归。

## 希望重点验证

1. 打开设置，确认基础设置、连接与安全、自动化与集成、团队与云端、关于与支持五个分类清晰可切换；取消和保存按钮应始终固定在窗口底部，无需滚动到底部。
2. 在设置搜索中输入 `Mosh`，激活结果后应切换到“连接与安全”、展开“高级协议与转发”并把焦点移到对应模块。折叠模块或切换分类后，尚未保存的草稿应保持。
3. 使用键盘完整操作设置窗口：Tab/Shift+Tab 应在模态窗口内首尾回绕，帮助按钮可访问，按 `Esc` 可关闭；macOS 使用 VoiceOver、Windows 使用 Narrator 时，五个分类、模块、搜索结果和固定操作栏应具有正确名称与角色。
4. Windows 在 100%/125%/150%/200% DPI 下打开设置并切换分类；所有内容与固定操作栏都应位于可见窗口和 UI Automation 树内。Kermit、OpenSSH、X11、RDP、MCP 等环境检测不应启动 `OpenConsole.exe`、Windows Terminal 或其他黑色控制台窗口。
5. 回归终端工具栏文件按钮及 macOS `⌘J`/Windows `Ctrl+J`，确认底部文件管理区可显示和隐藏；终端右键菜单应为中文的复制、粘贴、全选和清屏，点击外部或按 `Esc` 可关闭。
6. 回归 SFTP 长时间挂机后重连、目录展开状态、跨服务器并发传输、窗口关闭、浅色主题、中文 IME、Windows 10/11 x64、Windows 11 ARM64 与 macOS 13+ 安装及覆盖升级；ARM64 仍为 Preview。

请使用 [Beta 真机反馈](https://github.com/YaphetS0903/CNShell/issues/new?template=beta_report.yml) 提交结果。问题报告中不要包含密码、私钥、令牌、完整主机地址或未经检查的诊断文件。

## 后续正式发布

购买 Apple Developer Program 并取得 Windows 代码签名证书或云签名服务后，将切换到仓库现有的 `Signed Cross-platform Release Candidate` 流程：使用 Developer ID、Apple 公证和 Authenticode 生成正式候选，继续沿用同一 updater 公钥、版本清单、SHA-256 与源码附件门禁。
