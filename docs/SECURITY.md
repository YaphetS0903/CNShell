# 安全与隐私

## 高级能力边界

- SSH Agent 转发默认关闭并按连接保存。远端 root 或被入侵进程可能借用本机 Agent 签名，因此只应对完全可信主机启用。
- SCP 降级复用 CNshell 已验证的 SSH Session，不启动 `sshpass`、不把密码放入命令参数，也不关闭主机指纹校验。
- Mosh 复用相同的 SSH 认证与主机指纹校验启动远端服务；一次性 Mosh 密钥仅存在于内存和受管客户端环境中，不进入进程参数、数据库、日志或前端 IPC。内置客户端由固定哈希的 GPLv3 Mosh 1.4.0 与 BSD-3-Clause protobuf 21.12 源码构建。
- SSH Certificate 的私钥与证书文件在 macOS 分别使用只读 security-scoped Bookmark，在 Windows 保存经校验的绝对路径记录；私钥口令只存 macOS Keychain 或 Windows 凭据管理器。连接前验证证书是 OpenSSH 用户证书且处于有效期内；非空 principals 必须精确、区分大小写地包含目标用户名，空 principals 保留 OpenSSH 的不限制语义。不接受主机证书，也不提供 CA 私钥导入或签发入口。
- FIDO2 使用独立认证模式，只尝试 Agent 中的 OpenSSH `sk-*` 硬件身份，绝不静默回退到普通软件密钥。触摸和 PIN 由系统 OpenSSH Agent 及硬件设备处理，CNshell 不读取硬件 PIN；Windows 支持 OpenSSH named pipe，并兼容可用的 Pageant 后端。
- RDP 仍在独立受管 FreeRDP sidecar 中运行，主进程只保存有界诊断和状态，不把远端画面经 WebView IPC 转发。密码仅经 stdin；文本剪贴板默认允许而文件剪贴板关闭，音频、麦克风和本地目录映射默认关闭。目录映射在 macOS 使用读写 security-scoped Bookmark，在 Windows 使用用户选择且重新校验的绝对目录；只有一个明确选择的目录会暴露给远端。
- X11 转发默认关闭。macOS 只允许本机 Unix socket 或 loopback display；Windows 只允许 VcXsrv/Xming 提供的 loopback TCP display，拒绝远程 X Server。每个 SSH 会话使用随机假 cookie，入站首包不匹配即关闭，真实 cookie 不发送给远端；首包限制 64 KB、并发 channel 限制为 8，终端关闭或重连会停止旧桥接。
- 自动化仅接受固定步骤 Schema，限制步骤数、超时、正则长度和路径。调度时间按显式 IANA 时区计算，DST 重复墙上时间使用 occurrence key 去重；运行游标由后端重算并先持久化，再以 at-most-once 语义启动任务。受控 Python 只解析白名单 `cnshell.*` 调用，不启动系统 Python；`import`、变量、循环、函数、文件 API、网络 API、系统凭据库和子进程均在编译阶段拒绝。
- Python 脚本必须声明目标、权限和精确本地路径，运行前展示 SHA-256、步骤和高风险警告。操作录制只接收命令面板事件，不读取 xterm 原始键盘；密码、令牌、私钥和 `sshpass` 等敏感命令不会进入录制。
- 同步包在本机使用 Argon2id 派生密钥和 AES-256-GCM 加密。同步服务只接触密文；主机、私钥路径和凭据分别授权。同步口令默认不保存；用户明确启用生物识别保护时，macOS 将口令写入仅限本机且绑定当前指纹集合的 Data Protection Keychain 项，Windows 使用 Windows Hello 保护的 CNG 非导出密钥封装口令。解锁与同步均在 Rust 后端完成，口令不会返回 WebView；手动口令始终可用于恢复。
- 原生 WebDAV 只允许 HTTPS（loopback 测试除外），拒绝 URL 内嵌凭据并禁用跨地址重定向。WebDAV 密码存 macOS Keychain 或 Windows 凭据管理器的独立项；上传使用 ETag 条件写入并先保存远端版本，服务器不提供 ETag 时拒绝危险覆盖。响应 body 逐块累计读取，响应头返回后仍可取消，未知长度响应也不能绕过 50 MB 上限。
- 启动 WebDAV 导入默认关闭；启用时同步口令保存为系统凭据库的独立项，只做远端加密包导入，不静默上传本机凭据。未保存同步口令时启动阶段不发起网络请求。
- X11 在 macOS 缺少 XQuartz、`DISPLAY`、`xauth` 或本地 socket，或 Windows 缺少 VcXsrv/Xming、`DISPLAY`、`xauth.exe` 时保持禁用。Zmodem 与 Mosh 只有在依赖完整且通过协议握手后才启用，检测到可执行文件本身不代表连接成功。
- AI 默认不联网；只有用户配置 Provider、选中文本并确认预览后才发送。API Key 存系统凭据库的独立项，响应头返回后仍可取消且 body 累计限制为 64 KB，终端内容不会因路线图功能自动上传。
- MCP 默认关闭，只通过 stdio sidecar 和 `127.0.0.1` 随机端口访问本机 Broker。Broker 每次启动轮换 generation 与 256 位随机 secret；secret 只写入当前用户私有且拒绝符号链接的 discovery，macOS 权限为 `0600`，Windows 使用仅 Owner 完全控制 DACL。每个客户端的长期 secret 由受管 sidecar 自己创建并保存在 macOS Keychain 或 Windows 凭据管理器，SQLite 只保存 SHA-256；配置只含 sidecar 路径、客户端 ID 与 discovery 路径。请求同时校验协议版本、generation、恒定时间 token、客户端 secret 摘要、客户端状态、受管 sidecar 规范化路径与 SHA-256、连接/工具/远端根授权；每客户端最多 4 个短期会话、2 个执行中请求，并对读写分别限流。撤销客户端会先使数据库授权、短期会话、待审批和进行中请求立即失效，再校验路径确为当前包资源、当前文件 SHA-256 与主程序编译时内嵌摘要一致；sidecar 自验客户端 secret 摘要后删除它拥有的系统凭据。数据库保留的旧版本 sidecar 摘要仍须格式有效，但升级后不要求等于当前摘要，因此可安全清理旧凭据；物理清理失败不会恢复客户端权限，并会在后续 Broker 启动时后台重试。关闭 MCP 或退出也会取消待审批并使旧会话失效。
- MCP 命令与所有写入默认逐次审批，审批能力只存在于 Rust 内存，WebView 不能直接调用业务执行函数绕过批准。命令为非交互 SSH exec，最长 10 分钟，stdout/stderr 合计最多 1 MiB；文件读写、目录分页、消息和响应均有独立上限。远端路径拒绝 `..` 和授权根逃逸；本地上传/下载只接受系统选择器创建的 grant ID 与相对路径，拒绝绝对路径、符号链接、Windows reparse point、特殊文件和越界，下载使用 `.part` 与原子替换。
- MCP 审计最多保留 4,096 条，只记录客户端、工具、连接、目标摘要、风险、结果、耗时、字节数和截断状态，不记录命令输出、文件正文、本地绝对路径或秘密。用户触发的 JSON 导出拒绝符号链接，通过同目录临时文件、`fsync` 和原子替换写入，Unix 权限固定为 `0600`。精确命令规则只保存完整命令文本的 SHA-256 摘要，每客户端最多 256 条；仅固定只读命令和不含格式化/展开的单字面量 `printf` 可保存，其他命令始终逐次审批。升级时会撤销早期预览版规则，之后保存规则时后端再次校验完整命令和摘要一致性。设置页只显示摘要和元数据并允许逐条撤销，撤销会写入脱敏审计。生成客户端配置时会显式绑定随应用分发的 `cnshell-mcp` 规范化路径和 SHA-256；未知可执行文件不能通过首次请求自行认领身份。Codex/Claude 等父进程自身的平台签名证明仍属后续强化，因此 MCP 配置文件和当前本机账号必须保持可信。
- MCP 提供 4 个 Resources 和 2 个 Prompts。两个指南 Resources 与两个 Prompts 是静态安全内容；`cnshell://connections` 和 `cnshell://audit/recent` 必须经 Broker 校验 generation、token、客户端 secret、受管 sidecar 身份、授权和限流。连接资源只列出当前客户端明确拥有 `cnshell_list_connections` 权限的连接，最多 256 条；审计资源只返回当前客户端最近 100 条元数据，并移除 request ID、目标摘要、命令、路径、正文和输出。动态 Resource 是 Broker 内部操作，不出现在 `tools/list`，不能经 `tools/call` 绕过工具授权和写操作审批。
- 插件只信任用户明确导入且核对指纹的 Ed25519 发布者根；同一发布者 ID 不能静默换钥，撤销根会在同一事务中禁用其全部插件。签名按 RFC 8785 规范化 JSON 同时绑定完整 manifest 和 WASM SHA-256，插件 ID 还必须位于发布者命名空间。登记、启用和每次运行都会重验文件、签名、版本与权限，变化后立即失效并写审计。
- 插件运行时采用 Wasmi 1.1.0，不提供 WASI，限制模块 16 MB、内存 32 MB、单实例/单内存/单表、栈/递归和 1000 万燃料。唯一允许的宿主导入位于 `cnshell_v1`：限额日志、用户本次选择的脱敏连接元数据、当前终端明确选中的文本、用户指定的精确域名 HTTPS GET、用户授权目录只读清单/文件、需确认的单次终端输入，以及只创建确认请求的单次 `connectionTest` 凭据代理。网络无代理/重定向且拒绝私网解析；目录拒绝符号链接和越界；终端输入最多 4 KB 且确认前不发送；凭据代理两分钟过期、绑定插件摘要和 SSH 连接，后端使用系统凭据库中的凭据但不向插件返回凭据、令牌或诊断正文。插件审计导出不包含 manifest 本地路径、终端内容、输入正文或秘密，并通过同目录临时文件原子替换。
- 团队工作区的 Owner/Admin/Operator/Viewer 权限由 Rust 后端统一检查，成员变更与元数据审计同事务提交；至少保留一名 Owner，Admin 不能管理 Owner，跨工作区成员 ID 冲突被拒绝。Owner/Admin 的组织目录导出同样由后端授权，只包含工作区、成员、设备公钥/指纹和审计元数据，拒绝符号链接目标并原子落盘。在线 relay 对每个 REST/WebSocket 操作独立重查服务端 RBAC、活动设备和 epoch，不信任客户端角色；成员移除、恢复和角色变化推进密钥 epoch。审计最多 4,096 条，只含成员、动作、目标和时间，不记录命令、终端输出或凭据。生产部署边界见 `docs/TEAM_SECURITY.md` 与 `docs/TEAM_RELAY.md`。
- 团队设备 X25519/Ed25519 私钥只存 macOS Keychain 或 Windows 凭据管理器。连接分享使用 AES-256-GCM 内容密文、每设备 X25519+HKDF-SHA-256 独立密钥封装和覆盖完整 envelope 的 Ed25519 签名；打开时验证活动成员/设备与 epoch，错误设备、未来 epoch、篡改签名或密文均拒绝。epoch 轮换不会破坏当时已授权且仍活动设备的历史分享读取权，撤销状态仍优先拒绝。解密结果只在后端保留 5 分钟一次性预览，凭据确认后直接写入新的系统凭据项。分享不携带备注、启动命令、环境、代理、私钥或证书路径。
- 多人终端默认只读，输出/输入帧 64 KB 有界且序号严格单调；控制输入要求 10–300 秒单持有者租约，并在每帧重新校验双方角色、活动设备、epoch、租约和防重放序号。主持端输出与参与端输入都在 Rust 中使用房间 AES-256-GCM 密钥加密并由实际发送设备 Ed25519 签名，relay 只见 envelope 元数据和有界密文。客户端按房间串行生成序号，待发与观看历史各限制 512 帧/4 MiB，重连以服务端游标对账；参与者离开会撤销房间访问和其控制租约。Relay 已提供不含租户标签的 Prometheus 指标、固定 digest 的 TLS/WSS 限速代理与告警配置、默认强制 `age` 的备份恢复代码及先验 Sigsum proof 再解包的官方 release 验证脚本；容器 smoke 不替代正式 DNS 证书、真实邮件/告警、生产 identity 异地恢复和跨设备验收。
- Telnet 连接始终标记为未加密，连接配置强制使用无认证模式并拒绝保存密码；仅适用于受控内网或遗留设备维护，不应替代 SSH。
- Serial 连接只打开用户明确选择的 macOS `/dev` 设备或 Windows `COM1` 至 `COM256`，默认独占访问；波特率、数据位、校验位、停止位、流控、DTR/RTS 受后端枚举校验，拔出后只重连同一路径，不会扫描或打开其他设备。
- X/Ymodem 只在已打开的 Serial 会话中运行，传输时暂停普通读取和输入并限制为单任务。上传拒绝目录、符号链接、相对路径和超过 50 GB 的文件；下载先写同目录随机 `.part`，成功后原子改名，失败或取消会清理。Ymodem 文件名拒绝路径分隔符、控制字符和 `..`，不能逃出用户选择的目录；取消向设备发送双 CAN。
- Kermit 使用独立受管 G-Kermit 2.01 GPL-2.0 sidecar，参数直接传递且清空继承环境，不经过登录 shell。Serial 与 helper 只通过匿名管道桥接，取消会终止子进程。接收内容先进入应用隔离临时目录，最多 256 个、单文件 50 GB；只有普通文件会复制到用户选择目录内的随机 `.part`，`fsync` 后原子改名，符号链接、目录和异常文件名均被拒绝。应用包同时提供固定哈希的官方对应源码。

## 核心安全策略

- macOS 正式包采用 Developer ID 站外分发并显式启用 Hardened Runtime，主程序、FreeRDP、Mosh、G-Kermit 和 MCP sidecar 必须使用同一 Developer ID 与可信时间戳。App Sandbox 不启用：PTY、X11 Unix socket、Serial 和受管 sidecar 均为产品核心能力；文件访问仍通过原生选择器和 security-scoped Bookmark 最小授权。RDP 麦克风默认关闭，只有用户在连接设置中明确启用后才使用 `NSMicrophoneUsageDescription` 请求系统权限。
- Windows 安装包最低要求 Windows 10 22H2（build 19045），以当前用户模式安装并使用 WebView2 bootstrapper。取得 Authenticode 后，主程序、FreeRDP、Mosh、G-Kermit、MCP sidecar 和 NSIS 安装包必须全部签名并使用可信时间戳；在此之前 Beta 必须明确说明 SmartScreen 风险。Windows 本地 Shell 使用 ConPTY，路径访问使用原生选择器和经校验的绝对路径记录。
- macOS 外部验收预检固定使用系统 PATH，只读取条件状态且不发起网络连接、触发生物识别或打开设备。输出不包含证书名称、密钥、公钥、URL、主机、账号、cookie 或设备路径；落盘报告拒绝符号链接并通过同目录临时文件原子写入，权限固定为 `0600`。Windows 安装生命周期、凭据往返和原生关闭由隔离的 CI 账户执行；CI 证据不替代真机交互验收。
- 默认严格校验 SSH 主机密钥。首次连接显式确认，变化立即阻断。
- 密码、私钥口令与代理密码存储于 macOS Keychain 或 Windows 凭据管理器，不进入 SQLite、日志或诊断。macOS 私钥文件授权以只读 security-scoped Bookmark 保存到连接专属 Keychain 条目；Windows 保存经校验的绝对路径记录，超出凭据项大小上限时退回连接资料中的绝对路径并在每次访问前重新校验。路径只在认证期间使用。
- WebView 使用 CSP，正式包不加载远程页面；Tauri capability 仅开放对话框、系统打开和签名更新。
- 命令历史默认本地保存，包含 `password`、`token`、`secret`、`api_key`、`authorization` 等模式的命令不记录；可完全关闭。
- 普通备份不含凭据；凭据备份以随机 salt/nonce、Argon2id 与 AES-256-GCM 加密。
- RDP 密码仅经内置受管 sidecar 的标准输入传递，不进入进程参数或环境变量；多行密码会被拒绝。FreeRDP、OpenSSL、SDL、SDL_ttf 与 FreeType 静态链接进当前架构 sidecar；macOS 使用 universal helper，Windows 分发 x64/ARM64 PE，并随应用提供完整许可证文本和对应源码。
- 遥测和崩溃上传默认关闭；当前版本不包含任何远程遥测 SDK。

发现安全问题时不要公开敏感日志；先导出脱敏诊断并通过项目维护者提供的私密渠道报告。
