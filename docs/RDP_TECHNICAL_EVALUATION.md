# RDP 深度集成技术评估

## 结论

CNshell v1.5 采用独立 FreeRDP 窗口深度联动，不在当前架构中把 RDP 画面强行嵌入 Tauri WebView。该路线保留现有 sidecar 崩溃隔离和 SDL 原生输入链路，同时由 CNshell 管理连接状态、窗口位置、聚焦/隐藏、关闭、显示器、缩放、画质、音频、麦克风、剪贴板和目录映射。

这不是把“内嵌”改名为完成。IOSurface/Metal 真正嵌入仍是后续可选架构升级；本阶段选择的是总规划明确允许的第三条路线。

## 三条路线对比

| 维度 | 共享内存 / IOSurface + Metal | 原生 NSView / Metal 子视图 | 独立 SDL 窗口深度联动 |
| --- | --- | --- | --- |
| 帧路径 | FreeRDP 帧需跨进程共享；普通 WebView IPC 会产生额外复制，必须引入 IOSurface 才可接受 | 可直接渲染到原生层，但需要在 WKWebView 层级中维护子视图和裁剪 | 现有 SDL/Metal 原生路径，无新增帧复制 |
| 键鼠与 IME | CNshell 必须重新转发输入、组合文本、抓键和相对鼠标 | WKWebView 与 NSView 焦点、中文 IME、全屏和快捷键需自行协调 | SDL 已处理键鼠、IME、全屏和动态分辨率 |
| 剪贴板与重定向 | 需自行桥接 RDP channel 与 WebView 权限 | 仍需维护原生与前端状态同步 | 直接使用 FreeRDP channel，并由受控参数开关 |
| 多显示器 | 需实现纹理/窗口跨屏与 DPI 映射 | 需处理原生子视图跨屏和缩放 | 使用 FreeRDP `/list:monitor`、`/monitors` 和全屏实现 |
| 可访问性 | 画面本身仍是像素流，需要另建语义层 | 同样无法自动获得远端控件语义 | 与现有 RDP 客户端一致；CNshell 控制面保留可访问语义 |
| 崩溃隔离 | sidecar 可保留，但跨进程纹理协议复杂 | 若链接 FreeRDP 到主进程，崩溃会拖垮 CNshell | sidecar 异常不会终止 CNshell |
| 升级成本 | 每次 FreeRDP/Metal/IOSurface 变更都需维护私有桥接 | 与 Tauri/WKWebView/AppKit 和 FreeRDP ABI 同时耦合 | FreeRDP 参数和一个稳定状态标记补丁，升级面最小 |
| 当前可验证性 | 没有 Windows 主机，无法证明真实帧率、输入和重连 | 同样缺少 Windows 画面验收 | 可先完成进程、参数、窗口和错误状态自动化；画面验收留给真机 |

## 实现边界

- FreeRDP 3.28.0、OpenSSL、SDL3、SDL_ttf 与 FreeType 继续按固定版本和 SHA-256 构建 universal sidecar。
- CNshell 的补丁只做两件事：用户关闭 SDL 窗口时返回正常退出；`postConnect` 成功后写入固定、不含主机或凭据的 `CNSHELL_RDP_STATE=online` 日志标记。
- 密码和所有 FreeRDP 参数仅通过 `/args-from:stdin` 传递。诊断最多保留内存中的最后 64 KB，未知错误只显示退出状态，不回显原始日志。
- 独立窗口启动位置跟随 CNshell 主窗口偏移；标签切换可激活窗口，也可隐藏窗口。窗口操作使用 `NSRunningApplication`，不申请辅助功能或全局键盘监听权限。
- 剪贴板默认开启；音频、麦克风和本地目录映射默认关闭。目录映射仅允许用户选择的一个绝对目录，使用读写 security-scoped Bookmark，并在 sidecar 生命周期内持有授权。
- 全屏显示器编号来自同一 FreeRDP helper 的 `/list:monitor` 输出，不把 CoreGraphics ID 猜测为 FreeRDP ID。

## 自动化证据

- 参数测试覆盖动态分辨率、缩放适应、全屏、显示器、四档网络画质、剪贴板、音频、麦克风和目录映射，且断言密码不进入参数数组。
- 显示器解析限制 16 项、名称 256 字符和两位编号；目录拒绝逗号、控制字符、相对路径和不存在路径。
- 状态解析区分 `connecting`、固定在线标记、自动重连和关闭/失败；手动关闭、认证失败和传输失败分别分类。
- helper 仍由受管子进程承载，关闭失败不会丢失进程跟踪，应用销毁时会清理所有子进程。

## 外部验收

以下项目必须有 Windows 10/11 和 Windows Server 真机证据后才能标记通过：

- 首帧时间、持续帧率、输入延迟和弱网重连时间。
- 中文 IME、快捷键抓取、相对鼠标、剪贴板文本/文件方向。
- 动态分辨率、缩放、指定显示器、全屏和多显示器坐标。
- 本机/远端音频、麦克风授权、目录读写和断开后的资源清理。
- 注销、服务端关闭、断网、错误密码、账户锁定和 helper 崩溃。

如果未来具备 Windows 自动化主机并且 IOSurface 原型同时满足无额外帧复制、中文 IME、动态分辨率、崩溃隔离和可维护升级门槛，再重新评估真正内嵌。
