import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { defaultSettings, type ConnectionProfile } from "../../types";
import { ProxySettings } from "./ProxySettings";

const jump: ConnectionProfile = {
  id: "jump",
  folderId: null,
  protocol: "ssh",
  name: "办公跳板机",
  host: "jump.example.com",
  port: 2222,
  username: "ops",
  authType: "sshAgent",
  privateKeyPath: null,
  certificatePath: null,
  hostKeyPolicy: "strict",
  note: "",
  tags: [],
  encoding: "UTF-8",
  startupCommand: null,
  proxyId: null,
  environment: {},
  hasCredential: false,
  createdAt: "",
  updatedAt: "",
  lastConnectedAt: null,
};

describe("ProxySettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    localStorage.setItem("cnshell-settings", JSON.stringify(defaultSettings));
    vi.spyOn(api, "listProxies").mockResolvedValue([
      {
        id: "proxy",
        name: "公司出口",
        type: "sshJump",
        host: "",
        port: 1080,
        username: null,
        jumpConnectionId: "jump",
        hasCredential: false,
      },
    ]);
  });

  it("shows the complete proxy route and supports editing", async () => {
    const user = userEvent.setup();
    const save = vi.spyOn(api, "saveProxy").mockResolvedValue({
      id: "proxy",
      name: "公司出口",
      type: "sshJump",
      host: "",
      port: 1080,
      username: null,
      jumpConnectionId: "jump",
      hasCredential: false,
    });
    render(<ProxySettings connections={[jump]} onError={() => undefined} />);

    expect(
      await screen.findByText(
        "应用 → SSH 跳板 办公跳板机（ops@jump.example.com:2222） → 目标服务器",
      ),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "编辑 公司出口" }));
    expect(screen.getByText("编辑代理")).toBeVisible();
    expect(screen.getByRole("textbox", { name: "名称" })).toHaveValue(
      "公司出口",
    );
    await user.click(screen.getByRole("button", { name: "保存代理" }));
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({ id: "proxy", jumpConnectionId: "jump" }),
    );
  });
});
