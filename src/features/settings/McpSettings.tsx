import {
  Ban,
  Bot,
  Check,
  Clipboard,
  Copy,
  FolderOpen,
  LoaderCircle,
  Plus,
  RefreshCw,
  ServerCog,
  ShieldCheck,
} from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type {
  ConnectionProfile,
  McpApprovalRule,
  McpAuditEvent,
  McpClient,
  McpClientConfig,
  McpLocalGrant,
  McpStatus,
} from "../../types";

const TOOLS = [
  ["cnshell_list_connections", "列出授权连接", "只读"],
  ["cnshell_open_session", "打开短期会话", "只读"],
  ["cnshell_close_session", "关闭短期会话", "只读"],
  ["cnshell_file_list", "列出远端目录", "只读"],
  ["cnshell_file_read", "读取远端文本", "只读"],
  ["cnshell_system_info", "读取系统信息", "只读"],
  ["cnshell_run_command", "执行非交互命令", "每次审批"],
  ["cnshell_file_write", "写入远端文本", "每次审批"],
  ["cnshell_file_mkdir", "新建远端目录", "每次审批"],
  ["cnshell_file_rename", "重命名远端项目", "每次审批"],
  ["cnshell_file_delete", "删除远端项目", "每次审批"],
  ["cnshell_file_upload", "上传本地项目", "每次审批"],
  ["cnshell_file_download", "下载远端项目", "每次审批"],
] as const;

const DEFAULT_TOOLS = TOOLS.filter(([, , policy]) => policy === "只读").map(([id]) => id);

export function McpSettings({
  connections,
  onError,
}: {
  connections: ConnectionProfile[];
  onError: (message: string) => void;
}) {
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [clients, setClients] = useState<McpClient[]>([]);
  const [audit, setAudit] = useState<McpAuditEvent[]>([]);
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [selectedClientId, setSelectedClientId] = useState<string | null>(null);
  const [connectionIds, setConnectionIds] = useState<string[]>([]);
  const [tools, setTools] = useState<string[]>(DEFAULT_TOOLS);
  const [remoteRoot, setRemoteRoot] = useState("/");
  const [showHostnames, setShowHostnames] = useState(false);
  const [config, setConfig] = useState<McpClientConfig | null>(null);
  const [localGrants, setLocalGrants] = useState<McpLocalGrant[]>([]);
  const [approvalRules, setApprovalRules] = useState<McpApprovalRule[]>([]);
  const [copied, setCopied] = useState<"codex" | "json" | null>(null);

  const sshConnections = useMemo(
    () => connections.filter((connection) => connection.protocol === "ssh"),
    [connections],
  );
  const selectedClient = clients.find((client) => client.id === selectedClientId) ?? null;

  const refresh = useCallback(async () => {
    try {
      const [nextStatus, nextClients, nextAudit] = await Promise.all([
        api.mcpStatus(),
        api.mcpListClients(),
        api.mcpListAudit(),
      ]);
      setStatus(nextStatus);
      setClients(nextClients);
      setAudit(nextAudit.slice(0, 10));
    } catch (reason) {
      onError(errorMessage(reason));
    }
  }, [onError]);

  useEffect(() => { void refresh(); }, [refresh]);

  useEffect(() => {
    if (!selectedClient) {
      setLocalGrants([]);
      setApprovalRules([]);
      return;
    }
    setConnectionIds(selectedClient.connectionIds);
    setTools(selectedClient.tools.length ? selectedClient.tools : DEFAULT_TOOLS);
    setRemoteRoot(selectedClient.remoteRoot);
    setShowHostnames(selectedClient.showHostnames);
    setConfig(null);
    void Promise.all([
      api.mcpListLocalGrants(selectedClient.id),
      api.mcpListApprovalRules(selectedClient.id),
    ]).then(([grants, rules]) => {
      setLocalGrants(grants);
      setApprovalRules(rules);
    }).catch((reason) => onError(errorMessage(reason)));
  }, [selectedClient, onError]);

  const toggleEnabled = async (enabled: boolean) => {
    setBusy(true);
    try { setStatus(await api.mcpSetEnabled(enabled)); }
    catch (reason) { onError(errorMessage(reason)); }
    finally { setBusy(false); }
  };

  const createClient = async () => {
    if (!name.trim()) { onError("请填写 MCP 客户端名称"); return; }
    setBusy(true);
    try {
      const client = await api.mcpCreateClient(name.trim());
      setClients((current) => [client, ...current]);
      setSelectedClientId(client.id);
      setName("");
      await refresh();
    } catch (reason) { onError(errorMessage(reason)); }
    finally { setBusy(false); }
  };

  const saveGrants = async () => {
    if (!selectedClient) return;
    if (!connectionIds.length || !tools.length) { onError("至少选择一个 SSH 连接和一个 MCP 工具"); return; }
    setBusy(true);
    try {
      const saved = await api.mcpSaveClientGrants({
        clientId: selectedClient.id,
        connectionIds,
        tools,
        remoteRoot: remoteRoot.trim() || "/",
        showHostnames,
      });
      setClients((current) => current.map((client) => client.id === saved.id ? saved : client));
      setConfig(await api.mcpClientConfig(saved.id));
      await refresh();
    } catch (reason) { onError(errorMessage(reason)); }
    finally { setBusy(false); }
  };

  const loadConfig = async () => {
    if (!selectedClient) return;
    try { setConfig(await api.mcpClientConfig(selectedClient.id)); }
    catch (reason) { onError(errorMessage(reason)); }
  };

  const copyConfig = async (kind: "codex" | "json") => {
    if (!config) return;
    try {
      await navigator.clipboard.writeText(kind === "codex" ? config.codexToml : config.json);
      setCopied(kind);
      window.setTimeout(() => setCopied(null), 1600);
    } catch (reason) { onError(errorMessage(reason)); }
  };

  const exportAudit = async () => {
    try {
      const path = await save({ defaultPath: "cnshell-mcp-audit.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (path) await api.mcpExportAudit(path);
    } catch (reason) { onError(errorMessage(reason)); }
  };

  const revoke = async (client: McpClient) => {
    if (!confirm(`撤销 ${client.name} 的 MCP 权限？它的会话和待审批请求会立即失效。`)) return;
    setBusy(true);
    try {
      await api.mcpRevokeClient(client.id);
      if (selectedClientId === client.id) setSelectedClientId(null);
      await refresh();
    } catch (reason) { onError(errorMessage(reason)); }
    finally { setBusy(false); }
  };

  const createLocalGrant = async (direction: "upload" | "download", selection: "file" | "directory") => {
    if (!selectedClient) return;
    setBusy(true);
    try {
      const grant = await api.mcpCreateLocalGrant(selectedClient.id, direction, selection, false);
      if (grant) setLocalGrants(await api.mcpListLocalGrants(selectedClient.id));
    } catch (reason) { onError(errorMessage(reason)); }
    finally { setBusy(false); }
  };

  const revokeLocalGrant = async (grant: McpLocalGrant) => {
    if (!confirm(`撤销本地授权“${grant.displayName}”？未开始的 MCP 传输将无法再使用它。`)) return;
    try {
      await api.mcpRevokeLocalGrant(grant.id);
      if (selectedClient) setLocalGrants(await api.mcpListLocalGrants(selectedClient.id));
    } catch (reason) { onError(errorMessage(reason)); }
  };

  const revokeApprovalRule = async (rule: McpApprovalRule) => {
    if (!confirm(`撤销 ${rule.connectionName} 的精确命令规则？下次执行相同命令时将重新请求审批。`)) return;
    try {
      await api.mcpRevokeApprovalRule(rule.id);
      if (selectedClient) setApprovalRules(await api.mcpListApprovalRules(selectedClient.id));
      await refresh();
    } catch (reason) { onError(errorMessage(reason)); }
  };

  const toggleItem = (items: string[], value: string, checked: boolean, update: (values: string[]) => void) =>
    update(checked ? [...new Set([...items, value])] : items.filter((item) => item !== value));

  return <section className="mcp-settings" aria-label="MCP 服务">
    <div className="section-heading">
      <div><h3><Bot size={16}/>MCP 服务</h3><p>让本机 AI 客户端在明确授权和审批下使用 CNshell 的 SSH/SFTP 能力。</p></div>
      <button className="mini-button" onClick={() => void refresh()} disabled={busy}><RefreshCw size={13}/>刷新</button>
    </div>

    <div className="mcp-status-row">
      <span className={`mcp-status-dot ${status?.running ? "running" : ""}`}/>
      <div><strong>{status?.running ? "Broker 正在运行" : "Broker 已停止"}</strong><small>{status?.message ?? "正在读取状态..."}</small></div>
      <label className="mcp-switch"><input type="checkbox" aria-label="启用 MCP" checked={status?.enabled ?? false} disabled={busy || !status} onChange={(event) => void toggleEnabled(event.target.checked)}/><span/></label>
    </div>
    {status?.running && <div className="mcp-runtime-stats" aria-label="MCP 运行状态"><span>客户端 <b>{status.clientCount}</b></span><span>会话 <b>{status.sessionCount}</b></span><span>待审批 <b>{status.pendingApprovalCount}</b></span><span>仅本机 <code>{status.address}</code></span></div>}
    <div className="mcp-subsection">
      <div className="mcp-subheading"><div><strong>客户端授权</strong><small>客户端配置不包含密码、私钥或 Broker secret。</small></div></div>
      <div className="mcp-create-row"><label><span>客户端名称</span><input value={name} maxLength={128} placeholder="例如 Codex" onChange={(event) => setName(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); void createClient(); } }}/></label><button className="button secondary" disabled={busy || !api.isDesktop()} onClick={() => void createClient()}><Plus size={14}/>创建客户端</button></div>
      {!clients.length && <div className="mcp-empty"><ServerCog size={21}/><span>尚未登记 MCP 客户端</span></div>}
      <div className="mcp-client-list">
        {clients.map((client) => <article key={client.id} className={selectedClientId === client.id ? "selected" : ""}>
          <button className="mcp-client-main" onClick={() => client.status === "active" && setSelectedClientId(client.id)} disabled={client.status !== "active"}>
            <ShieldCheck size={16}/><span><strong>{client.name}</strong><small>{client.status === "active" ? `${client.connectionIds.length} 个连接 · ${client.tools.length} 个工具` : "已撤销"}</small></span>
          </button>
          {client.status === "active" && <button className="mini-button danger" aria-label={`撤销 ${client.name}`} onClick={() => void revoke(client)}><Ban size={12}/>撤销</button>}
        </article>)}
      </div>
    </div>

    {selectedClient && <div className="mcp-grant-editor">
      <header><div><strong>{selectedClient.name}</strong><small>只为勾选的连接和远端根签发权限</small></div>{busy && <LoaderCircle size={14} className="spin"/>}</header>
      <fieldset><legend>SSH 连接</legend><div className="mcp-check-grid">{sshConnections.map((connection) => <label key={connection.id}><input type="checkbox" checked={connectionIds.includes(connection.id)} onChange={(event) => toggleItem(connectionIds, connection.id, event.target.checked, setConnectionIds)}/><span><b>{connection.name}</b><small>{connection.username}@{connection.host}:{connection.port}</small></span></label>)}</div>{!sshConnections.length && <small>请先创建 SSH 连接。</small>}</fieldset>
      <fieldset><legend>工具权限</legend><div className="mcp-tool-grid">{TOOLS.map(([id, label, policy]) => <label key={id}><input type="checkbox" checked={tools.includes(id)} onChange={(event) => toggleItem(tools, id, event.target.checked, setTools)}/><span><b>{label}</b><small>{policy}</small></span></label>)}</div></fieldset>
      <label className="check-row"><input type="checkbox" checked={showHostnames} onChange={(event) => setShowHostnames(event.target.checked)}/><span>允许此客户端看到主机地址和用户名</span></label>
      <label className="mcp-root-field"><span>远端授权根</span><input value={remoteRoot} onChange={(event) => setRemoteRoot(event.target.value)} placeholder="/"/><small>所有远端文件操作都限制在规范化后的此目录内。</small></label>
      <fieldset><legend>本地文件授权</legend><div className="mcp-local-actions"><button className="mini-button" disabled={busy} onClick={() => void createLocalGrant("upload", "file")}><FolderOpen size={12}/>授权上传文件</button><button className="mini-button" disabled={busy} onClick={() => void createLocalGrant("upload", "directory")}><FolderOpen size={12}/>授权上传文件夹</button><button className="mini-button" disabled={busy} onClick={() => void createLocalGrant("download", "directory")}><FolderOpen size={12}/>授权下载目录</button></div>{!localGrants.filter((grant) => !grant.revokedAt).length ? <small>暂无活动授权。默认创建一次性授权，使用后立即失效。</small> : <div className="mcp-local-list">{localGrants.filter((grant) => !grant.revokedAt).map((grant) => <div key={grant.id}><span><b>{grant.displayName}</b><small>{grant.direction === "upload" ? "上传只读" : "下载可写"} · {grant.persistent ? "持久" : "一次性"}</small></span><code>{grant.id}</code><button className="mini-button danger" aria-label={`撤销本地授权 ${grant.displayName}`} onClick={() => void revokeLocalGrant(grant)}><Ban size={11}/>撤销</button></div>)}</div>}</fieldset>
      <fieldset><legend>精确命令规则</legend>{!approvalRules.length ? <small>暂无规则。只有低风险命令可以在审批时保存，命令明文不会写入规则表。</small> : <div className="mcp-rule-list">{approvalRules.map((rule) => <div key={rule.id}><span><b>{rule.connectionName}</b><small>{rule.tool} · {rule.lastUsedAt ? `最近使用 ${new Date(rule.lastUsedAt).toLocaleString()}` : `创建于 ${new Date(rule.createdAt).toLocaleString()}`}</small></span><code title={rule.targetSummary}>{rule.targetSummary}</code><button className="mini-button danger" aria-label={`撤销精确规则 ${rule.connectionName}`} onClick={() => void revokeApprovalRule(rule)}><Ban size={11}/>撤销</button></div>)}</div>}</fieldset>
      <div className="mcp-grant-actions"><button className="button secondary" disabled={busy} onClick={() => void loadConfig()}><Clipboard size={14}/>查看现有配置</button><button className="button primary" disabled={busy} onClick={() => void saveGrants()}><Check size={14}/>保存授权并生成配置</button></div>
      {config && <div className="mcp-config"><div><strong>客户端配置</strong><small>复制到对应 MCP Host 后，保持 CNshell 运行并启用 MCP。</small></div><pre>{config.codexToml}</pre><div className="mcp-config-actions"><button className="mini-button" onClick={() => void copyConfig("codex")}><Copy size={12}/>{copied === "codex" ? "已复制" : "复制 Codex TOML"}</button><button className="mini-button" onClick={() => void copyConfig("json")}><Copy size={12}/>{copied === "json" ? "已复制" : "复制通用 JSON"}</button></div></div>}
    </div>}

    <div className="mcp-subsection mcp-audit">
      <div className="mcp-subheading"><div><strong>最近审计</strong><small>仅保存工具、目标摘要、结果和耗时，不保存命令输出或文件正文。</small></div><button className="mini-button" disabled={!audit.length} onClick={() => void exportAudit()}><Clipboard size={12}/>导出</button></div>
      {!audit.length ? <div className="mcp-empty">暂无 MCP 审计记录</div> : <div className="mcp-audit-list">{audit.map((event) => <div key={event.id}><span className={`mcp-risk ${event.risk}`}>{event.risk}</span><code>{event.tool}</code><span>{event.targetSummary}</span><b>{event.outcome}</b><time>{new Date(event.createdAt).toLocaleString()}</time></div>)}</div>}
    </div>
  </section>;
}
