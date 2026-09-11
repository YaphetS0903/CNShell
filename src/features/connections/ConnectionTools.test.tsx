import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { ConnectionProfile, PortForward } from "../../types";
import { ConnectionDiagnostics } from "./ConnectionDiagnostics";
import { TunnelManager } from "./TunnelManager";

const connection: ConnectionProfile = {
  id: "connection-1",
  folderId: null,
  protocol: "ssh",
  name: "生产数据库",
  host: "secret.example.test",
  port: 22,
  username: "private-user",
  authType: "password",
  privateKeyPath: null,
  certificatePath: null,
  hostKeyPolicy: "strict",
  note: "",
  tags: [],
  encoding: "UTF-8",
  startupCommand: null,
  proxyId: null,
  environment: {},
  hasCredential: true,
  createdAt: "",
  updatedAt: "",
  lastConnectedAt: null,
};

describe("connection diagnostics and tunnels", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("copies a diagnostic summary without connection identifiers or raw errors", async () => {
    const user = userEvent.setup();
    const writeText = vi
      .spyOn(navigator.clipboard, "writeText")
      .mockResolvedValue(undefined);
    vi.spyOn(api, "startConnectionTest").mockResolvedValue({
      id: "task-1",
      kind: "connectionDiagnostic",
      status: "completed",
      result: [
        {
          stage: "dns",
          ok: false,
          message: "secret.example.test resolved to 192.0.2.42",
          latencyMs: 17,
          fingerprint: "SHA256:private-fingerprint",
          algorithm: "ssh-ed25519",
        },
      ],
      error: null,
      createdAt: "",
    });
    const onEdit = vi.fn();
    render(
      <ConnectionDiagnostics
        connection={connection}
        onClose={vi.fn()}
        onError={vi.fn()}
        onEdit={onEdit}
      />,
    );

    expect(await screen.findByText(/检查主机名拼写/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "复制脱敏摘要" }));
    const summary = writeText.mock.calls[0][0] as string;
    expect(summary).toContain("DNS：失败 · 17 ms");
    expect(summary).not.toMatch(
      /secret\.example|private-user|192\.0\.2\.42|private-fingerprint/,
    );
    await user.click(screen.getByRole("button", { name: "编辑连接" }));
    expect(onEdit).toHaveBeenCalledOnce();
  });

  it("previews tunnel direction and starts only after saving", async () => {
    vi.spyOn(api, "listForwards").mockResolvedValue([]);
    const saveForward = vi
      .spyOn(api, "saveForward")
      .mockImplementation(async (input) => input);
    const startForward = vi
      .spyOn(api, "startForward")
      .mockResolvedValue(undefined);
    const user = userEvent.setup();
    render(
      <TunnelManager
        connection={connection}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: "新建隧道" }));
    expect(
      screen.getByText("本机 127.0.0.1:8080 → 127.0.0.1:80（经 生产数据库）"),
    ).toBeInTheDocument();
    await user.selectOptions(
      screen.getByRole("combobox", { name: "转发类型" }),
      "remote",
    );
    expect(
      screen.getByText(
        "生产数据库 上的 127.0.0.1:8080 → 本机可达的 127.0.0.1:80",
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "保存并启动" }));
    await waitFor(() => expect(startForward).toHaveBeenCalledOnce());
    expect(saveForward).toHaveBeenCalledBefore(startForward);
    const saved = saveForward.mock.calls[0][0] as PortForward;
    expect(saved.type).toBe("remote");
  });
});
