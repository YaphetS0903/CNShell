import { useShallow } from "zustand/react/shallow";
import {
  Activity,
  ChevronDown,
  ChevronRight,
  Clipboard,
  Copy,
  Cpu,
  Download,
  HardDrive,
  LayoutDashboard,
  LoaderCircle,
  MemoryStick,
  Network,
  RefreshCw,
  Search,
  ServerCog,
  type LucideIcon,
} from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import type { SystemInfo } from "../../types";
import { IconButton } from "../../components/IconButton";
import { errorMessage, formatBytes } from "../../lib/format";
import { useAppStore } from "../../store/app-store";
import { NetworkDiagnostics } from "./NetworkDiagnostics";
import { isTemporaryDisk } from "../../lib/runtime-metrics";
import "./SystemInfoPanel.css";

type SectionId = "overview" | "resources" | "network" | "disks" | "connections";

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
  const [collectedAt, setCollectedAt] = useState<Date | null>(null);
  const [activeSection, setActiveSection] = useState<SectionId>("overview");
  const { setError } = useAppStore(
    useShallow((state) => ({ setError: state.setError })),
  );
  const load = useCallback(() => {
    setLoading(true);
    api
      .systemInfo(sessionId)
      .then((value) => {
        setInfo(value);
        setCollectedAt(new Date());
      })
      .catch((reason) => setError(errorMessage(reason)))
      .finally(() => setLoading(false));
  }, [sessionId, setError]);
  useEffect(() => {
    load();
  }, [load]);
  if (loading)
    return (
      <div className="loading-state">
        <LoaderCircle className="spin" />
        读取系统信息…
      </div>
    );
  if (!info)
    return (
      <div className="empty-files">
        <ServerCog size={28} />
        无法读取系统信息
      </div>
    );

  const text = JSON.stringify(info, null, 2);
  const exportInfo = async () => {
    const path = await save({
      defaultPath: `${info.hostname}-system-info.json`,
    });
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
        <small className="system-info-collected">
          {collectedAt
            ? `采集于 ${collectedAt.toLocaleTimeString()}`
            : "等待采集"}
        </small>
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
          const current = sections.findIndex(
            (item) => item.id === activeSection,
          );
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
        {activeSection === "network" && (
          <Interfaces info={info} onError={setError} />
        )}
        {activeSection === "disks" && <Disks info={info} onError={setError} />}
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
          <Cpu size={14} />
          CPU 硬件
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
          <Activity size={14} />
          CPU 实时占用
        </h3>
        <div className="cpu-usage-grid">
          {cpuMetrics.map(([label, value]) => (
            <CpuMetric key={label} label={label} value={value} />
          ))}
        </div>
      </section>
      <section className="system-info-block memory-block">
        <h3>
          <MemoryStick size={14} />
          内存与交换空间
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

function Interfaces({
  info,
  onError,
}: {
  info: SystemInfo;
  onError: (message: string) => void;
}) {
  const [query, setQuery] = useState("");
  const [expandedIpv6, setExpandedIpv6] = useState(() => new Set<string>());
  const interfaces = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase("zh-CN");
    return info.interfaces.filter((item) =>
      `${item.name} ${item.addresses.join(" ")}`
        .toLocaleLowerCase("zh-CN")
        .includes(normalized),
    );
  }, [info.interfaces, query]);
  const copy = async (item: SystemInfo["interfaces"][number]) => {
    try {
      await navigator.clipboard.writeText(
        [
          `接口：${item.name}`,
          `地址：${item.addresses.join(", ") || "无"}`,
          `接收：${formatBytes(item.rxTotalBytes)} · ${formatBytes(item.rxBytesPerSecond)}/s`,
          `发送：${formatBytes(item.txTotalBytes)} · ${formatBytes(item.txBytesPerSecond)}/s`,
        ].join("\n"),
      );
    } catch (error) {
      onError(`复制网络接口失败：${errorMessage(error)}`);
    }
  };
  return (
    <section className="system-info-block">
      <div className="system-block-heading">
        <h3>
          <Network size={14} />
          网络接口
        </h3>
        <label className="system-table-filter">
          <Search size={13} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="筛选接口或地址"
            aria-label="筛选网络接口"
          />
          <span>
            {interfaces.length}/{info.interfaces.length}
          </span>
        </label>
      </div>
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
              <th>
                <span className="sr-only">操作</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {interfaces.map((item) => {
              const ipv4 = item.addresses.filter(
                (address) => !address.includes(":"),
              );
              const ipv6 = item.addresses.filter((address) =>
                address.includes(":"),
              );
              const ipv6Open = expandedIpv6.has(item.name);
              return (
                <tr key={item.name}>
                  <td>{item.name}</td>
                  <td className="network-addresses">
                    <span title={ipv4.join(", ")}>
                      {ipv4.join(", ") || (!ipv6.length ? "—" : "仅 IPv6")}
                    </span>
                    {ipv6.length > 0 && (
                      <>
                        <button
                          type="button"
                          aria-expanded={ipv6Open}
                          onClick={() =>
                            setExpandedIpv6((current) => {
                              const next = new Set(current);
                              if (next.has(item.name)) next.delete(item.name);
                              else next.add(item.name);
                              return next;
                            })
                          }
                        >
                          {ipv6Open ? (
                            <ChevronDown size={12} />
                          ) : (
                            <ChevronRight size={12} />
                          )}
                          IPv6 {ipv6.length}
                        </button>
                        {ipv6Open && <code>{ipv6.join("\n")}</code>}
                      </>
                    )}
                  </td>
                  <td>{formatBytes(item.rxTotalBytes)}</td>
                  <td>{formatBytes(item.txTotalBytes)}</td>
                  <td className="network-rate receive">
                    ↓ {formatBytes(item.rxBytesPerSecond)}/s
                  </td>
                  <td className="network-rate send">
                    ↑ {formatBytes(item.txBytesPerSecond)}/s
                  </td>
                  <td>
                    <IconButton
                      icon={Copy}
                      label={`复制 ${item.name} 网络信息`}
                      onClick={() => void copy(item)}
                    />
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {!interfaces.length && (
          <div className="system-table-empty">没有匹配的网络接口</div>
        )}
      </div>
    </section>
  );
}

function Disks({
  info,
  onError,
}: {
  info: SystemInfo;
  onError: (message: string) => void;
}) {
  const [showAll, setShowAll] = useState(false);
  const [query, setQuery] = useState("");
  const filteredDisks = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase("zh-CN");
    return info.disks.filter((disk) =>
      `${disk.filesystem} ${disk.mountPoint}`
        .toLocaleLowerCase("zh-CN")
        .includes(normalized),
    );
  }, [info.disks, query]);
  const primaryDisks = useMemo(
    () => filteredDisks.filter((disk) => !isTemporaryDisk(disk)),
    [filteredDisks],
  );
  const hiddenCount = filteredDisks.length - primaryDisks.length;
  const disks = showAll ? filteredDisks : primaryDisks;
  const copy = async (disk: SystemInfo["disks"][number]) => {
    try {
      await navigator.clipboard.writeText(
        [
          `文件系统：${disk.filesystem}`,
          `挂载点：${disk.mountPoint}`,
          `容量：${formatBytes(disk.totalBytes)}`,
          `已用：${formatBytes(disk.usedBytes)} (${disk.usedPercent.toFixed(0)}%)`,
          `可用：${formatBytes(disk.availableBytes)}`,
        ].join("\n"),
      );
    } catch (error) {
      onError(`复制文件系统失败：${errorMessage(error)}`);
    }
  };
  return (
    <section className="system-info-block">
      <div className="system-block-heading system-disk-heading">
        <h3>
          <HardDrive size={14} />
          文件系统
        </h3>
        <label className="system-table-filter">
          <Search size={13} />
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="筛选设备或挂载点"
            aria-label="筛选文件系统"
          />
          <span>
            {disks.length}/{info.disks.length}
          </span>
        </label>
        {hiddenCount > 0 && (
          <button
            type="button"
            className="mini-button"
            onClick={() => setShowAll((value) => !value)}
          >
            {showAll
              ? "隐藏临时挂载"
              : `显示全部挂载点（另 ${hiddenCount} 项）`}
          </button>
        )}
      </div>
      <div className="system-table-wrap">
        <table className="disk-info-table">
          <thead>
            <tr>
              <th>设备</th>
              <th>挂载点</th>
              <th>大小</th>
              <th>已用</th>
              <th>可用</th>
              <th>
                <span className="sr-only">操作</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {disks.map((disk) => (
              <tr key={`${disk.filesystem}-${disk.mountPoint}`}>
                <td>{disk.filesystem}</td>
                <td>{disk.mountPoint}</td>
                <td>{formatBytes(disk.totalBytes)}</td>
                <td>
                  {formatBytes(disk.usedBytes)} ({disk.usedPercent.toFixed(0)}%)
                </td>
                <td>{formatBytes(disk.availableBytes)}</td>
                <td>
                  <IconButton
                    icon={Copy}
                    label={`复制 ${disk.mountPoint} 文件系统信息`}
                    onClick={() => void copy(disk)}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {!disks.length && (
          <div className="system-table-empty">没有匹配的文件系统</div>
        )}
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
