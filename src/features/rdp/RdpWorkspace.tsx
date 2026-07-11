import { MonitorUp, RefreshCw, Square } from "lucide-react";
import type { TerminalSession } from "../../types";
import "./RdpWorkspace.css";

export function RdpWorkspace({session,onReconnect,onClose}:{session:TerminalSession;onReconnect:()=>void;onClose:()=>void}) {
  return <section className="rdp-workspace" aria-label={`RDP 会话 ${session.title}`}>
    <div className="rdp-workspace-icon"><MonitorUp size={38}/></div>
    <h2>{session.title}</h2>
    <p>远程桌面正在独立的 FreeRDP 受管窗口中运行。窗口缩放、键鼠、剪贴板和动态分辨率由 FreeRDP 处理。</p>
    <span className={`rdp-state ${session.status}`}><i/>{rdpStatus(session.status)}</span>
    {session.lastError&&<div className="inline-error">{session.lastError}</div>}
    <div className="form-actions"><button className="button secondary" onClick={onReconnect}><RefreshCw size={14}/>重新打开</button><button className="button primary" onClick={onClose}><Square size={13}/>关闭远程桌面</button></div>
  </section>;
}

const rdpStatus=(status:TerminalSession["status"])=>({connecting:"正在启动",online:"窗口运行中",reconnecting:"正在重连",failed:"Helper 已异常退出",closed:"已关闭"}[status]);
