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

## 自动化证据

`services/team-relay/tests/relay_flow.rs` 使用真实 HTTP 和 WebSocket loopback 启动两个账号与两台
设备，覆盖注册、邀请、服务端 RBAC、设备 challenge、房间加入、签名密文、断线补帧、控制
租约、输入定向、重复拒绝、成员移除、epoch 推进和 token 失效。

服务端独立门禁：

```bash
cargo clippy --manifest-path services/team-relay/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path services/team-relay/Cargo.toml
```

## 尚未构成的外部验收

本地服务代码和 loopback HTTP/WebSocket 协议测试不等于已经上线正式服务。公开交付仍需要：

1. 正式域名、TLS 证书和只允许 `wss://`/`https://` 的反向代理。
2. 代理层登录/注册速率限制、邮件投递与邮箱验证、防滥用和告警。
3. 加密卷、自动备份、恢复演练、日志保留、监控和事故响应。
4. 至少两台真实设备跨网络完成观看、控制移交、断网恢复和撤销传播验收。
5. 客户端在线账号、工作区同步与多人终端 UI 接入；在完成前默认入口保持关闭。
