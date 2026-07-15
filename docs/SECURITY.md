# 安全与隐私

## 高级能力边界

- SSH Agent 转发默认关闭并按连接保存。远端 root 或被入侵进程可能借用本机 Agent 签名，因此只应对完全可信主机启用。
- SCP 降级复用 CNshell 已验证的 SSH Session，不启动 `sshpass`、不把密码放入命令参数，也不关闭主机指纹校验。
- Mosh 复用相同的 SSH 认证与主机指纹校验启动远端服务；一次性 Mosh 密钥仅存在于内存和受管客户端环境中，不进入进程参数、数据库、日志或前端 IPC。内置客户端由固定哈希的 GPLv3 Mosh 1.4.0 与 BSD-3-Clause protobuf 21.12 源码构建。
- SSH Certificate 的私钥与证书文件分别使用只读 security-scoped Bookmark；私钥口令只存 Keychain。连接前验证证书是 OpenSSH 用户证书且处于有效期内，不接受主机证书，也不提供 CA 私钥导入或签发入口。
- FIDO2 使用独立认证模式，只尝试 Agent 中的 OpenSSH `sk-*` 硬件身份，绝不静默回退到普通软件密钥。触摸和 PIN 由 macOS/OpenSSH Agent 处理，CNshell 不读取硬件 PIN。
- RDP 仍在独立受管 FreeRDP sidecar 中运行，主进程只保存有界诊断和状态，不把远端画面经 WebView IPC 转发。密码仅经 stdin；文本剪贴板默认允许而文件剪贴板关闭，音频、麦克风和本地目录映射默认关闭。目录映射使用读写 security-scoped Bookmark，只有用户明确选择的一个目录会暴露给远端。
- X11 转发默认关闭，只允许本机 Unix socket 或 loopback display，拒绝远程 X Server。每个 SSH 会话使用随机假 cookie，入站首包不匹配即关闭，真实 cookie 不发送给远端；首包限制 64 KB、并发 channel 限制为 8，终端关闭或重连会停止旧桥接。
- 自动化仅接受固定步骤 Schema，限制步骤数、超时、正则长度和路径。受控 Python 只解析白名单 `cnshell.*` 调用，不启动系统 Python；`import`、变量、循环、函数、文件 API、网络 API、Keychain 和子进程均在编译阶段拒绝。
- Python 脚本必须声明目标、权限和精确本地路径，运行前展示 SHA-256、步骤和高风险警告。操作录制只接收命令面板事件，不读取 xterm 原始键盘；密码、令牌、私钥和 `sshpass` 等敏感命令不会进入录制。
- 同步包在本机使用 Argon2id 派生密钥和 AES-256-GCM 加密。同步服务只接触密文；主机、私钥路径和凭据分别授权。同步口令默认不保存；用户明确启用 Touch ID 时，口令写入仅限本机、绑定当前指纹集合的 Data Protection Keychain 项，解锁与同步在 Rust 后端完成，口令不会返回 WebView。手动口令始终可用于恢复。
- 原生 WebDAV 只允许 HTTPS（loopback 测试除外），拒绝 URL 内嵌凭据并禁用跨地址重定向。WebDAV 密码存独立 Keychain 项；上传使用 ETag 条件写入并先保存远端版本，服务器不提供 ETag 时拒绝危险覆盖。
- 启动 WebDAV 导入默认关闭；启用时同步口令保存为独立 Keychain 项，只做远端加密包导入，不静默上传本机凭据。未保存同步口令时启动阶段不发起网络请求。
- X11 在 XQuartz、`DISPLAY`、`xauth` 或本地 socket 不完整时保持禁用。Zmodem 与 Mosh 只有在依赖完整且通过协议握手后才启用，检测到可执行文件本身不代表连接成功。
- AI 默认不联网；只有用户配置 Provider、选中文本并确认预览后才发送。API Key 存独立 Keychain，终端内容不会因路线图功能自动上传。通用插件尚未启用。
- 插件 manifest 支持检查和本地阻断登记，但不执行代码。入口必须是相对 WASM 路径；登记时固定 manifest SHA-256，权限变化、禁用和移除写入本地审计。审计导出不包含 manifest 本地路径或终端内容，并通过同目录临时文件原子替换。签名缺失或发布者不在受信任密钥列表时 `enabled/executable` 始终为 false；网络、目录、终端输入和凭据代理默认拒绝。
- Telnet 连接始终标记为未加密，连接配置强制使用无认证模式并拒绝保存密码；仅适用于受控内网或遗留设备维护，不应替代 SSH。
- Serial 连接只打开用户明确选择的 `/dev` 设备，默认独占访问；波特率、数据位、校验位、停止位、流控、DTR/RTS 受后端枚举校验，拔出后只重连同一路径，不会扫描或打开其他设备。
- X/Ymodem 只在已打开的 Serial 会话中运行，传输时暂停普通读取和输入并限制为单任务。上传拒绝目录、符号链接、相对路径和超过 50 GB 的文件；下载先写同目录随机 `.part`，成功后原子改名，失败或取消会清理。Ymodem 文件名拒绝路径分隔符、控制字符和 `..`，不能逃出用户选择的目录；取消向设备发送双 CAN。

## 核心安全策略

- 默认严格校验 SSH 主机密钥。首次连接显式确认，变化立即阻断。
- 密码、私钥口令与代理密码存储于 macOS Keychain，不进入 SQLite、日志或诊断。私钥文件授权以只读 security-scoped Bookmark 保存到连接专属 Keychain 条目，认证期间才启用访问。
- WebView 使用 CSP，正式包不加载远程页面；Tauri capability 仅开放对话框、系统打开和签名更新。
- 命令历史默认本地保存，包含 `password`、`token`、`secret`、`api_key`、`authorization` 等模式的命令不记录；可完全关闭。
- 普通备份不含凭据；凭据备份以随机 salt/nonce、Argon2id 与 AES-256-GCM 加密。
- RDP 密码仅经内置受管 sidecar 的标准输入传递，不进入进程参数或环境变量；多行密码会被拒绝。FreeRDP、OpenSSL、SDL、SDL_ttf 与 FreeType 静态链接进 universal sidecar，并随应用分发完整许可证文本。
- 遥测和崩溃上传默认关闭；当前版本不包含任何远程遥测 SDK。

发现安全问题时不要公开敏感日志；先导出脱敏诊断并通过项目维护者提供的私密渠道报告。
