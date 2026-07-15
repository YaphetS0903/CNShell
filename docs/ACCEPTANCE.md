# CNshell v0.1.0 验收矩阵

> 最后核验：2026-07-16（macOS 本机）
> 状态定义：**通过**＝已有自动化或本机产物证据；**部分**＝实现完成但验收环境不完整；**外部阻塞**＝需要其他设备、发行凭据或长时窗口。

## 1. 核心功能

| PLAN 要求 | 状态 | 实现位置 | 验收证据 |
| --- | --- | --- | --- |
| 文件夹式连接管理、搜索、协议筛选、最近连接、排序、右键与软删除恢复 | 通过 | `src/features/connections/ConnectionSidebar.tsx`、`src-tauri/src/db.rs` | Playwright 覆盖创建连接、嵌套文件夹、连接菜单移动入口和废纸篓恢复；连接支持拖放或菜单移动到任意层级；Rust 覆盖循环拒绝、递归删除、恢复与永久删除 |
| SSH 密码、私钥、Agent 与严格主机指纹 | 通过 | `src/features/connections/ConnectionEditor.tsx`、`src-tauri/src/bookmark.rs`、`src-tauri/src/ssh.rs` | 真实 OpenSSH/Paramiko 覆盖正确与错误凭据、未知与变化指纹；私钥支持原生文件选择和绝对路径输入，并通过 Keychain 内只读 security-scoped Bookmark 持久访问 |
| 分阶段连接诊断 | 通过 | `src-tauri/src/ssh.rs`、`src/features/connections/ConnectionDiagnostics.tsx` | 协议测试验证 TCP、主机密钥、认证、Shell 阶段；认证与 Shell 阻塞均有 30 秒恢复超时，macOS Keychain 授权等待不会让界面永久停在连接中 |
| SOCKS5、HTTP CONNECT、SSH Jump | 通过 | `src-tauri/src/ssh.rs`、`src/features/settings/AdvancedSettings.tsx` | 真实代理链协议测试全部通过 |
| 本地、远程、动态端口转发 | 通过 | `src-tauri/src/tunnel.rs`、`src/features/connections/TunnelManager.tsx` | 三类真实转发协议测试通过 |
| xterm.js、多标签、拆分、搜索、剪贴板、IME、True Color、PTY resize | 通过 | `src/features/terminal/TerminalView.tsx`、`src/features/terminal/TerminalWorkspace.tsx`、`src-tauri/src/ssh.rs` | E2E 直接验证拆分后主标签保持选中、左右各显示独立终端，选择副标签会安全收拢布局；单元/E2E、1 MB 输出及 PTY roundtrip 通过；浏览器端 IME 风格文本插入保留中文与 Emoji；本机密码 SSH 夹具验证交互 PTY、中文/Emoji 双向字节和 ANSI/True Color 全屏序列；最终 universal DMG 的只读挂载应用经真实 Canvas 截图确认中文宽字符、Emoji、线框、光标定位和 RGB 颜色无明显错位；腾讯云 PTY 的 `vim`、`top`、`tmux` 验收亦通过 |
| 自动重连与安全错误停止重试 | 通过 | `src-tauri/src/ssh.rs` | Rust 测试验证 1/2/5/10/30 秒及认证/指纹错误停止策略 |
| SFTP 目录树、导航、排序、隐藏文件、虚拟滚动与文件操作 | 通过 | `src/features/files/RemoteDirectoryTree.tsx`、`src/features/files/FileManager.tsx`、`src-tauri/src/sftp.rs` | 左侧目录树按需加载并自动展开活动路径祖先；文件右键含下载、上传、新建文件/文件夹、打开方式、编辑、复制、重命名、权限、压缩/解压和删除，新建文件采用排他创建不覆盖同名目标；组件/E2E 与真实 SFTP 小型协议覆盖。此前空目录、10 万文件、特殊文件名、符号链接和无权限目录证据保留 |
| 上传下载队列、速度/ETA、暂停/取消/重试、冲突策略 | 通过 | `src/features/files/TransferQueue.tsx`、`src-tauri/src/sftp.rs` | 1 GB 真实 SFTP 流式上传/下载及 SHA-256 一致性通过；中断临时文件、显式重试已覆盖 |
| 文件夹打包上传与下载 | 通过 | `src/features/files/FileManager.tsx`、`src-tauri/src/sftp.rs` | 本机真实 OpenSSH 覆盖两层目录、中文文件名、上传/下载往返内容、覆盖时两阶段备份交换，以及本地/远端临时归档清理；后台任务支持取消，失败不先删除原目标 |
| 下载临时文件与上传原子替换 | 通过 | `src-tauri/src/sftp.rs` | `.cnshell-part`、远端临时文件及原子 rename 已由协议/Rust 测试覆盖 |
| 10 MB 文本编辑、修改冲突与原子保存 | 通过 | `src/features/files/TextEditor.tsx`、`src-tauri/src/sftp.rs` | UTF-8 字节边界测试；冲突比较、临时文件、fsync 与原子 rename 路径由代码审计及真实 SFTP 覆盖 |
| CPU、内存、Swap、网络、进程、磁盘与 5 分钟历史 | 通过 | `src-tauri/src/monitor.rs`、`src/features/monitor/MonitorSidebar.tsx`、`src/features/monitor/MonitorHistoryChart.tsx` | CPU Sparkline 与 uPlot 网络上下行/延迟折线共享对齐的 5 分钟历史窗口并提供实时数值/ARIA 替代；采集解析、单项降级与窗口测试通过；腾讯云实测额外单核负载 0.576% |
| 系统信息复制与导出 | 通过 | `src/features/monitor/SystemInfoPanel.tsx`、`src-tauri/src/monitor.rs` | 腾讯云真机验证完整内容；Rust 验证 JSON 临时文件原子导出与清理 |
| 快捷命令、历史策略、帮助与首次引导 | 通过 | `src/features/terminal/TerminalWorkspace.tsx`、`src/features/help/HelpModal.tsx`、`src-tauri/src/db.rs` | Playwright 验证内置命令只读、用户命令删除和帮助弹窗可访问性；敏感历史检测与全量清空测试通过 |
| 连接导入导出与加密凭据备份 | 通过 | `src/features/connections/ConnectionSidebar.tsx`、`src/features/settings/AdvancedSettings.tsx`、`src-tauri/src/backup.rs` | 连接库工具栏与设置均有导入入口；Argon2id + AES-256-GCM 往返及错误口令拒绝测试通过 |
| 可折叠、可调尺寸的三栏与底部工具区 | 通过 | `src/App.tsx`、`src/features/terminal/TerminalWorkspace.tsx`、`src/lib/layout.ts` | 鼠标拖动与键盘方向键均可调整；侧栏尺寸进入工作区恢复，底部高度本机持久化；紧凑窗口 E2E 通过 |
| RDP 独立 FreeRDP adapter | 部分 | `src-tauri/src/rdp.rs`、`src/features/rdp/RdpWorkspace.tsx` | 本机实现已通过：RDP 密码保存 Keychain，全部参数与密码仅经 Helper stdin 传递；静态 helper 内置 NTLM 所需 MD4/MD5/RC4；动态分辨率、剪贴板、自动重连参数；受管 PID、标签状态、关闭、诊断翻译及应用退出清理。当前局域网测试目标的 TCP 3389 可建立，但对默认、Cookie 和 legacy RDP 协商探针均返回 0 字节并断开，需目标 Windows 开启有效 RDP 服务后再验收画面与输入 |
| 浅色、深色、高对比、键盘和 VoiceOver 语义 | 部分 | `src/styles.css`、`src/components/Modal.tsx`、`src/features/terminal/TerminalWorkspace.tsx`、`src/features/files/FileManager.tsx` | Playwright 与最终 DMG 辅助功能树验证跟随 macOS 浅色、手动主题优先级、高对比、模态焦点陷阱，会话/工具标准 tab/tabpanel、真实方向键切换与独立菜单入口，SFTP 虚拟表格行列/排序/总数，以及监控数值替代；完整 VoiceOver 朗读顺序仍需用户主动开启系统 VoiceOver 后人工巡检 |
| SQLite 历史迁移与失败前备份 | 通过 | `src-tauri/src/db.rs`、`src-tauri/migrations/` | v1–v4 无损升级、任务恢复、数据库 `.backup` 测试通过 |
| 本地 Shell / Telnet / Serial 基线 | 部分（代码与回环/枚举测试） | `src-tauri/src/local_shell.rs`、`src-tauri/src/telnet.rs`、`src-tauri/src/serial.rs`、连接编辑器 | 本地 PTY 和 Telnet 生命周期已通过；Serial 已完成设备枚举、参数校验、独占打开、DTR/RTS、输入、状态事件和同路径拔出重连代码与测试。当前没有实体串口设备，未声明真实网络设备、USB 转串口拔出/重插和硬件流控验收通过。 |
| Serial X/Ymodem | 部分（协议核心与双端回环通过） | `src-tauri/src/xymodem.rs`、`src-tauri/src/serial.rs`、`src/features/terminal/SerialTransferPanel.tsx` | Xmodem 128/1K、Checksum/CRC、Ymodem Batch、双 EOT、重复块、CRC 拒绝、双 CAN、进度、单会话互斥、原子下载、失败清理、50 GB/256 文件边界和远端路径隔离已由 Rust/前端测试覆盖。本机没有 `sx/rx/sb/rb` 和实体串口设备，未声明外部实现或硬件互操作通过。 |
| Serial Kermit | 部分（官方 helper 管道互操作通过） | `src-tauri/src/kermit.rs`、`scripts/build-kermit-sidecar.sh`、Serial 文件面板 | 官方 G-Kermit 2.01 已由固定源码 SHA-256 构建为 macOS 13+ universal sidecar，只链接系统库并随包附 GPL-2.0 与对应源码；两个独立 helper 完成 12,345 字节二进制互传。受管桥接、取消、环境清空、接收隔离、普通文件/数量/大小校验、跨磁盘临时复制、fsync、原子改名和冲突重命名已有测试。实体串口与第三方 Kermit 设备仍待真机验收。 |

## 2. 自动化证据

### 2.1 竞品路线图增量验收

| 能力 | 状态 | 证据 |
| --- | --- | --- |
| 智能命令、会话日志、高亮通知、批量执行 | 通过 | `smart-command.test.ts`、`SmartCommandEntry.test.tsx`、`SessionLogDialog.test.tsx`、`terminal-triggers.test.ts`、`BatchExecutionDialog.test.tsx`；Rust 覆盖 1 MB 日志、后端危险批量命令拒绝和任务隔离 |
| CodeMirror 编辑、进程/网络诊断、嵌套拆分 | 通过 | `TextEditor.test.tsx`、`ProcessManager.test.tsx`、`NetworkDiagnostics.test.tsx`、`terminal-layout.test.ts`；UTF-8/mtime/原子保存、PID 身份校验及可取消诊断由 Rust 覆盖 |
| OpenSSH 与终端安全 | 通过 | `OpenSshTools.test.tsx`、`terminal-safety.test.ts`；Rust 覆盖 `Include` 的 `*`/`?`、循环/越界、公钥权限部署；逐窗格时间戳 gutter、Copy Mode、跨标签搜索、最低对比度和增强光标通过 Lint/构建与单元测试 |
| SCP 安全降级 | 通过（代码/编译） | SFTP 子系统失败后使用 `ssh2::Session::scp_send/scp_recv`，复用现有认证、代理和主机指纹；临时文件、取消和大小校验保留。未破坏用户服务器以人为制造 SFTP 故障，因此未声明真机故障注入通过 |
| Agent 转发 | 通过（本机能力） | `ProtocolSettings.test.tsx` 验证逐连接选择和风险确认；Rust 连接及重连路径调用 `request_auth_agent_forwarding`。是否获远端 sshd 允许取决于目标主机配置 |
| 受限自动化 | 通过 | `AutomationSettings.test.tsx` 验证最终预览；Rust 验证固定 Schema、步骤/超时/正则边界，后端提供任务 ID、逐步结果、取消、失败停止和文件原子落盘 |
| 加密同步 | 通过（代码/本机密钥边界） | Rust 验证同步包不出现主机与私钥路径明文、旧包保留、错误口令拒绝和同 ID 本地连接不被覆盖；UI 默认关闭凭据同步。可选 Touch ID 口令使用设备专属 Data Protection Keychain 与当前指纹集合保护，解锁后不返回前端，手动口令恢复入口始终保留 |
| Zmodem/Mosh/X11 | 部分 | Zmodem 已与腾讯云 `lrzsz` 完成双向互操作；Mosh 已完成真实公网 UDP 短测；X11 已由本机 OpenSSH 接受真实 `x11-req` 并建立远端 `DISPLAY`。XQuartz GUI、Mosh 漫游/Intel 与对应外部环境仍待验收 |
| AI、插件、团队协作 | 部分（代码与 loopback 通过，生产/真机待补） | AI、可信插件沙箱、本地团队 RBAC/设备/审计、Keychain 设备密钥 E2E 离线连接分享、relay 服务端、客户端账号/工作区同步、在线多人终端 WebSocket/观看控制 UI 和 relay 备份恢复运维代码已完成。正式域名/TLS/WSS/邮件/限速/监控、真实加密异地恢复和双设备跨网络会话仍未完成，不声明生产在线团队服务验收通过 |

本轮遵守用户指示，不重跑 soak、1 GB 传输或 100k 文件测试。对应历史证据保留，但不计入本轮新增验收。

### 2026-07-15 Mosh 阶段增量验收

| 项目 | 结果 |
| --- | --- |
| 官方 Mosh sidecar | `scripts/build-mosh-sidecar.sh` 从固定哈希的 Mosh 1.4.0 与 protobuf 21.12 源码成功重建；产物为 arm64 + x86_64 universal binary、最低 macOS 13，仅链接 macOS 系统库，许可证与 ad-hoc 签名检查通过 |
| 自动化门禁 | `npm run lint`、99 个前端测试、114 个 Rust 库测试、`npm run build` 与 `git diff --check` 通过；覆盖握手严格解析、密钥脱敏、端口边界、旧配置默认值、外部会话 profile 生命周期及分块断线提示检测 |
| 腾讯云互操作短测 | Ubuntu 24.04 x86_64 的 `mosh-server` 完成 SSH 握手，修复云主机 NAT 内网地址误用后，内置客户端使用连接配置公网地址建立 UDP 会话、接收输入并正常退出；未重复长测，临时服务端会话已清理 |
| 保留验收边界 | Wi-Fi/IP 切换、30 秒断网恢复、Intel 真机运行和自动化屏幕 resize 捕获仍待对应环境；当前不声明这些场景通过 |

### 2026-07-15 SSH Certificate 增量验收

| 项目 | 结果 |
| --- | --- |
| 数据与文件授权 | `0005_ssh_certificate.sql` 历史迁移通过；私钥与证书使用独立 Keychain Bookmark，连接复制、永久删除、备份字段和旧数据默认值均纳入同一生命周期 |
| 元数据与有效期 | OpenSSH 用户证书类型、CA、Key ID、序列号、主体和 UTC 有效期解析通过；主机证书、格式错误、过期和尚未生效均拒绝认证 |
| 真实协议 | 本机临时 OpenSSH `sshd` 使用 Ed25519 CA 与 `TrustedUserCAKeys`，CNshell/libssh2 通过短期用户证书真实认证并执行固定命令；夹具退出清理 CA、密钥、证书与服务进程 |
| 自动化门禁 | Rust 118 项、前端 100 项、lint、TypeScript/Vite production build 与 diff 检查通过 |

### 2026-07-15 X11 增量验收

| 项目 | 结果 |
| --- | --- |
| SSH 协议 | 本机真实 OpenSSH `sshd` 接受 CNshell/libssh2 的 `x11-req`，远端命令确认 `DISPLAY` 已建立；请求仍复用 CNshell 主机指纹、认证和 Session |
| Cookie 与边界 | 大小端 X11 setup 首包均验证随机假 cookie 并替换为真实 cookie；错误 cookie、远程 display、畸形长度和超过 64 KB 首包拒绝，单会话最多 8 个 channel |
| 生命周期 | X11 forwarder 保存在终端 handle，关闭和重连替换会停止接收器与桥接线程；X11 与 Mosh 互斥，开关默认关闭并要求风险确认 |
| 保留验收边界 | 当前 Mac 未安装 XQuartz且没有 `DISPLAY`，因此真实 GUI 窗口、XQuartz 重启和多图形 channel 端到端仍待对应环境，不声明通过 |

### 2026-07-15 FIDO2 与 Touch ID 增量验收

| 项目 | 结果 |
| --- | --- |
| FIDO2 身份隔离 | 新增独立 `fido2Agent` 模式，只接受 OpenSSH `sk-ssh-ed25519`、`sk-ecdsa-sha2-nistp256` 及证书变体；普通 Agent 身份由解析器测试证明不会进入候选集 |
| FIDO2 交互与诊断 | 编辑器可枚举硬件身份的算法、备注和 OpenSSH SHA-256 指纹；后端覆盖无硬件身份以及 Agent 可提供信号时的触摸、PIN、取消、拔出分类，未知 Agent failure 保持诚实的综合提示 |
| Touch ID 密钥边界 | 同步目录以规范化路径 SHA-256 作为 Keychain account，不保存原路径；同步口令使用 `BIOMETRY_CURRENT_SET`、设备专属且设置密码保护的 Data Protection Keychain 项，读取与加密同步均留在 Rust 后端，临时字符串使用 zeroize 清理 |
| 恢复与 UI | 手动同步口令入口始终可用；Touch ID 可保存、移除、生成同步包和导入，取消/认证失败/指纹变化均提示手动恢复，不把 Touch ID 表述为远端 SSH 生物认证 |
| 自动化门禁 | FIDO2 blob/算法/指纹/错误分类、Touch ID 目录账户脱敏、缺失目录和前端保存/恢复/后端同步调用均有测试；Rust `127/127`（另跳过既有 soak）、前端 `105/105`、lint、production build 和 arm64 Tauri App 构建通过 |
| 本机安装 | arm64 App 完成严格 ad-hoc 深度签名并覆盖安装至 `/Applications/CNshell.app`；安装前后现有 SQLite 哈希一致，6 条连接仍在，应用进程正常启动。本机 `bioutil` 确认 Touch ID 解锁策略有效，但未自动弹出会打断用户的生物识别提示 |
| 保留验收边界 | 当前没有实体 FIDO2 设备，不声明真实触摸、PIN、取消或拔出通过；Touch ID 系统弹窗的保存、解锁与取消仍需用户在已安装应用中完成一次人工交互后才能标记真机通过 |

### 2026-07-15 RDP 深度集成增量验收

| 项目 | 结果 |
| --- | --- |
| 路线决策 | 共享帧/IOSurface、原生 NSView 子视图和独立 SDL 窗口三路线已记录比较；采用独立窗口深度联动以保留 SDL 原生 IME/Metal 与 sidecar 崩溃隔离，详见 `docs/RDP_TECHNICAL_EVALUATION.md` |
| sidecar | FreeRDP 3.28.0 universal（arm64 + x86_64）按固定源码哈希、OpenSSL/SDL 固定版本构建；修订版 4 包含用户关窗正常退出和 `CNSHELL_RDP_STATE=online` 状态标记，签名/架构/系统库检查通过 |
| 连接状态与窗口 | 返回 connecting，FreeRDP `postConnect` 标记在线；自动重连日志映射 reconnecting；退出映射 closed/failed，退出 131 作为 SDL 手动关窗；窗口位置跟随 CNshell，macOS `NSRunningApplication` 支持聚焦与隐藏 |
| 配置与权限 | UI 与 Rust 参数测试覆盖显示器、全屏、三种缩放、四档画质、文本剪贴板（文件剪贴板关闭）、声音、麦克风确认和单目录读写 Bookmark；密码和参数不进入 argv |
| 本机证据 | 安装包严格 ad-hoc 签名验证；`--rdp-preflight` 返回内置 helper；`--rdp-displays` 解析本机 FreeRDP 显示器列表；Rust `133/133`、前端 `107/107`、lint/build 通过 |
| 保留验收边界 | 当前没有 Windows 10/11 或 Server 主机，不能声明真实首帧、中文 IME、键鼠、剪贴板方向、声音/麦克风、动态分辨率、多显示器和真实断网重连通过 |

### 2026-07-16 插件完整权限体系增量验收

| 项目 | 结果 |
| --- | --- |
| 信任与供应链 | 用户导入 Ed25519 发布者根；插件 ID 绑定发布者命名空间；RFC 8785 规范化签名同时覆盖完整 manifest 与 WASM SHA-256；发布者撤销、换钥、manifest/WASM/权限或域名变化都会禁用或要求重新登记 |
| 沙箱与显式授权 | Wasmi 1.1.0 不提供 WASI，限制 16 MB 模块、32 MB 内存、单实例/内存/表、1,024 表元素、64 层递归、64 KiB 栈和 1,000 万燃料；只有用户在启用时逐项确认的权限写入 `grantedPermissions`，插件导入未声明或未授予能力时实例化前拒绝 |
| 网络适配器 | 只预加载用户本次填写且命中 manifest 精确域名的 HTTPS GET；固定 443、禁用系统代理和重定向，DNS 结果固定给客户端并拒绝本机、私网、链路本地、组播、未指定、CGNAT 与基准网段；响应流式限制 64 KB，不开放通用 socket |
| 目录适配器 | 每次运行由用户选择目录，macOS 使用只读 security-scoped Bookmark；只暴露最多 256 个顶层条目的 64 KB JSON 清单和用户指定的单个 64 KB 文件；根目录、目标文件、相对路径、符号链接、UTF-8 文件名和 canonical containment 均由 Rust 校验 |
| 终端与凭据 | 终端只读只接收当前明确选中的最多 64 KB 文本；终端输入最多 4 KB、两分钟单次使用，运行结果展示完整 JSON 转义内容，用户再次确认后才走现有会话输入路径；凭据代理只允许一次 `connectionTest`，绑定插件摘要和 SSH 连接，插件不能读取 Keychain、凭据、令牌或诊断正文 |
| 审计与隐私 | 插件登记、启用、禁用、运行、发布者撤销、凭据代理和终端输入决定进入有界本机审计；不记录终端正文、输入正文、URL、目录路径、凭据或 manifest 本地路径，导出使用同目录临时文件原子替换 |
| 自动化门禁 | `npm run check` 通过：前端 46 个文件、127 项测试；Rust 187 项测试；TypeScript、Vite production build、IPC 生成一致性、ESLint 与 `git diff --check` 通过。新增测试覆盖敏感权限默认拒绝和选择性授予、未知导入、燃料耗尽、HTTPS/域名/端口/片段拒绝、相对路径约束及签名插件生命周期 |
| universal 应用包 | `APPLE_SIGNING_IDENTITY=- npm run tauri build -- --target universal-apple-darwin --bundles app` 成功；打包后的主程序、FreeRDP、Mosh 和 G-Kermit 均由 `lipo` 验证为 `x86_64 arm64`，最低系统 13.0，`codesign --verify --deep --strict` 通过。包内 RDP preflight 返回可用，Mosh 报告 1.4.0，G-Kermit 报告 2.01；G-Kermit 对应源码归档 SHA-256 与 notice 一致。此处仅为 ad-hoc 本机包验证，构建日志明确因缺少 Apple 公证凭据跳过 notarization，不冒充正式发布通过 |

### 2026-07-16 团队终端加密传输层增量验收

| 项目 | 结果 |
| --- | --- |
| 房间邀请 | 每房间生成随机 32 字节内存密钥；邀请使用临时 X25519、HKDF-SHA-256 与 AES-256-GCM 只封装给指定活动设备，AAD 绑定工作区/房间/epoch/设备，主持设备 Ed25519 签名完整 RFC 8785 envelope；邀请 5 分钟过期，重复接受不能回滚序号 |
| 加密帧 | 输出和控制输入使用 AES-256-GCM，并由实际发送设备 Ed25519 签名；AAD 覆盖工作区、房间、epoch、成员、设备、方向、类型、序号、租约 ID 与 generation。接收端重新读取成员、设备、角色和 epoch 后才验签解密，单帧明文仍限制 64 KB |
| 断线恢复 | 主持端只在内存保留 5 分钟、最多 512 帧/4 MB 的密文；客户端按最后已接收序号补帧，窗口不足时明确要求重新加入。房间关闭、终端关闭或应用退出会清除重放帧并清零房间密钥 |
| 本机双客户端证据 | 两份独立 SQLite 客户端状态分别绑定主持 Owner 与参与 Operator，并使用各自 Keychain X25519/Ed25519 私钥完成邀请接受、输出解密、跳号拒绝、补帧恢复、密文篡改拒绝、控制输入、重复输入拒绝和设备撤销后失效 |
| 自动化门禁 | `npm run check` 通过：前端 46 个文件、127 项测试，Rust 187 项测试，IPC 类型一致、ESLint、TypeScript 与 Vite production build 通过；`git diff --check` 通过 |
| 附加静态检查 | `cargo clippy --all-targets -- -D warnings` 仍被本轮之前已存在的 20 类全仓 lint 阻断，涉及自动化、Bookmark、监控、Mosh、OpenSSH、插件、SFTP、SSH、WebDAV、Zmodem 和既有测试代码；本次新增协作模块没有出现在错误列表。该命令不是当前仓库 `npm run check` 门禁，不将其记录为通过 |
| 保留验收边界 | 本阶段当时只验收加密客户端协议；客户端在线 UI 已在后续增量接通。正式 relay 部署和两台真实设备跨网络会话仍未完成，本机回环不冒充真实网络或生产服务验收 |

### 2026-07-16 团队 relay 服务端基础增量验收

| 项目 | 结果 |
| --- | --- |
| 账号与设备会话 | 账号密码使用 Argon2id；未知账号仍执行等成本密码校验。账号 token 10 分钟、设备 token 15 分钟且数据库只存域分离 SHA-256 哈希；设备通过两分钟一次性 challenge 和 Ed25519 私钥签名刷新，重复 challenge 拒绝 |
| 服务端权限与撤销 | Owner/Admin/Operator/Viewer 权限在每个 REST 与 WebSocket 帧重新读取；加入、角色变化、成员移除和设备撤销推进 epoch 并关闭旧房间。成员移除会撤销其设备和所有设备 token；工作区始终保留 Owner 约束 |
| 密文路由与补帧 | relay 验证邀请/帧签名和路由元数据，但 schema 没有房间密钥、终端明文、连接凭据、主机、用户名或本地路径。输出只允许主持设备严格递增发布；输入只允许当前租约设备并仅路由主持端；补帧限制五分钟/512 帧/4 MiB |
| 资源与数据生命周期 | HTTP body 256 KiB；工作区成员/邀请、账号/设备会话、挑战、活动房间、房间设备、历史房间、审计和密文重放均有硬上限或过期清理。Owner 通过精确工作区 ID 确认可永久级联删除团队数据 |
| 自动化证据 | 真实 loopback HTTP/WebSocket 集成测试启动两个账号与两台设备，覆盖注册、邀请、RBAC 拒绝、challenge、签名密文、断线补帧、成员/租约握手恢复、控制输入定向、重复拒绝、参与者离开与租约撤销、成员移除、epoch、token 失效、审计隐私和永久删除；relay 独立 `cargo clippy --all-targets -- -D warnings` 与 `cargo test` 通过 |

### 2026-07-16 团队 relay 客户端账号与工作区同步增量验收

| 项目 | 结果 |
| --- | --- |
| endpoint 与秘密边界 | 生产地址强制 HTTPS，仅 loopback 测试允许 HTTP；禁用重定向并限制超时/响应。账号和设备 token 只进 Keychain，SQLite 只保存非秘密元数据和到期时间 |
| 工作区生命周期 | 已完成账号注册/登录/退出、工作区发布、邮箱邀请令牌、可重试邀请接受、成员/设备/epoch 快照同步、在线角色更新和设备撤销；本机公钥与 Keychain 私钥身份固定比对 |
| 自动刷新与证据 | 双账号、双 SQLite/Keychain 身份通过真实 relay HTTP 完成发布、邀请、角色同步；删除设备 token 后通过 Ed25519 一次性 challenge 自动恢复。客户端数据库检查不含明文密码或 token |
| 保留验收边界 | 本阶段只验收账号与目录同步；房间 WebSocket、观看/控制 UI 和备份恢复运维代码已在后续增量中接通。正式 TLS 域名、邮件、监控、真实加密异地恢复和两台真实设备跨网络证据仍未完成 |
| 部署边界 | 提供非 root、只读文件系统兼容的 Dockerfile 和仅绑定 host loopback 的 Compose 示例；非 loopback 启动必须显式确认位于 TLS 代理后。当前机器没有 Docker，未执行镜像构建；正式 DNS/TLS/WSS、邮件、代理限速、加密卷、生产备份恢复、监控告警和真实两设备仍待外部环境 |

### 2026-07-16 在线多人终端客户端增量验收

| 项目 | 结果 |
| --- | --- |
| 房间与重连 | 客户端按房间维护受管 WebSocket，设备 token 到期前刷新；`ready`/`accepted` 对账服务端输出和输入游标，已确认前缀丢弃、缺口拒绝、未确认帧重发。每房间加密与入队串行，待发限制 512 帧/4 MiB |
| 成员与控制 | 握手恢复服务端权威的已加入成员和当前租约；加入/离开、授权/撤销实时广播。参与者离开后服务端撤销房间访问及其租约；主持端收到租约广播后同步本地逐帧输入校验状态 |
| UI 与内容边界 | 常驻协作中心支持 SSH 主持、按设备邀请、参与者 xterm 观看、10–300 秒控制移交和关闭。只有匹配本机且未过期的租约开放输入；弹窗隐藏时仍保留有界输出缓冲。房间密钥、设备私钥、账号/设备 token 和密文加解密均不进入 React 状态 |
| 自动化证据 | 6 个 Rust 游标/队列测试覆盖 ready 前缀、缺口、单次 accepted、未确认重连和输入恢复；6 个 React 测试覆盖建房、邀请、主持输出转发、接受邀请、只读/租约输入、授权/撤销；真实 relay loopback 测试覆盖成员/租约初始快照和参与者离开 |
| 保留验收边界 | 当前只完成同机 loopback 自动化和运维代码演练，没有正式 DNS/TLS/WSS、邮件、限速、监控、真实 `age` 异地恢复，也没有两台真实设备跨网络观看、控制和断网恢复证据；因此仅记为代码与 loopback 通过 |

### 2026-07-16 团队 relay 运维基线增量验收

| 项目 | 结果 |
| --- | --- |
| 健康与停机 | `/health` 为纯 liveness；`/ready` 执行 SQLite `SELECT 1`，正常返回 200、连接池关闭返回 503。SIGINT/SIGTERM 停止接流，活动 WebSocket 收到关闭帧后退出；Compose 和镜像健康检查使用 `/ready`，Compose 预留 30 秒停机窗口 |
| 备份边界 | `VACUUM INTO` 生成在线一致性快照，执行 `quick_check`、`foreign_key_check` 和核心 schema 校验；生产路径要求 `CNSHELL_RELAY_AGE_RECIPIENT` 和 `age`，未配置时失败，不会降级明文。最终载荷与 SHA-256 sidecar 权限为 `0600`，保留策略只匹配严格时间戳文件名 |
| 恢复边界 | 恢复前校验 sidecar，密文要求独立 identity；拒绝符号链接、未知文件名、运行中 PID、已有目标和损坏/错误 schema，校验完成后才把新数据库安装到目标。服务停机确认是必填开关 |
| 本机演练 | `/usr/bin/sqlite3` 自动演练覆盖默认明文拒绝、符号链接拒绝、两份保留且不删除诱饵文件、完整往返、拒绝覆盖、篡改拒绝、真实 relay `/health`/`/ready` 和 SIGTERM 正常退出 |
| 保留验收边界 | 当前机器没有 `age` 和 Docker，只完成显式明文测试分支及二进制本机进程演练；未执行真实加密、容器构建、加密卷、对象存储、异地主机恢复、正式监控或事故演习，不将这些项目记为通过 |

| 命令 | 结果（2026-07-12） |
| --- | --- |
| `npm run lint` | 通过，0 warning |
| `npm run test` | 通过，36 个文件、87 个测试；覆盖竞品路线图新增智能命令、日志、高亮性能、批量执行、编辑器、诊断、OpenSSH、协议设置、自动化 UI，以及原有目录树、连接文件夹、布局恢复、监控和通知 |
| `npm run build` | 通过，TypeScript 与 Vite production build |
| `cargo test --manifest-path src-tauri/Cargo.toml -- --skip live_ssh_soak` | 通过，95 个测试、1 个 soak 明确跳过；覆盖自动化 Schema、加密同步密文/冲突、OpenSSH 通配、协议能力、日志完整性，以及原有数据库、RDP、Bookmark、Transport Pool、迁移、IPC 和安全路径；所有 live 环境变量均已清除 |
| `npm run test:e2e` | 通过，15 个 Playwright 场景；覆盖远端目录树、完整文件右键入口、嵌套连接文件夹/移动入口、布局恢复、左右拆分状态和终端偏好实时应用 |
| `npm run test:pty-fixture` | 通过；本机 Paramiko 密码认证夹具提供真实 PTY Shell，验证中文/Emoji 双向 UTF-8、ANSI 清屏/光标控制和 True Color 输出 |
| `CNSHELL_PROTOCOL_FILTER=live_ssh_directory_transfer_round_trip_and_cleanup npm run test:protocol` | 通过；本轮真实 OpenSSH 小目录覆盖嵌套中文文件、上传/下载、覆盖交换及本地/远端临时清理。此前 10 万文件、1 GB、代理、隧道等全量协议证据继续保留，本轮未重复消耗资源 |
| `npm audit --audit-level=moderate` | 本轮两次请求均在连接 npm registry 前发生 TLS 中断，未取得审计结论；不把网络失败记作 0 漏洞。提交前其余本地门禁不受影响，待 registry 恢复后重跑 |
| `zsh -n scripts/release.sh` 与发布门禁单测 | 通过；发布脚本从 Info.plist 读取实际小写可执行文件名，并检查最低 macOS 13、Developer ID、Gatekeeper、双架构、DMG、公证票据及 updater 签名产物 |
| `CNSHELL_SOAK_SECONDS=6 npm run test:soak` | 通过，脚本、独立 monitor Exec、PTY 与 RSS 指标可运行 |
| `APPLE_SIGNING_IDENTITY=- npm run tauri build -- --target universal-apple-darwin --bundles app,dmg` | 通过，当前源码生成最低 macOS 13、x86_64 + arm64 的 App 与 DMG；严格 ad-hoc 签名和 DMG 完整性校验通过；在仅保留 macOS 系统 PATH 的环境中，打包主程序 `--rdp-preflight` 返回 `available: true`，并解析到 App 内的 universal FreeRDP helper；DMG SHA-256：`1837594f3bacaae218dd3a99138cf510ebb734b6ddf3e8ba09fbae061af2f3d3`；应用已覆盖安装至 `/Applications/CNshell.app` 并成功启动。此前只读挂载辅助功能树及真实 PTY Canvas 证据继续有效 |

## 3. 必验场景与发布门槛

| 场景 | 状态 | 说明 |
| --- | --- | --- |
| 1 MB 连续终端输出、快速 resize、输入调度 < 50 ms | 通过 | 协议与 Playwright 性能用例通过 |
| 空目录、10 万文件、特殊字符、符号链接、无权限目录 | 通过 | 本机真实 OpenSSH/SFTP 测试通过 |
| 1 GB 单文件与 100 MB 中断、重试、最终校验一致 | 通过 | 1 GB 流式 SFTP 往返 SHA-256 一致；完成文件不会暴露为半成品 |
| 磁盘空间不足 | 通过 | Rust 使用真实 `ENOSPC` 故障注入验证明确“磁盘空间不足”错误、`.cnshell-part-*` 清理及最终目标不暴露；无需创建或填满受限卷 |
| 高延迟、5% 丢包、代理中断、睡眠唤醒、Wi-Fi 切换 | 部分 | 用户态 TCP 故障代理已验证固定双向延迟下 SSH 握手/命令往返、代理主动中断可观察，以及新代理上的重新认证恢复；不修改整机网络。精确 5% 丢包、睡眠唤醒和 Wi-Fi 切换仍需 Network Link Conditioner/真实系统切换窗口 |
| 服务端主动关闭与 keepalive 超时 | 部分 | 真实 OpenSSH 已验证服务端主动关闭后重新连接；生产终端每 30 秒发送要求响应的 SSH keepalive，TCP 同时配置 45 秒空闲、10 秒间隔、3 次探测，发送/Socket 错误进入既有退避重连，配置与调度边界单测通过。libssh2 不暴露 SSH keepalive 响应状态，静默黑洞的完整时序仍待 Network Link Conditioner 人工回归，不能虚报为已通过 |
| 日志/诊断/导出不泄露秘密、CSP、IPC 参数边界 | 通过 | 脱敏 schema、敏感历史、参数限制、CSP 配置和加密备份测试通过 |
| Ubuntu、Debian、Rocky、Alpine | 部分 | 腾讯云 Ubuntu 24.04 x86_64 真机已通过；Debian、Rocky、Alpine 仍需对应主机 |
| Linux 非 UTF-8 文件名真机操作 | 部分 | 原始字节路径编码、显示、解码和子路径拼接单测通过；macOS/APFS OpenSSH 夹具拒绝创建非法 UTF-8 文件名，仍需 Linux 文件系统真机注入验证 |
| Ventura、Sonoma、Sequoia 与 Intel 真机 | 外部阻塞 | 最低版本和 universal 构建可静态验证；仍需对应设备运行 |
| 连续 SSH + 监控、空闲内存 < 150 MB | 用户验收通过 | 用户于约 2 小时 50 分钟时主动结束长测并认可结果；期间 4 条 SSH TCP 连接持续建立，RSS 从约 36 MB 降至并稳定在约 21 MB。未宣称实际运行满 8 小时 |
| 无开发环境 Mac 安装、首次连接、升级、卸载 | 外部阻塞 | 本机已完成可回滚生命周期回归：覆盖安装、临时移除 App、恢复启动后，6 条连接记录稳定哈希与 6 个关联 Keychain 条目均保持不变；文档已提供。仍需另一台无开发环境的干净 Mac 验收首次安装与 Gatekeeper 流程 |
| Developer ID 签名、公证、正式 updater | 外部阻塞 | 应用内已实现手动检查、版本/发布说明展示、用户确认后下载并安装、进度和失败保留当前版本；权限仅开放 check 与 download-and-install。仍需要 Apple 证书、notary 凭据、正式 HTTPS endpoint 与 public key 才能完成真实更新验收；发布脚本会拒绝占位配置 |

本机候选包另已完成以下桌面证据：DMG 只读挂载后，包内应用通过 `codesign --verify --deep --strict` 并连续运行；辅助功能树可识别原生菜单、主工具栏、连接表单字段和安全密码输入框，模态打开后焦点进入关闭按钮，Escape 可关闭。该 ad-hoc 签名只用于本机结构验收，不等同 Developer ID 签名或 Apple 公证。

## 4. 腾讯云真实主机证据

2026-07-11 使用用户提供的腾讯云服务器完成以下非破坏性验收；密码未进入命令参数、日志或本文档，测试文件仅位于随机 `/tmp/cnshell-acceptance-*` 并由 trap 清理。

| 项目 | 结果 |
| --- | --- |
| 服务器环境 | Ubuntu 24.04 LTS、Linux 6.8、x86_64 |
| 主机身份 | CNshell 首次连接阻止并显示 ECDSA SHA-256 指纹；与独立 `ssh-keyscan` / `ssh-keygen` 结果一致后才信任 |
| 密码与 Keychain | 最终候选包创建 Keychain 条目，数据库仅保存 `credential_ref`；CNshell 密码认证成功 |
| SSH PTY | 会话进入“在线”，逐键执行命令后保持在线，监控与 SFTP 同时继续刷新；真实 socket nonblocking 路径另由 OpenSSH PTY 回归覆盖 |
| SFTP | 根目录实际列出 28 项；独立远端往返验证中文空格文件名与符号链接并完成清理 |
| 实时监控 | 实际显示主机、运行时长、负载、CPU、内存、Swap、进程、eth0/docker0、延迟与磁盘 |
| 系统信息 | 实际读取 Ubuntu 版本、内核、AMD EPYC CPU、内存、IPv4/IPv6 接口和文件系统表 |

真实桌面验收发现并修复五项发布级缺陷：缺少 `plugins.updater` 基础配置会导致打包应用启动 panic；常驻连接编辑器未在切换编辑对象时重置表单；SSH 会话只切换 libssh2 为非阻塞但底层 `TcpStream` 仍阻塞，导致 reader 持锁直到 socket 超时并把首次输入误判成断线；xterm 快速输入会并发发起 IPC，无法保证多段输入顺序；高延迟真机短暂出现 `transport read` 时 reader 会过早判定断线。现已采用每会话串行输入队列，并给瞬态读错误 2 秒恢复窗口，仅在 EOF 或错误持续时进入重连。最终 universal DMG 已在腾讯云真机复验 2 KB 快速输入后保持在线，SFTP 与监控继续刷新；复制连接通过复制后的 Keychain 凭据直接连接成功；单连接安全导出经字段扫描不含秘密，验收副本和临时文件已清理。

## 5. 结论

当前代码达到可本机试用的 **v0.1.0 候选版**：核心 SSH/SFTP/监控、高级代理和安全数据路径已通过自动化与真实协议测试，耐久测试已按用户认可的约 2 小时 50 分钟结果验收。正式对外发布仍必须完成第 3 节标为“外部阻塞”的真机矩阵、Developer ID 签名、公证和正式更新通道配置。

PLAN 要求的 universal DMG、版本更新清单、用户手册、快捷键表、架构说明、安全说明、故障排查和安装/升级/卸载说明均已存在，并由 `src/test/deliverables.test.ts` 检查文件、版本一致性和安装文档必要章节。文档存在不等同“已在干净 Mac 验证”，第 3 节对应门槛仍保持外部阻塞。

GitHub Actions 已提供提交/PR 的短时前端、Rust、WebKit E2E、本机 PTY 和 universal App 构建门禁，以及需要受保护 environment 和发行 secrets 的手动签名/公证候选流程。1 GB 协议与耐久测试不会在普通 CI 中重复消耗资源。

## 6. PLAN 架构偏差与后续范围

以下条目是对 `PLAN.md` 的逐项审计结果，不能用现有功能测试替代，也不影响当前本机候选版试用：

| PLAN 设计 | 当前状态 | 后续要求 |
| --- | --- | --- |
| SSH/SFTP 协议核心 | 已决策并同步 PLAN | 当前实现使用 `ssh2/libssh2` 并将同步协议工作隔离到 Tokio blocking task；真实 OpenSSH、代理、隧道、SFTP、Transport Pool、弱网和长连接证据均基于该实现。`PLAN.md` 已从未落地的 `russh` 假设更新为实际验证架构，避免把无用户收益的库替换误列为发布缺口 |
| 同一主机优先复用 SSH Transport | 通过 | `SessionManager` 内置按连接配置版本分组的 `TransportPool`；SFTP、监控、传输、归档和预览仅短时复用已认证 Transport，超过 20 秒闲置即淘汰，繁忙时自动附加连接，协议错误或 keepalive 失败时丢弃。终端和隧道始终建立不可复用的独占 Transport，真实 OpenSSH 回归覆盖“打开 PTY → 关闭 → 再打开 PTY”，避免服务器回收闲置连接后出现 `Session(-43)` |
| 所有长任务立即返回任务 ID | 通过 | 文件传输使用持久化传输队列；连接诊断、远端压缩/解压和默认应用预览使用统一 `TaskManager`，command 立即返回任务 ID，通过 `background-task` 事件报告结果，并支持快照查询和取消。短小 SFTP 元数据及 10 MB 内文本操作保留普通 command，不属于长任务 |
| 共享类型生成或 JSON Schema | 通过 | Rust `models.rs` 为 IPC 字段、可空性和嵌套结构的单一来源；`scripts/generate-ipc-types.mjs` 离线生成 `src/generated/ipc.ts`，`lint` 与 production build 均拒绝过期结果；前端仅用联合类型收窄业务枚举。本次生成迁移发现并修复了 `ProxyProfile.type` 曾被序列化为 `proxyType` 的真实漂移 |
| 私钥安全作用域 Bookmark | 通过 | macOS 使用 NSURL 创建只读 security-scoped Bookmark，Base64 存入连接专属 Keychain 条目；认证时解析真实路径并以 RAII 启停访问，复制/永久删除/保存失败均同步处理，旧记录无 Bookmark 时兼容路径回退。真实 OpenSSH 测试将 profile 路径故意设为不存在文件后仍通过 Bookmark 完成认证。当前候选包尚未启用 App Sandbox，这是独立发布权限决策，不再是 Bookmark 实现缺口 |
| RDP v1.5 深度联动 | 部分（代码完成/真机待验） | FreeRDP 3.28.0、OpenSSL、SDL、SDL_ttf 与 FreeType 已从固定哈希的官方源码静态构建为最低 macOS 13 的 arm64 + x86_64 sidecar，内置 NTLM 所需 MD4/MD5/RC4；参数与 Keychain 密码仅经 stdin，支持 connecting/online/reconnecting/closed/failed、窗口定位/聚焦/隐藏、全屏/显示器、动态分辨率/缩放、画质、文本剪贴板、音频、麦克风确认和单目录映射。仍需可正常响应 RDP 协商的 Windows 主机验证首帧、键鼠/中文 IME、剪贴板、声音、缩放、多分辨率和断网重连 |
