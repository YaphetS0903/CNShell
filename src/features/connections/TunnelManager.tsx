import {
  Circle,
  Play,
  Plus,
  Radio,
  Server,
  ShieldAlert,
  Square,
  Trash2,
} from "lucide-react";
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
  const [saving, setSaving] = useState<"save" | "start" | null>(null);
  const load = useCallback(
    () =>
      api
        .listForwards(connection.id)
        .then(setItems)
        .catch((error) => onError(errorMessage(error))),
    [connection.id, onError],
  );

  useEffect(() => {
    void load();
    const timer = setInterval(() => void load(), 2000);
    return () => clearInterval(timer);
  }, [load]);

  const add = () =>
    setEditing({
      id: crypto.randomUUID(),
      connectionId: connection.id,
      type: "local",
      bindHost: "127.0.0.1",
      bindPort: 8080,
      destinationHost: "127.0.0.1",
      destinationPort: 80,
      autoStart: false,
      status: "stopped",
      error: null,
    });
  const save = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!editing) return;
    const submitter = (event.nativeEvent as SubmitEvent)
      .submitter as HTMLButtonElement | null;
    const intent = submitter?.value === "start" ? "start" : "save";
    setSaving(intent);
    try {
      await api.saveForward(editing);
      if (intent === "start") await api.startForward(editing.id);
      setEditing(null);
      await load();
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setSaving(null);
    }
  };

  return (
    <Modal title={`${connection.name} · 端口转发`} onClose={onClose} wide>
      <div className="tunnel-manager">
        <div className="section-heading">
          <p>通过当前 SSH 连接建立本地、远程或 SOCKS5 隧道。</p>
          <button
            className="button primary"
            onClick={add}
            disabled={Boolean(editing)}
          >
            <Plus size={14} />
            新建隧道
          </button>
        </div>
        {items.map((item) => (
          <div className="tunnel-row" key={item.id}>
            <span className={`tunnel-status ${item.status}`}>
              <Circle size={9} fill="currentColor" />
            </span>
            <div>
              <strong>{tunnelLabel(item)}</strong>
              <small>
                {routeSummary(item, connection.name)}
                {item.error && ` · ${item.error}`}
              </small>
            </div>
            {item.status === "running" ? (
              <IconButton
                icon={Square}
                label="停止隧道"
                onClick={() =>
                  api
                    .stopForward(item.id)
                    .then(load)
                    .catch((error) => onError(errorMessage(error)))
                }
              />
            ) : (
              <IconButton
                icon={Play}
                label="启动隧道"
                onClick={() =>
                  api
                    .startForward(item.id)
                    .then(load)
                    .catch((error) => onError(errorMessage(error)))
                }
              />
            )}
            <IconButton
              icon={Trash2}
              label="删除隧道"
              onClick={() =>
                api
                  .deleteForward(item.id)
                  .then(load)
                  .catch((error) => onError(errorMessage(error)))
              }
            />
          </div>
        ))}
        {!items.length && !editing && (
          <div className="empty-files">尚未配置端口转发</div>
        )}
        {editing && (
          <form className="tunnel-form" onSubmit={save}>
            <label className="tunnel-type-field">
              <span>转发类型</span>
              <select
                value={editing.type}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    type: event.target.value as PortForward["type"],
                  })
                }
              >
                <option value="local">本地转发</option>
                <option value="remote">远程转发</option>
                <option value="dynamic">动态 SOCKS5</option>
              </select>
            </label>
            <output className="tunnel-route-preview" aria-live="polite">
              <Radio size={16} />
              <span>
                <small>当前链路</small>
                <strong>{routeSummary(editing, connection.name)}</strong>
              </span>
            </output>
            <fieldset className="tunnel-endpoint">
              <legend>
                <Radio size={14} />
                {editing.type === "remote" ? "远端监听" : "本机监听"}
              </legend>
              <label>
                <span>监听地址</span>
                <input
                  required
                  value={editing.bindHost}
                  onChange={(event) =>
                    setEditing({ ...editing, bindHost: event.target.value })
                  }
                />
              </label>
              <label>
                <span>监听端口</span>
                <input
                  required
                  type="number"
                  min="1"
                  max="65535"
                  value={editing.bindPort}
                  onChange={(event) =>
                    setEditing({
                      ...editing,
                      bindPort: Number(event.target.value),
                    })
                  }
                />
              </label>
            </fieldset>
            {editing.type !== "dynamic" && (
              <fieldset className="tunnel-endpoint">
                <legend>
                  <Server size={14} />
                  {editing.type === "local" ? "远端目标" : "本机目标"}
                </legend>
                <label>
                  <span>目标主机</span>
                  <input
                    required
                    value={editing.destinationHost ?? ""}
                    onChange={(event) =>
                      setEditing({
                        ...editing,
                        destinationHost: event.target.value,
                      })
                    }
                  />
                </label>
                <label>
                  <span>目标端口</span>
                  <input
                    required
                    type="number"
                    min="1"
                    max="65535"
                    value={editing.destinationPort ?? ""}
                    onChange={(event) =>
                      setEditing({
                        ...editing,
                        destinationPort: Number(event.target.value),
                      })
                    }
                  />
                </label>
              </fieldset>
            )}
            <label className="check-row tunnel-auto-start">
              <input
                type="checkbox"
                checked={editing.autoStart}
                onChange={(event) =>
                  setEditing({ ...editing, autoStart: event.target.checked })
                }
              />
              <span>以后连接成功后自动启动</span>
            </label>
            {!isLoopback(editing.bindHost) && (
              <div className="inline-warning">
                <ShieldAlert size={14} />
                监听非本机地址可能向局域网或公网开放端口，请确认防火墙策略。
              </div>
            )}
            <div className="form-actions tunnel-form-actions">
              <button
                type="button"
                className="button secondary"
                onClick={() => setEditing(null)}
                disabled={Boolean(saving)}
              >
                取消
              </button>
              <button
                className="button secondary"
                value="save"
                disabled={Boolean(saving)}
              >
                保存配置
              </button>
              <button
                className="button primary"
                value="start"
                disabled={Boolean(saving)}
              >
                <Play size={14} />
                保存并启动
              </button>
            </div>
          </form>
        )}
      </div>
    </Modal>
  );
}

const tunnelLabel = (item: PortForward) =>
  ({ local: "本地转发", remote: "远程转发", dynamic: "动态 SOCKS5" })[
    item.type
  ];
const isLoopback = (host: string) =>
  ["127.0.0.1", "localhost", "::1"].includes(host);

function routeSummary(item: PortForward, connectionName: string) {
  const bind = `${item.bindHost}:${item.bindPort}`;
  if (item.type === "dynamic")
    return `本机 ${bind} → SOCKS5（经 ${connectionName}）`;
  const destination = `${item.destinationHost || "目标主机"}:${item.destinationPort || "端口"}`;
  return item.type === "local"
    ? `本机 ${bind} → ${destination}（经 ${connectionName}）`
    : `${connectionName} 上的 ${bind} → 本机可达的 ${destination}`;
}
