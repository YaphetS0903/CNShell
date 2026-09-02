# CNshell v0.2.0-beta.10 跨平台 Beta

这是供真实设备测试的未签名预发布版，不是已完成商业代码签名的正式版本。

## 下载选择

- macOS 13 或更高版本、Apple Silicon/Intel：`CNshell_0.2.0-beta.10_universal.dmg`
- Windows 10 22H2（build 19045）或 Windows 11 x64：`CNshell_0.2.0-beta.10_x64-setup.exe`，状态为 **Beta**
- Windows 11 ARM64：`CNshell_0.2.0-beta.10_arm64-setup.exe`，状态为 **Preview**

安装前必须从本 Release 下载 `SHA256SUMS.txt` 并核对 SHA-256。不要从第三方分发站、网盘或聊天附件安装 CNshell。

## 系统安全提示

macOS 包采用 ad-hoc 签名，没有 Developer ID 和 Apple 公证，Gatekeeper 会显示来源或开发者验证提示。确认下载来源和 SHA-256 后，可在 Finder 中对 CNshell 使用右键“打开”；不要执行 `xattr -cr`，也不要关闭 Gatekeeper。

Windows 安装包尚未做 Authenticode，SmartScreen 可能显示“未知发布者”或信誉提示。只有安装包来自本 Release 且 SHA-256 完全一致时才继续；不要关闭 SmartScreen、Defender 或全局降低系统安全设置。

Tauri updater 更新包使用独立 minisign 密钥签名，应用会校验 `.sig`；这项签名用于更新完整性，不能替代 Developer ID、Apple 公证或 Windows Authenticode。

本次 Beta.10 重点验证监控栏与远端文件管理区在不同分辨率和系统缩放下的自动字号适配，以及两个面板独立的手动字号设置。Beta.9 的 Windows 私钥认证兼容性，以及 Beta.8 的设置中心和读屏可访问性继续纳入回归。

## 希望重点验证

1. 在 Windows 的 100%/125%/150%/200% DPI 下分别检查监控栏与文件管理区；其中 2K/4K 且系统缩放为 100%/125% 的大屏，字号应自动显示为 `自12px` 或 `自13px`，目录、文件表格、磁盘和进程信息应清晰可读。
2. 在 Windows 150%/200% DPI 或 macOS Retina 环境中确认自动字号通常保持 `自11px`，避免应用字号与系统缩放叠加导致内容过大。
3. 分别使用两个面板的 `−/+` 调节字号，确认监控栏和文件管理区互不影响，范围限制为 10–16px；重启 CNshell、切换连接后手动字号应保留。
4. 点击对应面板的“字”图标恢复自动模式，再切换显示器或调整系统缩放，确认自动字号会根据新的显示环境重新计算。
5. 回归 Windows 现代 OpenSSH、PEM、PKCS#8、Ed25519 与加密私钥认证，以及设置页后台环境检测，确认不启动 `OpenConsole.exe`、Windows Terminal 或其他黑色控制台窗口。
6. 回归 SFTP 长时间挂机后重连、目录展开状态、跨服务器并发传输、窗口关闭、浅色主题、中文 IME、Windows 10/11 x64、Windows 11 ARM64 与 macOS 13+ 安装及覆盖升级；ARM64 仍为 Preview。

请使用 [Beta 真机反馈](https://github.com/YaphetS0903/CNShell/issues/new?template=beta_report.yml) 提交结果。问题报告中不要包含密码、私钥、令牌、完整主机地址或未经检查的诊断文件。

## 后续正式发布

购买 Apple Developer Program 并取得 Windows 代码签名证书或云签名服务后，将切换到仓库现有的 `Signed Cross-platform Release Candidate` 流程：使用 Developer ID、Apple 公证和 Authenticode 生成正式候选，继续沿用同一 updater 公钥、版本清单、SHA-256 与源码附件门禁。
