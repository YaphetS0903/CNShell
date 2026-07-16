# CNshell Team Relay 运维手册

> 适用范围：`services/team-relay` 单实例 SQLite 部署。
> 当前证据：本机 loopback 的健康检查、指标、优雅停机、明文测试及 Sigsum 验证后的官方
> `age v1.3.1` 加密备份/恢复功能演练已通过；GitHub Ubuntu 24.04 Docker/Compose smoke 也已
> 通过。邮箱验证代码与客户端入口已完成；仓库已提供生产 NGINX/Prometheus/Alertmanager
> 配置和容器 smoke。正式 DNS 证书、真实 SMTP/告警投递、生产 identity、加密卷和异地主机
> 恢复仍需部署环境验收。

## 1. 运行边界

Relay 不终止 TLS。公网请求必须先经过反向代理或负载均衡，并满足以下条件：

- 仅开放 TLS 1.2+ 的 `https://` 和 `wss://`，HTTP 强制跳转 HTTPS。
- 正确保留 WebSocket Upgrade，并限制连接数、注册/登录频率和 256 KiB 请求体。
- 不记录 `Authorization`、邀请令牌、请求体、WebSocket envelope 或 URL 查询参数。
- Relay 端口只绑定 loopback 或私有服务网络，不直接暴露到公网。
- SQLite 数据库位于加密持久卷，文件和备份目录只允许服务账户与备份账户访问。

运行时至少配置：

```text
CNSHELL_RELAY_DATABASE_URL=sqlite:///data/relay.sqlite?mode=rwc
CNSHELL_RELAY_BIND=0.0.0.0:8787
CNSHELL_RELAY_BEHIND_TLS_PROXY=1
CNSHELL_RELAY_SMTP_HOST=smtp.example.com
CNSHELL_RELAY_SMTP_PORT=465
CNSHELL_RELAY_SMTP_SECURITY=tls
CNSHELL_RELAY_SMTP_FROM='CNshell <relay@example.com>'
CNSHELL_RELAY_SMTP_USERNAME=relay@example.com
CNSHELL_RELAY_SMTP_PASSWORD_FILE=/run/secrets/cnshell-relay-smtp-password
RUST_LOG=cnshell_team_relay=info,tower_http=info
```

非 loopback 监听必须显式设置 `CNSHELL_RELAY_BEHIND_TLS_PROXY=1`。这只是防误配置确认，
不会替代真实 TLS、认证代理或防火墙。

非 loopback 监听还会在 SMTP 未配置时拒绝启动。`tls` 默认使用 465，`starttls` 默认使用 587；
不支持明文 SMTP。密码可由运行时秘密变量或小型普通文件提供，两者不能同时设置，推荐容器秘密
文件。`CNSHELL_RELAY_ALLOW_UNVERIFIED_ACCOUNTS=1` 会跳过邮箱验证，只用于本机/CI 容器 smoke，
生产配置和任务环境必须确认该变量不存在或为 `0`。

### 生产 Compose 模板

`services/team-relay/docker-compose.production.yml` 将 Relay、TLS/WSS 代理、代理指标导出、
Prometheus 和 Alertmanager 拆成五个非 root、只读根文件系统容器。镜像固定版本和 manifest
digest，详见 `services/team-relay/production/THIRD_PARTY.md`。Relay 的 `8787` 不发布到宿主机；
Prometheus/Alertmanager 只绑定 `127.0.0.1`，生产访问应再通过受控运维通道。

启动前必须在仓库外准备并通过绝对路径传入：

- 与 `CNSHELL_RELAY_PUBLIC_HOST` 完全一致的正式 DNS 和 full-chain TLS 证书/私钥；
- Relay UID/GID `10001` 可读的 SMTP 密码普通文件；
- 从 `production/alertmanager.example.yml` 复制并替换强制占位值的真实告警接收配置；
- SMTP host、from、username，以及按目标平台设置的保留周期和宿主端口。

NGINX 使用非特权 UID/GID `101`；TLS 私钥应以组只读方式授权该账户，不得为了容器读取改为
全局可读。配置先执行 `docker compose ... config --quiet`，再分别用容器内 `nginx -t`、
`promtool check config/rules` 和 `amtool check-config` 验证。仓库的 Linux smoke 会自动执行这些
门禁，并验证 HTTP 固定域名跳转、未知 Host 拒绝、TLS 代理、认证限速、公共运维 endpoint
隐藏、代理日志不含测试秘密及 Relay/NGINX 指标抓取：

```bash
npm run test:relay-production-config
```

该 smoke 使用临时自签名证书、不可投递 SMTP 地址和无外发接收器，结束后删除容器与卷；它
只证明配置机制，不证明生产证书、邮件、告警通知或公网 WSS 已经通过。

## 2. 存活、就绪与停机

| Endpoint | 含义 | 预期响应 |
| --- | --- | --- |
| `GET /health` | 进程存活，不访问数据库 | `200 {"status":"ok"}` |
| `GET /ready` | 服务可接流量，实际执行 SQLite `SELECT 1` | `200 {"status":"ready"}` |
| `GET /metrics` | Prometheus 低基数进程与服务指标 | `200 text/plain` |

数据库不可访问时 `/ready` 返回 `503`，而 `/health` 仍可为 `200`。负载均衡摘流和容器
健康检查应使用 `/ready`；进程守护的 liveness 可使用 `/health`。两个 endpoint 都不应经过
账号认证，但只能由代理、编排器和监控网络访问。`/metrics` 不包含账号、工作区、设备、房间
或原始 URL 标签，但仍应按内部运维端点限制访问。

服务处理 `SIGINT` 和 `SIGTERM`。停机时先停止接收新请求，通知活动协作 WebSocket 关闭，
再等待 HTTP 任务退出。容器或进程管理器至少保留 30 秒退出窗口，超时强杀只作为兜底。

## 3. 加密备份

备份脚本使用 SQLite `VACUUM INTO` 创建在线一致性快照，再执行 `quick_check`、
`foreign_key_check` 和 relay 核心表校验。最终载荷与 SHA-256 sidecar 在同一目录暂存后改名，
权限为 `0600`。

下载官方 release 时必须先验证 Sigsum proof，不能只核对与下载文件同源的 SHA-256。项目保存
了 age 官方 `SIGSUM.md` 中的两把发布公钥，并提供固定验证入口。先从受信任渠道安装 Go，
再通过 Go module checksum 获取固定验证器：

```bash
go install sigsum.org/sigsum-go/cmd/sigsum-verify@v0.13.1
CNSHELL_SIGSUM_VERIFY_BIN="$(go env GOPATH)/bin/sigsum-verify" \
  npm run verify:relay-age -- /absolute/path/to/verified-age
```

脚本默认验证 v1.3.1，使用验证器内置的 `sigsum-generic-2025-1` 生产策略；只有 proof 通过后
才检查精确归档清单并解包。输出目录必须不存在，任何失败都会清理脚本创建的目录。v1.2.1
仍作为显式兼容版本保留，其他版本必须先审查归档结构并修改脚本。验证器本身必须严格显示
`sigsum-verify (sigsum-go module) v0.13.1`。

生产环境必须安装 `age` 和 `sqlite3`，使用专门的离线身份生成公开 recipient：

```bash
age-keygen -o /secure/offline/cnshell-relay-backup.agekey
age-keygen -y /secure/offline/cnshell-relay-backup.agekey
```

私有 identity 不应放在 relay 主机。备份主机只保存上一步输出的公开 recipient：

```bash
CNSHELL_RELAY_AGE_RECIPIENT='age1...' \
CNSHELL_RELAY_BACKUP_RETENTION_COUNT=30 \
services/team-relay/scripts/backup.sh \
  /data/relay.sqlite /offsite-staging/cnshell-relay
```

输出文件为 `cnshell-relay-YYYYMMDDTHHMMSSZ.sqlite.age` 和对应 `.sha256`。默认保留最近
14 份；`CNSHELL_RELAY_BACKUP_RETENTION_COUNT` 可调整，设为 `0` 时不自动删除。清理逻辑只
处理严格匹配该时间戳格式的 relay 备份和其 sidecar，不触碰其他文件。

必须把加密文件和 sidecar 一起同步到独立故障域，并为对象存储开启版本保留或不可变策略。
同一主机、同一磁盘上的副本不算灾难恢复备份。公开 recipient 可以进入调度器环境，私有
identity 不可以。

SHA-256 sidecar 用于发现传输或存储损坏，不是发布者签名；`age` recipient 也是公开信息，
不能证明是谁生成了密文。生产恢复必须从访问审计正常的不可变存储取回文件，并把任务系统
预先记录的摘要与 sidecar 交叉核对，不能只信任与备份放在一起的 sidecar。

`CNSHELL_RELAY_ALLOW_PLAINTEXT_BACKUP=1` 只用于本地自动测试。生产任务不得设置该变量；
未配置 recipient 时脚本默认失败，不会静默降级为明文。

## 4. 恢复演练

恢复会写入全新的目标文件，不覆盖已有数据库。操作步骤：

1. 记录事件单、备份时间、SHA-256 和恢复目标，停止 relay 并确认进程已经退出。
2. 将当前 `relay.sqlite`、`relay.sqlite-wal` 和 `relay.sqlite-shm` 作为一组移到受限隔离目录，
   不要直接删除。
3. 将离线 `age` identity 临时挂载到恢复主机，权限设为 `0600`。
4. 执行恢复：

```bash
CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1 \
CNSHELL_RELAY_AGE_IDENTITY=/secure/restore/cnshell-relay-backup.agekey \
services/team-relay/scripts/restore.sh \
  /offsite/cnshell-relay-20260716T010103Z.sqlite.age \
  /data/relay.sqlite
```

5. 确认新文件为 `0600`，启动 relay，等待 `/ready` 返回 `200`，再进行账号登录、工作区读取、
   房间创建和审计读取的只读/低风险 smoke test。
6. 测试完成后卸载并清除恢复主机上的私有 identity，记录恢复点目标和实际恢复时间。

恢复脚本在解密前验证 SHA-256，解密后再次执行 SQLite 完整性、外键和 relay schema 检查。
它要求 `CNSHELL_RELAY_CONFIRM_SERVICE_STOPPED=1`，可通过 `CNSHELL_RELAY_PID_FILE` 再检查
指定 PID 已退出，并拒绝符号链接、未知文件名、缺失 sidecar 和已存在的目标。

每月至少执行一次隔离恢复演练；每次 schema 迁移和重大 relay 发布前再执行一次。演练必须
使用备份文件的副本和隔离数据库，不能在生产目标上试恢复。

本机代码演练命令：

```bash
npm run test:relay-ops
```

该命令通过显式明文测试开关覆盖拒绝路径、保留策略、篡改、恢复、健康检查和 SIGTERM，
不等价于生产 `age` identity、对象存储或异地主机演练。测试机提供 `age` 和 `age-keygen` 时，
可通过 `CNSHELL_RELAY_AGE_BIN` 与 `CNSHELL_RELAY_AGE_KEYGEN_BIN` 让同一脚本追加真实密文、
正确/错误 identity 和私钥权限分支。应优先使用上述 Sigsum 验证目录中的二进制；该功能分支
仍不能替代生产 identity 和异地恢复。

## 5. 监控与告警

至少采集并设置以下告警：

- `/ready` 连续失败、HTTP 5xx、进程重启次数和优雅停机超时。
- 活动 WebSocket 数、异常断开率、认证失败率和注册/登录限速命中率；不采集帧正文。
- SQLite 文件/卷剩余空间、I/O 错误、锁等待和备份持续时间。
- 最近成功加密备份时间、备份大小突变、sidecar 缺失、异地复制失败和月度恢复演练逾期。
- TLS 证书到期、WSS Upgrade 失败、反向代理 4xx/5xx 和邮件队列失败。

日志只保留请求 ID、状态码、耗时和服务端元数据。集中日志平台必须配置字段级删除规则，
禁止采集 Header、Body、完整 URL、终端密文 envelope 和数据库文件。

Relay 自带 `/metrics` 可直接提供 process up、数据库 ready、uptime、HTTP 2xx/4xx/5xx、
readiness 检查/失败和授权 WebSocket 总数/活动数。磁盘、备份、TLS、代理、邮件与宿主进程
重启指标仍需部署平台采集；仅存在 endpoint 不等于监控告警已经上线。

生产模板默认采集 Relay 与 NGINX exporter，并提供 target down、数据库 not ready、readiness
失败、5xx、进程重启和代理 down 规则。Alertmanager 配置是必填的仓库外文件，必须接入真实
值班渠道并人工验证 firing/resolved 两种通知。NGINX access log 包含状态码，可由集中日志平台
统计 429；开源 `stub_status` 不提供状态码分类，因此不能把 exporter 在线误当成限速告警完成。

## 6. 事故处理

| 事件 | 立即动作 | 恢复条件 |
| --- | --- | --- |
| `/ready` 失败 | 摘流，检查卷、权限、空间和 SQLite 错误；不要循环重启写坏卷 | 隔离副本完整性通过且 `/ready` 稳定 |
| 数据库损坏 | 停服并保全数据库及 WAL/SHM，复制后分析，从最近通过校验的备份恢复 | 登录、工作区、审计 smoke test 通过 |
| Relay 主机泄露 | 隔离主机，吊销受影响设备/成员，轮换代理和邮件凭据，保全元数据日志 | 新主机从可信备份恢复并完成安全复核 |
| 备份 identity 泄露 | 立即轮换 `age` identity/recipient，后续备份改用新 recipient，评估历史备份暴露 | 新备份和隔离恢复演练通过 |
| TLS/代理失效 | 停止公网流量，禁止绕过代理直连 8787 | TLS、WSS、限速和日志脱敏复测通过 |

数据库包含账号密码哈希、短期 token 哈希、组织目录、审计元数据和端到端密文。它不包含
房间密钥或终端明文，但泄露仍属于安全事件，不能因为内容加密而降低响应级别。

## 7. 发布前未完成项

- GitHub Ubuntu 24.04 Linux amd64 已构建并运行示例镜像与 Compose；这不替代目标生产主机、
  加密卷、反向代理和持久化平台验收。
- 官方 `age v1.3.1` 的 Sigsum release 验证和本机加密/解密功能演练已通过；尚无生产
  identity、加密卷、对象存储或异地主机恢复证据。
- 邮箱验证协议、SMTP 适配、客户端验证/重发入口及生产代理限速/监控配置已完成；正式
  DNS/TLS/WSS、真实 SMTP 投递/退信、真实 Alertmanager 通知和对象存储尚未部署。
- 尚未使用两台真实设备跨网络验证观看、控制移交、断网恢复与撤销传播。

以上项目必须在目标部署环境单独签字验收，不能由本机 loopback 演练替代。
