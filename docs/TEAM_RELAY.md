# CNshell 在线团队服务

## 架构边界

`services/team-relay` 是独立于 macOS 客户端的 Rust/Axum 服务。客户端继续负责 X25519 房间
密钥封装、AES-256-GCM 加解密和 Ed25519 帧签名；relay 只读取签名 envelope 的路由元数据，
不持有房间密钥，也没有终端正文、连接凭据、主机、用户名或本地路径字段。

```text
CNshell host                         CNshell participant
     |  signed encrypted invitation          |
     +----------------> relay ----------------+
     |                                        |
     |  signed AES-GCM output frame           |
     +----------------> relay ---------------->
     |                                        |
     |  signed leased input frame             |
     <---------------- relay <----------------+
```

## 已实现服务契约

- 账号注册与登录使用 Argon2id，账号会话为 10 分钟随机 opaque token，数据库只存 SHA-256
  域分离哈希。
- 设备会话为 15 分钟随机 opaque token；过期后通过两分钟、一次性的随机 challenge 和设备
  Ed25519 私钥签名刷新。成员移除或设备撤销会立即撤销所有关联 token。
- 工作区邀请为 24 小时一次性随机 token，只能由目标邮箱账号接受。加入、角色变化、成员
  移除和设备撤销都会推进 `keyEpoch` 并关闭旧 epoch 房间。
- 所有 REST 和 WebSocket 操作重新读取服务端成员、设备、角色、epoch 和会话状态，不信任
  客户端提交的角色。
- 房间邀请进入 relay 前校验主持设备、接收设备、epoch、Ed25519 签名、公钥/nonce/封装密钥
  长度和 128 KiB envelope 上限。
- 输出仅允许主持设备按严格序号发布；输入仅允许当前控制租约设备按严格序号发布。输入密文
  只路由给主持设备，其他只读参与者不会收到控制输入 envelope。
- 断线输出只保留密文，窗口为五分钟、512 帧和 4 MiB 三者中的最小值。窗口缺失时拒绝
  静默跳号并要求重新加入。
- 服务端审计只记录成员、动作、目标和时间，每工作区最多 4,096 条，不记录 envelope 正文。
- `/health` 只报告进程存活，`/ready` 实际查询 SQLite；SIGINT/SIGTERM 停止接收新流量并
  通知活动 WebSocket 关闭，然后等待服务任务退出。
- `/metrics` 输出 Prometheus 文本，只包含进程、数据库就绪、运行时长、HTTP 状态类别、
  readiness 结果和授权 WebSocket 总数/活动数；没有账号、工作区、设备、房间或原始 URL 标签。

## 自动化证据

`services/team-relay/tests/relay_flow.rs` 使用真实 HTTP 和 WebSocket loopback 启动两个账号与两台
设备，覆盖注册、邀请、服务端 RBAC、设备 challenge、房间加入、签名密文、断线补帧、成员
与租约握手恢复、控制输入定向、重复拒绝、参与者离开、租约撤销、成员移除、epoch 推进和
token 失效。客户端另有游标恢复和观看/控制 UI 自动测试。

运维演练另行覆盖默认拒绝明文备份、符号链接、限定保留、SHA-256 篡改、拒绝覆盖恢复、
SQLite 完整性、`/health`、`/ready`、`/metrics` 和 SIGTERM。另使用临时 `age`/`age-keygen`
完成真实密文、正确 identity 恢复、错误 identity 拒绝和宽权限私钥拒绝。下载归档记录了
SHA-256，但因环境没有 Sigsum 验证器，该二进制不构成供应链验证，也不把本机演练记录为
生产加密异地恢复通过。

服务端独立门禁：

```bash
cargo clippy --manifest-path services/team-relay/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path services/team-relay/Cargo.toml
npm run test:relay-ops
```

## macOS 客户端接入

客户端只接受 `https://` relay 地址；`http://` 仅允许 `localhost`、`127.0.0.1` 和 `::1`
自动化测试。请求禁用重定向，连接和总请求有超时，响应逐块读取且累计限制为 1 MiB，
未知长度的 chunked 响应不能绕过上限。

- 账号 token 和每工作区设备 token 仅保存到 `cn.cnshell.team-relay` Keychain 服务；SQLite
  只保存 endpoint、账号 ID/邮箱、会话到期时间、工作区绑定和最后同步时间。
- 设备 token 临近过期或丢失时，客户端使用既有 Ed25519 私钥签署服务端一次性 challenge，
  不要求重新输入账号密码。
- 本地工作区发布前只允许单一 Owner/本机设备，避免把旧的手工成员目录误当成在线组织。
  发布后成员、设备、角色和 `keyEpoch` 以服务端快照为准。
- 邀请接受在本机保存只含邀请 token 哈希和公钥的待处理身份；网络响应丢失时复用同一设备
  ID 和 Keychain 私钥重试。服务端只对同账号、同成员和完全相同设备公钥进行幂等恢复。
- 快照落库前校验数量、UUID、角色、状态、公钥格式、组合指纹，并固定比对本机既有公钥，
  异常快照不能替换与 Keychain 私钥配对的本机身份。
- 在线终端为每个活动房间维护一个受管 WebSocket；设备 token 到期前自动刷新并重连，`ready`
  游标用于丢弃服务端已经提交的待发帧并拒绝序号缺口。加密和入队按房间串行，待发队列限制
  512 帧/4 MiB，关闭信号独立于帧队列。
- 主持端只转发绑定 SSH 会话的原始输出；参与端收到补帧后在 Rust 中验签、解密和检查严格
  序号，再把明文事件交给 xterm。输入只有本机设备持有未过期控制租约时可用，仍由 Rust
  加密签名后发送；relay 不获得明文。
- WebSocket 在初始 `ready` 后发送服务端权威的已加入成员和当前控制租约快照，并在加入、
  离开、授权或撤销后广播更新。参与者主动离开会撤销其租约和房间访问，主持端关闭会终止
  所有 socket。

真实 loopback 客户端测试使用两个独立 SQLite/Keychain 身份完成账号注册、工作区发布、邀请
接受、角色和 epoch 同步，并删除设备 token 验证 challenge 自动刷新；同时检查客户端 SQLite
不含明文账号密码或 token。

## 尚未构成的外部验收

本地服务代码和 loopback HTTP/WebSocket 协议测试不等于已经上线正式服务。公开交付仍需要：

1. 正式域名、TLS 证书和只允许 `wss://`/`https://` 的反向代理。
2. 代理层登录/注册速率限制、邮件投递与邮箱验证、防滥用和告警。
3. 在加密卷、经受信任供应链获取的生产 `age` identity、异地存储和隔离恢复主机上执行自动
   备份与恢复演练，并接入日志保留、监控和事故响应。脚本、指标和 runbook 已完成，本机
   临时二进制功能演练不能替代此项。
4. 至少两台真实设备跨网络完成观看、控制移交、断网恢复和撤销传播验收。

客户端 REST/WebSocket 与观看/控制入口已经接通，但在以上生产条件和真机证据齐备前只用于
loopback 或用户自行部署的测试 relay，不标记为正式在线团队服务。

部署、备份、恢复、监控和事故处理步骤见 `docs/TEAM_RELAY_OPERATIONS.md`。
