import { useShallow } from "zustand/react/shallow";
import {
  Activity,
  Clipboard,
  Cpu,
  Download,
  HardDrive,
  LayoutDashboard,
  LoaderCircle,
  MemoryStick,
  Network,
  RefreshCw,
  ServerCog,
  type LucideIcon,
} from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import type { SystemInfo } from "../../types";
import { IconButton } from "../../components/IconButton";
import { errorMessage, formatBytes } from "../../lib/format";
import { useAppStore } from "../../store/app-store";
import { NetworkDiagnostics } from "./NetworkDiagnostics";
import "./SystemInfoPanel.css";

type SectionId =
  | "overview"
  | "resources"
  | "network"
  | "disks"
  | "connections";

const sections: { id: SectionId; label: string; icon: LucideIcon }[] = [
  { id: "overview", label: "概览", icon: LayoutDashboard },
  { id: "resources", label: "CPU 与内存", icon: Cpu },
  { id: "network", label: "网络", icon: Network },
  { id: "disks", label: "磁盘", icon: HardDrive },
  { id: "connections", label: "端口与连接", icon: Activity },
];

export function SystemInfoPanel({ sessionId }: { sessionId: string }) {
  const [info, setInfo] = useState<SystemInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [activeSection, setActiveSection] = useState<SectionId>("overview");
  const { setError } = useAppStore(
    useShallow((state) => ({ setError: state.setError })),
  );
  const load = useCallback(() => {
    setLoading(true);
    api
      .systemInfo(sessionId)
      .then(setInfo)
      .catch((reason) => setError(errorMessage(reason)))
      .finally(() => setLoading(false));
  }, [sessionId, setError]);
  useEffect(() => {
    load();
  }, [load]);
  if (loading)
    return (
      <div className="loading-state">
        <LoaderCircle className="spin" />读取系统信息…
      </div>
    );
  if (!info)
    return (
      <div className="empty-files">
        <ServerCog size={28} />无法读取系统信息
      </div>
    );

  const text = JSON.stringify(info, null, 2);
  const exportInfo = async () => {
    const path = await save({ defaultPath: `${info.hostname}-system-info.json` });
    if (!path) return;
    try {
      await api.exportSystemInfo(sessionId, path);
    } catch (error) {
      setError(errorMessage(error));
    }
  };
  const copyInfo = async () => {
    try {
      await navigator.clipboard.writeText(text);
    } catch (error) {
      setError(`复制系统信息失败：${errorMessage(error)}`);
    }
  };

  return (
    <div className="system-info">
      <div className="system-info-toolbar">
        <strong>{info.hostname}</strong>
        <span>{info.os}</span>
        <IconButton icon={RefreshCw} label="刷新系统信息" onClick={load} />
        <IconButton
          icon={Clipboard}
          label="复制系统信息"
          onClick={() => void copyInfo()}
        />
        <IconButton
          icon={Download}
          label="导出系统信息"
          onClick={() => void exportInfo()}
        />
      </div>
      <nav
        className="system-info-tabs"
        role="tablist"
        aria-label="系统信息分类"
        onKeyDown={(event) => {
          if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
            return;
          event.preventDefault();
          const current = sections.findIndex((item) => item.id === activeSection);
          const next =
            event.key === "Home"
              ? 0
              : event.key === "End"
                ? sections.length - 1
                : event.key === "ArrowRight"
                  ? (current + 1) % sections.length
                  : (current - 1 + sections.length) % sections.length;
          const id = sections[next]?.id;
          if (!id) return;
          setActiveSection(id);
          requestAnimationFrame(() =>
            document.getElementById(`system-info-tab-${id}`)?.focus(),
          );
        }}
      >
        {sections.map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            id={`system-info-tab-${id}`}
            role="tab"
            aria-selected={activeSection === id}
            aria-controls={`system-info-view-${id}`}
            tabIndex={activeSection === id ? 0 : -1}
            className={activeSection === id ? "active" : ""}
            onClick={() => setActiveSection(id)}
          >
            <Icon size={14} />
            {label}
          </button>
        ))}
      </nav>
      <div
        id={`system-info-view-${activeSection}`}
        role="tabpanel"
        aria-labelledby={`system-info-tab-${activeSection}`}
        className={`system-info-view ${activeSection}`}
      >
        {activeSection === "overview" && <Overview info={info} />}
        {activeSection === "resources" && <Resources info={info} />}
        {activeSection === "network" && <Interfaces info={info} />}
        {activeSection === "disks" && <Disks info={info} />}
        {activeSection === "connections" && (
          <NetworkDiagnostics
            sessionId={sessionId}
            onError={(message) => setError(message)}
          />
        )}
      </div>
    </div>
  );
}

function Overview({ info }: { info: SystemInfo }) {
  return (
    <div className="system-overview-grid">
      <Info label="操作系统" value={info.os} wide />
      <Info
        label="内核"
        value={[info.kernelName, info.kernel].filter(Boolean).join(" ")}
      />
      <Info label="架构" value={info.architecture} />
      <Info label="运行时间" value={formatUptime(info.uptimeSeconds)} />
      <Info
        label="负载（1 / 5 / 15 分钟）"
        value={info.load.map(formatLoad).join("  /  ")}
      />
      <Info label="CPU" value={`${info.cpuModel} · ${info.cpuCores} 核`} wide />
      <Info
        label="内存"
        value={`${formatBytes(info.memoryUsedBytes)} / ${formatBytes(info.memoryTotalBytes)}`}
      />
      <Info
        label="交换空间"
        value={
          info.swapTotalBytes
            ? `${formatBytes(info.swapUsedBytes)} / ${formatBytes(info.swapTotalBytes)}`
            : "未启用"
        }
      />
    </div>
  );
}

function Resources({ info }: { info: SystemInfo }) {
  const usage = info.cpuUsage;
  const cpuMetrics = [
    ["用户", usage.userPercent],
    ["系统", usage.systemPercent],
    ["Nice", usage.nicePercent],
    ["空闲", usage.idlePercent],
    ["I/O 等待", usage.ioWaitPercent],
    ["硬中断", usage.irqPercent],
    ["软中断", usage.softIrqPercent],
    ["Steal", usage.stealPercent],
  ] as const;
  return (
    <>
      <section className="system-info-block">
        <h3>
          <Cpu size={14} />CPU 硬件
        </h3>
        <dl className="hardware-details">
          <Detail label="型号" value={info.cpuModel} wide />
          <Detail label="核心数" value={`${info.cpuCores} 核`} />
          <Detail
            label="当前频率"
            value={
              info.cpuFrequencyMhz > 0
                ? `${info.cpuFrequencyMhz.toLocaleString(undefined, { maximumFractionDigits: 1 })} MHz`
                : "—"
            }
          />
          <Detail label="缓存" value={info.cpuCache || "—"} />
        </dl>
        {info.cpuBogomips > 0 && (
          <details className="advanced-hardware">
            <summary>高级硬件信息</summary>
            <span>BogoMIPS {info.cpuBogomips.toFixed(2)}</span>
          </details>
        )}
      </section>
      <section className="system-info-block">
        <h3>
          <Activity size={14} />CPU 实时占用
        </h3>
        <div className="cpu-usage-grid">
          {cpuMetrics.map(([label, value]) => (
            <CpuMetric key={label} label={label} value={value} />
          ))}
        </div>
      </section>
      <section className="system-info-block memory-block">
        <h3>
          <MemoryStick size={14} />内存与交换空间
        </h3>
        <ResourceUsage
          label="内存"
          used={info.memoryUsedBytes}
          total={info.memoryTotalBytes}
          available={info.memoryAvailableBytes}
        />
        <ResourceUsage
          label="交换空间"
          used={info.swapUsedBytes}
          total={info.swapTotalBytes}
          available={info.swapAvailableBytes}
        />
      </section>
    </>
  );
}

function Interfaces({ info }: { info: SystemInfo }) {
  return (
    <section className="system-info-block">
      <h3>
        <Network size={14} />网络接口
      </h3>
      <div className="system-table-wrap">
        <table className="network-interface-table">
          <thead>
            <tr>
              <th>名称</th>
              <th>地址</th>
              <th>累计接收</th>
              <th>累计发送</th>
              <th>接收速度</th>
              <th>发送速度</th>
            </tr>
          </thead>
          <tbody>
            {info.interfaces.map((item) => (
              <tr key={item.name}>
                <td>{item.name}</td>
                <td title={item.addresses.join(", ")}>
                  {item.addresses.join(", ") || "—"}
                </td>
                <td>{formatBytes(item.rxTotalBytes)}</td>
                <td>{formatBytes(item.txTotalBytes)}</td>
                <td className="network-rate receive">
                  ↓ {formatBytes(item.rxBytesPerSecond)}/s
                </td>
                <td className="network-rate send">
                  ↑ {formatBytes(item.txBytesPerSecond)}/s
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function Disks({ info }: { info: SystemInfo }) {
  return (
    <section className="system-info-block">
      <h3>
        <HardDrive size={14} />文件系统
      </h3>
      <div className="system-table-wrap">
        <table className="disk-info-table">
          <thead>
            <tr>
              <th>设备</th>
              <th>挂载点</th>
              <th>大小</th>
              <th>已用</th>
              <th>可用</th>
            </tr>
          </thead>
          <tbody>
            {info.disks.map((disk) => (
              <tr key={`${disk.filesystem}-${disk.mountPoint}`}>
                <td>{disk.filesystem}</td>
                <td>{disk.mountPoint}</td>
                <td>{formatBytes(disk.totalBytes)}</td>
                <td>
                  {formatBytes(disk.usedBytes)} ({disk.usedPercent.toFixed(0)}%)
                </td>
                <td>{formatBytes(disk.availableBytes)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function Info({
  label,
  value,
  wide = false,
}: {
  label: string;
  value: string;
  wide?: boolean;
}) {
  return (
    <div className={`system-info-card ${wide ? "wide" : ""}`}>
      <small>{label}</small>
      <strong>{value || "—"}</strong>
    </div>
  );
}

function Detail({
  label,
  value,
  wide = false,
}: {
  label: string;
  value: string;
  wide?: boolean;
}) {
  return (
    <div className={wide ? "wide" : ""}>
      <dt>{label}</dt>
      <dd>{value || "—"}</dd>
    </div>
  );
}

function CpuMetric({ label, value }: { label: string; value: number }) {
  const normalized = Math.min(100, Math.max(0, value));
  return (
    <div className="cpu-usage-item">
      <span>{label}</span>
      <strong>{normalized.toFixed(1)}%</strong>
      <div
        className="system-resource-bar"
        role="progressbar"
        aria-label={`CPU ${label}`}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(normalized)}
      >
        <i style={{ width: `${normalized}%` }} />
      </div>
    </div>
  );
}

function ResourceUsage({
  label,
  used,
  total,
  available,
}: {
  label: string;
  used: number;
  total: number;
  available: number;
}) {
  const percentage = total ? Math.min(100, (used / total) * 100) : 0;
  return (
    <div className="system-resource-usage">
      <div>
        <strong>{label}</strong>
        <span>{total ? `${percentage.toFixed(1)}%` : "未启用"}</span>
      </div>
      <div
        className="system-resource-bar"
        role="progressbar"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(percentage)}
      >
        <i style={{ width: `${percentage}%` }} />
      </div>
      <small>
        总计 {formatBytes(total)} · 已用 {formatBytes(used)} · 可用{" "}
        {formatBytes(available)}
      </small>
    </div>
  );
}

function formatUptime(seconds: number) {
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  const minutes = Math.floor((seconds % 3_600) / 60);
  return `${days} 天 ${hours} 小时 ${minutes} 分钟`;
}

function formatLoad(value: number) {
  return value.toFixed(2);
}
