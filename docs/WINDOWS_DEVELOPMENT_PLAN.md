# CNshell Windows 双平台开发与发布计划

> 状态：开发中  
> 目标：在同一套 Tauri 2 + React + Rust 工程中提供 macOS、Windows x64 和 Windows ARM64 版本，并通过同一个 GitHub Release 发布。

## 一、目标与发布边界

- Windows 最低支持 Windows 10 22H2 和 Windows 11。
- 首发同时生成 x64 与 ARM64 NSIS 安装包；ARM64 在取得原生真机证据前标记为 Preview。
- 第一阶段优先完成 SSH、SFTP、监控、RDP、本地 Shell、Serial、备份/同步、AI、插件和团队协作等主要工作流。
- Windows Hello、原生 Mosh、Kermit、X11 和 FIDO2 作为第二阶段一致性能力，不阻塞首个核心 Beta。
- 首个 Windows Beta 可以没有 Authenticode 签名，但必须提供 Tauri updater 签名、SHA-256 和 SmartScreen 风险说明。
- macOS 与 Windows 复用数据库 schema、业务模型、前端和 Rust 业务层，不创建平台分叉数据库或独立 Windows 产品仓库。

## 二、跨平台基础改造

- 将 Tauri 配置拆分为公共配置以及 macOS、Windows 平台配置。
- macOS 保留当前 App/DMG、Hardened Runtime 和系统标题栏策略；Windows 使用原生系统窗口按钮、`.ico`、NSIS 和 WebView2 bootstrapper。
- 使用跨平台 Node 构建调度器替代 `package.json` 中直接串联 macOS shell 脚本的做法；macOS 调用现有 shell 脚本，Windows 调用 PowerShell/MSVC 脚本。
- 按 target 拆分 Rust 依赖：macOS 使用 Apple Keychain/Objective-C API，Windows 使用 Windows Credential Manager 和 Win32/CNG API。
- 增加统一 `PlatformCapabilities` 后端接口和前端类型，提供系统、架构、快捷键修饰键、凭据库名称及 RDP/Mosh/Kermit/X11/Agent/生物识别/Serial 能力状态。
- 所有 UI 文案按平台显示 Finder/文件资源管理器、Keychain/凭据管理器、Touch ID/Windows Hello、`⌘`/`Ctrl`；不可用能力显示原因，不保留无反应按钮。

## 三、Windows 核心功能

### 凭据与数据

- 密码、私钥口令、代理密码、AI Key、WebDAV 密码和团队令牌写入 Windows Credential Manager。
- SQLite、诊断包和普通导出不得包含秘密。
- macOS 继续使用 security-scoped Bookmark；Windows 使用原生选择器和规范化绝对路径，支持盘符、UNC、Unicode 和长路径。
- 跨平台导入遇到无效私钥路径时保留连接并提示重新选择，不静默删除。
- 加密备份中的凭据导入后写入目标系统的原生凭据库。

### 终端、SSH 与文件

- Windows 本地 Shell 使用 ConPTY，按 `pwsh.exe`、`powershell.exe`、`cmd.exe` 顺序选择；PowerShell 使用 `-NoLogo`，不传 Unix `-l`。
- SSH key/certificate 工具从 Windows OpenSSH 系统目录及 `PATH` 查找。
- 普通 Agent 支持 Windows OpenSSH Agent named pipe，并兼容可用的 Pageant 后端。
- SFTP、目录树、上传下载、远程编辑、冲突处理、Zmodem、隧道、批量执行、任务编排、会话日志和自动重连保持平台一致。
- 外部编辑使用 Windows ShellExecute，诊断文件使用 Explorer `/select,` 定位。

### Serial

- 接受 `COM1` 至 `COM256`，COM10 以上内部使用 `\\.\COM10` 形式。
- 设备选择、排序和校验不再要求 `/dev/*`。
- 内置 X/Ymodem 首版可用；Kermit 在第二阶段补齐原生 helper。

## 四、Windows RDP

- Windows 继续使用内置、独立但受 CNshell 管理的 FreeRDP 窗口，不依赖 `mstsc.exe`，不要求用户安装 FreeRDP。
- x64、ARM64 分别从固定源码构建 `sdl-freerdp.exe`，随包附许可证、notice 与对应源码。
- 密码和全部敏感参数继续通过 `/args-from:stdin` 传递，不进入命令行和环境变量。
- 统一使用 Tauri `resource_dir` 定位 helper，移除只适用于 `.app/Contents/Resources` 的假设。
- 使用 Win32 API 完成窗口聚焦、隐藏、恢复、位置联动；使用 Job Object 管理 helper，确保 CNshell 退出后无残留进程。
- 用户主动关闭 RDP 窗口映射为正常关闭；崩溃、认证失败和网络失败保留可读诊断。

## 五、高级一致性功能

1. Windows Hello：使用 Windows Hello/CNG 非导出密钥保护加密同步口令，保留手动口令恢复入口。
2. X11：检测 VcXsrv/Xming 等外部 X Server；Windows 使用 TCP DISPLAY，不调用 Unix socket，不捆绑完整 X Server。
3. FIDO2：通过 Windows OpenSSH Agent/FIDO2 身份执行触摸和 PIN 认证，普通 Agent 与硬件身份分开检测。
4. Mosh：从固定源码构建 x64/ARM64 helper，捆绑必要运行库、许可证和源码，不要求 WSL/MSYS2。
5. Kermit：将 G-Kermit 外部管道模式移植到 Windows x64/ARM64，保持取消、隔离接收、原子保存和冲突重命名边界。

某项 helper 未通过对应架构运行验证时，该能力必须保持禁用并显示原因，不能以“构建成功”冒充“功能通过”。

## 六、CI、安装包与 GitHub Release

- Windows x64 CI 执行 Rust/前端测试、ConPTY 夹具、协议夹具、NSIS 安装、启动、升级和卸载测试。
- Windows ARM64 CI 执行 MSVC 交叉编译、NSIS 打包、PE 架构、资源和依赖静态校验；取得 Windows ARM64 runner 后增加原生运行测试。
- 安装器使用当前用户模式，创建开始菜单入口，不默认向桌面写快捷方式；缺少 WebView2 时启动官方 bootstrapper。
- 同一 Release 发布：
  - `CNshell_<version>_universal.dmg`
  - `CNshell_<version>_x64-setup.exe`
  - `CNshell_<version>_arm64-setup.exe`
  - macOS `.app.tar.gz`、Windows `.nsis.zip` 及各自 `.sig`
  - `latest.json`、`SHA256SUMS.txt`、第三方许可证与源码说明
- `latest.json` 同时包含 `darwin-aarch64`、`darwin-x86_64`、`windows-x86_64` 和 `windows-aarch64`，每个平台指向自己的归档和签名。
- 发布 workflow 使用受保护的 `release` environment，先生成 Draft Release，核对验收记录后再公开。
- Tauri updater minisign 与 Authenticode 分开处理：未签名 Beta 仍必须验 updater 签名；取得 Windows 证书或云签名服务后，再签名主程序、sidecar 和安装包。

## 七、完成标准

- macOS universal、Windows x64、Windows ARM64 均能从干净 CI 构建。
- Windows 编译不包含 Unix socket/AppKit 代码，macOS 现有功能和数据保持兼容。
- Windows x64 应用启动无白屏，原生关闭按钮有效；ConPTY 输入、resize、关闭和重开通过。
- SSH 密码/私钥/主机指纹、重连、SFTP、监控、Zmodem、RDP helper 生命周期和 Credential Manager 安全边界通过测试。
- NSIS 安装、覆盖升级、卸载重装不破坏 SQLite、连接资料和凭据。
- x64 与 ARM64 安装包架构正确，SHA-256 与 Release 清单一致。
- Windows 10 x64、Windows 11 x64、Windows 11 ARM64 的中文输入、剪贴板、DPI、高对比、Narrator、睡眠唤醒、网络切换和真实 RDP 按设备条件记录真机证据。
- 没有真机证据的 ARM64、真实 RDP、Windows Hello、FIDO2、X11 和实体串口必须明确标记为未完整验收。

## 八、固定决策

- Windows 首发同时提供 x64 和 ARM64。
- x64 标记 Beta，ARM64 在原生验收前标记 Preview。
- 不支持 Windows 7/8；安装格式使用 NSIS，首版不提供 MSI。
- RDP 使用内置 FreeRDP，不使用系统 `mstsc.exe`。
- 首个 Windows Beta允许未做 Authenticode 签名，但必须提供 updater 签名、SHA-256 和 SmartScreen 说明。
- Windows Hello、Mosh、Kermit、X11、FIDO2 不阻塞首个核心 Beta，但必须纳入后续一致性开发。
