import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { PluginAuditEvent, PluginInstallRecord } from "../../types";
import { PluginSettings } from "./PluginSettings";

const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

const record: PluginInstallRecord = {
  id: "com.example.status",
  name: "Status",
  version: "1.0.0",
  manifestPath: "/tmp/status/manifest.json",
  digest: `sha256:${"a".repeat(64)}`,
  signatureStatus: "unsigned",
  requestedPermissions: ["ui", "network"],
  deniedPermissions: ["network"],
  enabled: false,
  executable: false,
  installedAt: "2026-07-15T00:00:00Z",
  updatedAt: "2026-07-15T00:00:00Z",
};
const audit: PluginAuditEvent = {
  id: "audit-1",
  pluginId: record.id,
  action: "registered-blocked",
  detail: "插件已登记但保持不可执行",
  digest: record.digest,
  createdAt: "2026-07-15T00:00:00Z",
};

describe("PluginSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    dialog.open.mockReset();
    dialog.save.mockReset();
    vi.spyOn(api, "listPlugins").mockResolvedValue([record]);
    vi.spyOn(api, "listPluginAudit").mockResolvedValue([audit]);
  });

  it("shows blocked plugin state and records removal", async () => {
    const user = userEvent.setup();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const remove = vi.spyOn(api, "removePlugin").mockResolvedValue();
    dialog.save.mockResolvedValue("/tmp/plugin-audit.json");
    const exportAudit = vi.spyOn(api, "exportPluginAudit").mockResolvedValue(1);
    render(<PluginSettings onError={vi.fn()}/>);

    expect(await screen.findByText("Status 1.0.0")).toBeInTheDocument();
    expect(screen.getByText(/阻断执行 · 已禁用/)).toBeInTheDocument();
    expect(screen.getByText(/registered-blocked/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "导出" }));
    await waitFor(() => expect(exportAudit).toHaveBeenCalledWith("/tmp/plugin-audit.json"));
    await user.click(screen.getByRole("button", { name: "移除 Status" }));
    await waitFor(() => expect(remove).toHaveBeenCalledWith(record.id));
  });

  it("requires confirmation before registering a manifest", async () => {
    const user = userEvent.setup();
    dialog.open.mockResolvedValue("/tmp/plugin/manifest.json");
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const register = vi.spyOn(api, "registerPlugin").mockResolvedValue(record);
    render(<PluginSettings onError={vi.fn()}/>);

    await user.click(screen.getByRole("button", { name: "登记为阻断插件" }));
    await waitFor(() => expect(register).toHaveBeenCalledWith("/tmp/plugin/manifest.json"));
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining("不可执行状态"));
  });
});
