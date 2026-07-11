# 安全与隐私

- 默认严格校验 SSH 主机密钥。首次连接显式确认，变化立即阻断。
- 密码、私钥口令与代理密码存储于 macOS Keychain，不进入 SQLite、日志或诊断。私钥文件授权以只读 security-scoped Bookmark 保存到连接专属 Keychain 条目，认证期间才启用访问。
- WebView 使用 CSP，正式包不加载远程页面；Tauri capability 仅开放对话框、系统打开和签名更新。
- 命令历史默认本地保存，包含 `password`、`token`、`secret`、`api_key`、`authorization` 等模式的命令不记录；可完全关闭。
- 普通备份不含凭据；凭据备份以随机 salt/nonce、Argon2id 与 AES-256-GCM 加密。
- RDP 密码仅经受管 Helper 的标准输入传递，不进入进程参数或环境变量；多行密码会被拒绝。CNshell 不捆绑 FreeRDP，以保持 GPL 边界清晰。
- 遥测和崩溃上传默认关闭；当前版本不包含任何远程遥测 SDK。

发现安全问题时不要公开敏感日志；先导出脱敏诊断并通过项目维护者提供的私密渠道报告。
