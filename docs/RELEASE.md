# 发布与公证

## 前置条件

- Apple Developer ID Application 证书
- App Store Connect API Issuer、Key ID 与私钥
- Tauri updater 私钥/公钥及 HTTPS 更新服务
- Intel 构建目标：`rustup target add x86_64-apple-darwin`

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

脚本执行 lint、前端/Rust/E2E 测试、依赖审计、universal 构建，并通过 Tauri 使用 Apple API 凭据完成签名/公证。构建后还会从 `Info.plist` 读取真实可执行文件名，校验 Developer ID 身份、严格签名、Gatekeeper、arm64 + x86_64、最低 macOS 13、DMG 完整性、App/DMG 公证票据，以及非空 updater 归档和签名。

真实 SSH/SFTP 协议测试须在发布前独立执行并记录到 `docs/ACCEPTANCE.md`。耐久测试沿用已经验收的约 2 小时 50 分钟结果，不在发布脚本中重复执行。正式发布前还必须在没有开发环境的 Ventura、Sonoma 和 Sequoia Mac 上验证安装、更新与卸载。

GitHub Actions 的 `CI` 工作流在提交和 PR 上运行短时质量、Rust、WebKit E2E、本机 PTY 夹具与 universal App 烟雾构建，不自动运行 1 GB 协议测试或耐久测试。`Signed Release Candidate` 只能手动触发，并要求受保护的 `release` environment 及以下 secrets：

- `TAURI_RELEASE_CONFIG`
- `APPLE_SIGNING_IDENTITY`
- `APPLE_API_ISSUER`
- `APPLE_API_KEY`
- `APPLE_API_KEY_CONTENT`
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

工作流只上传签名候选产物，不自动创建公开 GitHub Release；发布负责人仍需核对验收矩阵后决定是否公开。
