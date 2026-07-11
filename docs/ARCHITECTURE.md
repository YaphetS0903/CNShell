# 架构说明

React WebView 仅负责展示与输入，通过由 Rust 模型生成的 TypeScript IPC 类型调用 Tauri commands/events。网络、Keychain、SQLite、本地文件和外部 Helper 只由 Rust 访问。

- `src/features`：连接、终端、文件、监控、RDP、设置和帮助 UI。
- `src-tauri/src/ssh.rs`：SSH 握手、指纹、认证、Transport Pool、keepalive 与 PTY 生命周期。
- `src-tauri/src/sftp.rs`：SFTP 操作、后台传输、原子文件替换和远端预览。
- `src-tauri/src/monitor.rs`：无侵入 Linux 采集、解析和系统信息导出。
- `src-tauri/src/tunnel.rs`：本地、远程和动态转发。
- `src-tauri/src/backup.rs`：非敏感备份或 Argon2id/AES-256-GCM 加密凭据备份。
- `src-tauri/src/task.rs`：诊断、归档和预览共用的可取消后台任务模型。
- `src-tauri/src/rdp.rs`：进程隔离的外部 FreeRDP Helper 适配。

SFTP、监控、传输、压缩和预览优先租用同一连接配置下的空闲已认证 Transport；并发繁忙时池自动创建附加 Transport，配置、代理或主机身份变化时使旧池失效。终端 PTY 与隧道使用不可复用的独占 Transport，避免 `libssh2::Session` 互斥锁让交互输入被后台 Channel 阻塞。

连接诊断、压缩/解压和远端预览通过统一后台任务模型立即返回任务 ID，再以事件发送状态；传输使用可持久化的专用队列。SQLite 使用 WAL，数据库只保存 Keychain 引用。密码、私钥口令、代理密码和 security-scoped Bookmark 均由连接专属 Keychain 条目管理。

RDP 不进入 SSH Transport Pool。Rust 进程检测并管理用户安装的 FreeRDP Helper，密码仅写入子进程 stdin，Helper 退出通过终端状态事件反馈给共享标签工作区。
