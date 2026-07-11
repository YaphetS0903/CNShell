import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ConnectionProfile } from "../../types";
import { AdvancedSettings } from "./AdvancedSettings";

describe("AdvancedSettings", () => {
  it("maps an SSH jump proxy to a connection instead of hidden host fields", async () => {
    const connection: ConnectionProfile = {
      id: "jump", folderId: null, protocol: "ssh", name: "跳板服务器", host: "jump.example", port: 22, username: "ops",
      authType: "sshAgent", privateKeyPath: null, hostKeyPolicy: "strict", note: "", tags: [], encoding: "UTF-8",
      startupCommand: null, proxyId: null, environment: {}, hasCredential: false, createdAt: "", updatedAt: "", lastConnectedAt: null
    };
    const user = userEvent.setup();
    render(<AdvancedSettings connections={[connection]} onChanged={async () => undefined} onError={() => undefined}/>);

    await user.click(screen.getByRole("button", { name: "SSH 跳板" }));

    const selector = screen.getByRole("combobox", { name: "跳板连接" });
    expect(screen.queryByRole("textbox", { name: "主机" })).not.toBeInTheDocument();
    await user.selectOptions(selector, "jump");
    expect(selector).toHaveValue("jump");
  });
});
