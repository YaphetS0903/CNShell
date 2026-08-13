# CNshell v0.2.0-beta.6 跨平台 Beta

这是供真实设备测试的未签名预发布版，不是已完成商业代码签名的正式版本。

## 下载选择

- macOS 13 或更高版本、Apple Silicon/Intel：`CNshell_0.2.0-beta.6_universal.dmg`
- Windows 10 22H2（build 19045）或 Windows 11 x64：`CNshell_0.2.0-beta.6_x64-setup.exe`，状态为 **Beta**
- Windows 11 ARM64：`CNshell_0.2.0-beta.6_arm64-setup.exe`，状态为 **Preview**

安装前必须从本 Release 下载 `SHA256SUMS.txt` 并核对 SHA-256。不要从第三方分发站、网盘或聊天附件安装 CNshell。

## 系统安全提示

macOS 包采用 ad-hoc 签名，没有 Developer ID 和 Apple 公证，Gatekeeper 会显示来源或开发者验证提示。确认下载来源和 SHA-256 后，可在 Finder 中对 CNshell 使用右键“打开”；不要执行 `xattr -cr`，也不要关闭 Gatekeeper。

Windows 安装包尚未做 Authenticode，SmartScreen 可能显示“未知发布者”或信誉提示。只有安装包来自本 Release 且 SHA-256 完全一致时才继续；不要关闭 SmartScreen、Defender 或全局降低系统安全设置。

Tauri updater 更新包使用独立 minisign 密钥签名，应用会校验 `.sig`；这项签名用于更新完整性，不能替代 Developer ID、Apple 公证或 Windows Authenticode。

本次 Beta.6 重点验证 SFTP 长时间挂机、关闭连接和并发操作后的恢复能力，文件树展开位置保持、跨服务器传输隔离、窗口关闭失败恢复，以及大型远程文本编辑器的按需加载。

## 希望重点验证

1. 展开多层远端目录后切换到终端、快捷命令或监控，再返回文件区；活动路径应保持，历史目录不应在后台一次性全部加载。
2. 关闭 SSH 连接并挂机后重连，连续展开 `/`、`/home`、`/etc`、`/dev`；单次 SFTP 卡顿应在有界时间内报错并允许直接重试，不应要求重启应用。
3. 同时连接两台服务器并向相同远端路径上传文件；两个任务应相互独立。下载到同一本地目标仍应阻止冲突写入。
4. 编辑不同类型的远程文本文件，确认编辑器按需加载语法能力；输入非法权限值时必须拒绝，合法三位或四位八进制权限应正常提交。
5. Windows 10/11 x64、Windows 11 ARM64 与 macOS 13+ 的安装、覆盖升级、窗口关闭、主题、SSH/SFTP/监控、本地 Shell 和真实 RDP；同时巡检中文 IME、100%/125%/150%/200% DPI、高对比与 Narrator/VoiceOver，ARM64 仍为 Preview。

请使用 [Beta 真机反馈](https://github.com/YaphetS0903/CNShell/issues/new?template=beta_report.yml) 提交结果。问题报告中不要包含密码、私钥、令牌、完整主机地址或未经检查的诊断文件。

## 后续正式发布

购买 Apple Developer Program 并取得 Windows 代码签名证书或云签名服务后，将切换到仓库现有的 `Signed Cross-platform Release Candidate` 流程：使用 Developer ID、Apple 公证和 Authenticode 生成正式候选，继续沿用同一 updater 公钥、版本清单、SHA-256 与源码附件门禁。
