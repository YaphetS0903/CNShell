import {
  CheckCircle2,
  CircleX,
  Copy,
  LoaderCircle,
  Pencil,
  ShieldCheck,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Modal } from "../../components/Modal";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { waitForTask } from "../../lib/background-task";
import type { ConnectionDiagnostic, ConnectionProfile } from "../../types";

interface Props {
  connection: ConnectionProfile;
  onClose: () => void;
  onError: (message: string) => void;
  onEdit?: () => void;
}

export function ConnectionDiagnostics({
  connection,
  onClose,
  onError,
  onEdit,
}: Props) {
  const [items, setItems] = useState<ConnectionDiagnostic[]>([]);
  const [loading, setLoading] = useState(true);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const activeTask = useRef<string | null>(null);
  const run = useCallback(() => {
    if (activeTask.current) void api.cancelTask(activeTask.current);
    setLoading(true);
    void api
      .startConnectionTest(connection.id)
      .then(async (task) => {
        activeTask.current = task.id;
        setTaskId(task.id);
        try {
          const result = await waitForTask(task);
          if (activeTask.current === task.id)
            setItems(result as ConnectionDiagnostic[]);
        } catch (error) {
          if (
            activeTask.current === task.id &&
            (error as DOMException).name !== "AbortError"
          )
            onError(errorMessage(error));
        } finally {
          if (activeTask.current === task.id) {
            activeTask.current = null;
            setLoading(false);
            setTaskId(null);
          }
        }
      })
      .catch((error) => {
        setLoading(false);
        onError(errorMessage(error));
      });
  }, [connection.id, onError]);
  useEffect(() => {
    run();
    return () => {
      if (activeTask.current) void api.cancelTask(activeTask.current);
    };
  }, [run]);
  const unknown = items.find(
    (item) => item.stage === "hostKey" && !item.ok && item.fingerprint,
  );
  const trust = async () => {
    if (!unknown?.fingerprint || !unknown.algorithm) return;
    try {
      await api.trustHost(
        connection.id,
        unknown.fingerprint,
        unknown.algorithm,
      );
      run();
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const copySummary = async () => {
    try {
      await navigator.clipboard.writeText(
        buildDiagnosticSummary(connection.protocol, items),
      );
      setCopied(true);
      setTimeout(() => setCopied(false), 1600);
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const close = () => {
    if (taskId) void api.cancelTask(taskId);
    onClose();
  };
  return (
    <Modal title={`${connection.name} · 连接诊断`} onClose={close}>
      <div className="diagnostic-list">
        {loading && (
          <div className="loading-state">
            <LoaderCircle className="spin" />
            正在检查 DNS、TCP、指纹与认证…
          </div>
        )}
        {items.map((item, index) => (
          <div
            className={`diagnostic-row ${item.ok ? "ok" : "failed"}`}
            key={`${item.stage}-${index}`}
          >
            {item.ok ? <CheckCircle2 size={17} /> : <CircleX size={17} />}
            <div>
              <strong>{stageLabel(item.stage)}</strong>
              <span>{item.message}</span>
              {item.fingerprint && <code>{item.fingerprint}</code>}
              {!item.ok && (
                <div className="diagnostic-remedy">
                  <p>{remedyForStage(item.stage)}</p>
                  {onEdit && item.stage !== "hostKey" && (
                    <button className="mini-button" onClick={onEdit}>
                      <Pencil size={12} />
                      编辑连接
                    </button>
                  )}
                </div>
              )}
            </div>
            {item.latencyMs != null && <small>{item.latencyMs} ms</small>}
          </div>
        ))}
        {unknown && (
          <div className="diagnostic-trust">
            <ShieldCheck size={18} />
            <p>请先从服务器控制台核对指纹。确认一致后才能写入信任记录。</p>
            <button className="button primary" onClick={trust}>
              已核对，信任此指纹
            </button>
          </div>
        )}
        <footer className="form-actions diagnostic-footer">
          <span className="diagnostic-copy-status" role="status">
            {copied ? "脱敏摘要已复制" : "摘要不含主机、账号、IP、路径和指纹"}
          </span>
          {items.length > 0 && (
            <button
              className="button secondary"
              onClick={() => void copySummary()}
            >
              <Copy size={14} />
              复制脱敏摘要
            </button>
          )}
          {loading && (
            <button
              className="button secondary"
              onClick={() => taskId && void api.cancelTask(taskId)}
            >
              取消检测
            </button>
          )}
          <button className="button secondary" onClick={run} disabled={loading}>
            重新检测
          </button>
          <button className="button primary" onClick={close}>
            完成
          </button>
        </footer>
      </div>
    </Modal>
  );
}

const stageLabel = (stage: ConnectionDiagnostic["stage"]) =>
  ({
    dns: "DNS",
    tcp: "TCP / SSH",
    proxy: "代理 / 跳板机",
    hostKey: "主机指纹",
    authentication: "认证",
    shell: "Shell",
    complete: "完成",
  })[stage];

const remedyForStage = (stage: ConnectionDiagnostic["stage"]) =>
  ({
    dns: "检查主机名拼写、当前网络和 DNS 设置。",
    tcp: "检查端口、防火墙、安全组及 SSH 服务状态。",
    proxy: "检查代理或跳板机地址、凭据和可达性。",
    hostKey: "从服务器控制台核对指纹，再使用下方的信任操作。",
    authentication: "检查用户名、密码、私钥及服务器授权配置。",
    shell: "检查账号默认 Shell、启动命令及远端权限。",
    complete: "重新运行诊断并查看首个失败步骤。",
  })[stage];

function buildDiagnosticSummary(
  protocol: ConnectionProfile["protocol"],
  items: ConnectionDiagnostic[],
) {
  const protocolLabel = {
    ssh: "SSH",
    rdp: "RDP",
    local: "Local Shell",
    telnet: "Telnet",
    serial: "Serial",
  }[protocol];
  const lines = [
    "CNshell 连接诊断（脱敏）",
    `协议：${protocolLabel}`,
    `时间：${new Date().toLocaleString()}`,
  ];
  for (const item of items) {
    lines.push(
      `${stageLabel(item.stage)}：${item.ok ? "通过" : "失败"}${item.latencyMs != null ? ` · ${item.latencyMs} ms` : ""}${item.ok ? "" : ` · ${remedyForStage(item.stage)}`}`,
    );
  }
  return lines.join("\n");
}
