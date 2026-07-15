import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { ConnectionProfile, PluginAuditEvent, PluginInstallRecord, PluginPublisherRoot } from "../../types";
import { PluginSettings } from "./PluginSettings";

const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

const record: PluginInstallRecord = {
  id: "com.example.status",
  name: "Status",
  version: "1.0.0",
  manifestPath: "/tmp/status/manifest.json",
  digest: `sha256:${"a".repeat(64)}`,
  entrypointDigest: `sha256:${"b".repeat(64)}`,
  publisherId: "com.example",
  signatureStatus: "verified",
  requestedPermissions: ["ui"],
  deniedPermissions: [],
  grantedPermissions: [],
  enabled: false,
  executable: true,
  installedAt: "2026-07-15T00:00:00Z",
  updatedAt: "2026-07-15T00:00:00Z",
};
const publisher: PluginPublisherRoot = {
  id: "com.example",
  name: "Example",
  publicKey: `ed25519:${"a".repeat(43)}`,
  fingerprint: `sha256:${"c".repeat(64)}`,
  enabled: true,
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
const connection = {
  id: "server-1",
  name: "Production",
  protocol: "ssh",
  host: "server.example.com",
  port: 22,
} as ConnectionProfile;

describe("PluginSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    dialog.open.mockReset();
    dialog.save.mockReset();
    vi.spyOn(api, "listPlugins").mockResolvedValue([record]);
    vi.spyOn(api, "listPluginPublishers").mockResolvedValue([publisher]);
    vi.spyOn(api, "listPluginAudit").mockResolvedValue([audit]);
  });

  it("shows verified plugin and publisher state and records removal", async () => {
    const user = userEvent.setup();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const remove = vi.spyOn(api, "removePlugin").mockResolvedValue();
    dialog.save.mockResolvedValue("/tmp/plugin-audit.json");
    const exportAudit = vi.spyOn(api, "exportPluginAudit").mockResolvedValue(1);
    render(<PluginSettings connections={[]} onError={vi.fn()}/>);

    expect(await screen.findByText("Status 1.0.0")).toBeInTheDocument();
    expect(screen.getByText(/签名可执行 · 已禁用/)).toBeInTheDocument();
    expect(screen.getByText(/com\.example · 已信任/)).toBeInTheDocument();
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
    render(<PluginSettings connections={[]} onError={vi.fn()}/>);

    await user.click(screen.getByRole("button", { name: "登记插件" }));
    await waitFor(() => expect(register).toHaveBeenCalledWith("/tmp/plugin/manifest.json"));
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining("固定 manifest 与 WASM 摘要"));
  });

  it("requires confirmation before enabling a verified plugin", async () => {
    const user = userEvent.setup();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const enable = vi.spyOn(api, "enablePlugin").mockResolvedValue({ ...record, enabled: true, grantedPermissions: ["ui"] });
    render(<PluginSettings connections={[]} onError={vi.fn()}/>);

    await user.click(await screen.findByRole("button", { name: "启用" }));
    await waitFor(() => expect(enable).toHaveBeenCalledWith(record.id));
    expect(window.confirm).toHaveBeenCalledWith(expect.stringContaining("无 WASI"));
  });

  it("passes only the selected connection and exposes a rejectable one-shot proxy request", async () => {
    const user = userEvent.setup();
    const enabled = { ...record, enabled: true, grantedPermissions: ["connectionMetadata", "credentialProxy"] };
    vi.spyOn(api, "listPlugins").mockResolvedValue([enabled]);
    const run = vi.spyOn(api, "runPlugin").mockResolvedValue({
      pluginId: enabled.id,
      version: enabled.version,
      statusCode: 0,
      fuelConsumed: 12,
      durationMs: 1,
      logs: [],
      credentialProxyRequest: {
        requestId: "request-1",
        pluginId: enabled.id,
        pluginName: enabled.name,
        connectionId: connection.id,
        connectionName: connection.name,
        operation: "connectionTest",
        expiresAt: "2026-07-15T00:02:00Z",
      },
    });
    const reject = vi.spyOn(api, "rejectPluginCredentialProxy").mockResolvedValue();
    render(<PluginSettings connections={[connection]} onError={vi.fn()}/>);

    await user.selectOptions(await screen.findByRole("combobox", { name: /本次运行使用的连接/ }), connection.id);
    await user.click(screen.getByRole("button", { name: "运行" }));
    await waitFor(() => expect(run).toHaveBeenCalledWith({ id: enabled.id, connectionId: connection.id, selectedText: null }));
    expect(await screen.findByText(/请求一次性 connectionTest/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "拒绝" }));
    await waitFor(() => expect(reject).toHaveBeenCalledWith("request-1"));
  });
});
