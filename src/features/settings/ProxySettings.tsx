import { KeyRound, Pencil, Plus, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { usePlatformCapabilities } from "../../lib/platform";
import type {
  ConnectionProfile,
  ProxyProfile,
  ProxyType,
  SaveProxyInput,
} from "../../types";
import { useSettingsModuleDraftState } from "./settings-module-draft";

function proxyRoute(proxy: ProxyProfile, connections: ConnectionProfile[]) {
  if (proxy.type === "sshJump") {
    const jump = connections.find((item) => item.id === proxy.jumpConnectionId);
    return jump
      ? `SSH 跳板 ${jump.name}（${jump.username}@${jump.host}:${jump.port}）`
      : "SSH 跳板（连接不可用）";
  }
  const type = proxy.type === "socks5" ? "SOCKS5" : "HTTP";
  return `${type} ${proxy.username ? `${proxy.username}@` : ""}${proxy.host}:${proxy.port}`;
}

export function ProxySettings({
  connections,
  onError,
}: {
  connections: ConnectionProfile[];
  onError: (message: string) => void;
}) {
  const platform = usePlatformCapabilities();
  const [proxies, setProxies] = useState<ProxyProfile[]>([]);
  const [editing, setEditing] = useState<SaveProxyInput | null>(null);
  useSettingsModuleDraftState(editing !== null);
  const load = useCallback(
    () =>
      api
        .listProxies()
        .then(setProxies)
        .catch((error) => onError(errorMessage(error))),
    [onError],
  );

  useEffect(() => {
    void load();
  }, [load]);

  const add = (type: ProxyType) =>
    setEditing({
      id: crypto.randomUUID(),
      name: "",
      type,
      host: "",
      port: type === "http" ? 8080 : 1080,
      username: null,
      jumpConnectionId: null,
      credential: "",
    });
  const edit = (proxy: ProxyProfile) =>
    setEditing({
      id: proxy.id,
      name: proxy.name,
      type: proxy.type,
      host: proxy.host,
      port: proxy.port,
      username: proxy.username,
      jumpConnectionId: proxy.jumpConnectionId,
      credential: "",
    });
  const submit = async () => {
    if (!editing) return;
    try {
      await api.saveProxy(editing);
      setEditing(null);
      await load();
    } catch (error) {
      onError(errorMessage(error));
    }
  };

  return (
    <section className="proxy-settings" aria-label="代理与跳板机">
      <div className="section-heading">
        <h3>
          <KeyRound size={16} />
          代理与跳板机
        </h3>
        <div>
          <button className="mini-button" onClick={() => add("socks5")}>
            <Plus size={13} />
            SOCKS5
          </button>
          <button className="mini-button" onClick={() => add("http")}>
            <Plus size={13} />
            HTTP
          </button>
          <button className="mini-button" onClick={() => add("sshJump")}>
            <Plus size={13} />
            SSH 跳板
          </button>
        </div>
      </div>
      {proxies.map((proxy) => (
        <div className="proxy-row" key={proxy.id}>
          <div className="proxy-copy">
            <strong>{proxy.name}</strong>
            <small>应用 → {proxyRoute(proxy, connections)} → 目标服务器</small>
          </div>
          <div className="proxy-actions">
            <IconButton
              icon={Pencil}
              label={`编辑 ${proxy.name}`}
              onClick={() => edit(proxy)}
            />
            <IconButton
              icon={Trash2}
              label={`删除 ${proxy.name}`}
              onClick={() => {
                if (!confirm(`删除代理“${proxy.name}”？此操作无法撤销。`))
                  return;
                void api
                  .deleteProxy(proxy.id)
                  .then(load)
                  .catch((error) => onError(errorMessage(error)));
              }}
            />
          </div>
        </div>
      ))}
      {!proxies.length && !editing && (
        <p className="muted-copy">未配置代理，连接将直接访问服务器。</p>
      )}
      {editing && (
        <div className="proxy-form">
          <strong className="proxy-form-title">
            {proxies.some((proxy) => proxy.id === editing.id)
              ? "编辑代理"
              : "添加代理"}
          </strong>
          <label>
            <span>名称</span>
            <input
              value={editing.name}
              onChange={(event) =>
                setEditing({ ...editing, name: event.target.value })
              }
            />
          </label>
          {editing.type === "sshJump" ? (
            <label>
              <span>跳板连接</span>
              <select
                value={editing.jumpConnectionId ?? ""}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    jumpConnectionId: event.target.value || null,
                  })
                }
              >
                <option value="">选择 SSH 连接</option>
                {connections
                  .filter((item) => item.protocol === "ssh")
                  .map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.name}
                    </option>
                  ))}
              </select>
            </label>
          ) : (
            <>
              <label>
                <span>主机</span>
                <input
                  value={editing.host}
                  onChange={(event) =>
                    setEditing({ ...editing, host: event.target.value })
                  }
                />
              </label>
              <label>
                <span>端口</span>
                <input
                  type="number"
                  value={editing.port}
                  onChange={(event) =>
                    setEditing({ ...editing, port: Number(event.target.value) })
                  }
                />
              </label>
              <label>
                <span>用户名（可选）</span>
                <input
                  value={editing.username ?? ""}
                  onChange={(event) =>
                    setEditing({
                      ...editing,
                      username: event.target.value || null,
                    })
                  }
                />
              </label>
              <label>
                <span>密码（{platform.credentialStoreName}）</span>
                <input
                  type="password"
                  value={editing.credential ?? ""}
                  onChange={(event) =>
                    setEditing({ ...editing, credential: event.target.value })
                  }
                />
              </label>
            </>
          )}
          <div className="form-actions">
            <button
              className="button secondary"
              onClick={() => setEditing(null)}
            >
              取消
            </button>
            <button className="button primary" onClick={() => void submit()}>
              保存代理
            </button>
          </div>
        </div>
      )}
    </section>
  );
}
