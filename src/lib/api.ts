import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import type {
  AppSettings,
  AutomationPlan,
  BatchExecution,
  BackgroundTask,
  ConnectionProfile,
  ExternalEditSession,
  ExternalEditSnapshot,
  Folder,
  GeneratedSshKey,
  MonitorSnapshot,
  OpenSshHost,
  NetworkSocketReport,
  ProcessInfo,
  ProtocolCapability,
  ConnectionProtocolOptions,
  RdpPreflight,
  RemoteFile,
  SessionLogStatus,
  SaveConnectionInput,
  SystemInfo,
  SyncOptions,
  SyncResult,
  TerminalOutput,
  TerminalStatus,
  TerminalSession,
  TransferTask,
  ZmodemEvent
} from "../types";
import type { CommandSnippet, PortForward, ProxyProfile, SaveProxyInput } from "../types";
import { normalizeAppSettings } from "../types";

const isTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export interface FeedbackEnvironment {
  appVersion: string;
  operatingSystem: string;
  osVersion: string;
  architecture: string;
}

const demoConnection: ConnectionProfile = {
  id: "demo-localhost",
  folderId: null,
  protocol: "ssh",
  name: "演示服务器",
  host: "127.0.0.1",
  port: 22,
  username: "developer",
  authType: "sshAgent",
  privateKeyPath: null,
  hostKeyPolicy: "strict",
  note: "连接到真实服务器后显示实时数据",
  tags: ["Demo"],
  encoding: "UTF-8",
  startupCommand: null,
  proxyId: null,
  environment: {},
  hasCredential: false,
  createdAt: new Date().toISOString(),
  updatedAt: new Date().toISOString()
  ,lastConnectedAt: null
};

let browserConnections = [demoConnection];
let browserDeletedConnections:ConnectionProfile[]=[];
let browserSnippets:CommandSnippet[]=[];
let browserFolders:Folder[]=[];
const browserFiles:Record<string,RemoteFile[]>={
  "/":[{name:"home",path:"/home",kind:"directory",size:0,modifiedAt:null,permissions:"drwxr-xr-x",owner:0,group:0}],
  "/home":[{name:"developer",path:"/home/developer",kind:"directory",size:0,modifiedAt:null,permissions:"drwxr-xr-x",owner:501,group:20}],
  "/home/developer":[{name:"README.txt",path:"/home/developer/README.txt",kind:"file",size:128,modifiedAt:null,permissions:"-rw-r--r--",owner:501,group:20}]
};

export const api = {
  isDesktop: isTauri,
  async listConnections(): Promise<ConnectionProfile[]> {
    return isTauri() ? invoke("connection_list") : browserConnections;
  },
  async listDeletedConnections(): Promise<ConnectionProfile[]> { return isTauri() ? invoke("connection_deleted_list") : browserDeletedConnections; },
  async listFolders(): Promise<Folder[]> {
    return isTauri() ? invoke("folder_list") : browserFolders;
  },
  async saveFolder(id: string, name: string, parentId: string | null = null): Promise<Folder> { if(isTauri())return invoke("folder_save", { id, name, parentId });const existing=browserFolders.find((folder)=>folder.id===id);const saved={id,name,parentId,sortOrder:existing?.sortOrder??browserFolders.length};browserFolders=[...browserFolders.filter((folder)=>folder.id!==id),saved];return saved; },
  async deleteFolder(id: string): Promise<void> { if(isTauri())return invoke("folder_delete", { id });const removed=new Set([id]);let changed=true;while(changed){changed=false;for(const folder of browserFolders)if(folder.parentId&&removed.has(folder.parentId)&&!removed.has(folder.id)){removed.add(folder.id);changed=true;}}browserFolders=browserFolders.filter((folder)=>!removed.has(folder.id));browserConnections=browserConnections.map((connection)=>connection.folderId&&removed.has(connection.folderId)?{...connection,folderId:null}:connection); },
  async moveConnection(id: string, folderId: string | null): Promise<void> { if(isTauri())return invoke("connection_move", { id, folderId });browserConnections=browserConnections.map((connection)=>connection.id===id?{...connection,folderId}:connection); },
  async saveConnection(input: SaveConnectionInput): Promise<ConnectionProfile> {
    if (isTauri()) return invoke("connection_save", { input });
    const now = new Date().toISOString();
    const saved: ConnectionProfile = { ...input, hasCredential: Boolean(input.credential), createdAt: now, updatedAt: now, lastConnectedAt: null };
    browserConnections = [...browserConnections.filter((item) => item.id !== saved.id), saved];
    return saved;
  },
  async duplicateConnection(id:string,newId:string):Promise<ConnectionProfile>{return invoke("connection_duplicate",{id,newId});},
  async deleteConnection(id: string): Promise<void> {
    if (isTauri()) await invoke("connection_delete", { id });
    else {const deleted=browserConnections.find((item)=>item.id===id);if(deleted)browserDeletedConnections=[deleted,...browserDeletedConnections.filter((item)=>item.id!==id)];browserConnections=browserConnections.filter((item) => item.id !== id);}
  },
  async restoreConnection(id: string): Promise<void> { if(isTauri())await invoke("connection_restore",{id});else{const restored=browserDeletedConnections.find((item)=>item.id===id);if(restored)browserConnections=[...browserConnections,restored];browserDeletedConnections=browserDeletedConnections.filter((item)=>item.id!==id);} },
  async purgeConnection(id: string): Promise<void> { if(isTauri())await invoke("connection_purge",{id});else browserDeletedConnections=browserDeletedConnections.filter((item)=>item.id!==id); },
  async startConnectionTest(id: string): Promise<BackgroundTask> {
    if (isTauri()) return invoke("connection_test_start", { id });
    return { id: crypto.randomUUID(), kind: "connectionDiagnostic", status: "completed", result: [{ stage: "tcp", ok: false, message: "浏览器预览不建立真实 SSH 连接，请启动桌面版。" }], error: null, createdAt: new Date().toISOString() };
  },
  async trustHost(id: string, fingerprint: string, algorithm: string): Promise<void> {
    if (isTauri()) await invoke("connection_trust_host", { id, fingerprint, algorithm });
  },
  async importOpenSshConfig(path:string):Promise<OpenSshHost[]>{return invoke("openssh_import",{path});},
  async generateSshKey(path:string,comment:string):Promise<GeneratedSshKey>{return invoke("openssh_generate_key",{path,comment});},
  async deploySshKey(connectionId:string,publicKey:string):Promise<void>{return invoke("openssh_deploy_key",{connectionId,publicKey});},
  async protocolCapabilities():Promise<ProtocolCapability[]>{return isTauri()?invoke("protocol_capabilities"):[];},
  async getProtocolOptions(connectionId:string):Promise<ConnectionProtocolOptions>{return isTauri()?invoke("protocol_options_get",{connectionId}):{connectionId,agentForwarding:false};},
  async saveProtocolOptions(options:ConnectionProtocolOptions):Promise<ConnectionProtocolOptions>{return invoke("protocol_options_save",{options});},
  async validateAutomation(plan:AutomationPlan):Promise<AutomationPlan>{return invoke("automation_validate",{plan});},
  async startAutomation(plan:AutomationPlan):Promise<BackgroundTask>{return invoke("automation_start",{plan});},
  async writeEncryptedSync(folder:string,passphrase:string,options:SyncOptions):Promise<SyncResult>{return invoke("sync_write",{folder,passphrase,options});},
  async readEncryptedSync(folder:string,passphrase:string):Promise<SyncResult>{return invoke("sync_read",{folder,passphrase});},
  async listProxies(): Promise<ProxyProfile[]> { return isTauri() ? invoke("proxy_list") : []; },
  async saveProxy(input: SaveProxyInput): Promise<ProxyProfile> { return invoke("proxy_save", { input }); },
  async deleteProxy(id: string): Promise<void> { return invoke("proxy_delete", { id }); },
  async listForwards(connectionId: string): Promise<PortForward[]> { return isTauri() ? invoke("tunnel_list", { connectionId }) : []; },
  async saveForward(input: PortForward): Promise<PortForward> { return invoke("tunnel_save", { input }); },
  async startForward(id: string): Promise<void> { return invoke("tunnel_start", { id }); },
  async stopForward(id: string): Promise<void> { return invoke("tunnel_stop", { id }); },
  async deleteForward(id: string): Promise<void> { return invoke("tunnel_delete", { id }); },
  async openTerminal(connectionId: string, cols: number, rows: number): Promise<TerminalSession> {
    if (isTauri()) return invoke("terminal_open", { connectionId, cols, rows });
    return { id: crypto.randomUUID(), connectionId, sessionType: "terminal", title: "预览终端", status: "online", startedAt: new Date().toISOString(), lastError: null };
  },
  async terminalInput(sessionId: string, data: string) {
    if (isTauri()) await invoke("terminal_input", { sessionId, data });
  },
  async terminalResize(sessionId: string, cols: number, rows: number) {
    if (isTauri()) await invoke("terminal_resize", { sessionId, cols, rows });
  },
  async closeTerminal(sessionId: string) {
    if (isTauri()) await invoke("terminal_close", { sessionId });
  },
  async startSessionLog(sessionId:string,format:"text"|"jsonl",lineTimestamps:boolean,retentionDays:number,maxTotalBytes:number):Promise<SessionLogStatus>{
    if(!isTauri())throw new Error("会话日志需要运行 CNshell 桌面版");
    return invoke("terminal_log_start",{sessionId,format,lineTimestamps,retentionDays,maxTotalBytes});
  },
  async stopSessionLog(sessionId:string):Promise<SessionLogStatus>{return invoke("terminal_log_stop",{sessionId});},
  async sessionLogStatus(sessionId:string):Promise<SessionLogStatus>{return isTauri()?invoke("terminal_log_status",{sessionId}):{sessionId,active:false,path:null,format:null,lineTimestamps:false,startedAt:null,bytesWritten:0,error:null};},
  async exportSessionLog(sessionId:string,path:string):Promise<void>{return invoke("terminal_log_export",{sessionId,path});},
  async startBatch(connectionIds:string[],command:string,concurrency:number):Promise<BatchExecution>{return invoke("batch_start",{connectionIds,command,concurrency});},
  async getBatch(id:string):Promise<BatchExecution>{return invoke("batch_get",{id});},
  async cancelBatch(id:string):Promise<BatchExecution>{return invoke("batch_cancel",{id});},
  async onBatchExecution(handler:(execution:BatchExecution)=>void):Promise<UnlistenFn>{return isTauri()?listen<BatchExecution>("batch-execution",(event)=>handler(event.payload)):()=>undefined;},
  async startExternalEdit(sessionId:string,path:string,application?:string):Promise<ExternalEditSession>{return invoke("external_edit_start",{sessionId,path,application});},
  async readExternalEdit(id:string):Promise<ExternalEditSnapshot>{return invoke("external_edit_read",{id});},
  async discardExternalEdit(id:string):Promise<void>{return invoke("external_edit_discard",{id});},
  async onTerminalOutput(handler: (output: TerminalOutput) => void): Promise<UnlistenFn> {
    if (isTauri()) return listen<TerminalOutput>("terminal-output", (event) => handler(event.payload));
    return () => undefined;
  },
  async onTerminalStatus(handler: (status: TerminalStatus) => void): Promise<UnlistenFn> {
    if (isTauri()) return listen<TerminalStatus>("terminal-status", (event) => handler(event.payload));
    return () => undefined;
  },
  async startZmodem(sessionId:string,transferId:string,paths:string[]):Promise<ZmodemEvent>{
    return invoke("zmodem_start",{sessionId,transferId,paths});
  },
  async cancelZmodem(sessionId:string,transferId:string):Promise<ZmodemEvent>{
    return invoke("zmodem_cancel",{sessionId,transferId});
  },
  async onZmodemEvent(handler:(event:ZmodemEvent)=>void):Promise<UnlistenFn>{
    return isTauri()?listen<ZmodemEvent>("zmodem-event",(event)=>handler(event.payload)):()=>undefined;
  },
  async listFiles(sessionId: string, path: string, showHidden: boolean): Promise<RemoteFile[]> {
    if (isTauri()) return invoke("sftp_list", { sessionId, path, showHidden });
    return (browserFiles[path]??[]).filter((item)=>showHidden||!item.name.startsWith("."));
  },
  async joinRemotePath(parent:string,name:string):Promise<string>{return isTauri()?invoke("sftp_join_path",{parent,name}):`${parent.replace(/\/$/,"")}/${name}`;},
  async createDirectory(sessionId: string, path: string) {
    return invoke("sftp_mkdir", { sessionId, path });
  },
  async renameRemote(sessionId: string, source: string, destination: string) {
    return invoke("sftp_rename", { sessionId, source, destination });
  },
  async deleteRemote(sessionId: string, path: string, recursive = false) {
    return invoke("sftp_delete", { sessionId, path, recursive });
  },
  async chmodRemote(sessionId: string, path: string, mode: number) {
    return invoke("sftp_chmod", { sessionId, path, mode });
  },
  async openText(sessionId: string, path: string): Promise<{ content: string; modifiedAt: number | null }> {
    return invoke("sftp_open_text", { sessionId, path });
  },
  async saveText(sessionId: string, path: string, content: string, expectedModifiedAt: number | null) {
    return invoke("sftp_save_text", { sessionId, path, content, expectedModifiedAt });
  },
  async createText(sessionId: string, path: string) {
    return invoke("sftp_create_text", { sessionId, path });
  },
  async startArchiveRemote(sessionId: string, path: string, extract: boolean): Promise<BackgroundTask> { return invoke("sftp_archive_start", { sessionId, path, extract }); },
  async startOpenRemoteLocally(sessionId: string, path: string, application?:string): Promise<BackgroundTask> { return invoke("sftp_open_local_start", { sessionId, path, application }); },
  async startDirectoryTransfer(sessionId:string,direction:"upload"|"download",source:string,destination:string,conflictPolicy:Exclude<import("../types").ConflictPolicy,"ask">):Promise<BackgroundTask>{return invoke("sftp_directory_transfer_start",{sessionId,direction,source,destination,conflictPolicy});},
  async getTask(id:string):Promise<BackgroundTask>{return invoke("task_get",{id});},
  async cancelTask(id:string):Promise<void>{if(isTauri())await invoke("task_cancel",{id});},
  async onBackgroundTask(handler:(task:BackgroundTask)=>void):Promise<UnlistenFn>{return isTauri()?listen<BackgroundTask>("background-task",(event)=>handler(event.payload)):()=>undefined;},
  async enqueueTransfer(input: import("../types").TransferInput): Promise<TransferTask> {
    return invoke("transfer_enqueue", { input });
  },
  async listTransfers(): Promise<TransferTask[]> {
    return isTauri() ? invoke("transfer_list") : [];
  },
  async cancelTransfer(id: string) {
    return invoke("transfer_cancel", { id });
  },
  async retryTransfer(id: string) {
    return invoke<TransferTask>("transfer_retry", { id });
  },
  async pauseTransfer(id: string) { return invoke("transfer_pause", { id }); },
  async resumeTransfer(id: string) { return invoke("transfer_resume", { id }); },
  async onTransfer(handler: (task: TransferTask) => void): Promise<UnlistenFn> {
    if (isTauri()) return listen<TransferTask>("transfer-progress", (event) => handler(event.payload));
    return () => undefined;
  },
  async monitorSnapshot(sessionId: string): Promise<MonitorSnapshot> {
    return invoke("monitor_snapshot", { sessionId });
  },
  async signalProcess(sessionId:string,process:ProcessInfo,signal:"TERM"|"HUP"|"KILL"):Promise<void>{return invoke("monitor_process_signal",{sessionId,pid:process.pid,startedAt:process.startedAt,expectedCommand:process.command,signal});},
  async networkSockets(sessionId:string):Promise<NetworkSocketReport>{return invoke("monitor_network_sockets",{sessionId});},
  async startNetworkDiagnostic(sessionId:string,kind:"ping"|"traceroute",target:string):Promise<BackgroundTask>{return invoke("monitor_network_diagnostic_start",{sessionId,kind,target});},
  async systemInfo(sessionId: string): Promise<SystemInfo> {
    return invoke("monitor_system_info", { sessionId });
  },
  async exportSystemInfo(sessionId: string, path: string): Promise<void> { return invoke("monitor_export_system_info", { sessionId, path }); },
  async rdpPreflight(): Promise<RdpPreflight> {
    return isTauri() ? invoke("rdp_preflight") : { available: false, executable: null, message: "桌面版可检测 FreeRDP。" };
  },
  async rdpOpen(connectionId: string): Promise<TerminalSession> {
    return invoke("rdp_open", { connectionId });
  },
  async rdpClose(sessionId:string):Promise<void>{if(isTauri())await invoke("rdp_close",{sessionId});},
  async getSettings(): Promise<AppSettings> {
    if (!isTauri()) return normalizeAppSettings(JSON.parse(localStorage.getItem("cnshell-settings") ?? "null"));
    return invoke("settings_get");
  },
  async saveSettings(settings: AppSettings): Promise<AppSettings> {
    if (!isTauri()) { localStorage.setItem("cnshell-settings", JSON.stringify(settings)); return settings; }
    return invoke("settings_save", { settings });
  },
  async listSnippets(): Promise<CommandSnippet[]> { return isTauri() ? invoke("snippet_list") : browserSnippets; },
  async saveSnippet(input: CommandSnippet): Promise<CommandSnippet> { if(isTauri())return invoke("snippet_save", { input });browserSnippets=[...browserSnippets.filter((item)=>item.id!==input.id),input];return input; },
  async deleteSnippet(id: string): Promise<void> { if(isTauri())await invoke("snippet_delete", { id });else browserSnippets=browserSnippets.filter((item)=>item.id!==id); },
  async addHistory(connectionId: string, command: string): Promise<void> { if (isTauri()) await invoke("history_add", { connectionId, command }); },
  async listHistory(connectionId: string): Promise<string[]> { return isTauri() ? invoke("history_list", { connectionId }) : []; },
  async clearHistory(): Promise<number> { return isTauri() ? invoke("history_clear") : 0; },
  async exportConnections(path: string, includeSecrets = false, passphrase?: string): Promise<void> { return invoke("connection_export", { path, includeSecrets, passphrase }); },
  async exportConnection(id:string,path:string):Promise<void>{return invoke("connection_export_one",{id,path});},
  async importConnections(path: string, passphrase?: string): Promise<number> { return invoke("connection_import", { path, passphrase }); },
  async saveWorkspace(value: unknown): Promise<void> { if (isTauri()) await invoke("workspace_save", { value }); },
  async loadWorkspace<T>(): Promise<T | null> { return isTauri() ? invoke("workspace_load") : null; }
  ,async exportDiagnostics(path: string): Promise<void> { return invoke("diagnostics_export", { path }); }
  ,async feedbackEnvironment():Promise<FeedbackEnvironment>{
    if(isTauri())return invoke("diagnostics_environment");
    return {appVersion:"0.1.1",operatingSystem:"macos",osVersion:"浏览器预览",architecture:navigator.platform||"unknown"};
  }
  ,async revealDiagnostics(path:string):Promise<void>{return invoke("diagnostics_reveal",{path});}
  ,async openExternal(url:string):Promise<void>{
    if(isTauri())return openExternal(url);
    window.open(url,"_blank","noopener,noreferrer");
  }
};
