# 安装、升级与卸载

## 系统要求

- macOS 13 Ventura 或更高版本。
- Apple Silicon 或 Intel Mac；CNshell DMG 内的应用为 universal binary。
- SSH/SFTP/监控不需要本机开发环境。RDP 需另行安装 FreeRDP。

## 安装

1. 打开 `CNshell_0.1.0_universal.dmg`。
2. 将 `CNshell.app` 拖到“应用程序”。
3. 从“应用程序”启动 CNshell。
4. 创建 SSH 连接并通过可信渠道核对首次显示的 SHA-256 主机指纹。

当前仓库生成的候选 DMG 是 ad-hoc 签名，仅用于本机验收，不等同公开发行包。正式发布包必须通过 Developer ID 签名和 Apple 公证；不要通过绕过 Gatekeeper 的方式运行来源不明的副本。

## 升级

正式 updater 启用后，CNshell 只从发布配置中的 HTTPS endpoint 检查签名更新，验证失败会保留当前版本。当前候选版采用手动升级：

正式发行包可在“设置 → 软件更新”手动检查。CNshell 会先展示目标版本和发布说明，只有用户确认后才下载并安装；不会静默安装。候选版未配置 endpoint 时，界面会明确说明更新通道尚未启用。

1. 退出 CNshell，确保传输队列没有运行中任务。
2. 备份重要连接；普通安全导出不含凭据，需要跨 Mac 携带凭据时使用加密导出。
3. 打开新 DMG，将新 `CNshell.app` 覆盖到“应用程序”。
4. 启动后确认版本和连接库；SQLite migration 会在升级前创建数据库备份，失败时不会删除原数据库。

应用数据与 Keychain 凭据不存放在应用包内，因此覆盖应用不会主动清除它们。降级到更旧版本不受支持，降级前必须自行保存当前数据副本。

## 卸载

仅移除应用包不会自动删除 Keychain 凭据。需要完整清理时：

1. 在 CNshell 连接库的“已删除项目”中永久删除所有连接，使应用先清理对应 Keychain 条目。
2. 退出 CNshell，并将“应用程序”中的 `CNshell.app` 移到废纸篓。
3. 如需删除本机配置和历史，在 Finder 的“前往文件夹”中打开 `~/Library/Application Support/com.cnshell.desktop`，确认不再需要数据库备份后删除该目录。
4. 打开“钥匙串访问”，搜索 `com.cnshell.desktop`，核对后删除仍遗留的 CNshell 条目。

卸载不会修改远端服务器、`authorized_keys`、防火墙或用户自行安装的 FreeRDP。端口转发和受管 Helper 会随 CNshell 进程退出。

## 验收状态

上述步骤是正式操作说明。尚未在无开发环境的 Ventura、Sonoma、Sequoia 与 Intel Mac 上全部执行验证，因此发布门槛仍记录为外部验收项，详见 `docs/ACCEPTANCE.md`。
