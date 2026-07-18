# 发布与公证

## 前置条件

- Apple Developer ID Application 证书
- App Store Connect API Issuer、Key ID 与私钥
- Tauri updater 私钥/公钥及 HTTPS 更新服务
- Intel 构建目标：`rustup target add x86_64-apple-darwin`
- Windows x64/ARM64 使用 GitHub `windows-2025` runner、MSVC、CMake、vcpkg 与 NSIS；首个 Beta 可以没有 Authenticode 证书，但不能省略 updater 签名和 SHA-256。

从 `src-tauri/tauri.release.example.json` 创建不入库的 `src-tauri/tauri.release.json`，写入正式 updater HTTPS endpoint 与 public key，禁止保留 `.example` 或 `REPLACE_` 占位符。开发用 `tauri.conf.json` 保持空 endpoint，避免候选包误连正式更新服务。

```bash
export APPLE_SIGNING_IDENTITY="Developer ID Application: ..."
export APPLE_API_ISSUER="..."
export APPLE_API_KEY="..."
export APPLE_API_KEY_PATH="/secure/AuthKey_xxx.p8"
export TAURI_SIGNING_PRIVATE_KEY="..."
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="..."
./scripts/release.sh
```

脚本执行 lint、前端/Rust/E2E 测试、依赖审计、universal 构建，并通过 Tauri 使用 Apple API 凭据完成签名/公证。构建后还会从 `Info.plist` 读取真实可执行文件名，校验 Developer ID 身份、严格签名、Gatekeeper、arm64 + x86_64、最低 macOS 13、DMG 完整性和 App/DMG 公证票据。updater 不只检查归档和 `.sig` 非空：刚构建的 CNshell 可执行文件会使用与 Tauri updater 相同的 Base64/minisign 验证规则，确认归档、签名和 `tauri.release.json` 公钥真实匹配；无效公钥、篡改归档、签名错配、未启用 updater 产物或不安全 endpoint 都会阻断发布。

正式版本的 SQLite migration 必须保持向后兼容，只能增量新增表、索引或带兼容默认值/可空的列，禁止删除、重命名或改变旧字段语义。CNshell 允许旧版本忽略它不认识的更高 migration 版本，以便更新后回滚仍能打开原数据库；旧版本认识的 migration 仍逐个校验 checksum，任何历史 migration 被修改都会拒绝启动。每次迁移前仍会生成本地数据库备份。

真实 SSH/SFTP 协议测试须在发布前独立执行并记录到 `docs/ACCEPTANCE.md`。耐久测试沿用已经验收的约 2 小时 50 分钟结果，不在发布脚本中重复执行。正式发布前还必须在没有开发环境的 Ventura、Sonoma 和 Sequoia Mac 上验证安装、更新与卸载。

GitHub Actions 的 `CI` 工作流在提交和 PR 上运行短时质量、Rust、WebKit E2E、本机 PTY 夹具与 universal App 烟雾构建；`Windows Packaging` 另从固定源码生成 x64/ARM64 FreeRDP 和 NSIS，x64 执行 ConPTY、应用启动、安装、覆盖升级、卸载、数据保留与重装，ARM64 在没有原生 runner 时只执行编译、PE 架构和包结构门禁。工作流不自动运行 1 GB 协议测试或耐久测试。

所有 workflow 的第三方 Actions 均固定到完整 commit SHA；当前 `checkout 7.0.0`、`setup-node 7.0.0`、`upload-artifact 7.0.1`、`download-artifact 8.0.1` 与 `cache 4.3.0` 由 Dependabot 提供发布差异后合并审查。常规构建只有 `contents: read`；只有最终 Draft 汇总 job 临时取得 `contents: write`。checkout 使用 `persist-credentials: false`，后续 npm、Rust、脚本和第三方 Action 无法从 Git 配置取得仓库 token。Node 固定为 `20.20.2`，Rust 固定为 `1.96.0`。`Signed Cross-platform Release Candidate` 只能手动触发，并要求受保护的 `release` environment 及以下 secrets：

- `APPLE_CERTIFICATE_BASE64`（Developer ID Application `.p12` 的 Base64）
- `APPLE_CERTIFICATE_PASSWORD`
- `TAURI_RELEASE_CONFIG`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `APPLE_API_KEY_CONTENT`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

在 GitHub `release` environment 中另外设置非秘密变量 `UPDATER_DOWNLOAD_BASE_URL`，例如 `https://github.com/YaphetS0903/CNShell/releases/download`。汇总门禁会组合 `v<version>/<archive>`，拒绝非 HTTPS、带凭据、片段或文件名不匹配的地址，并生成同时覆盖 `darwin-aarch64`、`darwin-x86_64`、`windows-x86_64` 与 `windows-aarch64` 的 `latest.json`。

工作流把 `.p12` 导入随机密码保护的临时 Keychain，限制私钥供 `codesign` 使用，确认精确签名身份后才构建；无论前置步骤成功或失败，凭据清理步骤都会运行。证书、API 私钥、release 配置和临时 Keychain 全部删除且清理步骤成功后，才允许 Artifact Action 上传候选产物。Windows job 同样会在上传前删除私有 release 配置。FreeRDP 从固定哈希源码重建；macOS helper 使用同一 Developer ID、Hardened Runtime 和可信时间戳，Windows helper 与应用执行 PE 架构检查，x64 还执行运行烟雾测试。

CNshell 采用 Developer ID 站外分发，不以 Mac App Store 为目标。Hardened Runtime 显式开启；App Sandbox 保持关闭，因为本地 PTY、X11 Unix socket、Serial 设备和隔离 sidecar 是核心功能。用户文件仍只通过原生文件选择器和 security-scoped Bookmark 授权，RDP 麦克风重定向默认关闭且只在用户明确开启时触发系统权限提示。

汇总 job 会用 Windows x64 CNshell 验证 macOS、Windows x64、Windows ARM64 三个 updater 归档及其 `.sig`，再生成四平台 `latest.json`、`SHA256SUMS.txt`、第三方说明和对应源码附件。它只创建或更新 Draft Release，且拒绝覆盖同版本的公开 Release；发布负责人核对验收矩阵后才可人工公开。`latest.json` 仍必须部署到 `tauri.release.json` 配置的 endpoint，不得只上传 DMG/EXE 后宣称自动更新可用。

首个未做 Authenticode 的 Windows Beta 会触发 SmartScreen 信誉提示。发布页必须同时提供 SHA-256、仓库来源和该限制，不能指导用户关闭 SmartScreen 或系统安全功能。取得可信代码签名证书或云签名服务后，应同时签名主程序、FreeRDP helper 和 NSIS 安装包；Tauri updater minisign 不能替代 Authenticode，反之亦然。
