import { Circle, Play, Plus, ShieldAlert, Square, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { Modal } from "../../components/Modal";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { ConnectionProfile, PortForward } from "../../types";

interface Props {
  connection: ConnectionProfile;
  onClose: () => void;
  onError: (message: string) => void;
}

export function TunnelManager({ connection, onClose, onError }: Props) {
  const [items, setItems] = useState<PortForward[]>([]);
  const [editing, setEditing] = useState<PortForward | null>(null);
  const load = useCallback(
    () => api.listForwards(connection.id).then(setItems).catch((error) => onError(errorMessage(error))),
    [connection.id, onError]
  );

  useEffect(() => {
    void load();
    const timer = setInterval(() => void load(), 2000);
    return () => clearInterval(timer);
  }, [load]);

  const add = () => setEditing({
    id: crypto.randomUUID(), connectionId: connection.id, type: "local",
    bindHost: "127.0.0.1", bindPort: 8080, destinationHost: "127.0.0.1",
    destinationPort: 80, autoStart: false, status: "stopped", error: null
  });
  const save = async () => {
    if (!editing) return;
    try { await api.saveForward(editing); setEditing(null); await load(); }
    catch (error) { onError(errorMessage(error)); }
  };

  return <Modal title={`${connection.name} · 端口转发`} onClose={onClose} wide>
    <div className="tunnel-manager">
      <div className="section-heading">
        <p>本地、远程与动态 SOCKS5 转发均使用独立 SSH Transport。</p>
        <button className="button primary" onClick={add}><Plus size={14}/>新建隧道</button>
      </div>
      {items.map((item) => <div className="tunnel-row" key={item.id}>
        <span className={`tunnel-status ${item.status}`}><Circle size={9} fill="currentColor"/></span>
        <div><strong>{tunnelLabel(item)}</strong><small>{item.type === "dynamic" ? `${item.bindHost}:${item.bindPort} → SOCKS5` : `${item.bindHost}:${item.bindPort} → ${item.destinationHost}:${item.destinationPort}`}{item.error && ` · ${item.error}`}</small></div>
        {item.status === "running"
          ? <IconButton icon={Square} label="停止隧道" onClick={() => api.stopForward(item.id).then(load).catch((error) => onError(errorMessage(error)))}/>
          : <IconButton icon={Play} label="启动隧道" onClick={() => api.startForward(item.id).then(load).catch((error) => onError(errorMessage(error)))}/>
        }
        <IconButton icon={Trash2} label="删除隧道" onClick={() => api.deleteForward(item.id).then(load).catch((error) => onError(errorMessage(error)))}/>
      </div>)}
      {!items.length && !editing && <div className="empty-files">尚未配置端口转发</div>}
      {editing && <div className="tunnel-form">
        <label><span>类型</span><select value={editing.type} onChange={(event) => setEditing({ ...editing, type: event.target.value as PortForward["type"] })}><option value="local">本地转发</option><option value="remote">远程转发</option><option value="dynamic">动态 SOCKS5</option></select></label>
        <label><span>监听地址</span><input value={editing.bindHost} onChange={(event) => setEditing({ ...editing, bindHost: event.target.value })}/></label>
        <label><span>监听端口</span><input type="number" min="1" max="65535" value={editing.bindPort} onChange={(event) => setEditing({ ...editing, bindPort: Number(event.target.value) })}/></label>
        {editing.type !== "dynamic" && <>
          <label><span>目标主机</span><input value={editing.destinationHost ?? ""} onChange={(event) => setEditing({ ...editing, destinationHost: event.target.value })}/></label>
          <label><span>目标端口</span><input type="number" min="1" max="65535" value={editing.destinationPort ?? ""} onChange={(event) => setEditing({ ...editing, destinationPort: Number(event.target.value) })}/></label>
        </>}
        <label className="check-row"><input type="checkbox" checked={editing.autoStart} onChange={(event) => setEditing({ ...editing, autoStart: event.target.checked })}/><span>连接后自动启动</span></label>
        {!isLoopback(editing.bindHost) && <div className="inline-warning"><ShieldAlert size={14}/>监听非本机地址可能向局域网或公网开放端口，请确认防火墙策略。</div>}
        <div className="form-actions"><button className="button secondary" onClick={() => setEditing(null)}>取消</button><button className="button primary" onClick={save}>保存隧道</button></div>
      </div>}
    </div>
  </Modal>;
}

const tunnelLabel = (item: PortForward) => ({ local: "本地转发", remote: "远程转发", dynamic: "动态 SOCKS5" }[item.type]);
const isLoopback = (host: string) => ["127.0.0.1", "localhost", "::1"].includes(host);
