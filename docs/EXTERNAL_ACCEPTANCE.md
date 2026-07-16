# 外部验收执行说明

总规划中的本机代码主线和短时自动化已经完成。本文只用于 Developer ID、不同设备、真实网络和生产服务到位后的外部验收，不会把“检测到条件”写成“场景通过”。

## 安全预检

在仓库根目录运行：

```bash
npm run preflight:external
```

预检只读取当前环境，不会连接远端、切换网络、触发 Touch ID、打开串口、修改 Keychain 或打印敏感值。报告只包含状态和数量，不包含证书名称、密钥、公钥、URL、主机、账号、X11 cookie 或设备路径。

需要保存证据时：

```bash
./scripts/external-acceptance-preflight.sh --output ./external-acceptance-report.md
```

报告通过同目录临时文件原子写入，权限固定为 `0600`。`READY` 仅表示前置条件存在；实际执行对应场景并保留截图、日志或校验结果后，才能更新 `docs/ACCEPTANCE.md`。

## 可选环境标记

这些变量只供预检判断“资料是否已提供”以及 URL 是否为不含内嵌凭据和 fragment 的 HTTPS 形式。脚本不输出其内容，也不发起网络连接：

| 变量 | 含义 |
| --- | --- |
| `CNSHELL_ACCEPTANCE_RDP_WINDOWS_10` | Windows 10 RDP 目标资料已准备 |
| `CNSHELL_ACCEPTANCE_RDP_WINDOWS_11` | Windows 11 RDP 目标资料已准备 |
| `CNSHELL_ACCEPTANCE_RDP_WINDOWS_SERVER` | Windows Server RDP 目标资料已准备 |
| `CNSHELL_ACCEPTANCE_MOSH_TARGET` | 可直接访问 UDP 的真实 Mosh 目标已准备 |
| `CNSHELL_ACCEPTANCE_WEBDAV_URL` | 真实 WebDAV 环境已准备 |
| `CNSHELL_ACCEPTANCE_SECOND_DEVICE` | 第二台独立设备已准备 |
| `CNSHELL_ACCEPTANCE_RELAY_URL` | 正式 HTTPS/WSS Relay 已准备 |
| `CNSHELL_ACCEPTANCE_RELAY_BACKUP_TARGET` | 异地加密备份目标已准备 |

正式发布预检沿用发布工作流变量：`APPLE_SIGNING_IDENTITY`、`APPLE_API_ISSUER`、`APPLE_API_KEY`、`APPLE_API_KEY_PATH`、`TAURI_SIGNING_PRIVATE_KEY` 和 `UPDATER_DOWNLOAD_BASE_URL`。API 私钥必须是权限为 `0400` 或 `0600` 的普通文件。不要把这些变量值或 Developer ID `.p12` 写入仓库、报告或聊天记录。

## 验收边界

以下操作必须由人在对应环境中完成，预检不会替代：

1. Developer ID 签名、公证、Gatekeeper、正式 updater 升级与失败回滚。
2. Ventura、Sonoma、Sequoia、Intel 和无开发环境 Mac 的安装、升级、卸载与数据保留。
3. Windows 10/11/Server 的 RDP 首帧、中文 IME、键鼠、剪贴板、音频、缩放和重连。
4. XQuartz 图形窗口、FIDO2 触摸/PIN/取消/拔出、Touch ID 保存/解锁/取消/指纹变化。
5. 实体 Serial、X/Ymodem 和 Kermit 与第三方设备互操作。
6. Mosh 的 IP/Wi-Fi 切换和断网恢复，以及 WebDAV 双设备冲突。
7. 正式 Relay 的 TLS/WSS、邮件、限速、监控、生产 identity 异地恢复和两台设备跨网络控制。

不具备某项设备或服务时保持 `MISSING` 或 `MANUAL`，不得用模拟器、静态配置或本机 loopback 结果替代真机证据。
