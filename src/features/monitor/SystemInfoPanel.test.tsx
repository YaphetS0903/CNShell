import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import type { SystemInfo } from "../../types";
import { SystemInfoPanel } from "./SystemInfoPanel";

vi.mock("@tauri-apps/plugin-dialog", () => ({ save: vi.fn() }));

const info: SystemInfo = {
  hostname: "vm-example",
  os: "Ubuntu 24.04.4 LTS",
  kernelName: "Linux",
  kernel: "6.8.0-101-generic",
  architecture: "x86_64",
  cpuModel: "Example CPU",
  cpuCores: 4,
  cpuFrequencyMhz: 2595.12,
  cpuCache: "512 KB",
  cpuBogomips: 5190.24,
  cpuUsage: {
    userPercent: 12.5,
    systemPercent: 5.5,
    nicePercent: 0,
    idlePercent: 80,
    ioWaitPercent: 1,
    irqPercent: 0.2,
    softIrqPercent: 0.5,
    stealPercent: 0.3,
  },
  memoryUsedBytes: 1.5 * 1024 ** 3,
  memoryTotalBytes: 2 * 1024 ** 3,
  memoryAvailableBytes: 0.5 * 1024 ** 3,
  swapUsedBytes: 1 * 1024 ** 3,
  swapTotalBytes: 8 * 1024 ** 3,
  swapAvailableBytes: 7 * 1024 ** 3,
  uptimeSeconds: 90_061,
  load: [0.1, 0.2, 0.3],
  interfaces: [
    {
      name: "eth0",
      addresses: ["10.0.4.9/22"],
      rxBytesPerSecond: 4_915,
      txBytesPerSecond: 44_400,
      rxTotalBytes: 61_200_000_000,
      txTotalBytes: 39_700_000_000,
    },
  ],
  disks: [
    {
      filesystem: "/dev/vda2",
      mountPoint: "/",
      totalBytes: 39.3 * 1024 ** 3,
      usedBytes: 26.1 * 1024 ** 3,
      availableBytes: 11.5 * 1024 ** 3,
      usedPercent: 66.4,
    },
  ],
};

describe("SystemInfoPanel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAppStore.setState({ error: null });
    vi.spyOn(api, "systemInfo").mockResolvedValue(info);
  });

  it("separates overview, resources, interfaces and disks into discoverable tabs", async () => {
    render(<SystemInfoPanel sessionId="session-1" />);

    expect(await screen.findByText("vm-example")).toBeInTheDocument();
    expect(screen.getByText("Linux 6.8.0-101-generic")).toBeInTheDocument();
    expect(screen.getByText("1 天 1 小时 1 分钟")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "CPU 与内存" }));
    expect(screen.getByText("2,595.1 MHz")).toBeInTheDocument();
    expect(
      screen.getByRole("progressbar", { name: "CPU 用户" }),
    ).toHaveAttribute("aria-valuenow", "13");
    expect(screen.getByText(/总计 2.0 GB · 已用 1.5 GB/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "网络" }));
    expect(screen.getByText("10.0.4.9/22")).toBeInTheDocument();
    expect(screen.getByText("↓ 4.8 KB/s")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("tab", { name: "磁盘" }));
    expect(screen.getByText("/dev/vda2")).toBeInTheDocument();
    expect(screen.getByText(/26.1 GB \(66%\)/)).toBeInTheDocument();
  });
});
