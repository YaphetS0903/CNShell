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
import { usePlatformCapabilities } from "../../lib/platform";

export function ProtocolSettings({
  connections,
  onError,
}: {
  connections: ConnectionProfile[];
  onError: (message: string) => void;
}) {
  const platform = usePlatformCapabilities();
  const [capabilities, setCapabilities] = useState<ProtocolCapability[]>([]);
  const [selected, setSelected] = useState("");
  const [agentForwarding, setAgentForwarding] = useState(false);
  const [x11Enabled, setX11Enabled] = useState(false);
  const [moshEnabled, setMoshEnabled] = useState(false);
  const [moshPortStart, setMoshPortStart] = useState(60000);
  const [moshPortEnd, setMoshPortEnd] = useState(60010);
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
      setX11Enabled(false);
      setMoshEnabled(false);
      return;
    }
    void api
      .getProtocolOptions(selected)
      .then((value) => {
        setAgentForwarding(value.agentForwarding);
        setX11Enabled(value.x11Enabled);
        setMoshEnabled(value.moshEnabled);
        setMoshPortStart(value.moshPortStart);
        setMoshPortEnd(value.moshPortEnd);
      })
      .catch((error) => onError(errorMessage(error)));
  }, [selected, onError]);
  const saveAgent = async (value: boolean) => {
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
        x11Enabled,
        moshEnabled,
        moshPortStart,
        moshPortEnd,
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
  const moshAvailable =
    capabilities.find((item) => item.id === "mosh")?.available ?? false;
  const x11Available =
    capabilities.find((item) => item.id === "x11")?.available ?? false;
  const saveMosh = async (enabled = moshEnabled) => {
    if (!selected) return;
    setLoading(true);
    try {
      const saved = await api.saveProtocolOptions({
        connectionId: selected,
        agentForwarding,
        x11Enabled: enabled ? false : x11Enabled,
        moshEnabled: enabled,
        moshPortStart,
        moshPortEnd,
      });
      setMoshEnabled(saved.moshEnabled);
      setX11Enabled(saved.x11Enabled);
      setMoshPortStart(saved.moshPortStart);
      setMoshPortEnd(saved.moshPortEnd);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  };
  const saveX11 = async (enabled: boolean) => {
    if (!selected) return;
    if (
      enabled &&
      !confirm(
        `X11 转发允许远端图形程序访问本机${platform.operatingSystem === "windows" ? " X Server" : " XQuartz"}。CNshell 会隔离授权 cookie，但仍只应对完全可信的服务器启用。\n\n确认启用？`,
      )
    )
      return;
    setLoading(true);
    try {
      const saved = await api.saveProtocolOptions({
        connectionId: selected,
        agentForwarding,
        x11Enabled: enabled,
        moshEnabled: enabled ? false : moshEnabled,
        moshPortStart,
        moshPortEnd,
      });
      setX11Enabled(saved.x11Enabled);
      setMoshEnabled(saved.moshEnabled);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setLoading(false);
    }
  };
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
          <span>按连接配置协议选项</span>
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
            checked={x11Enabled}
            disabled={!selected || !x11Available || loading}
            onChange={(event) => void saveX11(event.target.checked)}
          />
          <span>为该连接启用 X11 转发（仅可信主机）</span>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={agentForwarding}
            disabled={!selected || !agentAvailable || loading}
            onChange={(event) => void saveAgent(event.target.checked)}
          />
          <span>为该连接启用 SSH Agent 转发（高风险）</span>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={moshEnabled}
            disabled={!selected || !moshAvailable || loading}
            onChange={(event) => void saveMosh(event.target.checked)}
          />
          <span>为该连接启用 Mosh 漫游终端</span>
        </label>
        {moshEnabled && (
          <div className="protocol-port-range">
            <label>
              <span>UDP 起始端口</span>
              <input
                type="number"
                min={1024}
                max={65535}
                value={moshPortStart}
                onChange={(event) => setMoshPortStart(Number(event.target.value))}
              />
            </label>
            <label>
              <span>UDP 结束端口</span>
              <input
                type="number"
                min={1024}
                max={65535}
                value={moshPortEnd}
                onChange={(event) => setMoshPortEnd(Number(event.target.value))}
              />
            </label>
            <button
              className="button secondary"
              disabled={loading}
              onClick={() => void saveMosh()}
            >
              保存端口范围
            </button>
          </div>
        )}
        {moshEnabled && (
          <p className="muted-copy">
            SSH 仅用于认证和启动服务；请在云防火墙与系统防火墙放行上述 UDP 范围。代理连接仍要求本机可直达服务器 UDP。
          </p>
        )}
        <p className="muted-copy">
          更改仅影响下一次连接；当前会话不会静默重建。
        </p>
      </div>
    </section>
  );
}
