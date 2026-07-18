# 架构说明

React WebView 仅负责展示与输入，通过由 Rust 模型生成的 TypeScript IPC 类型调用 Tauri commands/events。网络、macOS Keychain/Windows 凭据管理器、SQLite、本地文件和内置 sidecar 只由 Rust 访问。

- `src/features`：连接、终端、文件、监控、RDP、设置和帮助 UI。
- `src-tauri/src/ssh.rs`：SSH 握手、指纹、认证、Transport Pool、keepalive 与 PTY 生命周期。
- `src-tauri/src/sftp.rs`：SFTP 操作、后台传输、原子文件替换和远端预览。
- `src-tauri/src/monitor.rs`：无侵入 Linux 采集、解析和系统信息导出。
- `src-tauri/src/tunnel.rs`：本地、远程和动态转发。
- `src-tauri/src/backup.rs`：非敏感备份或 Argon2id/AES-256-GCM 加密凭据备份。
- `src-tauri/src/task.rs`：诊断、归档和预览共用的可取消后台任务模型。
- `src-tauri/src/rdp.rs`：进程隔离的内置 FreeRDP SDL sidecar 适配。
- `src-tauri/src/collaboration.rs`：团队终端房间、端到端邀请/帧加密、控制租约和本机补帧状态机。
- `src-tauri/src/team_relay.rs`：HTTPS relay 客户端、系统凭据库会话、设备 challenge、工作区发布/邀请与权威目录同步。
- `src-tauri/src/team_relay_terminal.rs`：在线房间 WebSocket、游标对账、逐房间串行加密、有界待发队列、重连和控制/成员事件。
- `src/features/terminal/TeamTerminalCenter.tsx`：常驻的主持/观看入口、邀请路由、参与者输出缓冲和控制租约交互。
- `services/team-relay`：独立在线账号、设备会话、服务端 RBAC/epoch/撤销和仅密文 WebSocket relay。

SFTP、监控、传输、压缩和预览优先租用同一连接配置下的空闲已认证 Transport；并发繁忙时池自动创建附加 Transport，配置、代理或主机身份变化时使旧池失效。终端 PTY 与隧道使用不可复用的独占 Transport，避免 `libssh2::Session` 互斥锁让交互输入被后台 Channel 阻塞。

连接诊断、压缩/解压和远端预览通过统一后台任务模型立即返回任务 ID，再以事件发送状态；传输使用可持久化的专用队列。SQLite 使用 WAL，数据库只保存凭据引用。密码、私钥口令和代理密码进入 macOS Keychain 或 Windows 凭据管理器；macOS security-scoped Bookmark 与 Windows 绝对路径授权记录也按连接隔离，数据库不保存其编码内容。

RDP 不进入 SSH Transport Pool。Rust 优先启动应用资源目录内当前架构的 FreeRDP sidecar：macOS 为 universal，Windows 为 x64/ARM64 PE。密码和连接参数仅写入子进程 stdin，sidecar 退出通过终端状态事件反馈给共享标签工作区；macOS 使用应用激活 API 管理窗口，Windows 使用 Win32 聚焦、隐藏、恢复、`SetWindowPos` 联动和 Job Object 清理。环境变量和系统 FreeRDP 检测仅用于开发覆盖，不属于用户安装流程。

## 团队 Relay

团队终端使用客户端生成的随机房间密钥。主持端以 X25519/HKDF/AES-GCM 分设备封装密钥，
输入输出 envelope 由实际发送设备 Ed25519 签名；relay 不获得内容密钥。独立 relay 只保存
账号、成员/设备公钥、服务端角色、epoch、短期 token 哈希、控制租约、路由元数据和有界
密文补帧。每个 WebSocket 帧都会重新读取设备、成员、角色、epoch 和租约，成员移除或设备
撤销会使旧 token 与旧房间立即失效。连接握手以服务端游标恢复待发序号，并同步已加入成员
与当前租约；客户端按房间串行加密，关闭通知与满载帧队列分离。生产服务部署和外部验收边界
见 `docs/TEAM_RELAY.md`。

## Mosh 终端

Mosh 会话先复用 CNshell 的 SSH 认证、代理和主机指纹校验启动远端 `mosh-server`，随后由 Rust `MoshManager` 在原生 PTY 中托管应用内 `mosh-client`。UDP 客户端使用连接配置中的公网主机名或地址，避免云主机 NAT 环境把 SSH 服务端的内网网卡地址误作目标。一次性 `MOSH_KEY` 仅通过子进程环境变量传递，不写入参数、数据库、日志或前端事件；终端输出继续走统一的 `terminal-output` IPC，SFTP 与监控使用相同连接资料建立独立 SSH 通道。

## SSH Certificate

SSH Certificate 连接将私钥与 OpenSSH 用户证书作为一组配置。macOS 为两份文件分别保存只读 security-scoped Bookmark，Windows 保存经校验的绝对路径记录；私钥口令只进入连接专属系统凭据项。Rust 在 macOS 使用系统 `/usr/bin/ssh-keygen`，在 Windows 从系统 OpenSSH 目录和 `PATH` 查找 `ssh-keygen.exe`，均以 `LC_ALL=C`、`TZ=UTC` 的最小环境解析证书元数据与有效期，并在每次认证前复核；实际认证通过 libssh2 的 public-key-from-file 接口同时传入证书公钥与对应私钥。

## X11 转发

CNshell 对 `ssh2` 0.9.6 保留一个最小本地补丁，仅公开底层 libssh2 已有的 `x11-req` 与入站 X11 channel callback，补丁源码和 MIT/Apache-2.0 许可证位于 `src-tauri/vendor/ssh2`。macOS 只连接 XQuartz 的本机 Unix socket 或 loopback display；Windows 只连接 VcXsrv/Xming 的 loopback TCP display，不引用 Unix socket。应用从平台对应的 `xauth` 读取真实 MIT cookie，向远端发送随机一次性假 cookie，并在入站 X11 setup 首包严格验证后原位替换；普通终端、认证、代理和主机指纹仍使用同一 CNshell Session。
