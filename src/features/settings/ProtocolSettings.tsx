import {
  AlertTriangle,
  CheckCircle2,
  RefreshCw,
  Radio,
  XCircle,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { ConnectionProfile, ProtocolCapability } from "../../types";

export function ProtocolSettings({
  connections,
  onError,
}: {
  connections: ConnectionProfile[];
  onError: (message: string) => void;
}) {
  const [capabilities, setCapabilities] = useState<ProtocolCapability[]>([]);
  const [selected, setSelected] = useState("");
  const [agentForwarding, setAgentForwarding] = useState(false);
  const [loading, setLoading] = useState(false);
  const refresh = useCallback(async () => {
    try {
      setCapabilities(await api.protocolCapabilities());
    } catch (error) {
      onError(errorMessage(error));
    }
  }, [onError]);
  useEffect(() => {
    void refresh();
  }, [refresh]);
  useEffect(() => {
    if (!selected) {
      setAgentForwarding(false);
      return;
    }
    void api
      .getProtocolOptions(selected)
      .then((value) => setAgentForwarding(value.agentForwarding))
      .catch((error) => onError(errorMessage(error)));
  }, [selected, onError]);
  const save = async (value: boolean) => {
    if (!selected) return;
    if (
      value &&
      !confirm(
        "Agent 转发会允许远端主机借用本机 Agent 进行签名。仅应对完全可信的服务器启用。\n\n确认启用？",
      )
    )
      return;
    setLoading(true);
    try {
      const saved = await api.saveProtocolOptions({
        connectionId: selected,
        agentForwarding: value,
      });
      setAgentForwarding(saved.agentForwarding);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  };
  const agentAvailable =
    capabilities.find((item) => item.id === "agentForwarding")?.available ??
    false;
  return (
    <section className="protocol-settings">
      <div className="section-heading">
        <div>
          <h3>
            <Radio size={16} />
            高级协议与转发
          </h3>
          <p>只启用当前环境能安全、完整支持的能力。</p>
        </div>
        <button className="mini-button" onClick={() => void refresh()}>
          <RefreshCw size={12} />
          重新探测
        </button>
      </div>
      <div className="capability-grid" aria-live="polite">
        {capabilities.map((item) => (
          <article
            key={item.id}
            className={item.available ? "available" : "unavailable"}
          >
            {item.available ? (
              <CheckCircle2 size={15} />
            ) : (
              <XCircle size={15} />
            )}
            <div>
              <strong>{item.label}</strong>
              <span>
                {item.available ? "依赖可用" : "依赖不完整"}
                {item.executable ? ` · ${item.executable}` : ""}
              </span>
              <p>{item.message}</p>
              {item.securityWarning && (
                <small>
                  <AlertTriangle size={12} />
                  {item.securityWarning}
                </small>
              )}
            </div>
          </article>
        ))}
      </div>
      <div className="protocol-option">
        <label>
          <span>按连接配置 Agent 转发</span>
          <select
            value={selected}
            onChange={(event) => setSelected(event.target.value)}
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
        <label className="check-row">
          <input
            type="checkbox"
            checked={agentForwarding}
            disabled={!selected || !agentAvailable || loading}
            onChange={(event) => void save(event.target.checked)}
          />
          <span>为该连接启用 SSH Agent 转发（高风险）</span>
        </label>
        <p className="muted-copy">
          更改仅影响下一次连接；当前会话不会静默重建。
        </p>
      </div>
    </section>
  );
}
