# CNshell Plugin SDK v1

CNshell 插件 v1 是签名的 WebAssembly 模块。SDK 提供少量有界宿主接口，所有输入都必须由
用户在本次运行中明确选择。运行时不提供 WASI、通用网络、文件系统、原始键盘输入或
Keychain 读取接口。原生 sidecar 不属于 v1 插件格式，不会被加载。

## 发布者根

发布者向用户提供独立的 JSON 公钥文件。用户应通过网站、代码仓库或线下渠道核对指纹后，
在“设置 > 插件信任与沙箱”中明确导入。

```json
{
  "schemaVersion": 1,
  "publisherId": "com.example",
  "name": "Example Publisher",
  "publicKey": "ed25519:<32-byte-base64url-without-padding>"
}
```

- `publisherId` 使用反向域名形式。
- 同一 ID 不能静默替换为另一把公钥；密钥轮换必须先撤销旧根，再导入新根。
- 撤销发布者会立即禁用该发布者的全部插件。
- 公钥不是秘密，CNshell 只保存公钥和 SHA-256 指纹，不保存发布者私钥。

## Manifest

`manifest.json` 与入口 `.wasm` 必须在同一目录树内，且均为非符号链接普通文件。

```json
{
  "id": "com.example.status",
  "name": "Status",
  "version": "1.0.0",
  "apiVersion": 1,
  "entrypoint": "plugin.wasm",
  "permissions": ["ui"],
  "networkDomains": [],
  "publisher": "com.example",
  "signature": "ed25519:<64-byte-base64url-without-padding>"
}
```

插件 ID 必须等于发布者 ID，或以 `<publisherId>.` 开头。入口必须是相对 `.wasm` 路径，
不能包含父目录跳转。manifest 最大 256 KB，WASM 最大 16 MB。

## 签名载荷

1. 计算入口文件 SHA-256，格式为 `sha256:<lowercase-hex>`。
2. 将 manifest 的 `signature` 设为 `null`。
3. 构造以下对象，并按 RFC 8785 JSON Canonicalization Scheme 序列化为 UTF-8：

```json
{
  "schemaVersion": 1,
  "manifest": { "signature": null },
  "entrypointSha256": "sha256:<lowercase-hex>"
}
```

示例中的 `manifest` 代表完整 manifest，不是只含一个字段。使用发布者 Ed25519 私钥签署
规范化字节，将 64 字节签名编码为无填充 Base64URL，并加 `ed25519:` 前缀。

CNshell 登记时固定 manifest 与 WASM 摘要，启用前和每次运行前都会重新读取并验证签名、
发布者根、版本、权限和两个摘要。任何变化都会禁用并失效该登记，必须重新检查和登记。

## ABI 与沙箱

入口模块必须导出：

```text
cnshell_main: () -> i32
```

返回值作为插件状态码展示，不会被当作 shell 命令。运行时只接受 `cnshell_v1` 模块中的
下列导入，其他导入（包括 `wasi_snapshot_preview1`）全部拒绝。返回值为非负长度/成功码，
`-1` 表示内存或 UTF-8 参数无效，`-2` 表示权限不足，`-3` 表示调用超过容量限制。

| 导入 | 签名 | 权限 | 边界 |
| --- | --- | --- | --- |
| `log` | `(ptr: i32, len: i32) -> i32` | `ui` | 单条 8 KB、最多 32 条、总计 32 KB；只显示在运行结果，不默认写系统日志 |
| `connection_metadata_len` | `() -> i32` | `connectionMetadata` | 返回本次用户所选连接的脱敏 JSON 长度 |
| `connection_metadata_read` | `(ptr: i32, capacity: i32) -> i32` | `connectionMetadata` | 最多 16 KB；不含备注、启动命令、环境变量、代理、密钥/证书路径或凭据 |
| `terminal_selection_len` | `() -> i32` | `terminalRead` | 只读取用户本次明确选中的终端文本 |
| `terminal_selection_read` | `(ptr: i32, capacity: i32) -> i32` | `terminalRead` | 最多 64 KB；不提供滚动缓冲区或实时键盘流 |
| `credential_proxy_connection_test` | `() -> i32` | `credentialProxy` | 只创建一次 `connectionTest` 确认请求，不向 WASM 返回凭据或诊断结果 |

连接元数据 JSON 当前只含 `id`、`name`、`protocol`、`host`、`port`、`username`、`tags`、
`encoding` 与 `hasCredential`。声明 `connectionMetadata` 或 `credentialProxy` 的插件运行前必须
由用户选择连接；声明 `terminalRead` 时必须先在当前终端选中文本。

凭据代理请求两分钟过期且只能使用一次。用户在 CNshell 中确认后，后端才使用当前 Keychain
凭据执行现有 SSH 连接诊断。插件拿不到密码、私钥、可复用令牌或诊断正文；禁用插件、摘要
变化或权限失效后，待确认请求也会被拒绝。

每次调用使用新的 Wasmi 1.1.0 Store，并应用以下限制：

- 10,000,000 执行燃料；耗尽后立即 trap。
- 单线性内存最多 32 MB，最多一块内存。
- 最多一个表、1,024 个表元素和一个实例。
- 递归深度 64、WASM 栈高度 64 KiB。
- 不复用实例，不在后台常驻，不提供原生 sidecar 回退。

当前可进入启用流程的权限是 `ui`、`connectionMetadata`、`terminalRead` 与
`credentialProxy`；用户仍需在启用时整体确认，并在每次运行时明确提供数据范围。`network`、
`directory` 和 `terminalInput` 继续阻止启用，直到分别完成域名、用户选择目录和受控输入的
独立能力设计，不能通过通用系统调用绕过。
