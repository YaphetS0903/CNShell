import uPlot from "uplot";
import { useEffect, useRef } from "react";
import type { MonitorHistorySample } from "../../lib/runtime-metrics";

export function MonitorHistoryChart({ samples, metric }: { samples: MonitorHistorySample[]; metric: "network" | "latency" }) {
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = host.current;
    if (!element || samples.length < 2) return;
    const timestamps = samples.map((sample) => sample.timestamp / 1000);
    const network = metric === "network";
    const data: uPlot.AlignedData = network
      ? [timestamps, samples.map((sample) => sample.received), samples.map((sample) => sample.sent)]
      : [timestamps, samples.map((sample) => sample.latency)];
    const style = getComputedStyle(document.documentElement);
    const chart = new uPlot({
      width: Math.max(120, element.clientWidth), height: 54,
      cursor: { show: false }, legend: { show: false },
      axes: [{ show: false }, { show: false }], scales: { x: { time: true }, y: { range: (_u, min, max) => [0, Math.max(1, max ?? min ?? 1)] } },
      series: network
        ? [{}, { label: "下载", stroke: style.getPropertyValue("--accent").trim(), width: 1.4 }, { label: "上传", stroke: style.getPropertyValue("--blue").trim(), width: 1.4 }]
        : [{}, { label: "延迟", stroke: style.getPropertyValue("--warning").trim(), width: 1.4 }],
    }, data, element);
    const resize = new ResizeObserver(() => chart.setSize({ width: Math.max(120, element.clientWidth), height: 54 }));
    resize.observe(element);
    return () => { resize.disconnect(); chart.destroy(); };
  }, [metric, samples]);
  const latest = samples.at(-1);
  const label = metric === "network"
    ? `网络最近五分钟趋势，当前下载 ${Math.round(latest?.received ?? 0)} 字节每秒，上传 ${Math.round(latest?.sent ?? 0)} 字节每秒`
    : `延迟最近五分钟趋势，当前 ${latest?.latency == null ? "不可用" : `${Math.round(latest.latency)} 毫秒`}`;
  return <div className="monitor-history-chart" ref={host} role="img" aria-label={label} />;
}
