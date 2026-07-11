# CNshell 产品与开发计划

> 文档状态：v1.0（实现基线；架构选型已按验证结果更新）
> 目标平台：macOS 13 Ventura 及以上，Apple Silicon 优先，兼容 Intel Mac
> 产品定位：面向开发者、运维人员和中小团队的原生 macOS SSH / SFTP / 服务器监控工具
> 参考范围：复刻所提供 7 张 FinalShell 截图中可识别的功能与工作流，不照搬其品牌、素材和 Windows 外观

## 1. 产品目标

CNshell 要把远程连接、命令行操作、文件传输和服务器状态监控集中在一个窗口中，让用户无需在 Terminal、Finder、`scp` 和监控命令之间切换。

首个正式版本应完成以下闭环：

1. 用户创建并安全保存 SSH 连接。
2. 用户从连接管理器一键打开多个远程终端。
3. 用户在同一会话内浏览、上传、下载和编辑远程文件。
4. 用户实时查看远程 Linux 主机的 CPU、内存、交换区、网络、进程和磁盘状态。
5. 用户可查看完整系统信息、搜索终端输出、调用命令历史，并在断线后恢复连接。
6. 用户可以创建 Windows RDP 连接，并在后续完整版本中从 CNshell 内打开远程桌面。

### 1.1 成功标准

- 在一台全新 Mac 上，首次启动后 3 分钟内可以创建连接并进入 SSH 终端。
- 终端可稳定运行 `vim`、`top`、`tmux`、彩色输出和中文输入，窗口缩放后 PTY 尺寸同步正确。
- 1 GB 单文件上传/下载可取消、可重试，失败不会产生被误认为完整文件的结果。
- 监控刷新不阻塞终端输入，默认额外远端负载低于单核 CPU 的 2%。
- 密码、私钥口令和代理凭据不以明文写入配置或日志。
- 核心流程支持纯键盘操作、VoiceOver、浅色/深色模式和 macOS 标准菜单快捷键。

## 2. 截图功能清单与 CNshell 对应设计

| 截图能力 | CNshell 设计 | 版本 |
| --- | --- | --- |
| 文件夹式连接管理、搜索、筛选 | 侧边栏连接库；文件夹分组、拖拽排序、关键字搜索、协议筛选、最近连接 | MVP |
| 新建 SSH（Linux）连接 | 名称、主机、端口、备注、用户名、密码/私钥/SSH Agent、多种主机密钥策略 | MVP |
| 代理服务器、隧道、终端高级设置 | SOCKS5/HTTP/SSH 跳板机；本地/远程/动态端口转发；终端编码、环境变量、启动命令 | v1.0 |
| 多标签终端 | 会话标签、状态点、关闭确认、复制标签、左右拆分、标签恢复 | MVP；拆分在 v1.0 |
| 命令输入框、历史、选项、搜索 | 终端内直接输入；可选独立命令栏；本地历史、会话内搜索、快捷命令面板 | MVP |
| 远程文件目录树与列表 | SFTP 双栏文件管理；排序、刷新、返回上级、路径输入、显示隐藏文件 | MVP |
| 文件右键菜单 | 打开、打开方式、内置文本编辑、复制路径、重命名、权限、删除、下载、上传、新建、压缩/解压 | v1.0 |
| 上传、下载、打包传输 | 后台传输队列、进度/速度/剩余时间、取消、重试、同名冲突处理；文件夹按需打包 | MVP / v1.0 |
| 左侧服务器状态 | IP、运行时长、负载、CPU/内存/Swap、进程、网速/流量、延迟、挂载磁盘 | MVP |
| 系统信息详情页 | 内核、架构、主机名、CPU、内存、网卡、文件系统完整表格，可复制/导出 | v1.0 |
| 终端帮助和“不再显示” | 首次使用引导、快捷键速查、帮助菜单，可在设置中重新打开 | MVP |
| Windows 远程桌面连接 | RDP 连接模型和独立适配层；完整阶段接入 FreeRDP Helper | v1.5 |

未在截图中出现的团队同步、云端账号、付费授权等功能不纳入首版；连接库先保持本地优先。

## 3. 信息架构与界面规范

### 3.1 主窗口

采用适合 macOS 的三段式工作区，不复制 Windows 窗口装饰：

```text
┌──────────────────────────────────────────────────────────────────┐
│ macOS Toolbar：连接库 / 新建 / 搜索 / 布局 / 传输 / 设置        │
├───────────────┬──────────────────────────────────────────────────┤
│ 服务器监控栏  │ 会话标签：Terminal / 系统信息 / RDP             │
│               ├──────────────────────────────────────────────────┤
│ IP、负载      │                                                  │
│ CPU/内存      │              SSH 终端区域                        │
│ 进程/网络     │                                                  │
│ 延迟/磁盘     ├──────────────────────────────────────────────────┤
│               │ SFTP 文件 | 快捷命令 | 传输队列                 │
└───────────────┴──────────────────────────────────────────────────┘
```

- 左侧监控栏和底部工具区均可折叠、拖动调整尺寸；窗口小于 900 px 时默认折叠监控栏。
- 会话标签显示连接状态：连接中、在线、重连中、失败、已关闭；状态不能只靠颜色表达。
- 终端获得焦点时，应用快捷键不得截获常用 shell / `tmux` / `vim` 组合键。
- 使用 macOS 系统字体，终端默认使用 SF Mono；图标统一采用 Lucide 线性图标，不使用 Emoji 充当功能图标。
- 默认跟随系统主题，提供浅色、深色和高对比选项；颜色全部使用语义令牌。
- 数据密集但保持 4/8 pt 间距体系；正文不小于 12 pt，交互热区不小于 28×28 pt，关键操作为 44×44 pt。

### 3.2 连接管理器

- 以 Sheet 或独立窗口展示连接库，左侧为文件夹树，右侧为可排序连接列表。
- 工具栏包含新建连接、新建文件夹、导入、搜索和协议筛选。
- 双击连接或按 Return 打开；支持右键连接、编辑、复制、移动、导出和删除。
- 删除进入废纸篓式软删除区，可撤销；涉及已打开会话时只删除配置，不强制断开会话。
- SSH 编辑页采用分组表单：常规、认证、终端、代理/跳板机、隧道、高级。
- 连接测试必须区分 DNS、TCP、代理、主机密钥、认证和远端 Shell 错误，并给出可恢复操作。

### 3.3 终端与文件区

- xterm.js 渲染 VT100/xterm-256color/True Color；支持 ANSI、鼠标报告、Bracketed Paste、OSC 8 链接和 IME。
- 快捷键：`⌘T` 新标签、`⌘W` 关闭、`⌘F` 搜索、`⌘K` 清屏、`⌘⇧C/V` 终端复制/粘贴、`⌘1…9` 切换标签。
- 关闭在线会话时可配置统一确认；由于通用 SSH PTY 无法可靠、可移植地识别任意远端 shell 的前台进程，当前采用保守确认而不虚报空闲判断。关闭应用且存在传输任务时始终单独确认。
- SFTP 列表使用虚拟滚动；列为名称、大小、类型、修改时间、权限、用户/用户组。
- 内置编辑器仅默认打开可识别文本且小于 10 MB 的文件；保存时先写临时文件再原子替换，冲突时比较远端修改时间。

### 3.4 监控与系统信息

- 默认 2 秒采样一次，仅保留最近 5 分钟内存数据；支持暂停并显示最后更新时间。
- CPU、内存和 Swap 用数值加进度条；网络和延迟用可暂停的流式折线图；图表始终有数值/表格替代。
- 进程榜支持按 CPU、内存排序，默认显示前 5 项；点击后打开完整进程列表。
- 首选读取 Linux `/proc`、`/sys`、`df`、`ip` 数据；命令不存在或权限不足时局部降级，不影响终端与 SFTP。
- 监控命令使用独立低频 Exec Channel，避免向用户交互 Shell 注入命令和输出。

## 4. 技术方案

### 4.1 技术栈

| 层 | 选型 | 用途 |
| --- | --- | --- |
| 桌面壳 | Tauri 2 | macOS 窗口、菜单、通知、自动更新、签名与轻量分发 |
| 前端 | React + TypeScript + Vite | 主界面、状态管理和模块化工作区 |
| UI | 语义化 React 组件 + CSS 语义令牌 + Lucide | 可访问组件、主题令牌和一致图标体系；避免为紧凑桌面界面引入未使用的 UI 运行时 |
| 终端 | xterm.js + fit/search/web-links addons | 终端渲染、尺寸同步、搜索和链接 |
| Rust 异步运行时 | Tokio | SSH、SFTP、传输和监控任务调度 |
| SSH | `ssh2` / vendored `libssh2` + OpenSSL | 会话、PTY、Exec Channel、端口转发、代理和跳板机；同步协议工作隔离在 Tokio blocking task 中 |
| SFTP | `libssh2` SFTP 客户端层 | 目录、文件、权限及流式传输；通过 Session/Transport Pool 封装并支持后续替换 |
| 本地数据 | SQLite（WAL）+ SQLx migrations | 连接、文件夹、偏好、历史、任务元数据 |
| 凭据 | macOS Keychain | 密码、私钥口令、代理凭据；数据库只保存 Keychain 引用 |
| 图表 | uPlot | 高频、低开销的实时监控折线图 |
| RDP | 独立 FreeRDP Helper 适配层 | v1.5 Windows 远程桌面，主应用与 GPL 组件进程隔离并单独完成许可审查 |
| 测试 | Vitest、React Testing Library、Playwright、Rust tests | 单元、组件、端到端和协议测试 |

不选 Electron：CNshell 的核心是长时间驻留的终端与监控工具，Tauri 可显著降低空闲内存和安装包体积。UI 仍使用 Web 技术，以获得成熟的 xterm.js 生态。

### 4.2 进程与模块边界

```text
React UI
  ├─ connection-store / workspace-store / transfer-store
  ├─ TerminalView / FileManager / MonitorPanel / SystemInfo
  └─ typed invoke + event bridge
                    │
Tauri Rust Core
  ├─ Connection Service ── SSH Session Pool ── PTY / Exec / Tunnel
  ├─ File Service ──────── SFTP Channel ───── Transfer Queue
  ├─ Monitor Service ───── Linux Collector ── Sample Stream
  ├─ Persistence ───────── SQLite + Keychain
  └─ RDP Adapter ───────── FreeRDP Helper（v1.5）
```

- 一个连接配置可对应多个会话；每个终端标签拥有独立 PTY Channel。
- 同一主机会话优先复用 SSH Transport，终端、SFTP、监控使用独立 Channel；服务端不允许复用时自动建立附加连接。
- Rust Core 是所有网络、文件和密钥操作的唯一入口；WebView 仅通过用户触发的原生文件选择器取得路径，再交给 Rust Core 处理，不直接访问任意网络。
- 前后端协议使用共享 TypeScript/Rust 类型生成或 JSON Schema 校验；终端、状态与传输事件包含 `sessionId`，通用后台任务的 `BackgroundTask.id` 作为 requestId 用于事件关联。

### 4.3 核心数据模型

```text
ConnectionProfile
  id, folderId, protocol(ssh|rdp), name, host, port, username,
  authType(password|privateKey|sshAgent), credentialRef, privateKeyBookmark,
  hostKeyPolicy, proxyId, terminalProfileId, note, tags, createdAt, updatedAt

ProxyProfile
  id, type(socks5|http|sshJump), host, port, username, credentialRef

PortForward
  id, connectionId, type(local|remote|dynamic), bindHost, bindPort,
  destinationHost, destinationPort, autoStart

SessionState
  id, connectionId, type(terminal|systemInfo|rdp), status,
  title, cwd, startedAt, lastError

TransferTask
  id, sessionId, direction, source, destination, totalBytes,
  transferredBytes, status, conflictPolicy, error, createdAt

CommandSnippet
  id, name, command, description, tags, sortOrder
```

- `hostKeyPolicy` 默认 `strict`：首次连接要求用户确认 SHA-256 指纹，后续变化必须阻止连接并明确告警。
- 文件路径存储为远端原始字节的安全表示；UI 无法解码时显示转义形式，避免损坏文件名。
- 命令历史默认仅保存在本机，可在设置中关闭或一键清空；可能含密码的命令支持不记录。

### 4.4 前后端接口

首版稳定接口按领域划分：

- 连接：`connection.list/save/delete/test/connect/disconnect/reconnect`
- 终端：`terminal.open/input/resize/close/searchMetadata`；输出通过有界事件流发送
- 文件：`sftp.list/stat/mkdir/rename/chmod/delete/openText/saveText`
- 传输：`transfer.enqueue/pause/resume/cancel/retry/list`
- 监控：`monitor.start/pause/resume/stop/snapshot/systemInfo`
- 隧道：`tunnel.start/stop/list`（v1.0）
- RDP：`rdp.preflight/open/close`（v1.5）

所有长任务立即返回任务 ID，再通过事件报告进度；UI 取消操作传入取消令牌，禁止长时间阻塞 Tauri command。

## 5. 安全、隐私与可靠性

- 严格校验 SSH 主机密钥；`known_hosts` 记录主机、端口、算法和指纹，提供显式的更新流程。
- 私钥优先通过 macOS 安全作用域 Bookmark 引用原文件；日志永不输出密码、口令、私钥内容或完整命令输入。
- WebView 启用严格 CSP，只允许打包资源；关闭远程页面导航、开发者工具（正式版）和未使用的 Tauri 权限。
- 剪贴板、打开本地文件、通知等能力按明确操作触发，不提供网页式任意访问。
- 下载先写入 `.cnshell-part` 临时文件，校验大小后原子改名；上传可选择覆盖、跳过、重命名或全部应用同一策略。
- 应用崩溃后只恢复窗口、连接标签和工作目录，不自动提交未完成命令；传输任务标记为“待恢复”并由用户确认。
- 断线采用 1、2、5、10、30 秒退避重连，最多自动尝试 5 次；认证失败和主机密钥变化禁止自动重试。
- 发布包执行 Apple Developer ID 签名、公证和 Tauri 签名更新；更新失败保留当前可运行版本。
- RDP Helper 在引入前完成 GPL 许可、打包与签名审查；若不能合规随包分发，则改为检测用户安装的 Helper，不降低 SSH 主线交付质量。

## 6. 分阶段实施计划

### 阶段 0：工程骨架与体验基线（1 周）

- 初始化 Tauri 2、React、TypeScript、Rust workspace、代码检查和 CI。
- 建立主题、窗口布局、错误边界、日志脱敏、类型化 IPC 和 SQLite migration。
- 建立 macOS 菜单、快捷键、图标、应用标识和开发/发布配置。
- 交付：可启动的空壳应用，可切换主题、调整三栏布局并通过基础测试。

### 阶段 1：SSH 终端 MVP（2～3 周）

- 实现连接文件夹、搜索、SSH 表单、Keychain、私钥和主机指纹确认。
- 实现 SSH Transport/PTY 生命周期、xterm.js、多标签、尺寸同步、复制粘贴、搜索和重连。
- 实现首次使用帮助、连接诊断和错误恢复。
- 交付：可日常使用的多标签 SSH 客户端。

### 阶段 2：SFTP 与传输 MVP（2 周）

- 实现远端目录树、文件表、路径导航、排序、隐藏文件和常用文件操作。
- 实现上传/下载队列、进度、速度、取消、重试、冲突策略和拖拽。
- 实现小型文本文件的安全打开、修改检测和原子保存。
- 交付：终端和远程文件管理完整闭环。

### 阶段 3：监控与系统信息（2 周）

- 实现 Linux 能力探测和 `/proc` 等无侵入采集器。
- 实现状态栏、进程榜、网络/延迟图、磁盘列表、暂停与降级状态。
- 实现完整系统信息页、复制和导出。
- 交付：覆盖截图中的实时服务器状态和系统信息功能。

### 阶段 4：高级连接与效率功能（2～3 周）

- 实现 SOCKS5/HTTP 代理、SSH 跳板机、本地/远程/动态端口转发。
- 实现终端左右拆分、会话布局恢复、快捷命令库、命令历史策略和文件夹打包传输。
- 实现连接导入/导出；导出默认不含密码，含密钥材料时必须加密并再次确认。
- 交付：CNshell v1.0 候选版。

### 阶段 5：质量、签名与 v1.0 发布（1～2 周）

- 完成性能、弱网、长连接、无障碍、Intel/Apple Silicon 和多版本 macOS 测试。
- 完成应用签名、公证、更新通道、隐私说明、诊断包和用户文档。
- 发布 universal DMG；崩溃报告默认关闭，用户主动开启后才上传脱敏数据。

### 阶段 6：Windows RDP（3～4 周，v1.5）

- 固化 RDP 配置模型，完成 FreeRDP Helper 进程通信、画面缩放、键鼠、剪贴板和分辨率变化。
- 处理 Helper 安装检测、签名、公证、沙盒权限、崩溃隔离和许可合规。
- RDP 与 SSH 共用连接管理、标签和凭据系统，但不与 SSH Session Pool 耦合。

按单人全职估算，v1.0 为 10～13 周，含 RDP 的 v1.5 为 13～17 周；若先只做可用 MVP，阶段 0～2 约 5～6 周。

## 7. 测试与验收计划

### 7.1 自动化测试

- Rust 单元测试：配置校验、主机指纹、路径编码、重连策略、传输状态机、监控解析器和日志脱敏。
- 前端组件测试：连接表单、标签状态、文件冲突对话框、传输进度、监控空/错/暂停状态。
- 协议集成测试：使用容器启动 OpenSSH，覆盖密码、密钥、错误凭据、SFTP、跳板机、端口转发和连接中断。
- 端到端测试：创建连接 → 接受指纹 → 运行命令 → 上传 → 编辑 → 下载 → 查看监控 → 断线重连。
- 数据迁移测试：每个历史 schema 都能无损升级，升级失败回滚并保留原数据库备份。

### 7.2 必验场景

- 终端：`vim`、`top`、`tmux`、中文输入、Emoji/宽字符、True Color、1 MB 连续输出、窗口快速缩放。
- 文件：空目录、10 万文件目录、中文/特殊字符名、符号链接、无权限目录、传输中断、磁盘空间不足。
- 网络：高延迟、5% 丢包、代理中断、睡眠唤醒、Wi-Fi 切换、服务端主动关闭和 SSH keepalive 超时。
- 安全：未知/变化的主机指纹、错误私钥口令、日志扫描、导出文件扫描、CSP 和 IPC 参数越权。
- 监控：Ubuntu、Debian、CentOS/Rocky、Alpine；缺少 `ip`/`sudo`/`procps` 时正确降级。
- macOS：Ventura、Sonoma 和 Sequoia，Apple Silicon 与 Intel；浅色/深色、高对比、VoiceOver、仅键盘操作。

### 7.3 v1.0 发布门槛

- P0/P1 缺陷为 0，核心流程自动化测试全部通过。
- 连续 8 小时 SSH + 监控运行无会话泄漏，内存增长稳定；空闲应用目标低于 150 MB。
- 终端输入到本地回显调度的 UI 延迟目标低于 50 ms，监控更新不得造成可感知卡顿。
- 100 MB 文件经限速/断网测试后可正确失败并重试，最终内容校验一致。
- 安装、首次连接、升级和卸载文档均在未配置开发环境的 Mac 上验证。

## 8. 交付物与暂定目录

```text
CNshell/
├── src/                       # React UI
│   ├── features/connections
│   ├── features/terminal
│   ├── features/files
│   ├── features/monitor
│   └── shared
├── src-tauri/                 # Rust Core 与 macOS 配置
│   ├── src/services
│   ├── src/domain
│   └── migrations
├── tests/                     # 集成与 E2E
├── docs/                      # 用户、架构、安全和发布文档
└── PLAN.md
```

v1.0 交付包括 universal DMG、版本更新清单、用户手册、快捷键表、架构说明、安全说明和故障诊断导出功能。

## 9. 已确定的默认决策

- 产品名使用 **CNshell**，首发仅支持 macOS。
- 以截图可见功能为目标范围，保留 FinalShell 的高效工作流，但采用 macOS 原生交互和原创视觉。
- 本地优先、无账号也能完整使用；首版不做云同步和多人协作。
- 首发完整支持 Linux SSH/SFTP/监控；其他 Unix 主机尽力兼容终端和 SFTP，监控允许降级。
- 默认严格校验主机密钥，默认不保存敏感命令历史，默认不上传遥测。
- 先发布 SSH/SFTP/监控 v1.0，再以独立适配层交付 RDP v1.5。
- 技术基线为 Tauri 2 + React/TypeScript + Rust/Tokio + xterm.js + SQLite + macOS Keychain。
