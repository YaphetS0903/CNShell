# 故障排查

- **DNS/TCP 失败**：检查主机、端口、VPN、防火墙与代理。
- **首次指纹**：在云控制台执行 `ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub -E sha256` 核对。
- **指纹变化**：不要直接接受；先确认服务器是否重装或密钥轮换。
- **认证失败**：检查 Keychain 条目、私钥权限、SSH Agent 身份和服务端认证策略。
- **SFTP 不可用**：确认服务端启用 SFTP subsystem，且用户具有目录权限。
- **监控为空**：非 Linux 系统或 `/proc` 不可用时会降级，终端仍可使用。
- **RDP 不可用**：运行 `brew install freerdp`，再重启 CNshell。
- **提交问题**：设置 → 故障诊断 → 导出脱敏诊断；先确认文件中没有不希望分享的信息。
