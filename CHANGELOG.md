# CNshell 版本更新清单

本项目采用语义化版本号。Beta 使用独立 updater 签名与更新通道；正式签名、公证与稳定更新通道完成前不标记为正式稳定版。

## 0.2.0-beta.3（Windows 桌面体验修复版）

### 新增

- 新增 Windows Beta.3 桌面启动与主题回归验证项，覆盖 GUI 子系统、原生标题栏、菜单行为和终端配色。

### 修复

- Windows release 程序使用 GUI 子系统启动，不再额外弹出黑色控制台窗口。
- Windows 原生窗口标题统一为 `CNshell`，移除与浅色界面不协调的 macOS 风格菜单栏；原菜单快捷键改由 Windows 前端快捷键保留。
- Windows 浅色主题下默认终端跟随浅色配色，不再残留黑色背景。

### 已知限制

- macOS 仍未配置 Developer ID 与公证；Windows 仍未配置 Authenticode。x64 保持 Beta，ARM64 保持 Preview。
- Windows 真机体验仍需用户安装本版本后验证，当前开发环境无法直接运行 Windows。

## 0.2.0-beta.2（跨平台更新链路候选版）

### 新增

- 本版本不新增产品功能，范围严格限定为更新链路验证、Windows 启动可靠性修复与构建门禁增强。

### 修复

- Windows 在数据库 migration 前创建备份时，若随机临时文件名发生冲突会自动重试，避免可恢复冲突阻断应用启动。

### 调整

- Core CI 在 macOS 与 Windows x64 桌面 Rust 构建中启用 `clippy -D warnings`，防止新增警告进入发布分支。
- 清理 macOS 与 Windows 专属 Rust 代码的 Clippy 警告，保持三平台构建门禁一致。

### 已知限制

- 本版本用于验证从 `v0.2.0-beta.1` 到 `v0.2.0-beta.2` 的真实自动更新链路，没有新增真机能力声明。
- macOS 仍未配置 Developer ID 与公证；Windows 仍未配置 Authenticode。x64 保持 Beta，ARM64 保持 Preview。
- Windows、Intel Mac、不同 macOS/Linux、辅助功能、睡眠唤醒及真实弱网矩阵仍以验收文档中的外部边界为准。

## 0.2.0-beta.1（跨平台测试候选版）

### 新增

- 新增 Windows 10 22H2/Windows 11 客户端，使用原生标题栏、WebView2 和当前用户 NSIS 安装；同时生成 x64 Beta 与 ARM64 Preview。
- Windows 密码、私钥口令、代理密码、AI Key、WebDAV 密码和团队令牌接入 Windows Credential Manager；连接秘密与路径记录已在 Windows x64 CI 完成真实往返。
- Windows 本地 Shell 使用 ConPTY，依次选择 PowerShell 7、Windows PowerShell 和 cmd；自动化覆盖输入、resize、关闭与重开。
- Windows 文件路径支持盘符、UNC、Unicode、目录拖放/传输和外部编辑器；超出 Credential Manager blob 上限的合法长路径安全退回 profile 绝对路径。
- Windows 原生构建并随包分发 FreeRDP 3.28.0、Mosh 1.4.0 和 G-Kermit 2.01 x64/ARM64 sidecar，不依赖 Homebrew、WSL、MSYS2 或用户手动安装运行库。
- Windows RDP 新增 Win32 聚焦、隐藏、恢复、窗口位置联动和 Job Object 生命周期；连接参数与密码继续只通过 helper stdin 传递。
- Windows Mosh 使用 WinSock/ConPTY 适配官方客户端核心，并以真实加密 UDP 双向回环验证 x64 运行；对应源码、构建脚本、自测脚本和许可证随包分发。
- Windows G-Kermit 使用受限外部管道模式，x64 CI 完成两个 helper 的二进制互传；ARM64 通过 PE 架构门禁。
- Windows Hello 使用 Microsoft Platform Crypto Provider 的高保护 CNG 密钥封装加密同步口令；Windows X11 支持 VcXsrv/Xming TCP `DISPLAY`；OpenSSH Agent/Pageant 与 FIDO2 身份检测完成平台适配。
- GitHub Release 工作流可在同一 Draft Release 生成 macOS universal、Windows x64 和 Windows ARM64 安装包、四平台 updater 清单、SHA-256、许可证及对应源码附件。

### 调整

- Windows 安装器拒绝低于 build 19045 的系统，创建开始菜单入口且不在干净账户默认创建桌面快捷方式。
- 跨平台导入保留路径失效的连接，并在使用私钥、SSH Certificate 或 RDP 映射目录时明确要求编辑连接后重新选择。
- 预发布版本由发布 workflow 自动标记为 GitHub prerelease；当前公开的 `v0.1.1` macOS 候选版不会被覆盖。

### 修复

- 修复 Windows Mosh 构建缓存重复应用 `read`/`write` 源码补丁，导致 `cnshell_cnshell_read`/`write` 编译失败。
- 修复 PowerShell 把 Mosh 加密 UDP 自测写入 stderr 的正常状态行误判为构建异常；自测现在分别校验 stdout、stderr 与真实退出码。
- 修复 Windows 上传、拖放、目录下载和传输列表错误使用完整 `C:\\...` 本地路径作为远端文件名。

### 已知限制

- Windows 10/11 x64、Windows 11 ARM64、真实 Windows RDP、Windows Hello、实体 FIDO2、VcXsrv/Xming、实体串口、中文 IME、DPI、高对比、Narrator、睡眠唤醒和真实网络切换仍需对应真机验收；ARM64 保持 Preview。
- Windows Beta 尚未配置 Authenticode；当前预发布已配置独立 Tauri updater 签名、HTTPS Beta endpoint、SHA-256 和 SmartScreen 说明，但 updater minisign 不能替代 Authenticode。
- macOS Developer ID、公证与正式 updater 仍需要 Apple Developer Program 会员和发行凭据。

## 0.1.1（测试候选版）

### 新增

- 补充 Mosh 终端尺寸回归验证：窗口 ResizeObserver 会在 fit 后把新列/行发送给同一 Mosh 会话，并在终端卸载时释放 observer；交付物测试会阻止验收文档把该项错误回退为待验。
- 新增只读外部验收预检：统一检查 Developer ID/公证/updater、系统架构、XQuartz、FIDO2、实体串口及 RDP/Mosh/WebDAV/Relay 环境是否就绪；报告不输出敏感值，可原子保存为权限 `0600` 的 Markdown，并明确区分前置条件与人工真机通过。
- 设置中的“反馈与诊断”可打开预填运行环境的错误报告、功能建议和当前 Release。
- 错误报告只预填 CNshell 版本、macOS 版本和运行架构，不读取或附带连接资料。
- 脱敏诊断导出成功后可直接在 Finder 中显示文件；诊断文件仍由用户检查并主动选择是否上传。
- 新增原生本地 Shell、受管 Telnet 和 Serial 串口连接；Serial 支持设备枚举、波特率、数据位、校验位、停止位、流控、DTR/RTS、独占打开及同路径拔出重连。
- Serial 文件面板新增内置 Xmodem 128/1K、Checksum/CRC 与 Ymodem Batch，支持系统文件授权、实时进度、取消、同名改名和失败临时文件清理。
- Serial 文件面板新增内置 G-Kermit 2.01 批量传输；helper 为 macOS 13+ universal binary，随包提供 GPL-2.0 与固定哈希的完整对应源码。
- Zmodem 已接管 SSH 原始协议流并支持安全的多文件上传、下载、冲突处理、取消和断线恢复；Mosh 已作为受管内置客户端接入现有终端工作区。
- 新增 SSH Certificate、X11 转发、FIDO2 Agent 身份筛选和 Touch ID 同步密钥保险库。
- 新增受控 Python 自动化、结构化操作录制、运行时定时任务以及原生 WebDAV 加密同步。
- 定时任务新增每日、每周和 IANA 时区配置；DST 回拨不会重复执行同一墙上时间，调度游标先持久化再启动远端任务，并由后端拒绝伪造历史和过期的一次性计划。
- AI 辅助支持隐私预览、脱敏、BYOK 兼容接口与本地 Ollama。
- WebDAV 与 AI 响应改为有累计上限的流式读取，响应头返回后仍可取消；WebDAV 细分并覆盖并发冲突、配额不足和服务端失败。
- SSH Certificate 在发网前校验非空 principals 必须精确包含目标用户名。
- 插件支持用户导入的 Ed25519 发布者根、manifest 与 WASM 联合签名验证、权限重新确认、撤销、摘要失效和审计；可信插件可在无 WASI 且带内存/栈/燃料限制的 Wasmi 沙箱中运行。
- 插件新增有界宿主 ABI：限额日志、用户本次选择的脱敏连接元数据、当前终端明确选中的文本，以及两分钟单次使用的 `connectionTest` 凭据代理；插件不能读取 Keychain、实时终端缓冲或诊断正文。
- 插件网络、目录和终端输入适配器现已开放：网络按 manifest 精确域名执行用户指定 HTTPS GET，目录通过安全 Bookmark 限制只读清单/文件，终端输入以最多 4 KB 的单次确认请求呈现；任何未确认输入都不会写入会话。
- 团队版新增工作区、Owner/Admin/Operator/Viewer 最小权限矩阵、成员生命周期、密钥 epoch 和最多 4,096 条的不含终端正文审计；支持 relay 在线账号、邀请和服务端权威目录同步。
- Owner/Admin 可原子导出不含连接与秘密的组织目录；密钥 epoch 轮换后，当时已授权且仍活动的设备仍可打开历史分享。
- 团队设备身份使用 Keychain 内 X25519/Ed25519 私钥；连接分享采用随机 AES-256-GCM 内容密钥、每设备 X25519+HKDF 密钥封装和发送设备签名，支持预览后导入、设备撤销和密文篡改拒绝。
- 多人终端新增端到端加密协议与在线协作中心：活动成员/设备校验、默认只读、10–300 秒单持有者控制租约；指定设备邀请通过 X25519/HKDF/AES-GCM 封装房间密钥，输入/输出帧由发送设备签名并端到端加密。客户端按房间维护 WebSocket、串行加密队列、服务端游标重连、成员/租约快照和 512 帧/4 MiB 有界观看历史，并支持主持、邀请、观看、控制移交和离开清理。
- 新增独立团队 relay 服务端基础：Argon2id 账号、短期 opaque token、Ed25519 设备 challenge、在线工作区邀请、服务端 RBAC/epoch/撤销、只转发签名密文的 WebSocket 房间、有界补帧、元数据审计和 Owner 永久删除；Docker/Compose 与双账号双设备 loopback 集成测试已纳入门禁。
- 团队设置已接入 relay 账号注册/登录、Keychain 短期会话、Ed25519 设备自动刷新、工作区发布、在线邀请接受、成员/设备/epoch 同步、服务端角色变更和设备撤销；终端工具栏已开放多人房间观看/控制入口。
- 团队 relay 新增数据库感知的 `/ready`、WebSocket 优雅停机、默认强制 `age` 的一致性备份、安全恢复脚本、本地恢复演练及部署/监控/事故 runbook；明文备份只允许显式测试开关。
- 团队 relay 新增低基数 Prometheus `/metrics` 和客户端 chunked 响应累计上限；新增固定发布者公钥、`sigsum-verify v0.13.1` 与官方生产策略的 `age` release 验证入口。Sigsum 验证后的 v1.3.1 本机演练覆盖正确/错误 identity 与私钥权限拒绝，生产异地恢复仍待部署环境。
- 团队 relay 新增 GitHub Ubuntu 24.04 Docker/Compose 真运行门禁，验证非 root、只读根文件系统、tmpfs、持久卷、loopback 端口、健康/指标和 SIGTERM 退出；Rust 与 Debian 基础镜像固定到已运行的 manifest digest。
- 团队 relay 新增生产邮箱验证：TLS SMTP 投递一小时一次性令牌，数据库只保存域分离哈希，验证前不签发账号会话，令牌单次使用且重发每分钟原子限流；客户端支持验证和重发，非 loopback 服务漏配 SMTP 会拒绝启动。
- 团队 relay 新增固定 digest 的生产 Compose：非 root NGINX 提供 TLS/WSS、严格 Host、认证/注册/IP 限速、WebSocket 连接限制和脱敏日志；Prometheus、NGINX exporter 与 Alertmanager 保持私网并提供 6 条告警规则。GitHub Linux smoke 验证真实 HTTPS 路由、429、公共运维端点隐藏、秘密不进入代理日志和内部指标抓取。

### 调整

- CI 与签名发布工作流固定所有外部 Actions 的 commit SHA；经 Dependabot 审查更新至 checkout 7.0.0、setup-node 7.0.0 和 upload-artifact 7.0.1，并固定 Node 20.20.2 与 Rust 1.96.0。工作流 token 仅只读仓库内容且 checkout 不持久化凭据；使用 minimal Rust profile 的任务显式安装 Clippy；发布凭据会在 Artifact 上传前清理，构建或清理失败均不会上传候选产物。
- 移除连接筛选栏中依赖浏览器提示框、在桌面端没有有效反馈的“新建文件夹”快捷按钮；已有文件夹及分组操作不受影响。

## 0.1.0（候选版）

### 新增

- 智能命令栏：历史/快捷命令/远端路径模糊补全、安全参数模板和执行预览。
- 会话日志、逐行时间戳、正则高亮/触发器、系统通知、最低对比度和增强光标。
- 多主机批量命令与同步输入、任意嵌套终端拆分、Copy Mode、跨标签搜索和粘贴安全预览。
- CodeMirror 6 远端编辑器、三方冲突 Diff、外部应用编辑回传、进程管理和 Ping/Traceroute/Socket 诊断。
- OpenSSH config 导入、Ed25519 密钥生成/部署，以及逐连接 SSH Agent 转发。
- SFTP 不可用时复用已验证 SSH 会话的 SCP 降级；协议依赖探测会明确显示 Zmodem、Mosh 和 X11 的可用边界。
- 受限自动化任务编排：命令、匹配、条件和文件传输，支持预览、逐步日志、取消、超时及失败重试。
- 用户自有 iCloud Drive/WebDAV/Git 本地目录的 AES-256-GCM 加密同步，分别控制主机、私钥路径和 Keychain 凭据，并保留冲突副本。

- SSH 密码、私钥、SSH Agent、严格主机指纹、SOCKS5、HTTP CONNECT 与 SSH 跳板连接。
- 多标签 xterm 终端、会话拆分、搜索、历史、快捷命令、自动重连与 SSH/TCP keepalive。
- SFTP 虚拟列表、文件操作、原子文本保存、后台上传下载、暂停、取消、重试和冲突策略。
- SFTP 左侧远端目录树支持按需展开、折叠和逐级导航。
- SFTP 文件右键菜单补齐下载、上传到目录、新建文件/文件夹；新建文件使用排他创建避免覆盖。
- 文件夹可按需打包上传或下载，支持覆盖、自动重命名、取消及临时归档清理。
- Linux CPU、内存、Swap、进程、网络、延迟、磁盘监控及系统信息导出；网络上下行与延迟显示最近 5 分钟折线趋势。
- 终端偏好支持字体、字号、行高、回滚行数、光标与四套配色；可全局设置或按连接覆盖，已打开终端实时生效，并支持快捷键缩放。
- RDP 随应用内置 universal SDL FreeRDP 客户端与完整第三方许可，不再要求用户安装 Homebrew 或 XQuartz。
- 本地连接库、文件夹、软删除、加密备份、脱敏诊断、Transport Pool 和类型生成 IPC。
- 嵌套连接文件夹支持展开、拖放和连接菜单直接移动。
- 内置 FreeRDP sidecar 的凭据隔离、受管生命周期和会话标签。
- universal macOS 13+ App/DMG、浅色/深色/高对比主题、键盘操作及基础 VoiceOver 语义。
- 手动安全更新入口；候选版不请求网络，正式配置下展示版本和说明并仅在确认后下载安装。

### 安全与可靠性

- 正式发布工作流新增 Developer ID `.p12` 临时 Keychain 导入、精确身份校验和失败后清理；FreeRDP、Mosh 与 G-Kermit 统一从固定源码重建并使用 Hardened Runtime/时间戳签名，App 包新增按需 RDP 麦克风用途说明；签名 universal 归档可生成同时覆盖 Apple Silicon/Intel 的 HTTPS updater `latest.json`。
- 正式 updater 门禁不再只检查 `.sig` 文件存在：刚构建的 CNshell 会使用与客户端相同的 Base64/minisign 规则验证归档、签名和 release 公钥匹配，并拒绝篡改归档、错误公钥与不安全 endpoint。
- 数据库迁移保持增量兼容，旧版本可在更新回滚后忽略未知的更高 migration 并继续读取原有数据；所有已知 migration checksum 仍严格校验，迁移前备份继续保留。
- 凭据与私钥 security-scoped Bookmark 保存于 macOS Keychain。
- 下载与远端保存使用临时文件和原子替换，避免半成品覆盖正式目标。
- RDP 密码仅通过 Helper stdin 传递，不进入参数或环境变量。
- 修复内置 FreeRDP 静态构建缺少 NTLM 所需 MD4/RC4，以及仅显示 `exit status: 147` 的问题；连接参数与密码统一经 stdin 传递，并将握手、登录和账户错误转换为可操作提示。
- SSH 认证和诊断阻塞具有恢复超时，网络断开使用有限退避重连。
- 修复关闭会话并闲置后再次连接可能复用已被服务器回收的 Transport、最终报 `Session(-43)`；终端连接改为始终新建独占 Transport，后台共享 Transport 增加闲置淘汰。
- Keychain 访问改为串行，避免终端、监控和 SFTP 并发触发重复系统授权；后台用户取消不再反复弹错。
- 错误通知默认 5 秒自动关闭；磁盘监控增加总量、已用与剩余空间明细。
- 修复从文件面板切换到快捷命令后再返回时，远端路径和目录树展开状态重置为根目录；浏览状态现在按 SSH 会话独立保留，目录内容返回时仍会重新读取。
- 磁盘监控不再只显示前 5 个挂载点；监控详情改为独立滚动区域，窗口高度不足时仍可查看完整磁盘列表。
- 修复监控采集命令出现在远端 `ps` 结果时，其命令行内的分隔标记被误识别，导致网络与磁盘数据持续显示不可用。

### 已知限制

- Developer ID、公证和正式 updater 仍需发行凭据与正式 HTTPS 服务。
- RDP 当前采用受管独立 FreeRDP 窗口深度联动；真实 Windows 画面、输入法、剪贴板和重连矩阵仍待可用主机验收。
- Debian、Rocky、Alpine，多版本/Intel Mac，完整弱网、VoiceOver 和干净 Mac 安装矩阵尚未完成。
- X11 已完成协议与本地转发代码，但本机缺少 XQuartz 图形端到端证据；FIDO2、Touch ID 和 Serial 仍缺对应实体硬件的完整人工验收。
- 团队终端加密协议、relay 服务端、客户端账号/工作区同步、邮箱验证、房间观看控制入口、备份/恢复及生产代理/监控配置已完成；正式域名证书、真实 SMTP/Alertmanager 投递、生产 `age` identity 的异地恢复和跨设备真机会话仍未完成。X/Ymodem/Kermit 的实体串口及第三方设备互操作待补。
