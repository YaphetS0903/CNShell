# CNshell Plugin SDK v1

CNshell 插件 v1 是签名的 WebAssembly 模块。当前 SDK 只开放确定性计算入口，不提供
WASI、网络、文件、终端、连接资料或 Keychain 宿主接口。原生 sidecar 不属于 v1 插件格式，
不会被加载。

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

返回值作为插件状态码展示，不会被当作 shell 命令。当前运行时拒绝所有导入，包括
`wasi_snapshot_preview1`。因此插件不能读取环境变量、参数、时钟、随机源、网络、文件、
终端或凭据。

每次调用使用新的 Wasmi 1.1.0 Store，并应用以下限制：

- 10,000,000 执行燃料；耗尽后立即 trap。
- 单线性内存最多 32 MB，最多一块内存。
- 最多一个表、1,024 个表元素和一个实例。
- 递归深度 64、WASM 栈高度 64 KiB。
- 不复用实例，不在后台常驻，不提供原生 sidecar 回退。

当前只有 `ui` 可进入启用流程，但 v1 尚未提供绘制或宿主回调；其余声明会显示为“当前未
开放”并阻止启用。后续增加终端只读、连接元数据、网络域名、目录或凭据代理时，必须分别
定义有界 ABI、重新确认权限并新增故障注入测试，不能通过通用系统调用绕过。
