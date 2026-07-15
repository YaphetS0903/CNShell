import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { WebDavSyncSettings } from "./WebDavSyncSettings";

describe("WebDavSyncSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "listWebDavProfiles").mockResolvedValue([]);
    vi.spyOn(api, "onWebDavSyncProgress").mockResolvedValue(() => undefined);
    vi.spyOn(api, "saveWebDavProfile").mockImplementation(async (input) => ({
      id: input.id,
      name: input.name,
      url: `${input.url.replace(/\/$/, "")}/`,
      username: input.username,
      hasCredential: true,
      syncOnStartup: input.syncOnStartup,
      hasSyncPassphrase: true,
      syncOptions: input.syncOptions,
    }));
  });

  it("saves credentials and startup sync as separate fields", async () => {
    const user = userEvent.setup();
    render(<WebDavSyncSettings onError={vi.fn()} />);
    await user.type(screen.getByRole("textbox", { name: "名称" }), "团队 DAV");
    await user.type(screen.getByRole("textbox", { name: "HTTPS 地址" }), "https://dav.example.test/cnshell");
    await user.type(screen.getByRole("textbox", { name: "用户名" }), "alice");
    await user.type(screen.getByLabelText("密码"), "webdav-secret");
    await user.click(screen.getByRole("checkbox", { name: /启动时自动导入/ }));
    await user.type(screen.getByLabelText("启动同步口令"), "sync-secret");
    await user.click(screen.getByRole("button", { name: "保存配置" }));
    await waitFor(() => expect(api.saveWebDavProfile).toHaveBeenCalledWith(expect.objectContaining({ username: "alice", password: "webdav-secret", syncOnStartup: true, syncPassphrase: "sync-secret" })));
  });
});
