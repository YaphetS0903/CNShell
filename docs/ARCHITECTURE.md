# 架构说明

React WebView 仅负责展示与输入，通过由 Rust 模型生成的 TypeScript IPC 类型调用 Tauri commands/events。网络、Keychain、SQLite、本地文件和内置 sidecar 只由 Rust 访问。

- `src/features`：连接、终端、文件、监控、RDP、设置和帮助 UI。
- `src-tauri/src/ssh.rs`：SSH 握手、指纹、认证、Transport Pool、keepalive 与 PTY 生命周期。
- `src-tauri/src/sftp.rs`：SFTP 操作、后台传输、原子文件替换和远端预览。
- `src-tauri/src/monitor.rs`：无侵入 Linux 采集、解析和系统信息导出。
- `src-tauri/src/tunnel.rs`：本地、远程和动态转发。
- `src-tauri/src/backup.rs`：非敏感备份或 Argon2id/AES-256-GCM 加密凭据备份。
- `src-tauri/src/task.rs`：诊断、归档和预览共用的可取消后台任务模型。
- `src-tauri/src/rdp.rs`：进程隔离的内置 FreeRDP SDL sidecar 适配。

SFTP、监控、传输、压缩和预览优先租用同一连接配置下的空闲已认证 Transport；并发繁忙时池自动创建附加 Transport，配置、代理或主机身份变化时使旧池失效。终端 PTY 与隧道使用不可复用的独占 Transport，避免 `libssh2::Session` 互斥锁让交互输入被后台 Channel 阻塞。

连接诊断、压缩/解压和远端预览通过统一后台任务模型立即返回任务 ID，再以事件发送状态；传输使用可持久化的专用队列。SQLite 使用 WAL，数据库只保存 Keychain 引用。密码、私钥口令、代理密码和 security-scoped Bookmark 均由连接专属 Keychain 条目管理。

RDP 不进入 SSH Transport Pool。Rust 优先启动应用资源目录内签名的 universal FreeRDP sidecar，密码仅写入子进程 stdin，sidecar 退出通过终端状态事件反馈给共享标签工作区。环境变量和系统 FreeRDP 检测仅用于开发覆盖，不属于用户安装流程。
## Mosh 终端

Mosh 会话先复用 CNshell 的 SSH 认证、代理和主机指纹校验启动远端 `mosh-server`，随后由 Rust `MoshManager` 在原生 PTY 中托管应用内 `mosh-client`。UDP 客户端使用连接配置中的公网主机名或地址，避免云主机 NAT 环境把 SSH 服务端的内网网卡地址误作目标。一次性 `MOSH_KEY` 仅通过子进程环境变量传递，不写入参数、数据库、日志或前端事件；终端输出继续走统一的 `terminal-output` IPC，SFTP 与监控使用相同连接资料建立独立 SSH 通道。

## SSH Certificate

SSH Certificate 连接将私钥与 OpenSSH 用户证书作为一组配置。两份文件分别保存只读 security-scoped Bookmark，私钥口令仍只进入连接专属 Keychain 条目。Rust 使用固定 `/usr/bin/ssh-keygen -L` 在 `LC_ALL=C`、`TZ=UTC` 的最小环境中解析证书元数据与有效期，并在每次认证前复核；实际认证通过 libssh2 的 public-key-from-file 接口同时传入证书公钥与对应私钥。

## X11 转发

CNshell 对 `ssh2` 0.9.6 保留一个最小本地补丁，仅公开底层 libssh2 已有的 `x11-req` 与入站 X11 channel callback，补丁源码和 MIT/Apache-2.0 许可证位于 `src-tauri/vendor/ssh2`。应用层只连接当前 Mac 的 Unix socket 或 loopback display，从 XQuartz `xauth` 读取真实 MIT cookie，向远端发送随机一次性假 cookie，并在入站 X11 setup 首包严格验证后原位替换；普通终端、认证、代理和主机指纹仍使用同一 CNshell Session。
