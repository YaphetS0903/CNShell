# CNshell v0.1.0 验收矩阵

> 最后核验：2026-07-11（macOS 本机）
> 状态定义：**通过**＝已有自动化或本机产物证据；**部分**＝实现完成但验收环境不完整；**外部阻塞**＝需要其他设备、发行凭据或长时窗口。

## 1. 核心功能

| PLAN 要求 | 状态 | 实现位置 | 验收证据 |
| --- | --- | --- | --- |
| 文件夹式连接管理、搜索、协议筛选、最近连接、排序、右键与软删除恢复 | 通过 | `src/features/connections/ConnectionSidebar.tsx`、`src-tauri/src/db.rs` | Playwright 覆盖创建连接、废纸篓恢复；连接支持拖到文件夹、拖拽排序和右键操作；Rust 覆盖恢复与永久删除 |
| SSH 密码、私钥、Agent 与严格主机指纹 | 通过 | `src/features/connections/ConnectionEditor.tsx`、`src-tauri/src/bookmark.rs`、`src-tauri/src/ssh.rs` | 真实 OpenSSH/Paramiko 覆盖正确与错误凭据、未知与变化指纹；私钥支持原生文件选择和绝对路径输入，并通过 Keychain 内只读 security-scoped Bookmark 持久访问 |
| 分阶段连接诊断 | 通过 | `src-tauri/src/ssh.rs`、`src/features/connections/ConnectionDiagnostics.tsx` | 协议测试验证 TCP、主机密钥、认证、Shell 阶段；认证与 Shell 阻塞均有 30 秒恢复超时，macOS Keychain 授权等待不会让界面永久停在连接中 |
| SOCKS5、HTTP CONNECT、SSH Jump | 通过 | `src-tauri/src/ssh.rs`、`src/features/settings/AdvancedSettings.tsx` | 真实代理链协议测试全部通过 |
| 本地、远程、动态端口转发 | 通过 | `src-tauri/src/tunnel.rs`、`src/features/connections/TunnelManager.tsx` | 三类真实转发协议测试通过 |
| xterm.js、多标签、拆分、搜索、剪贴板、IME、True Color、PTY resize | 通过 | `src/features/terminal/TerminalView.tsx`、`src/features/terminal/TerminalWorkspace.tsx`、`src-tauri/src/ssh.rs` | 单元/E2E、1 MB 输出及 PTY roundtrip 通过；浏览器端 IME 风格文本插入保留中文与 Emoji；本机密码 SSH 夹具验证交互 PTY、中文/Emoji 双向字节和 ANSI/True Color 全屏序列；最终 universal DMG 的只读挂载应用经真实 Canvas 截图确认中文宽字符、Emoji、线框、光标定位和 RGB 颜色无明显错位，会话/工具标签方向键切换通过；腾讯云 PTY 的 `vim`、`top`、`tmux` 验收亦通过。临时连接与 Keychain 条目已清理 |
| 自动重连与安全错误停止重试 | 通过 | `src-tauri/src/ssh.rs` | Rust 测试验证 1/2/5/10/30 秒及认证/指纹错误停止策略 |
| SFTP 导航、排序、隐藏文件、虚拟滚动与文件操作 | 通过 | `src/features/files/FileManager.tsx`、`src-tauri/src/sftp.rs` | 真实 SFTP 覆盖空目录、10 万文件、UTF-8 特殊文件名、符号链接、无权限目录；非法 UTF-8 原始字节使用无损 Base64 路径令牌和 `\\xNN` 显示，单测覆盖不碰撞、目录拼接与解码 |
| 上传下载队列、速度/ETA、暂停/取消/重试、冲突策略 | 通过 | `src/features/files/TransferQueue.tsx`、`src-tauri/src/sftp.rs` | 1 GB 真实 SFTP 流式上传/下载及 SHA-256 一致性通过；中断临时文件、显式重试已覆盖 |
| 下载临时文件与上传原子替换 | 通过 | `src-tauri/src/sftp.rs` | `.cnshell-part`、远端临时文件及原子 rename 已由协议/Rust 测试覆盖 |
| 10 MB 文本编辑、修改冲突与原子保存 | 通过 | `src/features/files/TextEditor.tsx`、`src-tauri/src/sftp.rs` | UTF-8 字节边界测试；冲突比较、临时文件、fsync 与原子 rename 路径由代码审计及真实 SFTP 覆盖 |
| CPU、内存、Swap、网络、进程、磁盘与 5 分钟历史 | 通过 | `src-tauri/src/monitor.rs`、`src/features/monitor/MonitorSidebar.tsx` | 采集解析、单项降级与 5 分钟窗口测试通过；腾讯云 30 次/60.77 秒同等采集命令累计远端 CPU 0.35 秒，折算单核 0.576%（低于 2%） |
| 系统信息复制与导出 | 通过 | `src/features/monitor/SystemInfoPanel.tsx`、`src-tauri/src/monitor.rs` | 腾讯云真机验证完整内容；Rust 验证 JSON 临时文件原子导出与清理 |
| 快捷命令、历史策略、帮助与首次引导 | 通过 | `src/features/terminal/TerminalWorkspace.tsx`、`src/features/help/HelpModal.tsx`、`src-tauri/src/db.rs` | Playwright 验证内置命令只读、用户命令删除和帮助弹窗可访问性；敏感历史检测与全量清空测试通过 |
| 连接导入导出与加密凭据备份 | 通过 | `src/features/connections/ConnectionSidebar.tsx`、`src/features/settings/AdvancedSettings.tsx`、`src-tauri/src/backup.rs` | 连接库工具栏与设置均有导入入口；Argon2id + AES-256-GCM 往返及错误口令拒绝测试通过 |
| 可折叠、可调尺寸的三栏与底部工具区 | 通过 | `src/App.tsx`、`src/features/terminal/TerminalWorkspace.tsx`、`src/lib/layout.ts` | 鼠标拖动与键盘方向键均可调整；侧栏尺寸进入工作区恢复，底部高度本机持久化；紧凑窗口 E2E 通过 |
| RDP 独立 FreeRDP adapter | 部分 | `src-tauri/src/rdp.rs`、`src/features/rdp/RdpWorkspace.tsx` | 本机实现已通过：RDP 密码保存 Keychain，仅经 Helper stdin 传递；动态分辨率、剪贴板、自动重连参数；受管 PID、标签状态、关闭、正常/异常退出事件及应用退出清理。真实 Windows 画面与输入互操作仍需外部主机 |
| 浅色、深色、高对比、键盘和 VoiceOver 语义 | 部分 | `src/styles.css`、`src/components/Modal.tsx`、`src/features/terminal/TerminalWorkspace.tsx`、`src/features/files/FileManager.tsx` | Playwright 与最终 DMG 辅助功能树验证跟随 macOS 浅色、手动主题优先级、高对比、模态焦点陷阱，会话/工具标准 tab/tabpanel、真实方向键切换与独立菜单入口，SFTP 虚拟表格行列/排序/总数，以及监控数值替代；完整 VoiceOver 朗读顺序仍需用户主动开启系统 VoiceOver 后人工巡检 |
| SQLite 历史迁移与失败前备份 | 通过 | `src-tauri/src/db.rs`、`src-tauri/migrations/` | v1–v4 无损升级、任务恢复、数据库 `.backup` 测试通过 |

## 2. 自动化证据

| 命令 | 结果（2026-07-11） |
| --- | --- |
| `npm run lint` | 通过，0 warning |
| `npm run test` | 通过，16 个文件、35 个测试；含 RDP 受管工作区和关闭入口、后台任务事件/快照竞态、私钥原生选择、可调布局、系统主题、连接编辑回填、SSH Jump 表单契约、终端 IPC 输入顺序、更新入口权限/候选通道，以及 PLAN/CI 交付文档和发布门禁契约测试 |
| `npm run build` | 通过，TypeScript 与 Vite production build |
| `cargo test --manifest-path src-tauri/Cargo.toml` | 通过，73 个测试；含 RDP stdin 凭据隔离、Helper 参数/终止和 RDP 凭据保留，macOS Bookmark、真实私钥认证、Transport Pool、用户态弱网中断恢复、认证阻塞恢复超时、SSH/TCP keepalive 配置与调度、后台任务、IPC 契约、磁盘不足、原始字节路径、跨 Mac 导入、跳板环和监控缓存生命周期 |
| `npm run test:e2e` | 通过，10 个 Playwright 场景；含可调布局与导入入口、系统浅色跟随、快捷命令只读/删除契约、IME 风格中文/Emoji 输入，以及会话/工具标签键盘操作与 SFTP 表格可访问语义 |
| `npm run test:pty-fixture` | 通过；本机 Paramiko 密码认证夹具提供真实 PTY Shell，验证中文/Emoji 双向 UTF-8、ANSI 清屏/光标控制和 True Color 输出 |
| `npm run test:protocol` | 通过，6 个真实协议场景；含安全 Bookmark 解析私钥后认证、Transport Pool 顺序复用/并发扩容/配置失效，以及 10 万文件、1 GB 文件往返校验、代理、隧道、acceptNew 与服务端主动关闭后恢复 |
| `npm audit --audit-level=moderate` | 通过，0 vulnerabilities |
| `zsh -n scripts/release.sh` 与发布门禁单测 | 通过；发布脚本从 Info.plist 读取实际小写可执行文件名，并检查最低 macOS 13、Developer ID、Gatekeeper、双架构、DMG、公证票据及 updater 签名产物 |
| `CNSHELL_SOAK_SECONDS=6 npm run test:soak` | 通过，脚本、独立 monitor Exec、PTY 与 RSS 指标可运行 |
| `APPLE_SIGNING_IDENTITY=- npm run tauri build -- --target universal-apple-darwin --bundles app,dmg` | 通过，包含应用内更新入口的当前源码生成最低 macOS 13、x86_64 + arm64 的 App 与 DMG；Info.plist 实际可执行文件 `cnshell` 存在，严格 ad-hoc 签名和 DMG 校验通过；DMG SHA-256：`a68319d9d8a16c9b3a333d3980b6bbc24e1c225512f01c9809b839ae24a72e1b`；只读挂载后应用启动、设置/更新入口辅助功能树及真实 PTY Canvas 均已复验 |

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
| 无开发环境 Mac 安装、首次连接、升级、卸载 | 外部阻塞 | 文档已提供，需干净 Mac 验收 |
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
| `russh` 与兼容 SFTP 层 | 计划偏差 | 当前实现使用 `ssh2/libssh2`；已通过协议与真机测试，但若必须严格遵循技术选型需单独迁移并重新完成全套协议回归 |
| 同一主机优先复用 SSH Transport | 通过 | `SessionManager` 内置按连接配置版本分组的 `TransportPool`；SFTP、监控、传输、归档和预览优先复用已认证闲置 Transport，繁忙时自动附加连接，协议错误或 keepalive 失败时丢弃，连接/代理/指纹变化时失效。终端和隧道使用不可复用独占租约，避免 libssh2 的 Session 互斥锁阻塞其他 Channel |
| 所有长任务立即返回任务 ID | 通过 | 文件传输使用持久化传输队列；连接诊断、远端压缩/解压和默认应用预览使用统一 `TaskManager`，command 立即返回任务 ID，通过 `background-task` 事件报告结果，并支持快照查询和取消。短小 SFTP 元数据及 10 MB 内文本操作保留普通 command，不属于长任务 |
| 共享类型生成或 JSON Schema | 通过 | Rust `models.rs` 为 IPC 字段、可空性和嵌套结构的单一来源；`scripts/generate-ipc-types.mjs` 离线生成 `src/generated/ipc.ts`，`lint` 与 production build 均拒绝过期结果；前端仅用联合类型收窄业务枚举。本次生成迁移发现并修复了 `ProxyProfile.type` 曾被序列化为 `proxyType` 的真实漂移 |
| 私钥安全作用域 Bookmark | 通过 | macOS 使用 NSURL 创建只读 security-scoped Bookmark，Base64 存入连接专属 Keychain 条目；认证时解析真实路径并以 RAII 启停访问，复制/永久删除/保存失败均同步处理，旧记录无 Bookmark 时兼容路径回退。真实 OpenSSH 测试将 profile 路径故意设为不存在文件后仍通过 Bookmark 完成认证。当前候选包尚未启用 App Sandbox，这是独立发布权限决策，不再是 Bookmark 实现缺口 |
| RDP v1.5 完整 Helper | 部分 | 已完成外部 Helper IPC 生命周期：自动检测/测试路径覆盖、Keychain 密码 stdin、动态分辨率/剪贴板/自动重连参数、受管会话标签、关闭与退出/崩溃隔离；CNshell 不捆绑 GPL FreeRDP。仍未完成画面内嵌及真实 Windows 键鼠、剪贴板、缩放、多分辨率与 Helper 签名验收 |
