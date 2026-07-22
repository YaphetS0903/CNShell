import { AlertTriangle, Check, Clock3, ShieldAlert, X, XCircle } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { McpApproval } from "../../types";

const TOOL_LABELS: Record<string, string> = {
  cnshell_run_command: "执行远端命令",
  cnshell_file_write: "写入远端文件",
  cnshell_file_mkdir: "新建远端目录",
  cnshell_file_rename: "重命名远端项目",
  cnshell_file_delete: "删除远端项目",
  cnshell_file_upload: "上传本地文件",
  cnshell_file_download: "下载远端文件",
};

export function McpApprovalCenter({ onError }: { onError: (message: string) => void }) {
  const [approvals, setApprovals] = useState<McpApproval[]>([]);
  const [open, setOpen] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [now, setNow] = useState(Date.now());

  const refresh = useCallback(async () => {
    try {
      const next = await api.mcpListApprovals();
      setApprovals(next);
      if (next.length) setOpen(true);
    } catch (reason) { onError(errorMessage(reason)); }
  }, [onError]);

  useEffect(() => {
    void refresh();
    const listener = api.onMcpApprovalChanged(() => void refresh());
    return () => { void listener.then((unlisten) => unlisten()); };
  }, [refresh]);

  useEffect(() => {
    if (!approvals.length) return;
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(timer);
  }, [approvals.length]);

  useEffect(() => {
    if (!open) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        setOpen(false);
      }
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [open]);

  const activeApprovals = useMemo(
    () => approvals.filter((approval) => new Date(approval.expiresAt).getTime() > now),
    [approvals, now],
  );

  const decide = async (approval: McpApproval, decision: "reject" | "once" | "session" | "persistent") => {
    setBusyId(approval.id);
    try {
      await api.mcpDecide(approval.id, decision);
      setApprovals((current) => current.filter((item) => item.id !== approval.id));
      await refresh();
    } catch (reason) { onError(errorMessage(reason)); }
    finally { setBusyId(null); }
  };

  if (!open) return activeApprovals.length ? <button className="mcp-approval-fab" aria-label={`打开 MCP 审批中心，${activeApprovals.length} 项待处理`} onClick={() => setOpen(true)}><ShieldAlert size={18}/><b>{activeApprovals.length}</b></button> : null;

  return <aside className="mcp-approval-drawer" aria-label="MCP 审批中心">
    <header><div><ShieldAlert size={18}/><span><strong>MCP 审批</strong><small aria-live="polite">{activeApprovals.length} 项待处理</small></span></div><IconButton icon={X} label="关闭 MCP 审批中心" onClick={() => setOpen(false)}/></header>
    <div className="mcp-approval-body">
      {!activeApprovals.length ? <div className="mcp-approval-empty"><Check size={25}/><strong>没有待审批请求</strong><span>新的敏感操作会在这里显示。</span></div> : activeApprovals.map((approval) => {
        const seconds = Math.max(0, Math.ceil((new Date(approval.expiresAt).getTime() - now) / 1000));
        const highRisk = approval.risk === "high" || approval.risk === "critical";
        const canSaveRule = approval.canSaveRule && approval.risk === "low";
        return <article key={approval.id} className={highRisk ? "high-risk" : ""}>
          <div className="mcp-approval-title"><span className={`mcp-risk ${approval.risk}`}>{highRisk && <AlertTriangle size={11}/>} {approval.risk}</span><span><Clock3 size={12}/>{seconds} 秒</span></div>
          <h3>{TOOL_LABELS[approval.tool] ?? approval.tool}</h3>
          <dl><div><dt>客户端</dt><dd>{approval.clientName}</dd></div><div><dt>连接</dt><dd>{approval.connectionName}</dd></div><div><dt>目标</dt><dd><code>{approval.target}</code></dd></div></dl>
          <pre>{approval.preview}</pre>
          <footer><button className="button secondary" disabled={busyId === approval.id} onClick={() => void decide(approval, "reject")}><XCircle size={14}/>拒绝</button>{approval.canAllowSession && <button className="button secondary" disabled={busyId === approval.id} onClick={() => void decide(approval, "session")}><Check size={14}/>本次运行允许</button>}{canSaveRule && <button className="button secondary" disabled={busyId === approval.id} onClick={() => void decide(approval, "persistent")}><Check size={14}/>保存精确规则</button>}<button className="button primary" disabled={busyId === approval.id} onClick={() => void decide(approval, "once")}><Check size={14}/>允许一次</button></footer>
        </article>;
      })}
    </div>
  </aside>;
}
