# Updater 签名密钥管理

CNshell 的 Tauri updater 使用独立 minisign 密钥验证更新归档。它不依赖 Apple Developer Program 或 Windows Authenticode，因此当前未签名 Beta 也必须完成 updater 签名。该签名只保护更新归档的来源与完整性，不能替代 Developer ID、Apple 公证、Authenticode、Gatekeeper 或 SmartScreen。

## 当前公钥

- 用途：`v0.2.0-beta.1` 及后续从该版本升级的 macOS/Windows 更新
- 公钥文本 SHA-256：`ffa2a9cc94b85eec6df771fcc6fffd5bd51933b83061d216d45582995b45331d`
- 客户端配置：`src-tauri/tauri.beta.json`
- Beta 清单：`https://raw.githubusercontent.com/YaphetS0903/CNShell/main/updates/beta/latest.json`

每次发布前应核对公钥文本 SHA-256，不能只按文件名判断密钥是否正确。

## 私钥保管

本机备份保存在 macOS Keychain，Service 为 `com.cnshell.release.updater`：

- Account `private-key`：Tauri updater 私钥
- Account `password`：私钥密码

GitHub 仓库 Actions 使用以下两个 Secret，值应从 Keychain 读取后直接写入 GitHub，不得写入仓库文件、终端日志、Issue、Release 或构建 Artifact：

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

不要在文档、脚本示例或 CI 错误输出中打印私钥或密码。密钥恢复和 GitHub Secret 更新应在受控设备上完成，操作后检查 shell history、剪贴板和临时文件。

## 轮换规则

已经安装的客户端只信任打包时写入的 updater 公钥，因此不能直接用新密钥覆盖现有密钥。直接轮换会让旧客户端永久拒绝后续更新。

需要轮换时，必须先用旧密钥发布一个过渡版本，使客户端同时具备经过审查的新信任路径；确认旧版本能够升级到过渡版本后，才能在下一版本停用旧密钥。具体迁移设计和回滚方案必须在执行前单独评审。

取得 Developer ID、公证和 Authenticode 能力后的正式版本必须继续使用当前 updater 密钥，除非已经完成上述兼容轮换。操作系统代码签名升级不构成更换 updater 密钥的理由。
