import {
  Component,
  createContext,
  type ErrorInfo,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import {
  Activity,
  ChevronDown,
  ChevronRight,
  Circle,
  Code2,
  Command,
  Edit3,
  FileText,
  Folder,
  HardDrive,
  KeyRound,
  LayoutDashboard,
  Monitor,
  MoreHorizontal,
  Network,
  Plus,
  RefreshCw,
  Save,
  Search,
  Server,
  Settings,
  ShieldCheck,
  SplitSquareHorizontal,
  TerminalSquare,
  Trash2,
  UploadCloud,
  X,
  Zap
} from "lucide-react";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Terminal } from "@xterm/xterm";
import { createInitialAppSnapshot, groupConnections, hydrateAppSnapshot } from "../domain/appState";
import { createLocalWorkspaceStorage } from "../domain/storage";
import type {
  ConnectionProfile,
  ConnectionProtocol,
  JumpHostConfig,
  KeyMappingProfile,
  KeyMappingRule,
  QuickCommand,
  ScriptRecording,
  ScriptRecordingEvent,
  SessionStatus,
  SessionTab,
  TransferJob
} from "../domain/models";
import type {
  CredentialStatus,
  CredentialVaultStatus,
  HostKeyVerificationEvent,
  RelayInfo,
  SshSessionConfig,
  TunnelInfo,
  TunnelMode,
  UpdateStatus
} from "../shared/ipc";
import { terminalTheme } from "./terminalTheme";

const workspaceStorage = createLocalWorkspaceStorage();

interface TriggerEvent {
  id: string;
  sessionId: string;
  severity: "error" | "warning";
  message: string;
  createdAt: string;
}

interface TunnelDraft {
  mode: TunnelMode;
  bindHost: string;
  bindPort: string;
  targetHost: string;
  targetPort: string;
}

interface SafePasteReview {
  text: string;
  reasons: string[];
}

interface BulkCommandReview {
  command: string;
  targetSessionIds: string[];
}

interface ConnectionFormDraft {
  id?: string;
  name: string;
  group: string;
  protocol: ConnectionProtocol;
  host: string;
  port: string;
  username: string;
  authMethod: ConnectionProfile["authMethod"];
  color: string;
  tags: string;
}

interface QuickCommandFormDraft {
  id?: string;
  title: string;
  command: string;
  scope: QuickCommand["scope"];
}

interface RemoteOperationDraft {
  type: "mkdir" | "rename" | "delete";
  targetPath: string;
  value: string;
}

interface MetricHistoryPoint {
  at: string;
  cpu: number;
  memory: number;
  disk: number;
  network: number;
  processes: number;
}

type ZmodemMode = "idle" | "upload" | "download" | "detected";
type Language = "zh-CN" | "en-US";
type PanelFocusKey = "credentials" | "tunnels" | "zmodem" | "logs";

interface AppErrorBoundaryState {
  error?: Error;
}

const LANGUAGE_STORAGE_KEY = "cnshell.ui.language.v1";

const translations = {
  "zh-CN": {
    languageName: "中文",
    languageChinese: "中文",
    languageEnglish: "English",
    recoveredTitle: "CNshell 已从渲染错误中恢复",
    returnToWorkspace: "返回工作台",
    loadingWorkspace: "正在加载工作区",
    loadingWorkspaceDetail: "准备连接、终端和运维面板",
    settingsTitle: "偏好设置",
    settingsSubtitle: "界面语言会立即生效，并保存到本机。",
    settingsLanguage: "界面语言",
    close: "关闭",
    consoleSubtitle: "SSH 运维控制台",
    connectionManager: "连接管理",
    connectionActions: "连接操作",
    searchConnections: "搜索连接",
    searchHostsPlaceholder: "搜索主机、标签、分组",
    newConnection: "新建",
    editConnection: "编辑连接",
    deleteConnection: "删除连接",
    connectionEditor: "连接配置",
    connectionEditorSubtitle: "保存后会立即更新侧边栏和会话入口。",
    protocol: "协议",
    group: "分组",
    tags: "标签",
    tagsHint: "用逗号分隔",
    color: "颜色",
    saveConnection: "保存连接",
    createConnection: "创建连接",
    connectionNameRequired: "请输入连接名称。",
    connectionHostRequired: "请输入主机地址。",
    connectionPortInvalid: "端口必须是 1 到 65535。",
    connectionUserRequired: "请输入用户名。",
    noConnectionsFound: "没有匹配的连接",
    connectionSettings: "连接设置",
    expandGroup: "展开分组",
    collapseGroup: "折叠分组",
    groupAria: (group: string) => `${group} 分组`,
    localShell: "本地 Shell",
    workspace: "CNshell 工作区",
    operationsPanels: "运维面板",
    openCommandPalette: "打开命令面板",
    toggleSyncInput: "切换同步输入",
    toggleHighlightRules: "切换高亮规则",
    openTunnelingManager: "打开隧道管理",
    openCredentialVault: "打开凭据保险库",
    focusPanel: (panel: string) => `定位到${panel}`,
    sessionTabs: "会话标签",
    newSessionTab: "新建会话标签",
    closeSessionTab: "关闭会话标签",
    localProtocol: "本地",
    status: {
      connected: "已连接",
      connecting: "连接中",
      disconnected: "未连接",
      error: "错误"
    },
    severity: {
      error: "错误",
      warning: "警告"
    },
    mode: {
      idle: "空闲",
      upload: "上传",
      download: "下载",
      detected: "已检测"
    },
    tunnelMode: {
      local: "本地",
      remote: "远程",
      dynamic: "动态"
    },
    terminalWorkbench: "终端工作区",
    terminalStarting: "CNshell 终端会话正在启动",
    profileLabel: "配置",
    sessionExited: (code: number | null) => `会话已退出，退出码 ${code}。`,
    sshProfileSelected: "已选择 SSH 配置。请在 SSH 面板输入凭据，然后点击连接。",
    rdpProfileSelected: "已选择 RDP 配置。请使用 RDP 面板启动 Windows 远程桌面。",
    terminalSearchPlaceholder: "搜索",
    find: "查找",
    split: "分屏",
    unsplit: "合并",
    splitPaneEnabled: "真实分屏会话已开启",
    splitPaneHint: "右侧会启动独立会话，可同时执行不同命令。",
    reconnect: "重连",
    moreTerminalActions: "更多终端操作",
    terminalActions: "终端操作",
    clearTerminalHint: "清屏请使用 Ctrl+L 或映射规则。",
    openLogsPanel: "打开日志面板",
    openZmodemPanel: "打开 ZMODEM 面板",
    reviewPaste: "粘贴审查",
    paste: "粘贴",
    cancel: "取消",
    composePane: "命令草稿",
    composePlaceholder: "先草拟命令，再发送到一个或多个会话",
    send: "发送",
    riskyPasteLines: (count: number) => `${count} 行`,
    riskyPasteShell: "包含 Shell 链式执行或变量展开",
    riskyPasteDangerous: "高风险命令",
    sshCredentials: "SSH 凭据",
    sshLogin: "SSH 登录",
    savedCredentialAvailable: "已保存凭据可用",
    noSavedCredential: "无已保存凭据",
    encryptionUnavailable: "加密不可用",
    vault: "保险库",
    masterPassword: "主密码",
    systemKeyring: "系统密钥环",
    locked: "已锁定",
    unlocked: "已解锁",
    active: "已启用",
    enterMasterPassword: "输入主密码",
    newMasterPassword: "新主密码",
    enable: "启用",
    unlock: "解锁",
    lock: "锁定",
    disable: "停用",
    hostKeyChanged: "主机密钥已变化",
    unknownHostKey: "未知主机密钥",
    expectedFingerprint: (fingerprint: string) => `期望 ${fingerprint}`,
    trustAndReconnect: "信任并重连",
    password: "密码",
    sessionOnly: "仅本次会话",
    privateKey: "私钥",
    import: "导入",
    pastePrivateKey: "粘贴本次会话使用的 OpenSSH 私钥",
    passphrase: "私钥口令",
    encryptedPrivateKeys: "用于加密私钥",
    connect: "连接",
    saveCredential: "保存凭据",
    deleteSaved: "删除已保存",
    rdpConnection: "RDP 连接",
    openRemoteDesktop: "打开远程桌面",
    jumpHostProxy: "跳板机代理",
    jumpHosts: "跳板机",
    addJumpHost: "添加跳板机",
    directSshConnection: "直连 SSH",
    name: "名称",
    host: "主机",
    port: "端口",
    user: "用户",
    remove: "移除",
    remoteFiles: "远程文件",
    cwdSync: "目录同步",
    refreshRemoteFiles: "刷新远程文件",
    createRemoteDirectory: "新建目录",
    renameRemotePath: "重命名",
    deleteRemotePath: "删除",
    remoteName: "远程名称",
    remoteOperation: "远程文件操作",
    directoryName: "目录名称",
    newPathName: "新名称或路径",
    confirmDeleteRemotePath: (name: string) => `确认删除 ${name}？`,
    remotePathRequired: "请输入远程路径。",
    remoteNameRequired: "请输入名称。",
    remoteOperationCompleted: "远程操作已完成",
    remotePath: "远程路径",
    loadingRemoteDirectory: "正在加载远程目录...",
    localPath: "本地路径",
    upload: "上传",
    download: "下载",
    transferDirection: {
      upload: "上传",
      download: "下载"
    },
    zmodemTransfer: "ZMODEM 传输",
    zmodemNoSession: "未检测到 ZMODEM 会话",
    zmodemUploadFlow: "正在通过兼容 ZMODEM 的流程上传",
    zmodemDownloadFlow: "正在通过兼容 ZMODEM 的流程下载",
    zmodemUploadDetected: "远端 rz 正在等待，请使用 ZMODEM 面板上传。",
    zmodemDownloadDetected: "检测到远端 sz 传输，请使用 ZMODEM 面板下载。",
    zmodemActivityDetected: "检测到 ZMODEM 活动。",
    localFilePath: "本地文件路径",
    remoteFilePath: "远程文件路径",
    remoteEditor: "远程文件编辑器",
    editor: "编辑器",
    save: "保存",
    noFileSelected: "未选择文件",
    selectRemoteFile: "请从 SFTP 选择远程文件",
    serverMetrics: "服务器监控",
    monitor: "监控",
    refreshMetrics: "刷新监控指标",
    collectingMetrics: "正在采集远程指标...",
    metricProcesses: "进程",
    metricLabel: {
      CPU: "CPU",
      Memory: "内存",
      Disk: "磁盘",
      Ping: "延迟",
      Network: "网络",
      Processes: "进程"
    },
    quickCommands: "快捷命令",
    manageQuickCommands: "管理快捷命令",
    quickCommandManager: "快捷命令管理",
    quickCommandManagerSubtitle: "管理常用命令，保存后命令面板和右侧快捷区会立即同步。",
    newQuickCommand: "新建命令",
    editQuickCommand: "编辑命令",
    commandTitle: "命令名称",
    commandText: "命令内容",
    commandScope: "作用域",
    saveCommand: "保存命令",
    deleteCommand: "删除命令",
    commandTitleRequired: "请输入命令名称。",
    commandTextRequired: "请输入命令内容。",
    noQuickCommands: "暂无快捷命令",
    triggerEvents: "触发事件",
    triggers: "触发器",
    noTriggerEvents: "暂无触发事件",
    processManager: "进程管理",
    processes: "进程",
    refreshProcesses: "刷新进程",
    loadingProcesses: "正在加载进程列表...",
    noProcessData: "暂无进程数据",
    terminate: "结束",
    sshTunnels: "SSH 隧道",
    tunnels: "隧道",
    startTunnel: "启动隧道",
    tunnelModeAria: "隧道模式",
    remoteBind: "远程绑定",
    localBind: "本地绑定",
    remotePort: "远程端口",
    localPort: "本地端口",
    targetHost: "目标主机",
    socksTarget: "SOCKS 目标",
    targetPort: "目标端口",
    noActiveTunnels: "暂无活动隧道",
    stop: "停止",
    cnRelay: "CN 中继",
    startRelay: "启动中继",
    relayBind: "中继绑定",
    relayPort: "中继端口",
    intranetHost: "内网主机",
    noRelayTunnels: "暂无中继隧道",
    keyMappingProfiles: "按键映射配置",
    keyMap: "按键映射",
    addKeyMapping: "添加按键映射",
    customMapping: "自定义映射",
    keyMappingDescription: "按键映射描述",
    noKeyMappingProfile: "暂无按键映射配置",
    shortcutAria: (description: string) => `${description} 快捷键`,
    sendSequenceAria: (description: string) => `${description} 发送序列`,
    scriptRecorder: "脚本录制",
    scripts: "脚本",
    record: "录制",
    recording: "录制中",
    idle: "空闲",
    eventCount: (count: number) => `${count} 个事件`,
    noRecordedScripts: "暂无录制脚本",
    replay: "回放",
    logs: "日志",
    audit: "审计",
    errors: "错误报告",
    refreshSessionLog: "刷新会话日志",
    refreshAuditLog: "刷新审计日志",
    refreshErrorReports: "刷新错误报告",
    noMatchingLogLines: "没有匹配的日志行",
    noAuditEntries: "暂无审计记录",
    noErrorReports: "暂无错误报告",
    filterLogLines: "筛选日志行",
    loadingLogs: "正在加载日志",
    cloudSync: "云同步",
    export: "导出",
    ready: "就绪",
    exportingEncryptedSettings: "正在导出加密设置",
    importingEncryptedSettings: "正在导入加密设置",
    exportCanceled: "已取消导出",
    importCanceled: "已取消导入",
    openingKeyFile: "正在打开密钥文件",
    privateKeyImportCanceled: "已取消导入",
    privateKeyImported: (fileName: string) => `已导入 ${fileName}`,
    privateKeyFallbackName: "私钥",
    exportedPath: (path: string) => `已导出 ${path}`,
    importedPath: (path: string) => `已导入 ${path}`,
    updates: "更新",
    channel: "通道",
    check: "检查",
    installUpdate: "安装更新",
    confirmBulkCommand: "确认批量命令",
    bulkSessions: (count: number) => `${count} 个会话`,
    sendToAll: "发送全部",
    commandPalette: "命令面板",
    searchQuickCommands: "搜索快捷命令",
    scope: {
      global: "全局",
      group: "分组",
      connection: "连接"
    },
    builtInGroups: {
      Production: "生产环境",
      Staging: "预发布",
      Windows: "Windows",
      Local: "本地"
    },
    builtInNames: {
      "Local PowerShell": "本地 PowerShell",
      "Restart service": "重启服务",
      "Disk usage": "磁盘使用",
      "Nginx errors": "Nginx 错误",
      "Default Terminal": "默认终端",
      "Clear terminal": "清空终端",
      "Interrupt process": "中断进程",
      "Send EOF": "发送 EOF",
      "Custom mapping": "自定义映射"
    },
    tunnelStatus: {
      starting: "启动中",
      running: "运行中",
      stopped: "已停止",
      error: "错误"
    },
    updateState: {
      idle: "空闲",
      checking: "检查中",
      available: "有更新",
      "not-available": "无更新",
      downloading: "下载中",
      downloaded: "已下载",
      error: "错误"
    }
  },
  "en-US": {
    languageName: "English",
    languageChinese: "中文",
    languageEnglish: "English",
    recoveredTitle: "CNshell recovered from a renderer error",
    returnToWorkspace: "Return to workspace",
    loadingWorkspace: "Loading workspace",
    loadingWorkspaceDetail: "Preparing connections, terminals, and operations panels",
    settingsTitle: "Preferences",
    settingsSubtitle: "Interface language applies immediately and is saved on this device.",
    settingsLanguage: "Interface language",
    close: "Close",
    consoleSubtitle: "SSH Operations Console",
    connectionManager: "Connection manager",
    connectionActions: "Connection actions",
    searchConnections: "Search connections",
    searchHostsPlaceholder: "Search hosts, tags, groups",
    newConnection: "New",
    editConnection: "Edit connection",
    deleteConnection: "Delete connection",
    connectionEditor: "Connection profile",
    connectionEditorSubtitle: "Saved changes update the sidebar and session entry points immediately.",
    protocol: "Protocol",
    group: "Group",
    tags: "Tags",
    tagsHint: "Separate with commas",
    color: "Color",
    saveConnection: "Save connection",
    createConnection: "Create connection",
    connectionNameRequired: "Enter a connection name.",
    connectionHostRequired: "Enter a host.",
    connectionPortInvalid: "Port must be between 1 and 65535.",
    connectionUserRequired: "Enter a username.",
    noConnectionsFound: "No matching connections",
    connectionSettings: "Connection settings",
    expandGroup: "Expand group",
    collapseGroup: "Collapse group",
    groupAria: (group: string) => `${group} group`,
    localShell: "Local shell",
    workspace: "CNshell workspace",
    operationsPanels: "Operations panels",
    openCommandPalette: "Open command palette",
    toggleSyncInput: "Toggle synchronized input",
    toggleHighlightRules: "Toggle highlight rules",
    openTunnelingManager: "Open tunneling manager",
    openCredentialVault: "Open credential vault",
    focusPanel: (panel: string) => `Focus ${panel}`,
    sessionTabs: "Session tabs",
    newSessionTab: "Open new session tab",
    closeSessionTab: "Close session tab",
    localProtocol: "local",
    status: {
      connected: "connected",
      connecting: "connecting",
      disconnected: "disconnected",
      error: "error"
    },
    severity: {
      error: "error",
      warning: "warning"
    },
    mode: {
      idle: "idle",
      upload: "upload",
      download: "download",
      detected: "detected"
    },
    tunnelMode: {
      local: "Local",
      remote: "Remote",
      dynamic: "Dynamic"
    },
    terminalWorkbench: "Terminal workbench",
    terminalStarting: "CNshell terminal session starting",
    profileLabel: "Profile",
    sessionExited: (code: number | null) => `Session exited with code ${code}.`,
    sshProfileSelected: "SSH profile selected. Enter credentials in the SSH panel, then press Connect.",
    rdpProfileSelected: "RDP profile selected. Use the RDP panel to launch Windows Remote Desktop.",
    terminalSearchPlaceholder: "Search",
    find: "Find",
    split: "Split",
    unsplit: "Unsplit",
    splitPaneEnabled: "Real split session enabled",
    splitPaneHint: "The right pane starts an independent session for separate commands.",
    reconnect: "Reconnect",
    moreTerminalActions: "More terminal actions",
    terminalActions: "Terminal actions",
    clearTerminalHint: "Use Ctrl+L or a key mapping rule to clear the terminal.",
    openLogsPanel: "Open logs panel",
    openZmodemPanel: "Open ZMODEM panel",
    reviewPaste: "Review paste",
    paste: "Paste",
    cancel: "Cancel",
    composePane: "Compose Pane",
    composePlaceholder: "Draft a command before sending to one or many sessions",
    send: "Send",
    riskyPasteLines: (count: number) => `${count} lines`,
    riskyPasteShell: "shell chaining or expansion",
    riskyPasteDangerous: "high-risk command",
    sshCredentials: "SSH credentials",
    sshLogin: "SSH Login",
    savedCredentialAvailable: "Saved credential available",
    noSavedCredential: "No saved credential",
    encryptionUnavailable: "Encryption unavailable",
    vault: "Vault",
    masterPassword: "Master password",
    systemKeyring: "System keyring",
    locked: "locked",
    unlocked: "unlocked",
    active: "active",
    enterMasterPassword: "Enter master password",
    newMasterPassword: "New master password",
    enable: "Enable",
    unlock: "Unlock",
    lock: "Lock",
    disable: "Disable",
    hostKeyChanged: "Host key changed",
    unknownHostKey: "Unknown host key",
    expectedFingerprint: (fingerprint: string) => `Expected ${fingerprint}`,
    trustAndReconnect: "Trust and reconnect",
    password: "Password",
    sessionOnly: "Session only",
    privateKey: "Private key",
    import: "Import",
    pastePrivateKey: "Paste an OpenSSH private key for this session",
    passphrase: "Passphrase",
    encryptedPrivateKeys: "For encrypted private keys",
    connect: "Connect",
    saveCredential: "Save credential",
    deleteSaved: "Delete saved",
    rdpConnection: "RDP connection",
    openRemoteDesktop: "Open Remote Desktop",
    jumpHostProxy: "Jump host proxy",
    jumpHosts: "Jump Hosts",
    addJumpHost: "Add jump host",
    directSshConnection: "Direct SSH connection",
    name: "Name",
    host: "Host",
    port: "Port",
    user: "User",
    remove: "Remove",
    remoteFiles: "Remote files",
    cwdSync: "cwd sync",
    refreshRemoteFiles: "Refresh remote files",
    createRemoteDirectory: "New directory",
    renameRemotePath: "Rename",
    deleteRemotePath: "Delete",
    remoteName: "Remote name",
    remoteOperation: "Remote file operation",
    directoryName: "Directory name",
    newPathName: "New name or path",
    confirmDeleteRemotePath: (name: string) => `Delete ${name}?`,
    remotePathRequired: "Enter a remote path.",
    remoteNameRequired: "Enter a name.",
    remoteOperationCompleted: "Remote operation completed",
    remotePath: "Remote path",
    loadingRemoteDirectory: "Loading remote directory...",
    localPath: "Local path",
    upload: "Upload",
    download: "Download",
    transferDirection: {
      upload: "upload",
      download: "download"
    },
    zmodemTransfer: "ZMODEM transfer",
    zmodemNoSession: "No ZMODEM session detected",
    zmodemUploadFlow: "Uploading through ZMODEM-compatible transfer flow",
    zmodemDownloadFlow: "Downloading through ZMODEM-compatible transfer flow",
    zmodemUploadDetected: "Remote rz is waiting. Use the ZMODEM panel to upload.",
    zmodemDownloadDetected: "Remote sz transfer detected. Use the ZMODEM panel to download.",
    zmodemActivityDetected: "ZMODEM activity detected.",
    localFilePath: "Local file path",
    remoteFilePath: "Remote file path",
    remoteEditor: "Remote file editor",
    editor: "Editor",
    save: "Save",
    noFileSelected: "No file selected",
    selectRemoteFile: "Select a remote file from SFTP",
    serverMetrics: "Server metrics",
    monitor: "Monitor",
    refreshMetrics: "Refresh metrics",
    collectingMetrics: "Collecting remote metrics...",
    metricProcesses: "Processes",
    metricLabel: {
      CPU: "CPU",
      Memory: "Memory",
      Disk: "Disk",
      Ping: "Ping",
      Network: "Network",
      Processes: "Processes"
    },
    quickCommands: "Quick Commands",
    manageQuickCommands: "Manage quick commands",
    quickCommandManager: "Quick command manager",
    quickCommandManagerSubtitle: "Manage common commands. Saved changes sync with the command palette and quick panel.",
    newQuickCommand: "New command",
    editQuickCommand: "Edit command",
    commandTitle: "Command title",
    commandText: "Command",
    commandScope: "Scope",
    saveCommand: "Save command",
    deleteCommand: "Delete command",
    commandTitleRequired: "Enter a command title.",
    commandTextRequired: "Enter a command.",
    noQuickCommands: "No quick commands",
    triggerEvents: "Trigger events",
    triggers: "Triggers",
    noTriggerEvents: "No trigger events",
    processManager: "Process manager",
    processes: "Processes",
    refreshProcesses: "Refresh processes",
    loadingProcesses: "Loading process list...",
    noProcessData: "No process data",
    terminate: "Term",
    sshTunnels: "SSH tunnels",
    tunnels: "Tunnels",
    startTunnel: "Start tunnel",
    tunnelModeAria: "Tunnel mode",
    remoteBind: "Remote bind",
    localBind: "Local bind",
    remotePort: "Remote port",
    localPort: "Local port",
    targetHost: "Target host",
    socksTarget: "SOCKS target",
    targetPort: "Target port",
    noActiveTunnels: "No active tunnels",
    stop: "Stop",
    cnRelay: "CN Relay",
    startRelay: "Start relay",
    relayBind: "Relay bind",
    relayPort: "Relay port",
    intranetHost: "Intranet host",
    noRelayTunnels: "No relay tunnels",
    keyMappingProfiles: "Key mapping profiles",
    keyMap: "Key Map",
    addKeyMapping: "Add key mapping",
    customMapping: "Custom mapping",
    keyMappingDescription: "Key mapping description",
    noKeyMappingProfile: "No key mapping profile",
    shortcutAria: (description: string) => `${description} shortcut`,
    sendSequenceAria: (description: string) => `${description} send sequence`,
    scriptRecorder: "Script recorder",
    scripts: "Scripts",
    record: "Record",
    recording: "rec",
    idle: "idle",
    eventCount: (count: number) => `${count} events`,
    noRecordedScripts: "No recorded scripts",
    replay: "Replay",
    logs: "Logs",
    audit: "Audit",
    errors: "Errors",
    refreshSessionLog: "Refresh session log",
    refreshAuditLog: "Refresh audit log",
    refreshErrorReports: "Refresh error reports",
    noMatchingLogLines: "No matching log lines",
    noAuditEntries: "No audit entries",
    noErrorReports: "No error reports",
    filterLogLines: "Filter log lines",
    loadingLogs: "Loading logs",
    cloudSync: "Cloud Sync",
    export: "Export",
    ready: "Ready",
    exportingEncryptedSettings: "Exporting encrypted settings",
    importingEncryptedSettings: "Importing encrypted settings",
    exportCanceled: "Export canceled",
    importCanceled: "Import canceled",
    openingKeyFile: "Opening key file",
    privateKeyImportCanceled: "Import canceled",
    privateKeyImported: (fileName: string) => `Imported ${fileName}`,
    privateKeyFallbackName: "private key",
    exportedPath: (path: string) => `Exported ${path}`,
    importedPath: (path: string) => `Imported ${path}`,
    updates: "Updates",
    channel: "Channel",
    check: "Check",
    installUpdate: "Install update",
    confirmBulkCommand: "Confirm Bulk Command",
    bulkSessions: (count: number) => `${count} sessions`,
    sendToAll: "Send to all",
    commandPalette: "Command palette",
    searchQuickCommands: "Search quick commands",
    scope: {
      global: "global",
      group: "group",
      connection: "connection"
    },
    builtInGroups: {
      Production: "Production",
      Staging: "Staging",
      Windows: "Windows",
      Local: "Local"
    },
    builtInNames: {
      "Local PowerShell": "Local PowerShell",
      "Restart service": "Restart service",
      "Disk usage": "Disk usage",
      "Nginx errors": "Nginx errors",
      "Default Terminal": "Default Terminal",
      "Clear terminal": "Clear terminal",
      "Interrupt process": "Interrupt process",
      "Send EOF": "Send EOF",
      "Custom mapping": "Custom mapping"
    },
    tunnelStatus: {
      starting: "starting",
      running: "running",
      stopped: "stopped",
      error: "error"
    },
    updateState: {
      idle: "idle",
      checking: "checking",
      available: "available",
      "not-available": "not available",
      downloading: "downloading",
      downloaded: "downloaded",
      error: "error"
    }
  }
};

type UiStrings = (typeof translations)["zh-CN"];

function readPreferredLanguage(): Language {
  try {
    const storedLanguage = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
    return storedLanguage === "en-US" ? "en-US" : "zh-CN";
  } catch {
    return "zh-CN";
  }
}

function displayStatus(status: SessionStatus, labels: UiStrings) {
  return labels.status[status];
}

function displayMode(mode: ZmodemMode, labels: UiStrings) {
  return labels.mode[mode];
}

function displayMetricLabel(label: string, labels: UiStrings) {
  return labels.metricLabel[label as keyof UiStrings["metricLabel"]] ?? label;
}

function displayBuiltInGroup(group: string, labels: UiStrings) {
  return labels.builtInGroups[group as keyof UiStrings["builtInGroups"]] ?? group;
}

function displayBuiltInName(name: string, labels: UiStrings) {
  return labels.builtInNames[name as keyof UiStrings["builtInNames"]] ?? name;
}

const TranslationContext = createContext<UiStrings>(translations["en-US"]);

function useUiStrings() {
  return useContext(TranslationContext);
}

class AppErrorBoundary extends Component<{ children: ReactNode }, AppErrorBoundaryState> {
  state: AppErrorBoundaryState = {};

  static getDerivedStateFromError(error: Error): AppErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    void window.cnshell?.logs.reportRendererError({
      message: error.message,
      stack: error.stack,
      componentStack: info.componentStack ?? undefined
    });
  }

  render() {
    if (this.state.error) {
      return (
        <TranslationContext.Consumer>
          {(labels) => (
            <main className="app-shell loading-shell">
              <section className="workspace-loading error-boundary" role="alert">
                <div className="brand-mark" aria-hidden="true">
                  CN
                </div>
                <strong>{labels.recoveredTitle}</strong>
                <span>{this.state.error?.message}</span>
                <button type="button" onClick={() => this.setState({ error: undefined })}>
                  {labels.returnToWorkspace}
                </button>
              </section>
            </main>
          )}
        </TranslationContext.Consumer>
      );
    }

    return this.props.children;
  }
}

const tunnelModes: Array<{ value: TunnelMode }> = [
  { value: "local" },
  { value: "remote" },
  { value: "dynamic" }
];

const connectionColors = ["#2f9e44", "#1971c2", "#d9480f", "#7048e8", "#0ca678", "#e67700"];

const modifierKeys = new Set(["Alt", "Control", "Meta", "Shift"]);

function createDefaultConnectionDraft(): ConnectionFormDraft {
  return {
    name: "",
    group: "Staging",
    protocol: "ssh",
    host: "",
    port: "22",
    username: "",
    authMethod: "password",
    color: connectionColors[0],
    tags: ""
  };
}

function createConnectionDraft(connection: ConnectionProfile): ConnectionFormDraft {
  return {
    id: connection.id,
    name: connection.name,
    group: connection.group,
    protocol: connection.protocol,
    host: connection.host,
    port: String(connection.port),
    username: connection.username,
    authMethod: connection.authMethod,
    color: connection.color,
    tags: connection.tags.join(", ")
  };
}

function createDefaultQuickCommandDraft(): QuickCommandFormDraft {
  return {
    title: "",
    command: "",
    scope: "global"
  };
}

function createQuickCommandDraft(command: QuickCommand): QuickCommandFormDraft {
  return {
    id: command.id,
    title: command.title,
    command: command.command,
    scope: command.scope
  };
}

function normalizeTags(value: string) {
  return value
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean);
}

function connectionMatchesQuery(connection: ConnectionProfile, query: string, labels: UiStrings) {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    return true;
  }

  const haystack = [
    connection.name,
    displayBuiltInName(connection.name, labels),
    connection.group,
    displayBuiltInGroup(connection.group, labels),
    connection.host,
    connection.username,
    connection.protocol,
    ...connection.tags
  ]
    .join(" ")
    .toLowerCase();

  return haystack.includes(normalizedQuery);
}

function makeSessionForConnection(connection: ConnectionProfile): SessionTab {
  return {
    id: `tab-${connection.id}-${Date.now()}`,
    connectionId: connection.id,
    title: connection.name,
    cwd: connection.protocol === "local" ? "~" : "/",
    status: "disconnected",
    startedAt: new Date().toISOString()
  };
}

function parsePort(value: string) {
  const port = Number(value);
  return Number.isInteger(port) && port > 0 && port <= 65535 ? port : null;
}

function formatKeyEvent(event: KeyboardEvent) {
  const parts: string[] = [];

  if (event.ctrlKey) {
    parts.push("Ctrl");
  }

  if (event.altKey) {
    parts.push("Alt");
  }

  if (event.shiftKey) {
    parts.push("Shift");
  }

  if (event.metaKey) {
    parts.push("Meta");
  }

  if (!modifierKeys.has(event.key)) {
    const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
    parts.push(key);
  }

  return parts.join("+");
}

function normalizeSendValue(value: string) {
  return value.replaceAll("\\r", "\r").replaceAll("\\n", "\n").replaceAll("\\t", "\t").replaceAll("\\e", "\x1b");
}

function normalizeRemotePath(path: string) {
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (!part || part === ".") {
      continue;
    }

    if (part === "..") {
      parts.pop();
      continue;
    }

    parts.push(part);
  }

  return `/${parts.join("/")}`;
}

function parentRemotePath(path: string) {
  const normalized = normalizeRemotePath(path);
  const parts = normalized.split("/").filter(Boolean);
  parts.pop();
  return parts.length === 0 ? "/" : `/${parts.join("/")}`;
}

function inferDirectoryFromCommand(command: string, currentPath: string) {
  const trimmed = command.trim();
  const match = /^cd(?:\s+(.+))?$/.exec(trimmed);
  if (!match) {
    return null;
  }

  const rawPath = (match[1] ?? "~").trim().replace(/^['"]|['"]$/g, "");
  if (!rawPath || rawPath === "~") {
    return "/";
  }

  if (rawPath === "-") {
    return null;
  }

  if (rawPath.startsWith("/")) {
    return normalizeRemotePath(rawPath);
  }

  return normalizeRemotePath(`${currentPath.replace(/\/$/, "")}/${rawPath}`);
}

function getActiveKeyRules(profiles: KeyMappingProfile[]) {
  return profiles.flatMap((profile) => (profile.enabled ? profile.rules.filter((rule) => rule.enabled) : []));
}

function inspectPastedText(text: string, labels: UiStrings = translations["en-US"]) {
  const reasons: string[] = [];
  const trimmed = text.trim();
  const lines = trimmed.split(/\r?\n/).filter(Boolean);

  if (lines.length > 1) {
    reasons.push(labels.riskyPasteLines(lines.length));
  }

  if (/[;&|`$()]/.test(trimmed)) {
    reasons.push(labels.riskyPasteShell);
  }

  if (/\b(rm\s+-[^\n]*[rf]|mkfs|dd\s+if=|chmod\s+-R\s+777|chown\s+-R|shutdown|reboot|:(){:|sudo\s+rm)\b/i.test(trimmed)) {
    reasons.push(labels.riskyPasteDangerous);
  }

  return reasons;
}

function shouldReviewPaste(text: string) {
  return inspectPastedText(text).length > 0;
}

function metricValue(metrics: ReturnType<typeof createInitialAppSnapshot>["serverMetrics"], label: string) {
  return metrics.find((metric) => metric.label === label)?.value ?? 0;
}

function describeTunnel(tunnel: TunnelInfo) {
  const bind = `${tunnel.bindHost}:${tunnel.bindPort}`;

  if (tunnel.mode === "dynamic") {
    return `${bind} SOCKS5`;
  }

  return `${bind} -> ${tunnel.targetHost ?? "?"}:${tunnel.targetPort ?? "?"}`;
}

function createSshConfig(
  connection: ConnectionProfile,
  draft: { password: string; privateKey: string; passphrase: string },
  useSavedCredential: boolean
): SshSessionConfig {
  return {
    connectionId: connection.id,
    host: connection.host,
    port: connection.port,
    username: connection.username,
    password: draft.password || undefined,
    privateKey: draft.privateKey || undefined,
    passphrase: draft.passphrase || undefined,
    useSavedCredential,
    gateways: connection.gateways
  };
}

function applyHighlightRules(data: string) {
  return data
    .split(/(\r?\n)/)
    .map((part) => {
      if (/(\r?\n)/.test(part)) {
        return part;
      }

      if (/\b(error|failed|failure|fatal|denied)\b/i.test(part)) {
        return `\x1b[31m${part}\x1b[0m`;
      }

      if (/\b(warn|warning|retry|slow)\b/i.test(part)) {
        return `\x1b[33m${part}\x1b[0m`;
      }

      if (/\b(success|succeeded|ok|ready|done)\b/i.test(part)) {
        return `\x1b[32m${part}\x1b[0m`;
      }

      return part;
    })
    .join("");
}

function detectTriggerEvents(sessionId: string, data: string): TriggerEvent[] {
  return data
    .split(/\r?\n/)
    .filter((line) => /\b(error|failed|failure|fatal|denied|warning)\b/i.test(line))
    .slice(-3)
    .map((line) => ({
      id: `${sessionId}-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      sessionId,
      severity: /\b(warn|warning)\b/i.test(line) ? "warning" : "error",
      message: line.trim().slice(0, 220),
      createdAt: new Date().toLocaleTimeString()
    }));
}

function detectZmodemMode(data: string): ZmodemMode {
  if (/rz\s+(waiting|ready)|\*\*B0|ZRQINIT/i.test(data) || data.includes("\x18B0")) {
    return "upload";
  }

  if (/sz\s+(sending|ready)|ZFILE|\*\*B1/i.test(data) || data.includes("\x18B1")) {
    return "download";
  }

  if (/zmodem/i.test(data)) {
    return "detected";
  }

  return "idle";
}

export function App() {
  const [snapshot, setSnapshot] = useState(() => createInitialAppSnapshot());
  const [isWorkspaceReady, setIsWorkspaceReady] = useState(false);
  const [language, setLanguage] = useState<Language>(() => readPreferredLanguage());
  const labels = translations[language];
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [connectionQuery, setConnectionQuery] = useState("");
  const [collapsedGroups, setCollapsedGroups] = useState<Record<string, boolean>>({});
  const [connectionDraft, setConnectionDraft] = useState<ConnectionFormDraft | null>(null);
  const [connectionFormError, setConnectionFormError] = useState("");
  const [quickCommandDraft, setQuickCommandDraft] = useState<QuickCommandFormDraft | null>(null);
  const [quickCommandFormError, setQuickCommandFormError] = useState("");
  const [splitTabId, setSplitTabId] = useState("");
  const [activeConnectionId, setActiveConnectionId] = useState(snapshot.connections[0].id);
  const [activeTabId, setActiveTabId] = useState(snapshot.sessions[0].id);
  const [appVersion, setAppVersion] = useState("dev");
  const [sshDrafts, setSshDrafts] = useState<Record<string, { password: string; privateKey: string; passphrase: string }>>({});
  const [sessionStartTokens, setSessionStartTokens] = useState<Record<string, number>>({});
  const [hostKeyPrompts, setHostKeyPrompts] = useState<Record<string, HostKeyVerificationEvent>>({});
  const [credentialStatuses, setCredentialStatuses] = useState<Record<string, CredentialStatus>>({});
  const [credentialErrors, setCredentialErrors] = useState<Record<string, string>>({});
  const [credentialVaultStatus, setCredentialVaultStatus] = useState<CredentialVaultStatus | null>(null);
  const [credentialVaultPassword, setCredentialVaultPassword] = useState("");
  const [credentialVaultError, setCredentialVaultError] = useState("");
  const [privateKeyImportStatus, setPrivateKeyImportStatus] = useState("");
  const [rdpStatus, setRdpStatus] = useState<"idle" | "launching" | "error">("idle");
  const [rdpError, setRdpError] = useState("");
  const [remoteFileEntries, setRemoteFileEntries] = useState(snapshot.remoteFiles);
  const [remotePath, setRemotePath] = useState("/var/www/cnshell");
  const [sftpStatus, setSftpStatus] = useState<"idle" | "loading" | "error">("idle");
  const [sftpError, setSftpError] = useState("");
  const [remoteOperationDraft, setRemoteOperationDraft] = useState<RemoteOperationDraft | null>(null);
  const [remoteOperationError, setRemoteOperationError] = useState("");
  const [liveMetrics, setLiveMetrics] = useState(snapshot.serverMetrics);
  const [metricsStatus, setMetricsStatus] = useState<"idle" | "loading" | "error">("idle");
  const [metricsError, setMetricsError] = useState("");
  const [metricHistory, setMetricHistory] = useState<MetricHistoryPoint[]>([]);
  const [remoteProcesses, setRemoteProcesses] = useState(snapshot.remoteProcesses);
  const [processStatus, setProcessStatus] = useState<"idle" | "loading" | "error">("idle");
  const [processError, setProcessError] = useState("");
  const [transferLocalPath, setTransferLocalPath] = useState("");
  const [transferRemotePath, setTransferRemotePath] = useState("");
  const [transferJobs, setTransferJobs] = useState<TransferJob[]>([]);
  const [zmodemMode, setZmodemMode] = useState<ZmodemMode>("idle");
  const [zmodemMessage, setZmodemMessage] = useState(() => translations["zh-CN"].zmodemNoSession);
  const [editorPath, setEditorPath] = useState("");
  const [editorContent, setEditorContent] = useState("");
  const [editorStatus, setEditorStatus] = useState<"idle" | "loading" | "saving" | "error" | "saved">("idle");
  const [editorError, setEditorError] = useState("");
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
  const [commandQuery, setCommandQuery] = useState("");
  const [bulkCommandReview, setBulkCommandReview] = useState<BulkCommandReview | null>(null);
  const [isSyncInputEnabled, setIsSyncInputEnabled] = useState(false);
  const [isHighlightEnabled, setIsHighlightEnabled] = useState(true);
  const [triggerEvents, setTriggerEvents] = useState<TriggerEvent[]>([]);
  const [tunnelDraft, setTunnelDraft] = useState<TunnelDraft>({
    mode: "local",
    bindHost: "127.0.0.1",
    bindPort: "8080",
    targetHost: "127.0.0.1",
    targetPort: "80"
  });
  const [tunnels, setTunnels] = useState<TunnelInfo[]>([]);
  const [relayDraft, setRelayDraft] = useState({
    relayHost: "0.0.0.0",
    relayPort: "18080",
    targetHost: "127.0.0.1",
    targetPort: "8080"
  });
  const [relays, setRelays] = useState<RelayInfo[]>([]);
  const [isRecordingScript, setIsRecordingScript] = useState(false);
  const [recordingStartedAt, setRecordingStartedAt] = useState<number | null>(null);
  const [recordingLastInputAt, setRecordingLastInputAt] = useState<number | null>(null);
  const [recordingEvents, setRecordingEvents] = useState<ScriptRecordingEvent[]>([]);
  const [logQuery, setLogQuery] = useState("");
  const [logLines, setLogLines] = useState<string[]>([]);
  const [logStatus, setLogStatus] = useState<"idle" | "loading" | "error">("idle");
  const [auditQuery, setAuditQuery] = useState("");
  const [auditLines, setAuditLines] = useState<string[]>([]);
  const [auditStatus, setAuditStatus] = useState<"idle" | "loading" | "error">("idle");
  const [errorQuery, setErrorQuery] = useState("");
  const [errorLines, setErrorLines] = useState<string[]>([]);
  const [errorStatus, setErrorStatus] = useState<"idle" | "loading" | "error">("idle");
  const [cloudSyncStatus, setCloudSyncStatus] = useState(() => translations["zh-CN"].ready);
  const [updateChannel, setUpdateChannel] = useState("latest");
  const [updateStatus, setUpdateStatus] = useState<UpdateStatus>({ state: "idle", channel: "latest" });
  const [sessionStatuses, setSessionStatuses] = useState<Record<string, SessionStatus>>(() =>
    Object.fromEntries(snapshot.sessions.map((session) => [session.id, session.status]))
  );
  const panelRefs = useRef<Record<PanelFocusKey, HTMLElement | null>>({
    credentials: null,
    tunnels: null,
    zmodem: null,
    logs: null
  });

  const filteredConnections = useMemo(
    () => snapshot.connections.filter((connection) => connectionMatchesQuery(connection, connectionQuery, labels)),
    [connectionQuery, labels, snapshot.connections]
  );

  const groupedConnections = useMemo(() => groupConnections(filteredConnections), [filteredConnections]);

  const activeConnection = useMemo(
    () => snapshot.connections.find((connection) => connection.id === activeConnectionId) ?? snapshot.connections[0],
    [activeConnectionId, snapshot.connections]
  );

  const activeTab = useMemo(
    () => {
      const tab = snapshot.sessions.find((session) => session.id === activeTabId) ?? snapshot.sessions[0];
      return {
        ...tab,
        status: sessionStatuses[tab.id] ?? tab.status
      };
    },
    [activeTabId, sessionStatuses, snapshot.sessions]
  );

  const sessionTabsWithStatus = useMemo(
    () =>
      snapshot.sessions.map((session) => ({
        ...session,
        status: sessionStatuses[session.id] ?? session.status
      })),
    [sessionStatuses, snapshot.sessions]
  );

  const splitTab = useMemo(
    () => sessionTabsWithStatus.find((session) => session.id === splitTabId),
    [sessionTabsWithStatus, splitTabId]
  );

  const setSessionStatus = useCallback((sessionId: string, status: SessionStatus) => {
    setSessionStatuses((current) => ({
      ...current,
      [sessionId]: status
    }));
  }, []);

  const activeSshDraft = useMemo(
    () => sshDrafts[activeConnection.id] ?? { password: "", privateKey: "", passphrase: "" },
    [activeConnection.id, sshDrafts]
  );

  const activeCredentialStatus = credentialStatuses[activeConnection.id];

  const focusPanel = useCallback((panel: PanelFocusKey) => {
    const element = panelRefs.current[panel];
    if (!element) {
      return;
    }

    element.scrollIntoView({ block: "start", behavior: "smooth" });
    element.classList.add("panel-section-focused");
    window.setTimeout(() => {
      element.classList.remove("panel-section-focused");
    }, 900);
  }, []);

  const setPanelRef = useCallback((panel: PanelFocusKey) => {
    return (element: HTMLElement | null) => {
      panelRefs.current[panel] = element;
    };
  }, []);

  const openNewConnectionEditor = () => {
    setConnectionFormError("");
    setConnectionDraft(createDefaultConnectionDraft());
  };

  const openActiveConnectionEditor = () => {
    setConnectionFormError("");
    setConnectionDraft(createConnectionDraft(activeConnection));
  };

  const saveConnectionDraft = () => {
    if (!connectionDraft) {
      return;
    }

    const name = connectionDraft.name.trim();
    const host = connectionDraft.host.trim();
    const username = connectionDraft.username.trim();
    const port = parsePort(connectionDraft.port);
    if (!name) {
      setConnectionFormError(labels.connectionNameRequired);
      return;
    }

    if (!host) {
      setConnectionFormError(labels.connectionHostRequired);
      return;
    }

    if (!port && connectionDraft.protocol !== "local") {
      setConnectionFormError(labels.connectionPortInvalid);
      return;
    }

    if (!username) {
      setConnectionFormError(labels.connectionUserRequired);
      return;
    }

    const connection: ConnectionProfile = {
      id: connectionDraft.id ?? `connection-${Date.now()}`,
      name,
      group: connectionDraft.group.trim() || labels.builtInGroups.Staging,
      protocol: connectionDraft.protocol,
      host,
      port: connectionDraft.protocol === "local" ? 0 : port ?? 22,
      username,
      authMethod: connectionDraft.authMethod,
      color: connectionDraft.color,
      tags: normalizeTags(connectionDraft.tags),
      gateways: connectionDraft.id
        ? snapshot.connections.find((item) => item.id === connectionDraft.id)?.gateways
        : undefined
    };

    const nextSession = connectionDraft.id ? null : makeSessionForConnection(connection);

    setSnapshot((current) => {
      const exists = current.connections.some((item) => item.id === connection.id);
      const nextConnections = exists
        ? current.connections.map((item) => (item.id === connection.id ? connection : item))
        : [...current.connections, connection];
      const nextSessions = exists
        ? current.sessions.map((session) =>
            session.connectionId === connection.id ? { ...session, title: connection.name } : session
          )
        : [...current.sessions, nextSession ?? makeSessionForConnection(connection)];

      return {
        ...current,
        connections: nextConnections,
        sessions: nextSessions
      };
    });

    if (!connectionDraft.id) {
      setActiveConnectionId(connection.id);
      setActiveTabId(nextSession?.id ?? "");
    }

    setConnectionDraft(null);
    setConnectionFormError("");
  };

  const deleteActiveConnection = () => {
    if (snapshot.connections.length <= 1) {
      return;
    }

    const nextConnection = snapshot.connections.find((connection) => connection.id !== activeConnection.id);
    if (!nextConnection) {
      return;
    }

    const nextSession = snapshot.sessions.find((session) => session.connectionId === nextConnection.id);
    const createdSession = nextSession ? null : makeSessionForConnection(nextConnection);
    setSnapshot((current) => ({
      ...current,
      connections: current.connections.filter((connection) => connection.id !== activeConnection.id),
      sessions: [
        ...current.sessions.filter((session) => session.connectionId !== activeConnection.id),
        ...(createdSession ? [createdSession] : [])
      ]
    }));
    setActiveConnectionId(nextConnection.id);
    setActiveTabId(nextSession?.id ?? createdSession?.id ?? "");
    setConnectionDraft(null);
  };

  const openNewQuickCommandEditor = () => {
    setQuickCommandFormError("");
    setQuickCommandDraft(createDefaultQuickCommandDraft());
  };

  const openQuickCommandEditor = (command: QuickCommand) => {
    setQuickCommandFormError("");
    setQuickCommandDraft(createQuickCommandDraft(command));
  };

  const saveQuickCommandDraft = () => {
    if (!quickCommandDraft) {
      return;
    }

    const title = quickCommandDraft.title.trim();
    const command = quickCommandDraft.command.trim();
    if (!title) {
      setQuickCommandFormError(labels.commandTitleRequired);
      return;
    }

    if (!command) {
      setQuickCommandFormError(labels.commandTextRequired);
      return;
    }

    const nextCommand: QuickCommand = {
      id: quickCommandDraft.id ?? `qc-${Date.now()}`,
      title,
      command,
      scope: quickCommandDraft.scope
    };

    setSnapshot((current) => ({
      ...current,
      quickCommands: current.quickCommands.some((item) => item.id === nextCommand.id)
        ? current.quickCommands.map((item) => (item.id === nextCommand.id ? nextCommand : item))
        : [nextCommand, ...current.quickCommands]
    }));
    setQuickCommandDraft(null);
    setQuickCommandFormError("");
  };

  const deleteQuickCommand = (commandId: string) => {
    setSnapshot((current) => ({
      ...current,
      quickCommands: current.quickCommands.filter((command) => command.id !== commandId)
    }));
    setQuickCommandDraft((current) => (current?.id === commandId ? createDefaultQuickCommandDraft() : current));
  };

  const toggleConnectionGroup = (group: string) => {
    setCollapsedGroups((current) => ({
      ...current,
      [group]: !current[group]
    }));
  };

  const updateActiveSshDraft = (field: "password" | "privateKey" | "passphrase", value: string) => {
    setSshDrafts((current) => ({
      ...current,
      [activeConnection.id]: {
        ...(current[activeConnection.id] ?? { password: "", privateKey: "", passphrase: "" }),
        [field]: value
      }
    }));
  };

  const updateActiveGateways = (gateways: JumpHostConfig[]) => {
    setSnapshot((current) => ({
      ...current,
      connections: current.connections.map((connection) =>
        connection.id === activeConnection.id ? { ...connection, gateways } : connection
      )
    }));
  };

  const updateKeyMappingProfiles = (profiles: KeyMappingProfile[]) => {
    setSnapshot((current) => ({
      ...current,
      keyMappingProfiles: profiles
    }));
  };

  const syncRemotePath = (path: string) => {
    const normalizedPath = normalizeRemotePath(path || "/");
    setRemotePath(normalizedPath);
    setSnapshot((current) => ({
      ...current,
      sessions: current.sessions.map((session) =>
        session.id === activeTab.id ? { ...session, cwd: normalizedPath } : session
      )
    }));
  };

  const appendRecordingInput = (input: string) => {
    if (!isRecordingScript || !input) {
      return;
    }

    const now = Date.now();
    const delayMs = recordingLastInputAt ? Math.min(now - recordingLastInputAt, 5000) : 0;
    setRecordingLastInputAt(now);
    setRecordingEvents((current) => [
      ...current,
      {
        id: `script-event-${now}-${current.length}`,
        input,
        delayMs
      }
    ]);
  };

  const sendTerminalInput = (sessionId: string, input: string, options: { record?: boolean } = {}) => {
    if (options.record !== false) {
      appendRecordingInput(input);
    }

    if (sessionId === activeTab.id && input.endsWith("\r")) {
      const nextPath = inferDirectoryFromCommand(input.replace(/\r$/, ""), remotePath);
      if (nextPath) {
        syncRemotePath(nextPath);
      }
    }

    void window.cnshell?.terminal.write(sessionId, input);
  };

  const startScriptRecording = () => {
    const now = Date.now();
    setIsRecordingScript(true);
    setRecordingStartedAt(now);
    setRecordingLastInputAt(now);
    setRecordingEvents([]);
  };

  const stopScriptRecording = () => {
    if (recordingEvents.length > 0) {
      const createdAt = new Date(recordingStartedAt ?? Date.now()).toISOString();
      const recording: ScriptRecording = {
        id: `script-${Date.now()}`,
        name: `Recording ${new Date().toLocaleTimeString()}`,
        createdAt,
        events: recordingEvents
      };

      setSnapshot((current) => ({
        ...current,
        scriptRecordings: [recording, ...current.scriptRecordings].slice(0, 12)
      }));
    }

    setIsRecordingScript(false);
    setRecordingStartedAt(null);
    setRecordingLastInputAt(null);
    setRecordingEvents([]);
  };

  const replayScriptRecording = (recording: ScriptRecording) => {
    let delay = 0;
    for (const event of recording.events) {
      delay += Math.min(event.delayMs, 3000);
      window.setTimeout(() => {
        sendTerminalInput(activeTab.id, event.input, { record: false });
      }, delay);
    }
  };

  const refreshSessionLog = useCallback(() => {
    setLogStatus("loading");
    void window.cnshell?.logs
      .readSession({
        sessionId: activeTab.id,
        query: logQuery,
        limit: 300
      })
      .then((result) => {
        setLogLines(result.lines);
        setLogStatus("idle");
      })
      .catch(() => {
        setLogLines([]);
        setLogStatus("error");
      });
  }, [activeTab.id, logQuery]);

  const refreshAuditLog = useCallback(() => {
    setAuditStatus("loading");
    void window.cnshell?.logs
      .readAudit({
        query: auditQuery,
        limit: 300
      })
      .then((result) => {
        setAuditLines(result.lines);
        setAuditStatus("idle");
      })
      .catch(() => {
        setAuditLines([]);
        setAuditStatus("error");
      });
  }, [auditQuery]);

  const refreshErrorReports = useCallback(() => {
    setErrorStatus("loading");
    void window.cnshell?.logs
      .readErrors({
        query: errorQuery,
        limit: 300
      })
      .then((result) => {
        setErrorLines(result.lines);
        setErrorStatus("idle");
      })
      .catch(() => {
        setErrorLines([]);
        setErrorStatus("error");
      });
  }, [errorQuery]);

  const changeLanguage = (nextLanguage: Language) => {
    setLanguage(nextLanguage);
    void window.cnshell?.setLanguage(nextLanguage);
    try {
      window.localStorage.setItem(LANGUAGE_STORAGE_KEY, nextLanguage);
    } catch {
      // Ignore storage failures; the in-memory language still updates immediately.
    }
  };

  const exportCloudSyncSettings = () => {
    setCloudSyncStatus(labels.exportingEncryptedSettings);
    void window.cnshell?.cloudSync
      .exportSettings({ snapshot })
      .then((result) => {
        setCloudSyncStatus(result.ok ? labels.exportedPath(result.path ?? "") : labels.exportCanceled);
      })
      .catch((error: Error) => {
        setCloudSyncStatus(error.message);
      });
  };

  const importCloudSyncSettings = () => {
    setCloudSyncStatus(labels.importingEncryptedSettings);
    void window.cnshell?.cloudSync
      .importSettings()
      .then((result) => {
        if (!result.ok || !result.importedSnapshot) {
          setCloudSyncStatus(labels.importCanceled);
          return;
        }

        const importedSnapshot = hydrateAppSnapshot(result.importedSnapshot);
        setSnapshot(importedSnapshot);
        setRemoteFileEntries(importedSnapshot.remoteFiles);
        setLiveMetrics(importedSnapshot.serverMetrics);
        setRemoteProcesses(importedSnapshot.remoteProcesses);
        setActiveConnectionId(importedSnapshot.connections[0]?.id ?? "");
        setActiveTabId(importedSnapshot.sessions[0]?.id ?? "");
        setCloudSyncStatus(labels.importedPath(result.path ?? ""));
      })
      .catch((error: Error) => {
        setCloudSyncStatus(error.message);
      });
  };

  const checkForUpdates = () => {
    void window.cnshell?.updates
      .check({ channel: updateChannel })
      .then(setUpdateStatus)
      .catch((error: Error) => {
        setUpdateStatus({ state: "error", channel: updateChannel, message: error.message });
      });
  };

  const installDownloadedUpdate = () => {
    void window.cnshell?.updates.quitAndInstall();
  };

  const startActiveSession = () => {
    setSessionStartTokens((current) => ({
      ...current,
      [activeTab.id]: (current[activeTab.id] ?? 0) + 1
    }));
  };

  const startSession = (sessionId: string) => {
    setSessionStartTokens((current) => ({
      ...current,
      [sessionId]: (current[sessionId] ?? 0) + 1
    }));
  };

  const trustActiveHost = () => {
    const prompt = hostKeyPrompts[activeTab.id];
    if (!prompt || prompt.status === "changed") {
      return;
    }

    void window.cnshell?.terminal.trustHost(prompt).then(() => {
      setHostKeyPrompts((current) => {
        const next = { ...current };
        delete next[activeTab.id];
        return next;
      });
      startActiveSession();
    });
  };

  const refreshCredentialStatus = useCallback((connectionId: string) => {
    void window.cnshell?.credentials.status(connectionId).then((status) => {
      setCredentialStatuses((current) => ({
        ...current,
        [connectionId]: status
      }));
    });
  }, []);

  const refreshCredentialVaultStatus = useCallback(() => {
    void window.cnshell?.credentials.vaultStatus().then((status) => {
      setCredentialVaultStatus(status);
    });
  }, []);

  const refreshActiveCredentialSecurity = useCallback(() => {
    refreshCredentialVaultStatus();
    if (activeConnection.protocol === "ssh") {
      refreshCredentialStatus(activeConnection.id);
    }
  }, [activeConnection.id, activeConnection.protocol, refreshCredentialStatus, refreshCredentialVaultStatus]);

  const enableCredentialVault = () => {
    setCredentialVaultError("");
    void window.cnshell?.credentials
      .enableVault({ masterPassword: credentialVaultPassword })
      .then((status) => {
        setCredentialVaultStatus(status);
        setCredentialVaultPassword("");
        refreshActiveCredentialSecurity();
      })
      .catch((error: Error) => {
        setCredentialVaultError(error.message);
      });
  };

  const unlockCredentialVault = () => {
    setCredentialVaultError("");
    void window.cnshell?.credentials
      .unlockVault({ masterPassword: credentialVaultPassword })
      .then((status) => {
        setCredentialVaultStatus(status);
        setCredentialVaultPassword("");
        refreshActiveCredentialSecurity();
      })
      .catch((error: Error) => {
        setCredentialVaultError(error.message);
      });
  };

  const lockCredentialVault = () => {
    setCredentialVaultError("");
    void window.cnshell?.credentials.lockVault().then((status) => {
      setCredentialVaultStatus(status);
      refreshActiveCredentialSecurity();
    });
  };

  const disableCredentialVault = () => {
    setCredentialVaultError("");
    void window.cnshell?.credentials
      .disableVault({ masterPassword: credentialVaultPassword || undefined })
      .then((status) => {
        setCredentialVaultStatus(status);
        setCredentialVaultPassword("");
        refreshActiveCredentialSecurity();
      })
      .catch((error: Error) => {
        setCredentialVaultError(error.message);
      });
  };

  const saveActiveCredential = () => {
    void window.cnshell?.credentials
      .save({
        connectionId: activeConnection.id,
        secret: {
          password: activeSshDraft.password || undefined,
          privateKey: activeSshDraft.privateKey || undefined,
          passphrase: activeSshDraft.passphrase || undefined
        }
      })
      .then((status) => {
        setCredentialStatuses((current) => ({
          ...current,
          [activeConnection.id]: status
        }));
        setCredentialErrors((current) => ({
          ...current,
          [activeConnection.id]: ""
        }));
        setSshDrafts((current) => ({
          ...current,
          [activeConnection.id]: { password: "", privateKey: "", passphrase: "" }
        }));
      })
      .catch((error: Error) => {
        setCredentialErrors((current) => ({
          ...current,
          [activeConnection.id]: error.message
        }));
      });
  };

  const deleteActiveCredential = () => {
    void window.cnshell?.credentials.delete(activeConnection.id).then((status) => {
      setCredentialStatuses((current) => ({
        ...current,
        [activeConnection.id]: status
      }));
      setCredentialErrors((current) => ({
        ...current,
        [activeConnection.id]: ""
      }));
    });
  };

  const importPrivateKey = () => {
    setPrivateKeyImportStatus(labels.openingKeyFile);
    void window.cnshell?.credentials
      .importPrivateKey()
      .then((result) => {
        if (!result.ok || !result.privateKey) {
          setPrivateKeyImportStatus(labels.privateKeyImportCanceled);
          return;
        }

        updateActiveSshDraft("privateKey", result.privateKey);
        setPrivateKeyImportStatus(labels.privateKeyImported(result.fileName ?? labels.privateKeyFallbackName));
      })
      .catch((error: Error) => {
        setPrivateKeyImportStatus(error.message);
      });
  };

  const openActiveRdp = () => {
    if (activeConnection.protocol !== "rdp") {
      return;
    }

    setRdpStatus("launching");
    setRdpError("");

    void window.cnshell?.rdp
      .open({
        host: activeConnection.host,
        port: activeConnection.port || 3389,
        username: activeConnection.username
      })
      .then(() => {
        setRdpStatus("idle");
      })
      .catch((error: Error) => {
        setRdpStatus("error");
        setRdpError(error.message);
      });
  };

  const refreshRemoteFiles = () => {
    if (activeConnection.protocol !== "ssh") {
      setRemoteFileEntries(snapshot.remoteFiles);
      return;
    }

    setSftpStatus("loading");
    setSftpError("");

    void window.cnshell?.sftp
      .listDirectory({
        path: remotePath,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((listing) => {
        syncRemotePath(listing.path);
        setRemoteFileEntries(listing.entries);
        setSftpStatus("idle");
      })
      .catch((error: Error) => {
        setSftpError(error.message);
        setSftpStatus("error");
      });
  };

  const startTransfer = (direction: "upload" | "download") => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    const localPath = transferLocalPath.trim();
    const remoteTransferPath = transferRemotePath.trim();
    if (!localPath || !remoteTransferPath) {
      return;
    }

    const jobId = `${direction}-${Date.now()}`;
    const job: TransferJob = {
      id: jobId,
      direction,
      localPath,
      remotePath: remoteTransferPath,
      status: "running"
    };

    setTransferJobs((current) => [job, ...current].slice(0, 8));

    void window.cnshell?.sftp
      .transferFile({
        direction,
        localPath,
        remotePath: remoteTransferPath,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then(() => {
        setTransferJobs((current) =>
          current.map((item) => (item.id === jobId ? { ...item, status: "completed", message: "Done" } : item))
        );
        refreshRemoteFiles();
      })
      .catch((error: Error) => {
        setTransferJobs((current) =>
          current.map((item) => (item.id === jobId ? { ...item, status: "error", message: error.message } : item))
        );
      });
  };

  const openRemoteFile = (remoteFilePath: string) => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    setEditorPath(remoteFilePath);
    setEditorStatus("loading");
    setEditorError("");

    void window.cnshell?.sftp
      .readFile({
        remotePath: remoteFilePath,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((result) => {
        setEditorPath(result.remotePath);
        setEditorContent(result.content);
        setEditorStatus("idle");
      })
      .catch((error: Error) => {
        setEditorError(error.message);
        setEditorStatus("error");
      });
  };

  const saveRemoteFile = () => {
    if (activeConnection.protocol !== "ssh" || !editorPath) {
      return;
    }

    setEditorStatus("saving");
    setEditorError("");

    void window.cnshell?.sftp
      .writeFile({
        remotePath: editorPath,
        content: editorContent,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then(() => {
        setEditorStatus("saved");
        refreshRemoteFiles();
      })
      .catch((error: Error) => {
        setEditorError(error.message);
        setEditorStatus("error");
      });
  };

  const openCreateRemoteDirectory = () => {
    setRemoteOperationError("");
    setRemoteOperationDraft({ type: "mkdir", targetPath: remotePath, value: "" });
  };

  const openRenameRemotePath = (remoteFilePath: string) => {
    setRemoteOperationError("");
    setRemoteOperationDraft({
      type: "rename",
      targetPath: remoteFilePath,
      value: remoteFilePath.split("/").filter(Boolean).at(-1) ?? remoteFilePath
    });
  };

  const openDeleteRemotePath = (remoteFilePath: string) => {
    setRemoteOperationError("");
    setRemoteOperationDraft({ type: "delete", targetPath: remoteFilePath, value: "" });
  };

  const runRemoteOperation = () => {
    if (!remoteOperationDraft || activeConnection.protocol !== "ssh") {
      return;
    }

    const ssh = createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential));
    const targetPath = remoteOperationDraft.targetPath.trim();
    const value = remoteOperationDraft.value.trim();
    if (!targetPath) {
      setRemoteOperationError(labels.remotePathRequired);
      return;
    }

    setSftpStatus("loading");
    setSftpError("");
    setRemoteOperationError("");

    const operation =
      remoteOperationDraft.type === "mkdir"
        ? value
          ? window.cnshell?.sftp.createDirectory({ ssh, remotePath: normalizeRemotePath(`${targetPath}/${value}`) })
          : Promise.reject(new Error(labels.remoteNameRequired))
        : remoteOperationDraft.type === "rename"
          ? value
            ? window.cnshell?.sftp.renamePath({
                ssh,
                oldPath: targetPath,
                newPath: value.includes("/") ? normalizeRemotePath(value) : normalizeRemotePath(`${parentRemotePath(targetPath)}/${value}`)
              })
            : Promise.reject(new Error(labels.remoteNameRequired))
          : window.cnshell?.sftp.deletePath({ ssh, remotePath: targetPath });

    void operation
      ?.then(() => {
        setRemoteOperationDraft(null);
        setSftpStatus("idle");
        setSftpError("");
        refreshRemoteFiles();
      })
      .catch((error: Error) => {
        setRemoteOperationError(error.message);
        setSftpStatus("error");
      });
  };

  const appendMetricHistory = (
    metrics: ReturnType<typeof createInitialAppSnapshot>["serverMetrics"],
    processCount = remoteProcesses.length
  ) => {
    const now = new Date();
    setMetricHistory((current) =>
      [
        ...current,
        {
          at: now.toLocaleTimeString(),
          cpu: metricValue(metrics, "CPU"),
          memory: metricValue(metrics, "Memory"),
          disk: metricValue(metrics, "Disk"),
          network: metricValue(metrics, "Ping"),
          processes: processCount
        }
      ].slice(-20)
    );
  };

  const refreshMetrics = () => {
    if (activeConnection.protocol !== "ssh") {
      setLiveMetrics(snapshot.serverMetrics);
      return;
    }

    setMetricsStatus("loading");
    setMetricsError("");

    void window.cnshell?.metrics
      .collect({
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((result) => {
        setLiveMetrics(result.metrics);
        appendMetricHistory(result.metrics);
        setMetricsStatus("idle");
      })
      .catch((error: Error) => {
        setMetricsError(error.message);
        setMetricsStatus("error");
      });
  };

  const refreshProcesses = () => {
    if (activeConnection.protocol !== "ssh") {
      setRemoteProcesses(snapshot.remoteProcesses);
      return;
    }

    setProcessStatus("loading");
    setProcessError("");

    void window.cnshell?.metrics
      .listProcesses({
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((result) => {
        setRemoteProcesses(result.processes);
        appendMetricHistory(liveMetrics, result.processes.length);
        setProcessStatus("idle");
      })
      .catch((error: Error) => {
        setProcessError(error.message);
        setProcessStatus("error");
      });
  };

  const killProcess = (pid: number) => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    setProcessStatus("loading");
    setProcessError("");

    void window.cnshell?.metrics
      .killProcess({
        pid,
        signal: "TERM",
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then(() => refreshProcesses())
      .catch((error: Error) => {
        setProcessError(error.message);
        setProcessStatus("error");
      });
  };

  const dispatchCommandToSessions = (command: string, targetSessionIds: string[]) => {
    for (const sessionId of targetSessionIds) {
      sendTerminalInput(sessionId, `${command}\r`);
    }

    setIsCommandPaletteOpen(false);
    setCommandQuery("");
  };

  const executeCommand = (command: string) => {
    const targetSessionIds = isSyncInputEnabled
      ? sessionTabsWithStatus.filter((session) => session.status !== "error").map((session) => session.id)
      : [activeTab.id];

    if (targetSessionIds.length > 1) {
      setBulkCommandReview({ command, targetSessionIds });
      return;
    }

    dispatchCommandToSessions(command, targetSessionIds);
  };

  const confirmBulkCommand = () => {
    if (!bulkCommandReview) {
      return;
    }

    dispatchCommandToSessions(bulkCommandReview.command, bulkCommandReview.targetSessionIds);
    setBulkCommandReview(null);
  };

  const cancelBulkCommand = () => {
    setBulkCommandReview(null);
  };

  const addTriggerEvents = useCallback((events: TriggerEvent[]) => {
    if (events.length === 0) {
      return;
    }

    setTriggerEvents((current) => [...events, ...current].slice(0, 8));
  }, []);

  const handleZmodemDetected = useCallback((mode: ZmodemMode) => {
    if (mode === "idle") {
      return;
    }

    setZmodemMode(mode);
    setZmodemMessage(
      mode === "upload"
        ? labels.zmodemUploadDetected
        : mode === "download"
          ? labels.zmodemDownloadDetected
          : labels.zmodemActivityDetected
    );
  }, [labels]);

  const startTunnel = () => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    const tunnelId = `tunnel-${Date.now()}`;
    const bindPort = parsePort(tunnelDraft.bindPort);
    const parsedTargetPort = tunnelDraft.mode === "dynamic" ? null : parsePort(tunnelDraft.targetPort);
    const bindHost = tunnelDraft.bindHost.trim();
    const targetHost = tunnelDraft.targetHost.trim();
    if (!bindPort || !bindHost || (tunnelDraft.mode !== "dynamic" && (!parsedTargetPort || !targetHost))) {
      return;
    }
    const targetPort = tunnelDraft.mode === "dynamic" ? undefined : parsedTargetPort ?? undefined;

    const startingTunnel: TunnelInfo = {
      id: tunnelId,
      mode: tunnelDraft.mode,
      bindHost,
      bindPort,
      targetHost: tunnelDraft.mode === "dynamic" ? undefined : targetHost,
      targetPort,
      status: "starting"
    };
    setTunnels((current) => [startingTunnel, ...current].slice(0, 6));

    void window.cnshell?.tunnels
      .start({
        id: tunnelId,
        mode: tunnelDraft.mode,
        bindHost,
        bindPort,
        targetHost: tunnelDraft.mode === "dynamic" ? undefined : targetHost,
        targetPort,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((info) => {
        setTunnels((current) => current.map((tunnel) => (tunnel.id === tunnelId ? info : tunnel)));
      })
      .catch((error: Error) => {
        setTunnels((current) =>
          current.map((tunnel) =>
            tunnel.id === tunnelId ? { ...tunnel, status: "error", message: error.message } : tunnel
          )
        );
      });
  };

  const stopTunnel = (id: string) => {
    void window.cnshell?.tunnels.stop(id).then(() => {
      setTunnels((current) =>
        current.map((tunnel) => (tunnel.id === id ? { ...tunnel, status: "stopped" } : tunnel))
      );
    });
  };

  const startRelay = () => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    const relayPort = parsePort(relayDraft.relayPort);
    const targetPort = parsePort(relayDraft.targetPort);
    const relayHost = relayDraft.relayHost.trim();
    const targetHost = relayDraft.targetHost.trim();
    if (!relayPort || !targetPort || !relayHost || !targetHost) {
      return;
    }

    const relayId = `relay-${Date.now()}`;
    const startingRelay: RelayInfo = {
      id: relayId,
      relayHost,
      relayPort,
      targetHost,
      targetPort,
      status: "starting"
    };
    setRelays((current) => [startingRelay, ...current].slice(0, 5));

    void window.cnshell?.relay
      .start({
        id: relayId,
        relayHost,
        relayPort,
        targetHost,
        targetPort,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((info) => {
        setRelays((current) => current.map((relay) => (relay.id === relayId ? info : relay)));
      })
      .catch((error: Error) => {
        setRelays((current) =>
          current.map((relay) => (relay.id === relayId ? { ...relay, status: "error", message: error.message } : relay))
        );
      });
  };

  const stopRelay = (id: string) => {
    void window.cnshell?.relay.stop(id).then(() => {
      setRelays((current) => current.map((relay) => (relay.id === id ? { ...relay, status: "stopped" } : relay)));
    });
  };

  const createSessionForConnection = (connection: ConnectionProfile, titleSuffix = "") => {
    const sessionId = `tab-${connection.id}-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    const nextSession: SessionTab = {
      id: sessionId,
      connectionId: connection.id,
      title: `${connection.name}${titleSuffix}`,
      cwd: connection.protocol === "local" ? "~" : "/",
      status: "disconnected",
      startedAt: new Date().toISOString()
    };

    setSnapshot((current) => ({
      ...current,
      sessions: [...current.sessions, nextSession]
    }));
    setSessionStatuses((current) => ({
      ...current,
      [sessionId]: "disconnected"
    }));
    return nextSession;
  };

  const createSessionForActiveConnection = () => {
    const nextSession = createSessionForConnection(activeConnection);
    setActiveTabId(nextSession.id);
  };

  const closeSessionTab = (sessionId: string) => {
    if (snapshot.sessions.length <= 1) {
      return;
    }

    const remainingSessions = snapshot.sessions.filter((session) => session.id !== sessionId);
    const fallbackSession =
      remainingSessions.find((session) => session.connectionId === activeConnection.id) ?? remainingSessions[0];

    void window.cnshell?.terminal.stop(sessionId);
    setSnapshot((current) => ({
      ...current,
      sessions: current.sessions.filter((session) => session.id !== sessionId)
    }));
    setSessionStatuses((current) => {
      const next = { ...current };
      delete next[sessionId];
      return next;
    });
    setSessionStartTokens((current) => {
      const next = { ...current };
      delete next[sessionId];
      return next;
    });
    if (activeTabId === sessionId) {
      setActiveTabId(fallbackSession.id);
      setActiveConnectionId(fallbackSession.connectionId);
    }
    if (splitTabId === sessionId) {
      setSplitTabId("");
    }
  };

  const createSplitSession = () => {
    if (splitTabId) {
      setSplitTabId("");
      return;
    }

    const nextSession = createSessionForConnection(activeConnection, " split");
    setSplitTabId(nextSession.id);
    window.setTimeout(() => startSession(nextSession.id), 0);
  };

  useEffect(() => {
    void window.cnshell?.getVersion().then(setAppVersion);
    void window.cnshell?.setLanguage(language);
    refreshCredentialVaultStatus();
    void window.cnshell?.updates.status().then(setUpdateStatus);
  }, [language, refreshCredentialVaultStatus]);

  useEffect(() => {
    return window.cnshell?.updates.onStatus(setUpdateStatus);
  }, []);

  useEffect(() => {
    void workspaceStorage.loadSnapshot().then((storedSnapshot) => {
      if (storedSnapshot) {
        const hydratedSnapshot = hydrateAppSnapshot(storedSnapshot);
        setSnapshot(hydratedSnapshot);
        setRemoteFileEntries(hydratedSnapshot.remoteFiles);
        setLiveMetrics(hydratedSnapshot.serverMetrics);
        setRemoteProcesses(hydratedSnapshot.remoteProcesses);
        setActiveConnectionId(hydratedSnapshot.connections[0]?.id ?? "");
        setActiveTabId(hydratedSnapshot.sessions[0]?.id ?? "");
      }

      setIsWorkspaceReady(true);
    });
  }, []);

  useEffect(() => {
    if (isWorkspaceReady) {
      void workspaceStorage.saveSnapshot(snapshot);
    }
  }, [isWorkspaceReady, snapshot]);

  useEffect(() => {
    return window.cnshell?.terminal.onHostKeyVerification((event) => {
      setHostKeyPrompts((current) => ({
        ...current,
        [event.id]: event
      }));
      setSessionStatus(event.id, "error");
    });
  }, [setSessionStatus]);

  useEffect(() => {
    if (activeConnection.protocol === "ssh") {
      refreshCredentialStatus(activeConnection.id);
    }
  }, [activeConnection.id, activeConnection.protocol, refreshCredentialStatus]);

  useEffect(() => {
    refreshSessionLog();
  }, [refreshSessionLog]);

  useEffect(() => {
    refreshAuditLog();
  }, [refreshAuditLog]);

  useEffect(() => {
    refreshErrorReports();
  }, [refreshErrorReports]);

  useEffect(() => {
    setRemotePath(activeTab.cwd || "/");
  }, [activeTab.cwd, activeTab.id]);

  if (!isWorkspaceReady) {
    return (
      <main className="app-shell loading-shell">
        <section className="workspace-loading" aria-live="polite">
          <div className="brand-mark" aria-hidden="true">
            CN
          </div>
          <strong>{labels.loadingWorkspace}</strong>
          <span>{labels.loadingWorkspaceDetail}</span>
        </section>
      </main>
    );
  }

  return (
    <TranslationContext.Provider value={labels}>
      <AppErrorBoundary>
      <main className="app-shell">
      <ConnectionSidebar
        groupedConnections={groupedConnections}
        activeConnectionId={activeConnectionId}
        query={connectionQuery}
        collapsedGroups={collapsedGroups}
        onQueryChange={setConnectionQuery}
        onCreate={openNewConnectionEditor}
        onEditActive={openActiveConnectionEditor}
        onToggleGroup={toggleConnectionGroup}
        onOpenSettings={() => setIsSettingsOpen(true)}
        onSelect={(connectionId) => {
          setActiveConnectionId(connectionId);
          const nextTab = snapshot.sessions.find((tab) => tab.connectionId === connectionId);
          if (nextTab) {
            setActiveTabId(nextTab.id);
          }
        }}
      />
      <section className="workspace" aria-label={labels.workspace}>
        <TopBar
          activeConnection={activeConnection}
          status={activeTab.status}
          version={appVersion}
          isSyncInputEnabled={isSyncInputEnabled}
          isHighlightEnabled={isHighlightEnabled}
          onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
          onToggleSyncInput={() => setIsSyncInputEnabled((current) => !current)}
          onToggleHighlight={() => setIsHighlightEnabled((current) => !current)}
          onFocusPanel={focusPanel}
        />
        <TabStrip
          tabs={sessionTabsWithStatus}
          activeTabId={activeTabId}
          onSelect={setActiveTabId}
          onCreate={createSessionForActiveConnection}
          onClose={closeSessionTab}
        />
        <section className="workspace-grid">
          <div className={`terminal-split-layout ${splitTab ? "active" : ""}`}>
            <TerminalPane
              activeConnection={activeConnection}
              activeTab={activeTab}
              sshDraft={activeSshDraft}
              useSavedCredential={Boolean(activeCredentialStatus?.hasCredential)}
              keyMappingProfiles={snapshot.keyMappingProfiles}
              startToken={sessionStartTokens[activeTab.id] ?? 0}
              isHighlightEnabled={isHighlightEnabled}
              isSplitActive={Boolean(splitTab)}
              zmodemMode={zmodemMode}
              onStatusChange={setSessionStatus}
              onReconnect={startActiveSession}
              onSplit={createSplitSession}
              onFocusPanel={focusPanel}
              onDispatchCommand={executeCommand}
              onTerminalInput={sendTerminalInput}
              onTriggerEvents={addTriggerEvents}
              onZmodemDetected={handleZmodemDetected}
            />
            {splitTab ? (
              <TerminalPane
                activeConnection={activeConnection}
                activeTab={splitTab}
                sshDraft={activeSshDraft}
                useSavedCredential={Boolean(activeCredentialStatus?.hasCredential)}
                keyMappingProfiles={snapshot.keyMappingProfiles}
                startToken={sessionStartTokens[splitTab.id] ?? 0}
                isHighlightEnabled={isHighlightEnabled}
                isSplitActive
                isSecondaryPane
                zmodemMode={zmodemMode}
                onStatusChange={setSessionStatus}
                onReconnect={() => startSession(splitTab.id)}
                onSplit={createSplitSession}
                onFocusPanel={focusPanel}
                onDispatchCommand={(command) => sendTerminalInput(splitTab.id, `${command}\r`)}
                onTerminalInput={sendTerminalInput}
                onTriggerEvents={addTriggerEvents}
                onZmodemDetected={handleZmodemDetected}
              />
            ) : null}
          </div>
          <aside className="ops-panel" aria-label={labels.operationsPanels}>
            {activeConnection.protocol === "ssh" ? (
              <SshCredentialPanel
                panelRef={setPanelRef("credentials")}
                authMethod={activeConnection.authMethod}
                draft={activeSshDraft}
                credentialStatus={activeCredentialStatus}
                credentialError={credentialErrors[activeConnection.id]}
                vaultStatus={credentialVaultStatus}
                vaultPassword={credentialVaultPassword}
                vaultError={credentialVaultError}
                privateKeyImportStatus={privateKeyImportStatus}
                hostKeyPrompt={hostKeyPrompts[activeTab.id]}
                onChange={updateActiveSshDraft}
                onVaultPasswordChange={setCredentialVaultPassword}
                onImportPrivateKey={importPrivateKey}
                onConnect={startActiveSession}
                onSaveCredential={saveActiveCredential}
                onDeleteCredential={deleteActiveCredential}
                onEnableVault={enableCredentialVault}
                onUnlockVault={unlockCredentialVault}
                onLockVault={lockCredentialVault}
                onDisableVault={disableCredentialVault}
                onTrustHost={trustActiveHost}
              />
            ) : null}
            {activeConnection.protocol === "rdp" ? (
              <RdpPanel connection={activeConnection} status={rdpStatus} error={rdpError} onOpen={openActiveRdp} />
            ) : null}
            {activeConnection.protocol === "ssh" ? (
              <JumpHostPanel gateways={activeConnection.gateways ?? []} onChange={updateActiveGateways} />
            ) : null}
            <FilePanel
              remoteFiles={remoteFileEntries}
              path={remotePath}
              status={sftpStatus}
              error={sftpError}
              localPath={transferLocalPath}
              transferRemotePath={transferRemotePath}
              transferJobs={transferJobs}
              onPathChange={syncRemotePath}
              onLocalPathChange={setTransferLocalPath}
              onTransferRemotePathChange={setTransferRemotePath}
              onRefresh={refreshRemoteFiles}
              onTransfer={startTransfer}
              onOpenFile={openRemoteFile}
              onCreateDirectory={openCreateRemoteDirectory}
              onRenamePath={openRenameRemotePath}
              onDeletePath={openDeleteRemotePath}
            />
            <ZmodemPanel
              panelRef={setPanelRef("zmodem")}
              mode={zmodemMode}
              message={zmodemMessage}
              localPath={transferLocalPath}
              remotePath={transferRemotePath}
              onLocalPathChange={setTransferLocalPath}
              onRemotePathChange={setTransferRemotePath}
              onUpload={() => {
                setZmodemMode("upload");
                setZmodemMessage(labels.zmodemUploadFlow);
                startTransfer("upload");
              }}
              onDownload={() => {
                setZmodemMode("download");
                setZmodemMessage(labels.zmodemDownloadFlow);
                startTransfer("download");
              }}
            />
            <RemoteEditorPanel
              path={editorPath}
              content={editorContent}
              status={editorStatus}
              error={editorError}
              onContentChange={setEditorContent}
              onSave={saveRemoteFile}
            />
            <MetricsPanel
              metrics={liveMetrics}
              history={metricHistory}
              status={metricsStatus}
              error={metricsError}
              onRefresh={refreshMetrics}
            />
            <ProcessPanel
              processes={remoteProcesses}
              status={processStatus}
              error={processError}
              onRefresh={refreshProcesses}
              onKill={killProcess}
            />
            <TunnelPanel
              panelRef={setPanelRef("tunnels")}
              draft={tunnelDraft}
              tunnels={tunnels}
              onDraftChange={setTunnelDraft}
              onStart={startTunnel}
              onStop={stopTunnel}
            />
            <RelayPanel
              draft={relayDraft}
              relays={relays}
              onDraftChange={setRelayDraft}
              onStart={startRelay}
              onStop={stopRelay}
            />
            <KeyMappingPanel profiles={snapshot.keyMappingProfiles} onChange={updateKeyMappingProfiles} />
            <ScriptRecorderPanel
              isRecording={isRecordingScript}
              eventCount={recordingEvents.length}
              recordings={snapshot.scriptRecordings}
              onStart={startScriptRecording}
              onStop={stopScriptRecording}
              onReplay={replayScriptRecording}
            />
            <LogViewerPanel
              panelRef={setPanelRef("logs")}
              title={labels.logs}
              refreshLabel={labels.refreshSessionLog}
              emptyText={labels.noMatchingLogLines}
              query={logQuery}
              lines={logLines}
              status={logStatus}
              onQueryChange={setLogQuery}
              onRefresh={refreshSessionLog}
            />
            <LogViewerPanel
              title={labels.audit}
              refreshLabel={labels.refreshAuditLog}
              emptyText={labels.noAuditEntries}
              query={auditQuery}
              lines={auditLines}
              status={auditStatus}
              onQueryChange={setAuditQuery}
              onRefresh={refreshAuditLog}
            />
            <LogViewerPanel
              title={labels.errors}
              refreshLabel={labels.refreshErrorReports}
              emptyText={labels.noErrorReports}
              query={errorQuery}
              lines={errorLines}
              status={errorStatus}
              onQueryChange={setErrorQuery}
              onRefresh={refreshErrorReports}
            />
            <QuickCommandPanel
              quickCommands={snapshot.quickCommands}
              onExecute={executeCommand}
              onManage={openNewQuickCommandEditor}
            />
            <TriggerPanel events={triggerEvents} />
            <CloudSyncPanel
              status={cloudSyncStatus}
              onExport={exportCloudSyncSettings}
              onImport={importCloudSyncSettings}
            />
            <UpdatePanel
              channel={updateChannel}
              status={updateStatus}
              onChannelChange={setUpdateChannel}
              onCheck={checkForUpdates}
              onInstall={installDownloadedUpdate}
            />
          </aside>
        </section>
      </section>
      {isCommandPaletteOpen ? (
        <CommandPalette
          commands={snapshot.quickCommands}
          query={commandQuery}
          onQueryChange={setCommandQuery}
          onExecute={executeCommand}
          onClose={() => setIsCommandPaletteOpen(false)}
        />
      ) : null}
      {bulkCommandReview ? (
        <BulkCommandConfirmation
          command={bulkCommandReview.command}
          targets={bulkCommandReview.targetSessionIds.map((sessionId) => {
            const session = sessionTabsWithStatus.find((item) => item.id === sessionId);
            return {
              id: sessionId,
              title: session?.title ?? sessionId,
              status: session?.status ?? "disconnected"
            };
          })}
          onConfirm={confirmBulkCommand}
          onCancel={cancelBulkCommand}
        />
      ) : null}
      {isSettingsOpen ? (
        <SettingsDialog
          language={language}
          onLanguageChange={changeLanguage}
          onClose={() => setIsSettingsOpen(false)}
        />
      ) : null}
      {connectionDraft ? (
        <ConnectionEditorDialog
          draft={connectionDraft}
          error={connectionFormError}
          canDelete={Boolean(connectionDraft.id) && snapshot.connections.length > 1}
          onChange={setConnectionDraft}
          onSave={saveConnectionDraft}
          onDelete={deleteActiveConnection}
          onClose={() => setConnectionDraft(null)}
        />
      ) : null}
      {quickCommandDraft ? (
        <QuickCommandManagerDialog
          commands={snapshot.quickCommands}
          draft={quickCommandDraft}
          error={quickCommandFormError}
          onDraftChange={setQuickCommandDraft}
          onNew={openNewQuickCommandEditor}
          onEdit={openQuickCommandEditor}
          onSave={saveQuickCommandDraft}
          onDelete={deleteQuickCommand}
          onClose={() => setQuickCommandDraft(null)}
        />
      ) : null}
      {remoteOperationDraft ? (
        <RemoteOperationDialog
          draft={remoteOperationDraft}
          error={remoteOperationError}
          onChange={setRemoteOperationDraft}
          onConfirm={runRemoteOperation}
          onClose={() => setRemoteOperationDraft(null)}
        />
      ) : null}
      </main>
      </AppErrorBoundary>
    </TranslationContext.Provider>
  );
}

interface ConnectionSidebarProps {
  groupedConnections: Record<string, ConnectionProfile[]>;
  activeConnectionId: string;
  query: string;
  collapsedGroups: Record<string, boolean>;
  onQueryChange: (query: string) => void;
  onCreate: () => void;
  onEditActive: () => void;
  onToggleGroup: (group: string) => void;
  onSelect: (connectionId: string) => void;
  onOpenSettings: () => void;
}

function ConnectionSidebar({
  groupedConnections,
  activeConnectionId,
  query,
  collapsedGroups,
  onQueryChange,
  onCreate,
  onEditActive,
  onToggleGroup,
  onSelect,
  onOpenSettings
}: ConnectionSidebarProps) {
  const labels = useUiStrings();
  const groupEntries = Object.entries(groupedConnections);
  return (
    <aside className="sidebar" aria-label={labels.connectionManager}>
      <div className="brand-row">
        <div className="brand-mark" aria-hidden="true">
          CN
        </div>
        <div>
          <h1>CNshell</h1>
          <p>{labels.consoleSubtitle}</p>
        </div>
      </div>

      <label className="search-box">
        <Search size={17} aria-hidden="true" />
        <span className="sr-only">{labels.searchConnections}</span>
        <input value={query} placeholder={labels.searchHostsPlaceholder} onChange={(event) => onQueryChange(event.target.value)} />
      </label>

      <div className="sidebar-actions" aria-label={labels.connectionActions}>
        <button type="button" onClick={onCreate}>
          <Plus size={16} aria-hidden="true" />
          {labels.newConnection}
        </button>
        <button type="button" aria-label={labels.editConnection} onClick={onEditActive}>
          <Edit3 size={16} aria-hidden="true" />
        </button>
        <button type="button" aria-label={labels.connectionSettings} onClick={onOpenSettings}>
          <Settings size={16} aria-hidden="true" />
        </button>
      </div>

      <nav className="connection-tree">
        {groupEntries.length === 0 ? <div className="sidebar-empty">{labels.noConnectionsFound}</div> : null}
        {groupEntries.map(([group, connections]) => {
          const isCollapsed = Boolean(collapsedGroups[group]);
          return (
            <section key={group} className="connection-group" aria-label={labels.groupAria(group)}>
            <button
              type="button"
              className="group-title"
              aria-expanded={!isCollapsed}
              aria-label={isCollapsed ? labels.expandGroup : labels.collapseGroup}
              onClick={() => onToggleGroup(group)}
            >
              {isCollapsed ? <ChevronRight size={15} aria-hidden="true" /> : <ChevronDown size={15} aria-hidden="true" />}
              {displayBuiltInGroup(group, labels)}
              <span>{connections.length}</span>
            </button>
            {isCollapsed ? null : connections.map((connection) => (
              <button
                type="button"
                key={connection.id}
                className={`connection-item ${connection.id === activeConnectionId ? "active" : ""}`}
                onClick={() => onSelect(connection.id)}
              >
                <span className="connection-color" style={{ background: connection.color }} aria-hidden="true" />
                <span className="connection-copy">
                  <strong>{displayBuiltInName(connection.name, labels)}</strong>
                  <small>
                    {connection.username}@{connection.host}
                  </small>
                </span>
                <ProtocolIcon protocol={connection.protocol} />
              </button>
            ))}
          </section>
          );
        })}
      </nav>
    </aside>
  );
}

function ProtocolIcon({ protocol }: { protocol: ConnectionProfile["protocol"] }) {
  const labels = useUiStrings();
  if (protocol === "rdp") {
    return <Monitor size={15} aria-label="RDP" />;
  }

  if (protocol === "local") {
    return <TerminalSquare size={15} aria-label={labels.localShell} />;
  }

  return <Server size={15} aria-label="SSH" />;
}

function TopBar({
  activeConnection,
  status,
  version,
  isSyncInputEnabled,
  isHighlightEnabled,
  onOpenCommandPalette,
  onToggleSyncInput,
  onToggleHighlight,
  onFocusPanel
}: {
  activeConnection: ConnectionProfile;
  status: SessionStatus;
  version: string;
  isSyncInputEnabled: boolean;
  isHighlightEnabled: boolean;
  onOpenCommandPalette: () => void;
  onToggleSyncInput: () => void;
  onToggleHighlight: () => void;
  onFocusPanel: (panel: PanelFocusKey) => void;
}) {
  const labels = useUiStrings();
  return (
    <header className="topbar">
      <div className="host-summary">
        <span className={`status-pill ${status}`}>
          <Circle size={9} fill="currentColor" aria-hidden="true" />
          {displayStatus(status, labels)}
        </span>
        <div>
          <strong>{activeConnection.name}</strong>
          <span>
            {activeConnection.protocol.toUpperCase()} / {activeConnection.host}:{activeConnection.port || labels.localProtocol}
          </span>
        </div>
      </div>
      <div className="topbar-actions">
        <button type="button" aria-label={labels.openCommandPalette} onClick={onOpenCommandPalette}>
          <Command size={17} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={isSyncInputEnabled ? "active" : ""}
          aria-label={labels.toggleSyncInput}
          aria-pressed={isSyncInputEnabled}
          onClick={onToggleSyncInput}
        >
          <SplitSquareHorizontal size={17} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={isHighlightEnabled ? "active" : ""}
          aria-label={labels.toggleHighlightRules}
          aria-pressed={isHighlightEnabled}
          onClick={onToggleHighlight}
        >
          <Zap size={17} aria-hidden="true" />
        </button>
        <button type="button" aria-label={labels.openTunnelingManager} onClick={() => onFocusPanel("tunnels")}>
          <Network size={17} aria-hidden="true" />
        </button>
        <button type="button" aria-label={labels.openCredentialVault} onClick={() => onFocusPanel("credentials")}>
          <KeyRound size={17} aria-hidden="true" />
        </button>
        <span className="version-label">v{version}</span>
      </div>
    </header>
  );
}

export function TabStrip({
  tabs,
  activeTabId,
  onSelect,
  onCreate,
  onClose
}: {
  tabs: SessionTab[];
  activeTabId: string;
  onSelect: (tabId: string) => void;
  onCreate: () => void;
  onClose: (tabId: string) => void;
}) {
  const labels = useUiStrings();
  return (
    <div className="tab-strip" role="tablist" aria-label={labels.sessionTabs}>
      {tabs.map((tab) => (
        <div key={tab.id} className={`session-tab ${tab.id === activeTabId ? "active" : ""}`}>
          <button
            type="button"
            role="tab"
            aria-selected={tab.id === activeTabId}
            className="session-tab-main"
            onClick={() => onSelect(tab.id)}
          >
            <TerminalSquare size={15} aria-hidden="true" />
            <span>{tab.title}</span>
            <small className={tab.status}>{displayStatus(tab.status, labels)}</small>
          </button>
          <button type="button" className="session-tab-close" aria-label={labels.closeSessionTab} onClick={() => onClose(tab.id)}>
            <X size={13} aria-hidden="true" />
          </button>
        </div>
      ))}
      <button type="button" className="new-tab" aria-label={labels.newSessionTab} onClick={onCreate}>
        <Plus size={16} aria-hidden="true" />
      </button>
    </div>
  );
}

function TerminalPane({
  activeConnection,
  activeTab,
  sshDraft,
  useSavedCredential,
  keyMappingProfiles,
  startToken,
  isHighlightEnabled,
  isSplitActive,
  isSecondaryPane = false,
  zmodemMode,
  onStatusChange,
  onReconnect,
  onSplit,
  onFocusPanel,
  onDispatchCommand,
  onTerminalInput,
  onTriggerEvents,
  onZmodemDetected
}: {
  activeConnection: ConnectionProfile;
  activeTab: SessionTab;
  sshDraft: { password: string; privateKey: string; passphrase: string };
  useSavedCredential: boolean;
  keyMappingProfiles: KeyMappingProfile[];
  startToken: number;
  isHighlightEnabled: boolean;
  isSplitActive: boolean;
  isSecondaryPane?: boolean;
  zmodemMode: ZmodemMode;
  onStatusChange: (sessionId: string, status: SessionStatus) => void;
  onReconnect: () => void;
  onSplit: () => void;
  onFocusPanel: (panel: PanelFocusKey) => void;
  onDispatchCommand: (command: string) => void;
  onTerminalInput: (sessionId: string, input: string, options?: { record?: boolean }) => void;
  onTriggerEvents: (events: TriggerEvent[]) => void;
  onZmodemDetected: (mode: ZmodemMode) => void;
}) {
  const labels = useUiStrings();
  const [composeValue, setComposeValue] = useState("");
  const [terminalSearch, setTerminalSearch] = useState("");
  const [isActionsOpen, setIsActionsOpen] = useState(false);
  const [searchAddon, setSearchAddon] = useState<SearchAddon | null>(null);
  const [safePasteReview, setSafePasteReview] = useState<SafePasteReview | null>(null);
  const [safePasteSessionId, setSafePasteSessionId] = useState("");
  const terminalHostRef = useRef<HTMLDivElement | null>(null);
  const activeConnectionRef = useRef(activeConnection);
  const sshDraftRef = useRef(sshDraft);
  const useSavedCredentialRef = useRef(useSavedCredential);
  const keyMappingProfilesRef = useRef(keyMappingProfiles);
  const isHighlightEnabledRef = useRef(isHighlightEnabled);
  const onStatusChangeRef = useRef(onStatusChange);
  const onTerminalInputRef = useRef(onTerminalInput);
  const onTriggerEventsRef = useRef(onTriggerEvents);
  const onZmodemDetectedRef = useRef(onZmodemDetected);
  const labelsRef = useRef(labels);

  activeConnectionRef.current = activeConnection;
  sshDraftRef.current = sshDraft;
  useSavedCredentialRef.current = useSavedCredential;
  keyMappingProfilesRef.current = keyMappingProfiles;
  isHighlightEnabledRef.current = isHighlightEnabled;
  onStatusChangeRef.current = onStatusChange;
  onTerminalInputRef.current = onTerminalInput;
  onTriggerEventsRef.current = onTriggerEvents;
  onZmodemDetectedRef.current = onZmodemDetected;
  labelsRef.current = labels;

  const sessionId = activeTab.id;
  const connectionGateways = activeConnection.gateways;
  const connectionHost = activeConnection.host;
  const connectionId = activeConnection.id;
  const connectionPort = activeConnection.port;
  const connectionProtocol = activeConnection.protocol;
  const connectionUsername = activeConnection.username;

  useEffect(() => {
    const host = connectionHost;
    const terminalHost = terminalHostRef.current;
    if (!terminalHost) {
      return;
    }

    terminalHost.innerHTML = "";
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: "'Cascadia Code', 'JetBrains Mono', Consolas, monospace",
      fontSize: 13,
      lineHeight: 1.32,
      theme: terminalTheme
    });

    const fitAddon = new FitAddon();
    const activeSearchAddon = new SearchAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(activeSearchAddon);
    setSearchAddon(activeSearchAddon);
    terminal.open(terminalHost);
    fitAddon.fit();
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") {
        return true;
      }

      const key = formatKeyEvent(event);
      const rule = getActiveKeyRules(keyMappingProfilesRef.current).find((item) => item.key === key);
      if (!rule) {
        return true;
      }

      onTerminalInputRef.current(sessionId, normalizeSendValue(rule.send));
      return false;
    });
    terminal.writeln(`\x1b[1;32m${labelsRef.current.terminalStarting}\x1b[0m`);
    terminal.writeln(`${labelsRef.current.profileLabel}: ${connectionUsername}@${host}`);
    terminal.writeln("");

    const removeDataListener = window.cnshell?.terminal.onData(({ id, data }) => {
      if (id === sessionId) {
        onTriggerEventsRef.current(detectTriggerEvents(sessionId, data));
        onZmodemDetectedRef.current(detectZmodemMode(data));
        terminal.write(isHighlightEnabledRef.current ? applyHighlightRules(data) : data);
      }
    });

    const removeExitListener = window.cnshell?.terminal.onExit(({ id, exitCode }) => {
      if (id === sessionId) {
        terminal.writeln("");
        terminal.writeln(`\x1b[33m${labelsRef.current.sessionExited(exitCode)}\x1b[0m`);
        onStatusChangeRef.current(sessionId, "disconnected");
      }
    });

    const dataDisposable = terminal.onData((data) => {
      onTerminalInputRef.current(sessionId, data);
    });

    const pasteHandler = (event: ClipboardEvent) => {
      const text = event.clipboardData?.getData("text/plain") ?? "";
      if (!text || !shouldReviewPaste(text)) {
        return;
      }

      event.preventDefault();
      setSafePasteSessionId(sessionId);
        setSafePasteReview({
          text,
          reasons: inspectPastedText(text, labelsRef.current)
        });
    };
    terminal.textarea?.addEventListener("paste", pasteHandler);

    const resizeSession = () => {
      fitAddon.fit();
      void window.cnshell?.terminal.resize({
        id: sessionId,
        cols: terminal.cols,
        rows: terminal.rows
      });
    };

    const removeErrorListener = window.cnshell?.terminal.onError(({ id, message }) => {
      if (id === sessionId) {
        terminal.writeln("");
        terminal.writeln(`\x1b[31m${message}\x1b[0m`);
        onStatusChangeRef.current(sessionId, "error");
      }
    });

    const startTerminalSession = () => {
      onStatusChangeRef.current(sessionId, "connecting");

      void window.cnshell?.terminal
        .start({
          id: sessionId,
          kind: connectionProtocol === "ssh" ? "ssh" : "local",
          cols: terminal.cols,
          rows: terminal.rows,
          ssh:
            connectionProtocol === "ssh"
              ? createSshConfig(activeConnectionRef.current, sshDraftRef.current, useSavedCredentialRef.current)
              : undefined
        })
        .then(() => onStatusChangeRef.current(sessionId, "connected"))
        .catch((error: Error) => {
          terminal.writeln(`\x1b[31m${error.message}\x1b[0m`);
          onStatusChangeRef.current(sessionId, "error");
        });
    };

    if (connectionProtocol === "ssh") {
      terminal.writeln(`\x1b[33m${labelsRef.current.sshProfileSelected}\x1b[0m`);
      if (startToken > 0) {
        startTerminalSession();
      } else {
        onStatusChangeRef.current(sessionId, "disconnected");
      }
    } else {
      if (connectionProtocol === "rdp") {
        terminal.writeln(`\x1b[33m${labelsRef.current.rdpProfileSelected}\x1b[0m`);
        onStatusChangeRef.current(sessionId, "disconnected");
      } else {
        startTerminalSession();
      }
    }

    const resizeObserver = new ResizeObserver(resizeSession);
    resizeObserver.observe(terminalHost);

    return () => {
      resizeObserver.disconnect();
      dataDisposable.dispose();
      removeDataListener?.();
      removeExitListener?.();
      removeErrorListener?.();
      terminal.textarea?.removeEventListener("paste", pasteHandler);
      void window.cnshell?.terminal.stop(sessionId);
      onStatusChangeRef.current(sessionId, "disconnected");
      setSearchAddon(null);
      terminal.dispose();
    };
  }, [
    connectionGateways,
    connectionHost,
    connectionId,
    connectionPort,
    connectionProtocol,
    connectionUsername,
    sessionId,
    startToken
  ]);

  const sendComposeValue = () => {
    const command = composeValue.trim();
    if (!command) {
      return;
    }

    onDispatchCommand(command);
    setComposeValue("");
  };

  const findNext = () => {
    if (terminalSearch.trim()) {
      searchAddon?.findNext(terminalSearch);
    }
  };

  const approveSafePaste = () => {
    if (!safePasteReview) {
      return;
    }

    onTerminalInput(safePasteSessionId || activeTab.id, safePasteReview.text);
    setSafePasteReview(null);
    setSafePasteSessionId("");
  };

  const cancelSafePaste = () => {
    setSafePasteReview(null);
    setSafePasteSessionId("");
  };

  return (
    <section className="terminal-workbench" aria-label={labels.terminalWorkbench}>
      <div className="terminal-toolbar">
        <div className="breadcrumb">
          <HardDrive size={16} aria-hidden="true" />
          <span>{activeTab.cwd}</span>
        </div>
        <div className="terminal-tools">
          <label className="terminal-search">
            <Search size={15} aria-hidden="true" />
            <input
              value={terminalSearch}
              placeholder={labels.terminalSearchPlaceholder}
              onChange={(event) => setTerminalSearch(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  findNext();
                }
              }}
            />
          </label>
          <button type="button" onClick={findNext}>
            {labels.find}
          </button>
          <button type="button" className={isSplitActive ? "active" : ""} aria-pressed={isSplitActive} onClick={onSplit}>
            <SplitSquareHorizontal size={16} aria-hidden="true" />
            {isSplitActive ? labels.unsplit : labels.split}
          </button>
          <button type="button" className={zmodemMode !== "idle" ? "active" : ""} onClick={() => onFocusPanel("zmodem")}>
            <UploadCloud size={16} aria-hidden="true" />
            ZMODEM
          </button>
          <button type="button" onClick={onReconnect}>
            <RefreshCw size={16} aria-hidden="true" />
            {labels.reconnect}
          </button>
          <button
            type="button"
            aria-label={labels.moreTerminalActions}
            aria-expanded={isActionsOpen}
            onClick={() => setIsActionsOpen((current) => !current)}
          >
            <MoreHorizontal size={16} aria-hidden="true" />
          </button>
          {isActionsOpen ? (
            <div className="terminal-action-menu" role="menu" aria-label={labels.terminalActions}>
              <button type="button" role="menuitem" onClick={() => onFocusPanel("logs")}>
                <FileText size={15} aria-hidden="true" />
                {labels.openLogsPanel}
              </button>
              <button type="button" role="menuitem" onClick={() => onFocusPanel("zmodem")}>
                <UploadCloud size={15} aria-hidden="true" />
                {labels.openZmodemPanel}
              </button>
              <button type="button" role="menuitem" onClick={onReconnect}>
                <RefreshCw size={15} aria-hidden="true" />
                {labels.reconnect}
              </button>
              <span>{labels.clearTerminalHint}</span>
            </div>
          ) : null}
        </div>
      </div>
      <div className="terminal-surface">
        {isSecondaryPane ? (
          <div className="split-session-banner">
            <SplitSquareHorizontal size={15} aria-hidden="true" />
            <span>{labels.splitPaneEnabled}</span>
          </div>
        ) : null}
        <div ref={terminalHostRef} className="terminal-host" />
      </div>
      {safePasteReview ? (
        <div className="safe-paste-review" role="alert">
          <div>
            <strong>{labels.reviewPaste}</strong>
            <span>{safePasteReview.reasons.join(" / ")}</span>
          </div>
          <pre>{safePasteReview.text.slice(0, 420)}</pre>
          <button type="button" onClick={approveSafePaste}>
            {labels.paste}
          </button>
          <button type="button" onClick={cancelSafePaste}>
            {labels.cancel}
          </button>
        </div>
      ) : null}
      <div className="compose-pane">
        <div>
          <Code2 size={16} aria-hidden="true" />
          <span>{labels.composePane}</span>
        </div>
        <textarea
          value={composeValue}
          placeholder={labels.composePlaceholder}
          onChange={(event) => setComposeValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
              event.preventDefault();
              sendComposeValue();
            }
          }}
        />
        <button type="button" onClick={sendComposeValue}>
          {labels.send}
        </button>
      </div>
    </section>
  );
}

function SshCredentialPanel({
  panelRef,
  authMethod,
  draft,
  credentialStatus,
  credentialError,
  vaultStatus,
  vaultPassword,
  vaultError,
  privateKeyImportStatus,
  hostKeyPrompt,
  onChange,
  onVaultPasswordChange,
  onImportPrivateKey,
  onConnect,
  onSaveCredential,
  onDeleteCredential,
  onEnableVault,
  onUnlockVault,
  onLockVault,
  onDisableVault,
  onTrustHost
}: {
  panelRef?: (element: HTMLElement | null) => void;
  authMethod: ConnectionProfile["authMethod"];
  draft: { password: string; privateKey: string; passphrase: string };
  credentialStatus?: CredentialStatus;
  credentialError?: string;
  vaultStatus: CredentialVaultStatus | null;
  vaultPassword: string;
  vaultError: string;
  privateKeyImportStatus: string;
  hostKeyPrompt?: HostKeyVerificationEvent;
  onChange: (field: "password" | "privateKey" | "passphrase", value: string) => void;
  onVaultPasswordChange: (value: string) => void;
  onImportPrivateKey: () => void;
  onConnect: () => void;
  onSaveCredential: () => void;
  onDeleteCredential: () => void;
  onEnableVault: () => void;
  onUnlockVault: () => void;
  onLockVault: () => void;
  onDisableVault: () => void;
  onTrustHost: () => void;
}) {
  const labels = useUiStrings();
  const hasDraftSecret = Boolean(draft.password || draft.privateKey);
  const isVaultMasterMode = vaultStatus?.mode === "master";
  const isVaultLocked = Boolean(vaultStatus?.locked);
  const hasVaultPassword = Boolean(vaultPassword.trim());
  const isEncryptionUnavailable = credentialStatus?.encryptionAvailable === false || vaultStatus?.encryptionAvailable === false;

  return (
    <section ref={panelRef} className="panel-section ssh-panel" aria-label={labels.sshCredentials}>
      <div className="panel-heading">
        <div>
          <KeyRound size={16} aria-hidden="true" />
          <h2>{labels.sshLogin}</h2>
        </div>
        <span className="poll-rate">{authMethod}</span>
      </div>
      <div className="ssh-form">
        <div className="credential-status-row">
          <span className={credentialStatus?.hasCredential ? "saved" : ""}>
            {credentialStatus?.hasCredential ? labels.savedCredentialAvailable : labels.noSavedCredential}
          </span>
          {credentialStatus?.hasCredential ? <small>{credentialStatus.protection}</small> : null}
          {isEncryptionUnavailable ? <small>{labels.encryptionUnavailable}</small> : null}
        </div>
        {credentialError ? (
          <div className="credential-error" role="alert">
            {credentialError}
          </div>
        ) : null}
        <div className="credential-vault-panel">
          <div className="credential-vault-state">
            <span>{labels.vault}</span>
            <strong>{isVaultMasterMode ? labels.masterPassword : labels.systemKeyring}</strong>
            <small>{isVaultMasterMode ? (isVaultLocked ? labels.locked : labels.unlocked) : labels.active}</small>
          </div>
          <label className="credential-vault-password">
            <span>{labels.masterPassword}</span>
            <input
              type="password"
              value={vaultPassword}
              placeholder={isVaultMasterMode ? labels.enterMasterPassword : labels.newMasterPassword}
              onChange={(event) => onVaultPasswordChange(event.target.value)}
            />
          </label>
          {vaultError ? (
            <div className="credential-error" role="alert">
              {vaultError}
            </div>
          ) : null}
          <div className="credential-vault-actions">
            <button type="button" disabled={isEncryptionUnavailable || isVaultMasterMode || !hasVaultPassword} onClick={onEnableVault}>
              {labels.enable}
            </button>
            <button type="button" disabled={!isVaultMasterMode || !isVaultLocked || !hasVaultPassword} onClick={onUnlockVault}>
              {labels.unlock}
            </button>
            <button type="button" disabled={!isVaultMasterMode || isVaultLocked} onClick={onLockVault}>
              {labels.lock}
            </button>
            <button type="button" disabled={!isVaultMasterMode || (isVaultLocked && !hasVaultPassword)} onClick={onDisableVault}>
              {labels.disable}
            </button>
          </div>
        </div>
        {hostKeyPrompt ? (
          <div className={`host-key-prompt ${hostKeyPrompt.status}`} role="alert">
            <strong>{hostKeyPrompt.status === "changed" ? labels.hostKeyChanged : labels.unknownHostKey}</strong>
            <span>
              {hostKeyPrompt.host}:{hostKeyPrompt.port}
            </span>
            <code>{hostKeyPrompt.fingerprint}</code>
            {hostKeyPrompt.expectedFingerprint ? <small>{labels.expectedFingerprint(hostKeyPrompt.expectedFingerprint)}</small> : null}
            <button type="button" disabled={hostKeyPrompt.status === "changed"} onClick={onTrustHost}>
              <ShieldCheck size={16} aria-hidden="true" />
              {labels.trustAndReconnect}
            </button>
          </div>
        ) : null}
        <label>
          <span>{labels.password}</span>
          <input
            type="password"
            value={draft.password}
            placeholder={labels.sessionOnly}
            onChange={(event) => onChange("password", event.target.value)}
          />
        </label>
        <label>
          <span className="private-key-label">
            {labels.privateKey}
            <button type="button" onClick={onImportPrivateKey}>
              <UploadCloud size={14} aria-hidden="true" />
              {labels.import}
            </button>
          </span>
          <textarea
            value={draft.privateKey}
            placeholder={labels.pastePrivateKey}
            onChange={(event) => onChange("privateKey", event.target.value)}
          />
        </label>
        {privateKeyImportStatus ? <div className="private-key-import-status">{privateKeyImportStatus}</div> : null}
        <label>
          <span>{labels.passphrase}</span>
          <input
            type="password"
            value={draft.passphrase}
            placeholder={labels.encryptedPrivateKeys}
            onChange={(event) => onChange("passphrase", event.target.value)}
          />
        </label>
        <button type="button" onClick={onConnect}>
          <TerminalSquare size={16} aria-hidden="true" />
          {labels.connect}
        </button>
        <div className="credential-actions">
          <button
            type="button"
            disabled={!hasDraftSecret || isEncryptionUnavailable || (isVaultMasterMode && isVaultLocked)}
            onClick={onSaveCredential}
          >
            <ShieldCheck size={16} aria-hidden="true" />
            {labels.saveCredential}
          </button>
          <button type="button" disabled={!credentialStatus?.hasCredential} onClick={onDeleteCredential}>
            {labels.deleteSaved}
          </button>
        </div>
      </div>
    </section>
  );
}

function RdpPanel({
  connection,
  status,
  error,
  onOpen
}: {
  connection: ConnectionProfile;
  status: "idle" | "launching" | "error";
  error: string;
  onOpen: () => void;
}) {
  const labels = useUiStrings();
  return (
    <section className="panel-section rdp-panel" aria-label={labels.rdpConnection}>
      <div className="panel-heading">
        <div>
          <Monitor size={16} aria-hidden="true" />
          <h2>RDP</h2>
        </div>
        <span className={`rdp-status ${status}`}>{status}</span>
      </div>
      <div className="rdp-body">
        <div className="rdp-target">
          <strong>{connection.host}:{connection.port || 3389}</strong>
          <span>{connection.username}</span>
        </div>
        {error ? (
          <div className="credential-error" role="alert">
            {error}
          </div>
        ) : null}
        <button type="button" onClick={onOpen}>
          <Monitor size={16} aria-hidden="true" />
          {labels.openRemoteDesktop}
        </button>
      </div>
    </section>
  );
}

function JumpHostPanel({
  gateways,
  onChange
}: {
  gateways: JumpHostConfig[];
  onChange: (gateways: JumpHostConfig[]) => void;
}) {
  const labels = useUiStrings();
  const addGateway = () => {
    onChange([
      ...gateways,
      {
        id: `gateway-${Date.now()}`,
        name: `jump-${gateways.length + 1}`,
        host: "127.0.0.1",
        port: 22,
        username: "deploy"
      }
    ]);
  };

  const updateGateway = (id: string, patch: Partial<JumpHostConfig>) => {
    onChange(gateways.map((gateway) => (gateway.id === id ? { ...gateway, ...patch } : gateway)));
  };

  const removeGateway = (id: string) => {
    onChange(gateways.filter((gateway) => gateway.id !== id));
  };

  return (
    <section className="panel-section" aria-label={labels.jumpHostProxy}>
      <div className="panel-heading">
        <div>
          <SplitSquareHorizontal size={16} aria-hidden="true" />
          <h2>{labels.jumpHosts}</h2>
        </div>
        <button type="button" aria-label={labels.addJumpHost} onClick={addGateway}>
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="jump-host-list">
        {gateways.length === 0 ? (
          <div className="trigger-empty">{labels.directSshConnection}</div>
        ) : (
          gateways.map((gateway, index) => (
            <div key={gateway.id} className="jump-host-row">
              <strong>{index + 1}</strong>
              <input
                value={gateway.name}
                placeholder={labels.name}
                aria-label={`${labels.jumpHosts} ${index + 1} ${labels.name}`}
                onChange={(event) => updateGateway(gateway.id, { name: event.target.value })}
              />
              <input
                value={gateway.host}
                placeholder={labels.host}
                aria-label={`${labels.jumpHosts} ${index + 1} ${labels.host}`}
                onChange={(event) => updateGateway(gateway.id, { host: event.target.value })}
              />
              <input
                value={gateway.port}
                placeholder={labels.port}
                aria-label={`${labels.jumpHosts} ${index + 1} ${labels.port}`}
                onChange={(event) => updateGateway(gateway.id, { port: Number(event.target.value) || 22 })}
              />
              <input
                value={gateway.username}
                placeholder={labels.user}
                aria-label={`${labels.jumpHosts} ${index + 1} ${labels.user}`}
                onChange={(event) => updateGateway(gateway.id, { username: event.target.value })}
              />
              <button type="button" onClick={() => removeGateway(gateway.id)}>
                {labels.remove}
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

export function FilePanel({
  remoteFiles,
  path,
  status,
  error,
  localPath,
  transferRemotePath,
  transferJobs,
  onPathChange,
  onLocalPathChange,
  onTransferRemotePathChange,
  onRefresh,
  onTransfer,
  onOpenFile,
  onCreateDirectory,
  onRenamePath,
  onDeletePath
}: {
  remoteFiles: ReturnType<typeof createInitialAppSnapshot>["remoteFiles"];
  path: string;
  status: "idle" | "loading" | "error";
  error: string;
  localPath: string;
  transferRemotePath: string;
  transferJobs: TransferJob[];
  onPathChange: (path: string) => void;
  onLocalPathChange: (path: string) => void;
  onTransferRemotePathChange: (path: string) => void;
  onRefresh: () => void;
  onTransfer: (direction: "upload" | "download") => void;
  onOpenFile: (path: string) => void;
  onCreateDirectory: () => void;
  onRenamePath: (path: string) => void;
  onDeletePath: (path: string) => void;
}) {
  const labels = useUiStrings();
  return (
    <section className="panel-section file-panel" aria-label={labels.remoteFiles}>
      <div className="panel-heading">
        <div>
          <FileText size={16} aria-hidden="true" />
          <h2>SFTP</h2>
        </div>
        <div className="panel-actions">
          <span className="sync-pill">{labels.cwdSync}</span>
          <button type="button" aria-label={labels.createRemoteDirectory} onClick={onCreateDirectory}>
            <Plus size={16} aria-hidden="true" />
          </button>
          <button type="button" aria-label={labels.refreshRemoteFiles} onClick={onRefresh}>
            <RefreshCw size={16} aria-hidden="true" />
          </button>
        </div>
      </div>
      <label className="path-row">
        <span className="sr-only">{labels.remotePath}</span>
        <input value={path} onChange={(event) => onPathChange(event.target.value)} />
      </label>
      {status === "loading" ? <div className="sftp-state">{labels.loadingRemoteDirectory}</div> : null}
      {status === "error" ? (
        <div className="sftp-state error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="transfer-box">
        <label>
          <span>{labels.localPath}</span>
          <input value={localPath} onChange={(event) => onLocalPathChange(event.target.value)} />
        </label>
        <label>
          <span>{labels.remotePath}</span>
          <input value={transferRemotePath} onChange={(event) => onTransferRemotePathChange(event.target.value)} />
        </label>
        <div className="transfer-actions">
          <button type="button" onClick={() => onTransfer("upload")}>
            {labels.upload}
          </button>
          <button type="button" onClick={() => onTransfer("download")}>
            {labels.download}
          </button>
        </div>
      </div>
      <div className="file-list">
        {remoteFiles.map((file) => (
          <div key={file.id} className="file-row">
            <button
              type="button"
              className="file-row-main"
              onClick={() => {
                if (file.type === "directory") {
                  onPathChange(file.path);
                  return;
                }

                onOpenFile(file.path);
              }}
            >
              <Folder size={16} aria-hidden="true" />
              <span>
                <strong>{file.name}</strong>
                <small>
                  {file.mode} / {file.modifiedAt}
                </small>
              </span>
              <em>{file.type === "directory" ? "-" : `${Math.max(1, Math.round(file.size / 1024))} KB`}</em>
            </button>
            <div className="file-row-actions">
              <button type="button" onClick={() => onRenamePath(file.path)}>
                {labels.renameRemotePath}
              </button>
              <button type="button" onClick={() => onDeletePath(file.path)}>
                {labels.deleteRemotePath}
              </button>
            </div>
          </div>
        ))}
      </div>
      {transferJobs.length > 0 ? (
        <div className="transfer-list">
          {transferJobs.map((job) => (
            <div key={job.id} className={`transfer-row ${job.status}`}>
              <strong>{labels.transferDirection[job.direction]}</strong>
              <span>{job.direction === "upload" ? job.localPath : job.remotePath}</span>
              <small>{job.message ?? job.status}</small>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function ZmodemPanel({
  panelRef,
  mode,
  message,
  localPath,
  remotePath,
  onLocalPathChange,
  onRemotePathChange,
  onUpload,
  onDownload
}: {
  panelRef?: (element: HTMLElement | null) => void;
  mode: ZmodemMode;
  message: string;
  localPath: string;
  remotePath: string;
  onLocalPathChange: (path: string) => void;
  onRemotePathChange: (path: string) => void;
  onUpload: () => void;
  onDownload: () => void;
}) {
  const labels = useUiStrings();
  return (
    <section ref={panelRef} className="panel-section" aria-label={labels.zmodemTransfer}>
      <div className="panel-heading">
        <div>
          <UploadCloud size={16} aria-hidden="true" />
          <h2>ZMODEM</h2>
        </div>
        <span className={`zmodem-pill ${mode}`}>{displayMode(mode, labels)}</span>
      </div>
      <div className="zmodem-panel">
        <div className="zmodem-state">{message}</div>
        <input value={localPath} placeholder={labels.localFilePath} onChange={(event) => onLocalPathChange(event.target.value)} />
        <input value={remotePath} placeholder={labels.remoteFilePath} onChange={(event) => onRemotePathChange(event.target.value)} />
        <div className="zmodem-actions">
          <button type="button" onClick={onUpload}>
            {labels.upload}
          </button>
          <button type="button" onClick={onDownload}>
            {labels.download}
          </button>
        </div>
      </div>
    </section>
  );
}

function RemoteEditorPanel({
  path,
  content,
  status,
  error,
  onContentChange,
  onSave
}: {
  path: string;
  content: string;
  status: "idle" | "loading" | "saving" | "error" | "saved";
  error: string;
  onContentChange: (content: string) => void;
  onSave: () => void;
}) {
  const labels = useUiStrings();
  return (
    <section className="panel-section remote-editor" aria-label={labels.remoteEditor}>
      <div className="panel-heading">
        <div>
          <Code2 size={16} aria-hidden="true" />
          <h2>{labels.editor}</h2>
        </div>
        <button type="button" disabled={!path || status === "loading" || status === "saving"} onClick={onSave}>
          {labels.save}
        </button>
      </div>
      <div className="remote-editor-body">
        <div className={`editor-status ${status}`}>
          <span>{path || labels.noFileSelected}</span>
          <small>{error || status}</small>
        </div>
        <textarea
          value={content}
          placeholder={labels.selectRemoteFile}
          disabled={!path || status === "loading"}
          onChange={(event) => onContentChange(event.target.value)}
        />
      </div>
    </section>
  );
}

function MetricsPanel({
  metrics,
  history,
  status,
  error,
  onRefresh
}: {
  metrics: ReturnType<typeof createInitialAppSnapshot>["serverMetrics"];
  history: MetricHistoryPoint[];
  status: "idle" | "loading" | "error";
  error: string;
  onRefresh: () => void;
}) {
  const labels = useUiStrings();
  const chartSeries = [
    { label: "CPU", key: "cpu" as const, unit: "%", max: 100 },
    { label: "Memory", key: "memory" as const, unit: "%", max: 100 },
    { label: "Disk", key: "disk" as const, unit: "%", max: 100 },
    { label: "Network", key: "network" as const, unit: "ms", max: 200 },
    { label: labels.metricProcesses, key: "processes" as const, unit: "", max: Math.max(20, ...history.map((point) => point.processes)) }
  ];

  return (
    <section className="panel-section" aria-label={labels.serverMetrics}>
      <div className="panel-heading">
        <div>
          <Activity size={16} aria-hidden="true" />
          <h2>{labels.monitor}</h2>
        </div>
        <button type="button" aria-label={labels.refreshMetrics} onClick={onRefresh}>
          <RefreshCw size={16} aria-hidden="true" />
        </button>
      </div>
      {status === "loading" ? <div className="sftp-state">{labels.collectingMetrics}</div> : null}
      {status === "error" ? (
        <div className="sftp-state error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="metric-grid">
        {metrics.map((metric) => (
          <article key={metric.label} className="metric-tile">
            <span>{displayMetricLabel(metric.label, labels)}</span>
            <strong>
              {metric.value}
              {metric.unit}
            </strong>
            <div className={`metric-bar ${metric.trend}`}>
              <span style={{ width: `${Math.min(metric.value, 100)}%` }} />
            </div>
          </article>
        ))}
      </div>
      <div className="monitor-chart-grid">
        {chartSeries.map((series) => (
          <MetricSparkline
            key={series.label}
            label={series.label}
            unit={series.unit}
            max={series.max}
            values={history.map((point) => point[series.key])}
          />
        ))}
      </div>
    </section>
  );
}

function MetricSparkline({ label, unit, max, values }: { label: string; unit: string; max: number; values: number[] }) {
  const points =
    values.length === 0
      ? ""
      : values
          .map((value, index) => {
            const x = values.length === 1 ? 100 : (index / (values.length - 1)) * 100;
            const y = 34 - Math.min(value / max, 1) * 30;
            return `${x.toFixed(1)},${y.toFixed(1)}`;
          })
          .join(" ");
  const latest = values.at(-1) ?? 0;

  return (
    <article className="monitor-chart">
      <div>
        <span>{label}</span>
        <strong>
          {Math.round(latest)}
          {unit}
        </strong>
      </div>
      <svg viewBox="0 0 100 36" preserveAspectRatio="none" aria-hidden="true">
        <polyline points={points} />
      </svg>
    </article>
  );
}

export function QuickCommandPanel({
  quickCommands,
  onExecute,
  onManage
}: {
  quickCommands: ReturnType<typeof createInitialAppSnapshot>["quickCommands"];
  onExecute: (command: string) => void;
  onManage: () => void;
}) {
  const labels = useUiStrings();
  return (
    <section className="panel-section" aria-label={labels.quickCommands}>
      <div className="panel-heading">
        <div>
          <Zap size={16} aria-hidden="true" />
          <h2>{labels.quickCommands}</h2>
        </div>
        <button type="button" aria-label={labels.manageQuickCommands} onClick={onManage}>
          <LayoutDashboard size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="quick-list">
        {quickCommands.length === 0 ? <div className="trigger-empty">{labels.noQuickCommands}</div> : null}
        {quickCommands.map((command) => (
          <button type="button" key={command.id} className="quick-command" onClick={() => onExecute(command.command)}>
            <span>
              <strong>{displayBuiltInName(command.title, labels)}</strong>
              <small>{command.command}</small>
            </span>
            <ShieldCheck size={15} aria-label={labels.scope[command.scope]} />
          </button>
        ))}
      </div>
    </section>
  );
}

function TriggerPanel({ events }: { events: TriggerEvent[] }) {
  const labels = useUiStrings();
  return (
    <section className="panel-section" aria-label={labels.triggerEvents}>
      <div className="panel-heading">
        <div>
          <Zap size={16} aria-hidden="true" />
          <h2>{labels.triggers}</h2>
        </div>
        <span className="poll-rate">{events.length}</span>
      </div>
      <div className="trigger-list">
        {events.length === 0 ? (
          <div className="trigger-empty">{labels.noTriggerEvents}</div>
        ) : (
          events.map((event) => (
            <div key={event.id} className={`trigger-row ${event.severity}`}>
              <strong>{labels.severity[event.severity]}</strong>
              <span>{event.message}</span>
              <small>{event.createdAt}</small>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function ProcessPanel({
  processes,
  status,
  error,
  onRefresh,
  onKill
}: {
  processes: ReturnType<typeof createInitialAppSnapshot>["remoteProcesses"];
  status: "idle" | "loading" | "error";
  error: string;
  onRefresh: () => void;
  onKill: (pid: number) => void;
}) {
  const labels = useUiStrings();
  return (
    <section className="panel-section" aria-label={labels.processManager}>
      <div className="panel-heading">
        <div>
          <Activity size={16} aria-hidden="true" />
          <h2>{labels.processes}</h2>
        </div>
        <button type="button" aria-label={labels.refreshProcesses} onClick={onRefresh}>
          <RefreshCw size={16} aria-hidden="true" />
        </button>
      </div>
      {status === "loading" ? <div className="sftp-state">{labels.loadingProcesses}</div> : null}
      {status === "error" ? (
        <div className="sftp-state error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="process-list">
        {processes.length === 0 ? (
          <div className="trigger-empty">{labels.noProcessData}</div>
        ) : (
          processes.map((process) => (
            <div key={process.pid} className="process-row">
              <strong>{process.pid}</strong>
              <span title={process.args || process.command}>{process.command}</span>
              <small>{process.cpu.toFixed(1)}%</small>
              <small>{process.memory.toFixed(1)}%</small>
              <button type="button" onClick={() => onKill(process.pid)}>
                {labels.terminate}
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function TunnelPanel({
  panelRef,
  draft,
  tunnels,
  onDraftChange,
  onStart,
  onStop
}: {
  panelRef?: (element: HTMLElement | null) => void;
  draft: TunnelDraft;
  tunnels: TunnelInfo[];
  onDraftChange: (draft: TunnelDraft) => void;
  onStart: () => void;
  onStop: (id: string) => void;
}) {
  const labels = useUiStrings();
  const requiresTarget = draft.mode !== "dynamic";

  return (
    <section ref={panelRef} className="panel-section" aria-label={labels.sshTunnels}>
      <div className="panel-heading">
        <div>
          <Network size={16} aria-hidden="true" />
          <h2>{labels.tunnels}</h2>
        </div>
        <button type="button" aria-label={labels.startTunnel} onClick={onStart}>
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="tunnel-mode-switch" role="tablist" aria-label={labels.tunnelModeAria}>
        {tunnelModes.map((mode) => (
          <button
            key={mode.value}
            type="button"
            aria-pressed={draft.mode === mode.value}
            onClick={() => onDraftChange({ ...draft, mode: mode.value })}
          >
            {labels.tunnelMode[mode.value]}
          </button>
        ))}
      </div>
      <div className="tunnel-form">
        <input
          value={draft.bindHost}
          placeholder={draft.mode === "remote" ? labels.remoteBind : labels.localBind}
          onChange={(event) => onDraftChange({ ...draft, bindHost: event.target.value })}
        />
        <input
          value={draft.bindPort}
          placeholder={draft.mode === "remote" ? labels.remotePort : labels.localPort}
          onChange={(event) => onDraftChange({ ...draft, bindPort: event.target.value })}
        />
        <input
          value={draft.targetHost}
          placeholder={requiresTarget ? labels.targetHost : labels.socksTarget}
          disabled={!requiresTarget}
          onChange={(event) => onDraftChange({ ...draft, targetHost: event.target.value })}
        />
        <input
          value={draft.targetPort}
          placeholder={labels.targetPort}
          disabled={!requiresTarget}
          onChange={(event) => onDraftChange({ ...draft, targetPort: event.target.value })}
        />
      </div>
      <div className="tunnel-list">
        {tunnels.length === 0 ? (
          <div className="trigger-empty">{labels.noActiveTunnels}</div>
        ) : (
          tunnels.map((tunnel) => (
            <div key={tunnel.id} className={`tunnel-row ${tunnel.status}`}>
              <strong>{tunnel.mode}</strong>
              <span>{describeTunnel(tunnel)}</span>
              <small>{tunnel.message ?? labels.tunnelStatus[tunnel.status]}</small>
              <button type="button" onClick={() => onStop(tunnel.id)}>
                {labels.stop}
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function RelayPanel({
  draft,
  relays,
  onDraftChange,
  onStart,
  onStop
}: {
  draft: { relayHost: string; relayPort: string; targetHost: string; targetPort: string };
  relays: RelayInfo[];
  onDraftChange: (draft: { relayHost: string; relayPort: string; targetHost: string; targetPort: string }) => void;
  onStart: () => void;
  onStop: (id: string) => void;
}) {
  const labels = useUiStrings();
  return (
    <section className="panel-section" aria-label={labels.cnRelay}>
      <div className="panel-heading">
        <div>
          <Network size={16} aria-hidden="true" />
          <h2>{labels.cnRelay}</h2>
        </div>
        <button type="button" aria-label={labels.startRelay} onClick={onStart}>
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="relay-form">
        <input
          value={draft.relayHost}
          placeholder={labels.relayBind}
          onChange={(event) => onDraftChange({ ...draft, relayHost: event.target.value })}
        />
        <input
          value={draft.relayPort}
          placeholder={labels.relayPort}
          onChange={(event) => onDraftChange({ ...draft, relayPort: event.target.value })}
        />
        <input
          value={draft.targetHost}
          placeholder={labels.intranetHost}
          onChange={(event) => onDraftChange({ ...draft, targetHost: event.target.value })}
        />
        <input
          value={draft.targetPort}
          placeholder={labels.targetPort}
          onChange={(event) => onDraftChange({ ...draft, targetPort: event.target.value })}
        />
      </div>
      <div className="relay-list">
        {relays.length === 0 ? (
          <div className="trigger-empty">{labels.noRelayTunnels}</div>
        ) : (
          relays.map((relay) => (
            <div key={relay.id} className={`relay-row ${relay.status}`}>
              <span>{`${relay.relayHost}:${relay.relayPort} -> ${relay.targetHost}:${relay.targetPort}`}</span>
              <small>{relay.message ?? labels.tunnelStatus[relay.status]}</small>
              <button type="button" onClick={() => onStop(relay.id)}>
                {labels.stop}
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function KeyMappingPanel({
  profiles,
  onChange
}: {
  profiles: KeyMappingProfile[];
  onChange: (profiles: KeyMappingProfile[]) => void;
}) {
  const labels = useUiStrings();
  const activeProfile = profiles[0];

  const updateProfile = (patch: Partial<KeyMappingProfile>) => {
    if (!activeProfile) {
      return;
    }

    onChange(profiles.map((profile) => (profile.id === activeProfile.id ? { ...profile, ...patch } : profile)));
  };

  const updateRule = (ruleId: string, patch: Partial<KeyMappingRule>) => {
    if (!activeProfile) {
      return;
    }

    updateProfile({
      rules: activeProfile.rules.map((rule) => (rule.id === ruleId ? { ...rule, ...patch } : rule))
    });
  };

  const addRule = () => {
    if (!activeProfile) {
      return;
    }

    updateProfile({
      rules: [
        ...activeProfile.rules,
        {
          id: `key-rule-${Date.now()}`,
          key: "Ctrl+K",
          send: "\\r",
          description: labels.customMapping,
          enabled: true
        }
      ]
    });
  };

  const removeRule = (ruleId: string) => {
    if (!activeProfile) {
      return;
    }

    updateProfile({
      rules: activeProfile.rules.filter((rule) => rule.id !== ruleId)
    });
  };

  return (
    <section className="panel-section" aria-label={labels.keyMappingProfiles}>
      <div className="panel-heading">
        <div>
          <Command size={16} aria-hidden="true" />
          <h2>{labels.keyMap}</h2>
        </div>
        <button type="button" aria-label={labels.addKeyMapping} onClick={addRule}>
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
      {activeProfile ? (
        <div className="keymap-panel">
          <label className="keymap-profile-toggle">
            <input
              type="checkbox"
              checked={activeProfile.enabled}
              onChange={(event) => updateProfile({ enabled: event.target.checked })}
            />
            <span>{displayBuiltInName(activeProfile.name, labels)}</span>
          </label>
          <div className="keymap-list">
            {activeProfile.rules.map((rule) => (
              <div key={rule.id} className="keymap-row">
                <input
                  value={rule.key}
                  aria-label={labels.shortcutAria(displayBuiltInName(rule.description, labels))}
                  onChange={(event) => updateRule(rule.id, { key: event.target.value })}
                />
                <input
                  value={rule.send}
                  aria-label={labels.sendSequenceAria(displayBuiltInName(rule.description, labels))}
                  onChange={(event) => updateRule(rule.id, { send: event.target.value })}
                />
                <input
                  value={rule.description}
                  aria-label={labels.keyMappingDescription}
                  onChange={(event) => updateRule(rule.id, { description: event.target.value })}
                />
                <label className="keymap-enabled">
                  <input
                    type="checkbox"
                    checked={rule.enabled}
                    onChange={(event) => updateRule(rule.id, { enabled: event.target.checked })}
                  />
                </label>
                <button type="button" onClick={() => removeRule(rule.id)}>
                  {labels.remove}
                </button>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div className="trigger-empty">{labels.noKeyMappingProfile}</div>
      )}
    </section>
  );
}

function ScriptRecorderPanel({
  isRecording,
  eventCount,
  recordings,
  onStart,
  onStop,
  onReplay
}: {
  isRecording: boolean;
  eventCount: number;
  recordings: ScriptRecording[];
  onStart: () => void;
  onStop: () => void;
  onReplay: (recording: ScriptRecording) => void;
}) {
  const labels = useUiStrings();
  return (
    <section className="panel-section" aria-label={labels.scriptRecorder}>
      <div className="panel-heading">
        <div>
          <FileText size={16} aria-hidden="true" />
          <h2>{labels.scripts}</h2>
        </div>
        <span className={`recording-pill ${isRecording ? "active" : ""}`}>{isRecording ? labels.recording : labels.idle}</span>
      </div>
      <div className="script-recorder">
        <div className="script-actions">
          <button type="button" disabled={isRecording} onClick={onStart}>
            {labels.record}
          </button>
          <button type="button" disabled={!isRecording} onClick={onStop}>
            {labels.stop}
          </button>
          <span>{labels.eventCount(eventCount)}</span>
        </div>
        <div className="script-list">
          {recordings.length === 0 ? (
            <div className="trigger-empty">{labels.noRecordedScripts}</div>
          ) : (
            recordings.slice(0, 4).map((recording) => (
              <div key={recording.id} className="script-row">
                <div>
                  <strong>{recording.name}</strong>
                  <small>
                    {labels.eventCount(recording.events.length)} / {new Date(recording.createdAt).toLocaleDateString()}
                  </small>
                </div>
                <button type="button" onClick={() => onReplay(recording)}>
                  {labels.replay}
                </button>
              </div>
            ))
          )}
        </div>
      </div>
    </section>
  );
}

export function LogViewerPanel({
  panelRef,
  title,
  refreshLabel,
  emptyText,
  query,
  lines,
  status,
  onQueryChange,
  onRefresh
}: {
  panelRef?: (element: HTMLElement | null) => void;
  title: string;
  refreshLabel: string;
  emptyText: string;
  query: string;
  lines: string[];
  status: "idle" | "loading" | "error";
  onQueryChange: (query: string) => void;
  onRefresh: () => void;
}) {
  const labels = useUiStrings();
  return (
    <section ref={panelRef} className="panel-section" aria-label={title}>
      <div className="panel-heading">
        <div>
          <FileText size={16} aria-hidden="true" />
          <h2>{title}</h2>
        </div>
        <button type="button" aria-label={refreshLabel} onClick={onRefresh}>
          <RefreshCw size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="log-viewer">
        <input
          value={query}
          placeholder={labels.filterLogLines}
          onChange={(event) => onQueryChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onRefresh();
            }
          }}
        />
        <div className={`log-lines ${status}`}>
          {lines.length === 0 ? (
            <div className="trigger-empty">{status === "loading" ? labels.loadingLogs : emptyText}</div>
          ) : (
            lines.map((line, index) => (
              <pre key={`${index}-${line.slice(0, 16)}`}>{line || " "}</pre>
            ))
          )}
        </div>
      </div>
    </section>
  );
}

function CloudSyncPanel({
  status,
  onExport,
  onImport
}: {
  status: string;
  onExport: () => void;
  onImport: () => void;
}) {
  const labels = useUiStrings();
  return (
    <section className="panel-section" aria-label={labels.cloudSync}>
      <div className="panel-heading">
        <div>
          <ShieldCheck size={16} aria-hidden="true" />
          <h2>{labels.cloudSync}</h2>
        </div>
      </div>
      <div className="cloud-sync-panel">
        <div className="cloud-sync-state">{status}</div>
        <div className="cloud-sync-actions">
          <button type="button" onClick={onExport}>
            {labels.export}
          </button>
          <button type="button" onClick={onImport}>
            {labels.import}
          </button>
        </div>
      </div>
    </section>
  );
}

function UpdatePanel({
  channel,
  status,
  onChannelChange,
  onCheck,
  onInstall
}: {
  channel: string;
  status: UpdateStatus;
  onChannelChange: (channel: string) => void;
  onCheck: () => void;
  onInstall: () => void;
}) {
  const labels = useUiStrings();
  const canInstall = status.state === "downloaded";
  return (
    <section className="panel-section" aria-label={labels.updates}>
      <div className="panel-heading">
        <div>
          <RefreshCw size={16} aria-hidden="true" />
          <h2>{labels.updates}</h2>
        </div>
        <span className={`update-state ${status.state}`}>{labels.updateState[status.state]}</span>
      </div>
      <div className="update-panel">
        <div className="update-row">
          <label>
            <span>{labels.channel}</span>
            <select value={channel} onChange={(event) => onChannelChange(event.target.value)}>
              <option value="latest">latest</option>
              <option value="beta">beta</option>
              <option value="alpha">alpha</option>
            </select>
          </label>
          <button type="button" onClick={onCheck}>
            {labels.check}
          </button>
        </div>
        <div className="update-message">
          <strong>{status.version ?? status.channel}</strong>
          <span>{status.message ?? (status.percent !== undefined ? `${status.percent}%` : labels.ready)}</span>
        </div>
        <button type="button" disabled={!canInstall} onClick={onInstall}>
          {labels.installUpdate}
        </button>
      </div>
    </section>
  );
}

export function BulkCommandConfirmation({
  command,
  targets,
  onConfirm,
  onCancel
}: {
  command: string;
  targets: Array<{ id: string; title: string; status: SessionStatus }>;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const labels = useUiStrings();
  return (
    <div className="palette-backdrop" role="presentation" onClick={onCancel}>
      <section
        className="bulk-command-dialog"
        role="dialog"
        aria-label={labels.confirmBulkCommand}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="bulk-command-heading">
          <div>
            <Command size={17} aria-hidden="true" />
            <h2>{labels.confirmBulkCommand}</h2>
          </div>
          <span>{labels.bulkSessions(targets.length)}</span>
        </div>
        <pre>{command}</pre>
        <div className="bulk-command-targets">
          {targets.map((target) => (
            <div key={target.id}>
              <strong>{target.title}</strong>
              <small>{displayStatus(target.status, labels)}</small>
            </div>
          ))}
        </div>
        <div className="bulk-command-actions">
          <button type="button" onClick={onCancel}>
            {labels.cancel}
          </button>
          <button type="button" onClick={onConfirm}>
            {labels.sendToAll}
          </button>
        </div>
      </section>
    </div>
  );
}

export function CommandPalette({
  commands,
  query,
  onQueryChange,
  onExecute,
  onClose
}: {
  commands: ReturnType<typeof createInitialAppSnapshot>["quickCommands"];
  query: string;
  onQueryChange: (query: string) => void;
  onExecute: (command: string) => void;
  onClose: () => void;
}) {
  const labels = useUiStrings();
  const filteredCommands = commands.filter((command) => {
    const haystack = `${command.title} ${command.command}`.toLowerCase();
    return haystack.includes(query.toLowerCase());
  });

  return (
    <div className="palette-backdrop" role="presentation" onClick={onClose}>
      <section
        className="command-palette"
        role="dialog"
        aria-label={labels.commandPalette}
        onClick={(event) => event.stopPropagation()}
      >
        <label className="palette-search">
          <Search size={17} aria-hidden="true" />
          <input
            autoFocus
            value={query}
            placeholder={labels.searchQuickCommands}
            onChange={(event) => onQueryChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                onClose();
              }
            }}
          />
        </label>
        <div className="palette-results">
          {filteredCommands.map((command) => (
            <button type="button" key={command.id} onClick={() => onExecute(command.command)}>
              <strong>{displayBuiltInName(command.title, labels)}</strong>
              <small>{command.command}</small>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}

export function ConnectionEditorDialog({
  draft,
  error,
  canDelete,
  onChange,
  onSave,
  onDelete,
  onClose
}: {
  draft: ConnectionFormDraft;
  error: string;
  canDelete: boolean;
  onChange: (draft: ConnectionFormDraft) => void;
  onSave: () => void;
  onDelete: () => void;
  onClose: () => void;
}) {
  const labels = useUiStrings();
  const isEditing = Boolean(draft.id);
  const update = (patch: Partial<ConnectionFormDraft>) => onChange({ ...draft, ...patch });

  return (
    <div className="palette-backdrop" role="presentation" onClick={onClose}>
      <section
        className="editor-dialog"
        role="dialog"
        aria-label={labels.connectionEditor}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialog-heading">
          <div>
            <Server size={18} aria-hidden="true" />
            <h2>{isEditing ? labels.editConnection : labels.connectionEditor}</h2>
          </div>
          <button type="button" aria-label={labels.close} onClick={onClose}>
            <X size={16} aria-hidden="true" />
          </button>
        </div>
        <p>{labels.connectionEditorSubtitle}</p>
        {error ? <div className="form-error" role="alert">{error}</div> : null}
        <div className="connection-form">
          <label>
            <span>{labels.name}</span>
            <input value={draft.name} onChange={(event) => update({ name: event.target.value })} />
          </label>
          <label>
            <span>{labels.protocol}</span>
            <select
              value={draft.protocol}
              onChange={(event) => {
                const protocol = event.target.value as ConnectionProtocol;
                update({
                  protocol,
                  port: protocol === "rdp" ? "3389" : protocol === "local" ? "0" : draft.port === "3389" ? "22" : draft.port,
                  authMethod: protocol === "local" ? "agent" : draft.authMethod
                });
              }}
            >
              <option value="ssh">SSH</option>
              <option value="rdp">RDP</option>
              <option value="local">{labels.localShell}</option>
            </select>
          </label>
          <label>
            <span>{labels.group}</span>
            <input value={draft.group} onChange={(event) => update({ group: event.target.value })} />
          </label>
          <label>
            <span>{labels.host}</span>
            <input value={draft.host} onChange={(event) => update({ host: event.target.value })} />
          </label>
          <label>
            <span>{labels.port}</span>
            <input value={draft.port} inputMode="numeric" onChange={(event) => update({ port: event.target.value })} />
          </label>
          <label>
            <span>{labels.user}</span>
            <input value={draft.username} onChange={(event) => update({ username: event.target.value })} />
          </label>
          <label>
            <span>{labels.sshLogin}</span>
            <select
              value={draft.authMethod}
              disabled={draft.protocol === "local"}
              onChange={(event) => update({ authMethod: event.target.value as ConnectionProfile["authMethod"] })}
            >
              <option value="password">{labels.password}</option>
              <option value="privateKey">{labels.privateKey}</option>
              <option value="agent">Agent</option>
            </select>
          </label>
          <label>
            <span>{labels.tags}</span>
            <input value={draft.tags} placeholder={labels.tagsHint} onChange={(event) => update({ tags: event.target.value })} />
          </label>
          <fieldset className="color-field">
            <legend>{labels.color}</legend>
            <div>
              {connectionColors.map((color) => (
                <button
                  type="button"
                  key={color}
                  className={draft.color === color ? "active" : ""}
                  aria-label={color}
                  onClick={() => update({ color })}
                >
                  <span style={{ background: color }} />
                </button>
              ))}
            </div>
          </fieldset>
        </div>
        <div className="dialog-actions">
          {isEditing ? (
            <button type="button" className="danger-action" disabled={!canDelete} onClick={onDelete}>
              <Trash2 size={15} aria-hidden="true" />
              {labels.deleteConnection}
            </button>
          ) : null}
          <button type="button" onClick={onClose}>{labels.cancel}</button>
          <button type="button" className="primary-action" onClick={onSave}>
            <Save size={15} aria-hidden="true" />
            {isEditing ? labels.saveConnection : labels.createConnection}
          </button>
        </div>
      </section>
    </div>
  );
}

export function QuickCommandManagerDialog({
  commands,
  draft,
  error,
  onDraftChange,
  onNew,
  onEdit,
  onSave,
  onDelete,
  onClose
}: {
  commands: QuickCommand[];
  draft: QuickCommandFormDraft;
  error: string;
  onDraftChange: (draft: QuickCommandFormDraft) => void;
  onNew: () => void;
  onEdit: (command: QuickCommand) => void;
  onSave: () => void;
  onDelete: (commandId: string) => void;
  onClose: () => void;
}) {
  const labels = useUiStrings();
  const update = (patch: Partial<QuickCommandFormDraft>) => onDraftChange({ ...draft, ...patch });

  return (
    <div className="palette-backdrop" role="presentation" onClick={onClose}>
      <section
        className="editor-dialog quick-manager-dialog"
        role="dialog"
        aria-label={labels.quickCommandManager}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialog-heading">
          <div>
            <LayoutDashboard size={18} aria-hidden="true" />
            <h2>{labels.quickCommandManager}</h2>
          </div>
          <button type="button" aria-label={labels.close} onClick={onClose}>
            <X size={16} aria-hidden="true" />
          </button>
        </div>
        <p>{labels.quickCommandManagerSubtitle}</p>
        <div className="quick-manager-grid">
          <div className="managed-command-list">
            <button type="button" className="managed-command-new" onClick={onNew}>
              <Plus size={15} aria-hidden="true" />
              {labels.newQuickCommand}
            </button>
            {commands.length === 0 ? <div className="trigger-empty">{labels.noQuickCommands}</div> : null}
            {commands.map((command) => (
              <button
                type="button"
                key={command.id}
                className={draft.id === command.id ? "active" : ""}
                onClick={() => onEdit(command)}
              >
                <strong>{displayBuiltInName(command.title, labels)}</strong>
                <small>{command.command}</small>
              </button>
            ))}
          </div>
          <div className="quick-command-form">
            <h3>{draft.id ? labels.editQuickCommand : labels.newQuickCommand}</h3>
            {error ? <div className="form-error" role="alert">{error}</div> : null}
            <label>
              <span>{labels.commandTitle}</span>
              <input value={draft.title} onChange={(event) => update({ title: event.target.value })} />
            </label>
            <label>
              <span>{labels.commandText}</span>
              <textarea value={draft.command} onChange={(event) => update({ command: event.target.value })} />
            </label>
            <label>
              <span>{labels.commandScope}</span>
              <select value={draft.scope} onChange={(event) => update({ scope: event.target.value as QuickCommand["scope"] })}>
                <option value="global">{labels.scope.global}</option>
                <option value="group">{labels.scope.group}</option>
                <option value="connection">{labels.scope.connection}</option>
              </select>
            </label>
            <div className="dialog-actions compact-actions">
              {draft.id ? (
                <button type="button" className="danger-action" onClick={() => onDelete(draft.id ?? "")}>
                  <Trash2 size={15} aria-hidden="true" />
                  {labels.deleteCommand}
                </button>
              ) : null}
              <button type="button" className="primary-action" onClick={onSave}>
                <Save size={15} aria-hidden="true" />
                {labels.saveCommand}
              </button>
            </div>
          </div>
        </div>
      </section>
    </div>
  );
}

export function RemoteOperationDialog({
  draft,
  error,
  onChange,
  onConfirm,
  onClose
}: {
  draft: RemoteOperationDraft;
  error: string;
  onChange: (draft: RemoteOperationDraft) => void;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const labels = useUiStrings();
  const isDelete = draft.type === "delete";
  const title =
    draft.type === "mkdir"
      ? labels.createRemoteDirectory
      : draft.type === "rename"
        ? labels.renameRemotePath
        : labels.deleteRemotePath;

  return (
    <div className="palette-backdrop" role="presentation" onClick={onClose}>
      <section
        className="editor-dialog remote-operation-dialog"
        role="dialog"
        aria-label={labels.remoteOperation}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="dialog-heading">
          <div>
            <Folder size={18} aria-hidden="true" />
            <h2>{title}</h2>
          </div>
          <button type="button" aria-label={labels.close} onClick={onClose}>
            <X size={16} aria-hidden="true" />
          </button>
        </div>
        {error ? <div className="form-error" role="alert">{error}</div> : null}
        <label className="settings-field">
          <span>{labels.remotePath}</span>
          <input
            value={draft.targetPath}
            disabled={isDelete}
            onChange={(event) => onChange({ ...draft, targetPath: event.target.value })}
          />
        </label>
        {isDelete ? (
          <div className="delete-confirmation">{labels.confirmDeleteRemotePath(draft.targetPath)}</div>
        ) : (
          <label className="settings-field">
            <span>{draft.type === "mkdir" ? labels.directoryName : labels.newPathName}</span>
            <input value={draft.value} onChange={(event) => onChange({ ...draft, value: event.target.value })} autoFocus />
          </label>
        )}
        <div className="dialog-actions">
          <button type="button" onClick={onClose}>{labels.cancel}</button>
          <button type="button" className={isDelete ? "danger-action" : "primary-action"} onClick={onConfirm}>
            {isDelete ? labels.deleteRemotePath : labels.save}
          </button>
        </div>
      </section>
    </div>
  );
}

function SettingsDialog({
  language,
  onLanguageChange,
  onClose
}: {
  language: Language;
  onLanguageChange: (language: Language) => void;
  onClose: () => void;
}) {
  const labels = useUiStrings();

  return (
    <div className="palette-backdrop" role="presentation" onClick={onClose}>
      <section
        className="settings-dialog"
        role="dialog"
        aria-label={labels.settingsTitle}
        onClick={(event) => event.stopPropagation()}
      >
        <div className="settings-heading">
          <div>
            <Settings size={18} aria-hidden="true" />
            <h2>{labels.settingsTitle}</h2>
          </div>
          <button type="button" aria-label={labels.close} onClick={onClose}>
            {labels.close}
          </button>
        </div>
        <p>{labels.settingsSubtitle}</p>
        <label className="settings-field">
          <span>{labels.settingsLanguage}</span>
          <select value={language} onChange={(event) => onLanguageChange(event.target.value as Language)}>
            <option value="zh-CN">{labels.languageChinese}</option>
            <option value="en-US">{labels.languageEnglish}</option>
          </select>
        </label>
      </section>
    </div>
  );
}
