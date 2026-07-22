# 故障排查

- **DNS/TCP 失败**：检查主机、端口、VPN、防火墙与代理。
- **首次指纹**：在云控制台执行 `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256` 核对。
- **指纹变化**：不要直接接受；先确认服务器是否重装或密钥轮换。
- **认证失败**：检查 macOS Keychain 或 Windows 凭据管理器条目、私钥权限、SSH Agent 身份和服务端认证策略。
- **SSH Certificate 无效**：确认选择的是 `*-cert.pub` 用户证书及其对应私钥，证书当前已生效且未过期，主体包含登录用户名，服务端 `TrustedUserCAKeys` 信任签发 CA；CNshell 不会自动放宽这些检查。
- **FIDO2 身份未检测到**：先插入安全密钥，并确认 `ssh-add -L` 中存在 `sk-ssh-ed25519@openssh.com` 或 `sk-ecdsa-sha2-nistp256@openssh.com` 身份；驻留密钥也必须先加载进当前 OpenSSH Agent。连接时按提示触摸设备或输入硬件 PIN，普通 RSA/Ed25519 Agent 密钥不会被此模式使用。
- **Touch ID / Windows Hello 同步不可用**：macOS 确认设备支持 Touch ID、已录入指纹且当前会话已解锁；Windows 确认已设置 Windows Hello PIN 或生物识别并解锁当前会话。取消、锁定、指纹集合变化或保护密钥丢失后，可直接改用原同步口令；需要重新绑定时先移除已保存口令，再用手动口令重新保存。
- **X11 不可用**：macOS 先启动 XQuartz，并确认 `DISPLAY` 与 `/opt/X11/bin/xauth list "$DISPLAY"` 有 MIT cookie；Windows 先启动 VcXsrv 或 Xming，并确认对应 `xauth.exe` 可用且 `DISPLAY` 指向 loopback TCP display。随后回到 CNshell 重新探测并为可信连接启用。服务端还必须允许 `X11Forwarding`，Mosh 模式不能承载 X11 channel。
- **SFTP 不可用**：确认服务端启用 SFTP subsystem，且用户具有目录权限。
- **监控为空**：非 Linux 系统或 `/proc` 不可用时会降级，终端仍可使用。
- **RDP 组件缺失或损坏**：macOS 重新从完整 DMG 安装，Windows 重新运行对应架构的 NSIS 安装包；不要单独移动或删除应用资源目录中的 FreeRDP 文件。
- **RDP 窗口一直显示正在启动**：确认受管 FreeRDP 窗口没有被系统隐藏或安全软件拦截，并等待协商完成；内置 helper 会在真正 `postConnect` 后报告在线。若目标服务端拒绝连接，标签会进入失败并保留可操作错误，重新连接不会复用旧进程。
- **RDP 显示器列表为空**：显示器编号来自内置 FreeRDP 的 `/list:monitor`，重新插拔显示器后重新打开连接编辑页；指定显示器只在全屏模式生效。
- **RDP 本地目录映射失败**：确认目录仍存在且没有逗号或控制字符；macOS Bookmark 过期或 Windows 绝对路径在本机失效时重新选择。映射是读写权限，连接结束后 sidecar 会释放 macOS 路径授权并终止对应 helper。
- **目标端口未返回有效 RDP 协商响应**：确认 Windows“设置 → 系统 → 远程桌面”已开启，系统版本支持作为远程桌面主机，连接端口与防火墙规则正确；端口仅能建立 TCP 并不代表 RDP 服务正在响应。
- **提交问题**：设置 → 故障诊断 → 导出脱敏诊断；先确认文件中没有不希望分享的信息。

## MCP

- **客户端提示找不到 CNshell 或 Broker 未运行**：保持 CNshell 打开，在“设置 → MCP 服务”启用 MCP；关闭 MCP 或重启 CNshell 后，旧请求和旧短期会话会失效，重新调用即可。
- **提示未找到 `cnshell-mcp`**：从完整 DMG 或对应架构 NSIS 安装包重新安装，不要单独移动、改名或删除应用资源目录中的 `mcp/cnshell-mcp`。开发源码运行时先执行 `npm run build:mcp`。
- **客户端身份或摘要变化**：升级/替换 sidecar 后旧授权会拒绝请求。回到 MCP 设置撤销旧客户端，重新创建并复制配置；不要手工修改配置里的客户端 ID 或 discovery 路径。
- **撤销后提示凭据仍待清理**：客户端权限、短期会话和请求已经立即失效，但 Keychain/Credential Manager 的物理清理没有完成。保持同版本应用资源中的 `cnshell-mcp` 未被移动或替换，重新启动 MCP Broker 后会自动重试；若资源已变化，先重新安装同版本完整应用。不要手工运行未验证的 sidecar 删除模式。
- **一直等待审批**：打开 CNshell 右侧 MCP 审批抽屉；请求 120 秒后自动拒绝。客户端断开时审批也会被清理。普通的单次批准不会让后续命令、写入、删除、上传或下载自动放行；只有用户明确保存且仍有效的精确命令规则可以匹配同一完整命令文本。规则只适用于保守低风险命令；规则可在对应客户端设置中撤销，撤销后相同命令会重新要求审批。
- **动态 Resources 没有内容**：`cnshell://connections` 只显示当前客户端被明确授予 `cnshell_list_connections` 权限的连接；只授予其他工具并不足以列出连接。`cnshell://audit/recent` 只显示当前客户端自己的最近审计元数据。修改授权后重新发起 Resource 读取，不要尝试把内部 `resource:*` 操作作为 Tool 调用。
- **本地上传/下载授权无效**：一次性授权在批准后即消费，且最迟 24 小时过期；重新通过系统选择器创建授权。`..`、绝对路径、符号链接、junction/reparse point 和授权目录外路径会被拒绝。
- **响应过大或请求过于频繁**：单条 MCP/Broker 消息和命令总输出上限为 1 MiB；文本读取应使用 `offset` 分页。每客户端最多 2 个执行中请求，读请求每分钟 60 次、写请求每分钟 10 次。

## Mosh 无法连接或一直等待 UDP

确认远端已安装 `mosh-server`，并在云安全组、云防火墙和服务器防火墙同时放行连接设置中的 UDP 范围（默认 `60000–60010`）。SSH 代理只用于启动远端服务，Mosh 数据不会穿过 SOCKS/HTTP/SSH 跳板；使用代理的主机仍需从运行 CNshell 的本机直接访问服务器 UDP 地址。
