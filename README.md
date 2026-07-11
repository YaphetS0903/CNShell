# CNshell

CNshell 是面向 macOS 的 SSH、SFTP、Linux 监控与 RDP 启动工作区。它使用 Tauri 2、React、Rust、libssh2、SQLite 与 macOS Keychain 构建。

## 开发

要求 macOS 13+、Xcode Command Line Tools、Node.js 20+、Rust stable。

```bash
npm install
npm run tauri dev
```

质量检查：

```bash
npm run check
```

本地打包：

```bash
npm run tauri build -- --bundles app,dmg
```

## 功能

- SSH 密码、私钥、SSH Agent 与严格 SHA-256 主机指纹确认；私钥通过 macOS security-scoped Bookmark 持久授权
- 多标签 xterm 终端、搜索、True Color、IME、快捷命令和脱敏历史
- SFTP 文件管理、原子文本保存、上传下载、暂停/恢复/重试；后台任务可查询、取消并报告进度
- CPU、内存、Swap、网络、进程、磁盘与 Linux 系统信息
- SOCKS5、HTTP CONNECT、SSH 跳板机、本地/远程/动态端口转发
- 本地连接库、Keychain 凭据、加密备份与脱敏诊断
- SFTP、监控和文件任务复用受控 SSH Transport Pool，终端与隧道使用独占连接
- 受管外部 FreeRDP Helper 生命周期；密码仅经标准输入传递，不进入参数或环境变量，且不随应用捆绑 GPL 组件

安装、升级和卸载见 [docs/INSTALLATION.md](docs/INSTALLATION.md)，用户说明见 [docs/USER_GUIDE.md](docs/USER_GUIDE.md)，版本变化见 [CHANGELOG.md](CHANGELOG.md)。架构、安全、快捷键和故障排查文档位于 `docs/`。
