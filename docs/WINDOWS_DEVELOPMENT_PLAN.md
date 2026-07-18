# CNshell Windows 双平台开发与发布实施记录

> 最后更新：2026-07-18
>
> 目标：在同一套 Tauri 2 + React + Rust 工程中提供 macOS、Windows x64 和 Windows ARM64 版本，并通过同一个 GitHub Release 发布。
>
> 当前工程状态：Windows 功能代码与打包链已实现；`v0.2.0-beta.1` 目标提交 `47b48c0` 的 Core CI `29640190967` 与 Windows Packaging `29640190971` 已全部通过。没有真机或发行凭据的项目仍按外部验收边界记录，不以交叉编译代替真机结论。

## 一、状态定义

| 状态 | 含义 |
| --- | --- |
| 已实现 | 代码、配置、自动化和对应单元/集成测试已经落地 |
| CI 通过 | 已在 GitHub 托管的 Windows x64 runner 或 ARM64 交叉构建中通过 |
| 真机待验 | 实现已存在，但需要用户当前没有的设备、外设或交互环境 |
| 发行待配置 | 代码已就绪，但需要签名证书、私钥、受保护环境或正式服务 |

## 二、固定产品决策

- Windows 最低支持 Windows 10 22H2（build 19045）和 Windows 11。
- 首发同时生成 x64 与 ARM64 NSIS 安装包；x64 标记 Beta，ARM64 在取得原生真机证据前标记 Preview。
- Windows 使用原生标题栏和系统关闭按钮；macOS 保留 Overlay 标题栏。
- Windows 安装采用当前用户模式，创建开始菜单入口，不默认创建桌面快捷方式；首版不提供 MSI。
- RDP 使用随包分发的独立 FreeRDP SDL 窗口，由 CNshell 管理生命周期，不依赖 `mstsc.exe`，不要求用户另装 FreeRDP。
- 首个 Windows Beta 可以没有 Authenticode，但不能省略 Tauri updater 签名、SHA-256、来源说明和 SmartScreen 风险说明。
- macOS 与 Windows 复用数据库 schema、前端和 Rust 业务层，不维护独立 Windows 产品仓库或平台分叉数据库。

## 三、功能实现矩阵

| 范围 | 状态 | 代码证据 | 自动化证据 |
| --- | --- | --- | --- |
| 平台配置与构建调度 | 已实现 | `src-tauri/tauri.conf.json`、`tauri.macos.conf.json`、`tauri.windows.conf.json`、`scripts/build-desktop.mjs`、`scripts/build-sidecar.mjs` | 配置测试校验 macOS/Windows overlay、NSIS、WebView2 和资源布局；CI 分别构建 universal App、Windows x64/ARM64 |
| Windows 原生 UI 与能力模型 | 已实现 | `src-tauri/src/platform.rs`、`src/lib/platform.ts`、`src/generated/ipc.ts` | `PlatformCapabilities` 覆盖系统、架构、快捷键、凭据库、文件管理器、RDP、Mosh、Kermit、X11、Agent、生物识别和 Serial；前端测试覆盖 `Ctrl`、文件资源管理器和 Windows 外部程序选择 |
| 凭据与秘密隔离 | CI 通过 | `src-tauri/src/ssh.rs`、`bookmark.rs`、`backup.rs`、`ai.rs`、`webdav.rs`、`team_relay.rs` | SSH/RDP 密码、私钥口令、代理密码、AI Key、WebDAV 密码和团队令牌使用 Windows Credential Manager；Windows x64 真往返覆盖连接秘密和路径记录；加密备份导入把凭据写入目标系统 store，SQLite、普通导出和诊断只保存引用或脱敏字段 |
| Windows 本地路径与跨平台导入 | CI 通过 | `src/lib/local-path.ts`、`src-tauri/src/bookmark.rs`、`backup.rs`、`platform.rs`、`sftp.rs` | 盘符、UNC、Unicode、目录上传、目录下载、拖放、传输文件名、ShellExecuteW 和 Explorer `/select,` 均有定向测试或 Windows 编译证据；无效私钥路径保留连接并要求重新选择；超过 Credential Manager blob 上限的路径授权清理冗余记录并退回 profile 绝对路径 |
| 本地 Shell / ConPTY | CI 通过 | `src-tauri/src/local_shell.rs` | 按 `pwsh.exe`、`powershell.exe`、`cmd.exe` 选择；PowerShell 使用 `-NoLogo`；Windows x64 测试覆盖输入、输出、resize、关闭和重开 |
| SSH、证书、Agent 与 FIDO2 | 已实现，FIDO2 真机待验 | `src-tauri/src/ssh.rs`、`openssh.rs`、`certificate.rs`、vendored `ssh2` | OpenSSH 工具从系统目录和 `PATH` 发现；Agent 兼容 Windows OpenSSH named pipe/Pageant；FIDO2 仅选择 `sk-*` 身份。普通 SSH/证书/Agent 自动化通过，实体 FIDO2 交互未声明通过 |
| SFTP、传输与远程编辑 | 已实现 | `src-tauri/src/sftp.rs`、`src/features/files/*` | 复用跨平台协议核心；Windows 路径适配、目录树状态、冲突处理、Zmodem、批量执行、任务、日志和自动重连纳入前端/Rust 回归 |
| Serial 与 X/Ymodem | 已实现，硬件待验 | `src-tauri/src/serial.rs`、`xymodem.rs`、`SerialTransferPanel.tsx` | 接受 `COM1` 至 `COM256`，COM10+ 使用 `\\.\COMn`；协议双端回环和异常边界通过，实体 USB 串口未验 |
| Windows G-Kermit | CI 通过，设备待验 | `scripts/kermit-windows/*`、`scripts/build-kermit-sidecar.ps1`、`src-tauri/src/kermit.rs` | 固定 G-Kermit 2.01 源码构建 x64/ARM64 PE；Windows x64 两 helper 完成 12,345 字节外部管道互传；取消、隔离接收、原子保存和冲突重命名由 Rust 测试覆盖 |
| Windows RDP | 已实现，真实 RDP 待验 | `src-tauri/src/rdp.rs`、`scripts/build-freerdp-sidecar.ps1`、`scripts/patches/*` | 固定 FreeRDP 3.28.0 构建 x64/ARM64；参数与密码只经 stdin；Win32 聚焦、隐藏、恢复、主窗口移动联动和 Job Object 清理已实现；ARM64 PE 已通过，真实首帧/输入仍待 Windows 主机 |
| Windows Hello | 已实现，交互待验 | `src-tauri/src/touch_id.rs` | 使用 Microsoft Platform Crypto Provider 创建高保护、仅解密用途的 CNG 密钥封装同步口令，并保留手动口令恢复；Windows ARM64 编译通过，真实 Hello 弹窗未验 |
| Windows X11 | 已实现，图形待验 | `src-tauri/src/x11.rs` | 检测 VcXsrv/Xming `xauth.exe`，使用 TCP `DISPLAY`，Windows 不引用 Unix socket；cookie 解析与隔离测试通过，真实 GUI 未验 |
| Windows Mosh | CI 通过 | `scripts/mosh-windows/*`、`scripts/build-mosh-sidecar.ps1`、`scripts/test-mosh-windows.ps1`、`src-tauri/src/mosh.rs` | 固定 Mosh 1.4.0、protobuf 21.12、OpenSSL 与 zlib，静态 MSVC runtime；x64 真实加密 UDP 双向回环与 ARM64 PE 均通过，不依赖 WSL/MSYS2 |
| AI、插件、自动化与团队 | 已实现 | `src-tauri/src/ai.rs`、`plugin.rs`、`automation.rs`、`python_automation.rs`、`team*.rs`、`services/team-relay` | 与 macOS 复用功能和安全模型；Windows 凭据文案/后端已适配；前端、Rust、relay、Docker 与生产配置短时门禁通过 |

## 四、Windows RDP 实现边界

1. FreeRDP、OpenSSL、SDL、SDL_ttf 与 FreeType 从固定版本和哈希构建，x64 与 ARM64 分别生成 PE sidecar。
2. `/args-from:stdin` 承载全部连接参数和密码，秘密不进入 argv 或环境变量。
3. `resource_dir`/可执行文件邻接资源定位兼容安装目录，不使用 `.app/Contents/Resources` 假设。
4. Win32 API 支持聚焦、隐藏、恢复和窗口位置同步；位置同步仅作用于窗口模式，不移动全屏会话，也不持续覆盖用户手动移动的位置。
5. Job Object 使用 `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`，CNshell 退出时清理 helper。
6. 用户主动关闭 SDL 窗口映射为正常关闭；认证、DNS、TLS、端口和异常退出保留可读诊断。

## 五、CI、安装包与发布实现

| 门禁 | 实现 |
| --- | --- |
| Core CI | macOS 前端/Rust、WebKit E2E、PTY、relay；Windows x64 完整 Rust 测试与 Windows ARM64 `cargo check --all-targets` |
| Windows Packaging x64 | 构建并运行 G-Kermit、Mosh 加密 UDP 回环、FreeRDP `/version`、应用 PE、NSIS；静默安装后检查三类 sidecar、许可证与对应源码，并验证无默认桌面快捷方式、用户已有快捷方式升级保留、WebView2、SQLite、Credential Manager、原生关闭、覆盖升级、卸载和重装 |
| Windows Packaging ARM64 | 固定源码构建三类 sidecar，检查 ARM64 PE，构建应用与 NSIS，上传 Preview artifact；没有 x64 runner 上伪运行 ARM64 二进制 |
| 最低系统 | `src-tauri/windows/installer-hooks.nsh` 使用 `${AtLeastBuild} 19045` 阻止旧系统安装 |
| Updater | macOS 生成 updater 归档，Windows x64/ARM64 直接使用 NSIS 安装器；三者均生成 `.sig`，统一 `latest.json` 覆盖 `darwin-aarch64`、`darwin-x86_64`、`windows-x86_64`、`windows-aarch64` |
| Release | 手动 `Signed Cross-platform Release Candidate` workflow 使用受保护 `release` environment，只创建 Draft Release；最终公开由发布负责人确认 |
| 许可证与源码 | FreeRDP/Mosh/G-Kermit 许可证、固定源码归档及 Windows 适配源码随应用和 Release 分发；Release 分别提供 FreeRDP/Mosh/G-Kermit Windows 适配源码包，Mosh 包含独立加密 UDP 自测脚本 |

同一 Release 的目标产物为：

- `CNshell_<version>_universal.dmg`
- `CNshell_<version>_x64-setup.exe`
- `CNshell_<version>_arm64-setup.exe`
- macOS `.app.tar.gz` 与 `.sig`、Windows x64/ARM64 `-setup.exe.sig`
- `latest.json`、`SHA256SUMS.txt`、第三方许可证与对应源码附件

## 六、完成标准审计

| 标准 | 当前证据 | 结论 |
| --- | --- | --- |
| macOS universal、Windows x64、Windows ARM64 从干净 CI 构建 | 提交 `47b48c0` 的 Core CI `29640190967` 与 Windows Packaging `29640190971` 均通过；Windows ARM64 是交叉构建/PE/NSIS 证据 | 通过；ARM64 原生运行仍待真机 |
| Windows 编译不包含 Unix socket/AppKit 路径 | 平台模块均使用 `cfg` 隔离；Windows x64 测试和 ARM64 `cargo check --all-targets` 通过 | 通过 |
| Windows x64 启动无白屏、原生关闭有效、ConPTY 可重开 | Windows x64 Rust 208 项测试包含 ConPTY 输入/resize/关闭/重开；NSIS 生命周期启动真实 WebView2 并接受原生关闭 | CI 通过；不同 Windows 真机仍待验 |
| SSH/SFTP/监控/Zmodem/RDP/Credential Manager 安全边界 | 跨平台协议测试、Windows Credential Manager 真往返、RDP stdin/Job Object/参数测试 | 代码与 CI 通过；真实 RDP 待验 |
| NSIS 安装、升级、卸载、重装保留用户数据 | 临时 CI 账户中的 SQLite sentinel 与 namespaced Credential Manager 项贯穿完整生命周期 | CI 通过 |
| x64/ARM64 架构和 Release 清单一致 | 两个 Packaging job 均通过应用/sidecar PE 与 NSIS；四平台 updater manifest 和发布汇总门禁由定向测试验证 | CI 通过；正式签名候选仍待发行配置 |
| Windows 10/11/ARM64 体验矩阵 | 当前没有对应真机 | 真机待验，不能写成通过 |

## 七、必须保留的真机与发行边界

以下项目不是继续编写同一套业务代码就能诚实关闭的门禁，必须取得对应环境证据：

- Windows 10 22H2 x64 与 Windows 11 x64 的真实安装、启动、中文 IME、剪贴板、DPI、高对比、Narrator、睡眠唤醒和网络切换。
- Windows 11 ARM64 原生运行、性能、WebView2 与 sidecar 生命周期；在此之前 ARM64 必须标记 Preview。
- 真实 Windows RDP 的首帧、键鼠、中文输入、剪贴板方向、声音、麦克风、动态分辨率、多显示器和断网重连。
- Windows Hello 保存/解锁/取消、实体 FIDO2 触摸/PIN/拔出、VcXsrv/Xming 图形程序、实体 COM 串口及外部 X/Ymodem/Kermit 设备互操作。
- Authenticode 证书或云签名服务；取得后需同时签名主程序、FreeRDP/Mosh/Kermit sidecar 和 NSIS 安装包。
- Tauri updater 私钥、公钥、HTTPS endpoint 和 GitHub `release` environment secrets；这些与 Authenticode 是两套独立门禁。
- macOS Developer ID、公证和不同 macOS/Intel 真机仍按 `docs/ACCEPTANCE.md` 的既有边界处理。

## 八、发布顺序

1. 确认 Core CI 与 Windows Packaging 在目标提交上全部通过。
2. 配置 Tauri updater 私钥、公钥、HTTPS endpoint 和受保护的 GitHub `release` environment。
3. 手动运行 `Signed Cross-platform Release Candidate`，验证 macOS、Windows x64、Windows ARM64 updater 签名、SHA-256 和源码附件。
4. 在 Draft Release 中核对安装包名称、架构、SmartScreen 说明、ARM64 Preview 标识和本文件的真机边界。
5. 完成可用设备上的人工验收后再公开 Release；没有证据的项目继续保留为已知限制。

该顺序允许代码与自动化先完成，但禁止把交叉编译、PE 检查或 CI 虚拟机结果写成真实 Windows 设备体验通过。
