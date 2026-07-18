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

## Mosh 无法连接或一直等待 UDP

确认远端已安装 `mosh-server`，并在云安全组、云防火墙和服务器防火墙同时放行连接设置中的 UDP 范围（默认 `60000–60010`）。SSH 代理只用于启动远端服务，Mosh 数据不会穿过 SOCKS/HTTP/SSH 跳板；使用代理的主机仍需从运行 CNshell 的本机直接访问服务器 UDP 地址。
