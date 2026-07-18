# 安装、升级与卸载

## 系统要求

- macOS 13 Ventura 或更高版本，支持 Apple Silicon 与 Intel universal DMG。
- Windows 10 22H2（build 19045）或 Windows 11，提供 x64 Beta 与 ARM64 Preview NSIS。
- SSH/SFTP/监控和 RDP 均不需要 Homebrew、WSL、MSYS2 或本机开发环境；对应架构的 FreeRDP 已包含在 CNshell 应用包中。

## 安装

macOS：

1. 从本仓库 GitHub Release 下载 `CNshell_<version>_universal.dmg` 和 `SHA256SUMS.txt`，先核对 SHA-256。
2. 打开 DMG，将 `CNshell.app` 拖到“应用程序”。
3. 当前 Beta 没有 Developer ID 和 Apple 公证。确认来源与哈希后，在 Finder 的“应用程序”中对 CNshell 使用右键“打开”，并仔细阅读 Gatekeeper 提示。
4. 创建 SSH 连接并通过可信渠道核对首次显示的 SHA-256 主机指纹。

当前 Beta DMG 是 ad-hoc 签名，不等同通过 Developer ID 签名和 Apple 公证的正式发行包。不要执行 `xattr -cr`，不要关闭 Gatekeeper，也不要通过其他全局绕过方式运行来源不明的副本。

Windows：

1. x64 设备下载 `CNshell_<version>_x64-setup.exe`；Windows on ARM 下载 `CNshell_<version>_arm64-setup.exe`，后者在原生真机验收完成前标记为 Preview。
2. 从仓库 Release 同时下载 `SHA256SUMS.txt`，使用 PowerShell `Get-FileHash <安装包> -Algorithm SHA256` 核对结果。
3. 运行当前用户 NSIS 安装器。安装器创建开始菜单入口，不会在未明确选择时写桌面快捷方式；WebView2 缺失时使用随包 bootstrapper 安装。
4. 首个未做 Authenticode 的 Beta 可能显示 SmartScreen 信誉提示。只在哈希与本仓库 Release 一致时继续；不要关闭 SmartScreen 或全局降低系统安全设置。

Tauri updater 的 minisign 签名会验证更新包，但不能替代 Developer ID、Apple 公证或 Windows Authenticode。x64 包在真机扩大验收期间标记为 Beta，ARM64 包在完成原生设备验收前标记为 Preview。

## 升级

`v0.2.0-beta.1` 使用独立 Beta updater endpoint；CNshell 只接受由内置公钥验证通过的更新包，验证失败会保留当前版本。macOS 与 Windows 使用同一份四平台版本清单：

在“设置 → 软件更新”可手动检查。CNshell 会先展示目标版本和发布说明，只有用户确认后才下载并安装；不会静默安装。未来正式发布仍需 Developer ID、公证和 Authenticode，并继续沿用兼容的 updater 签名信任链。

1. 退出 CNshell，确保传输队列没有运行中任务。
2. 备份重要连接；普通安全导出不含凭据，需要跨设备或跨平台携带凭据时使用加密导出。
3. macOS 打开新 DMG 并覆盖“应用程序”中的 App；Windows 运行同架构的新 NSIS 安装包完成当前用户覆盖升级。
4. 启动后确认版本和连接库；SQLite migration 会在升级前创建数据库备份，失败时不会删除原数据库。

应用数据与 macOS Keychain/Windows 凭据管理器中的凭据不存放在应用包内，因此覆盖应用不会主动清除它们。降级到更旧版本不受支持，降级前必须自行保存当前数据副本。

## 卸载

卸载程序默认不会删除连接数据库和系统凭据，以便覆盖升级或重装恢复。需要完整清理时，应先在 CNshell 内永久删除连接，再按平台移除剩余数据。

macOS：

1. 在 CNshell 连接库的“已删除项目”中永久删除所有连接，使应用先清理对应 Keychain 条目。
2. 退出 CNshell，并将“应用程序”中的 `CNshell.app` 移到废纸篓。
3. 如需删除本机配置和历史，在 Finder 的“前往文件夹”中打开 `~/Library/Application Support/com.cnshell.desktop`，确认不再需要数据库备份后删除该目录。
4. 打开“钥匙串访问”，搜索 `com.cnshell.desktop`，核对后删除仍遗留的 CNshell 条目。

Windows：

1. 在 CNshell 连接库的“已删除项目”中永久删除所有连接，使应用先清理对应 Windows 凭据管理器条目。
2. 打开“设置 → 应用 → 已安装的应用 → CNshell → 卸载”。普通卸载会删除程序、开始菜单项和受管 helper，但保留 `%APPDATA%\com.cnshell.desktop` 中的数据库与备份，方便重装恢复。
3. 如需完整清理，在确认不再需要备份后删除 `%APPDATA%\com.cnshell.desktop`。
4. 打开“凭据管理器 → Windows 凭据”，核对后删除仍遗留的 `com.cnshell.desktop` 相关条目。

卸载不会修改远端服务器、`authorized_keys` 或防火墙。端口转发和受管 FreeRDP sidecar 会随 CNshell 进程退出。

## 验收状态

上述步骤是正式操作说明。自动化会覆盖 Windows x64 静默安装、覆盖升级、卸载、数据保留和重装，ARM64 只做编译与包结构验证；Windows 10/11/ARM64 真机、无开发环境的 Ventura/Sonoma/Sequoia 与 Intel Mac 仍属于外部验收项，详见 `docs/ACCEPTANCE.md`。
