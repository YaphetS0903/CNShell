# CNshell 版本更新清单

本项目采用语义化版本号。当前版本仍是本机候选版；正式签名、公证与更新通道完成前不标记为公开正式发布。

## 0.1.0（候选版）

### 新增

- SSH 密码、私钥、SSH Agent、严格主机指纹、SOCKS5、HTTP CONNECT 与 SSH 跳板连接。
- 多标签 xterm 终端、会话拆分、搜索、历史、快捷命令、自动重连与 SSH/TCP keepalive。
- SFTP 虚拟列表、文件操作、原子文本保存、后台上传下载、暂停、取消、重试和冲突策略。
- Linux CPU、内存、Swap、进程、网络、延迟、磁盘监控及系统信息导出。
- 本地连接库、文件夹、软删除、加密备份、脱敏诊断、Transport Pool 和类型生成 IPC。
- 外部 FreeRDP Helper 的检测、凭据隔离、受管生命周期和会话标签。
- universal macOS 13+ App/DMG、浅色/深色/高对比主题、键盘操作及基础 VoiceOver 语义。
- 手动安全更新入口；候选版不请求网络，正式配置下展示版本和说明并仅在确认后下载安装。

### 安全与可靠性

- 凭据与私钥 security-scoped Bookmark 保存于 macOS Keychain。
- 下载与远端保存使用临时文件和原子替换，避免半成品覆盖正式目标。
- RDP 密码仅通过 Helper stdin 传递，不进入参数或环境变量。
- SSH 认证和诊断阻塞具有恢复超时，网络断开使用有限退避重连。

### 已知限制

- Developer ID、公证和正式 updater 仍需发行凭据与正式 HTTPS 服务。
- RDP 当前打开独立 FreeRDP 窗口，尚未内嵌画面；真实 Windows 互操作仍待验收。
- Debian、Rocky、Alpine，多版本/Intel Mac，完整弱网及 VoiceOver 真机矩阵尚未完成。
- SSH 实现使用 `ssh2/libssh2`，与最初 `PLAN.md` 的 `russh` 选型存在已记录偏差。
