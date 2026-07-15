import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { EncryptedSyncSettings } from "./EncryptedSyncSettings";

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

describe("EncryptedSyncSettings Touch ID vault", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    dialog.open.mockResolvedValue("/Users/test/Sync");
  });

  it("keeps the manual passphrase recovery path when Touch ID is unavailable", async () => {
    vi.spyOn(api, "touchIdSyncStatus").mockResolvedValue({supported:false,saved:false,message:"当前 Mac 未提供可用的 Touch ID"});
    const user = userEvent.setup();
    render(<EncryptedSyncSettings onError={vi.fn()}/>);

    await user.click(screen.getByRole("button", {name:"选择"}));

    expect(await screen.findByText("当前 Mac 未提供可用的 Touch ID")).toBeInTheDocument();
    expect(screen.getByText("同步口令（至少 8 位；默认不保存）")).toBeInTheDocument();
    expect(screen.getByRole("button", {name:/用手动口令生成/})).toBeDisabled();
  });

  it("saves the current passphrase behind Touch ID without displaying it again", async () => {
    vi.spyOn(api, "touchIdSyncStatus").mockResolvedValue({supported:true,saved:false,message:"可使用 Touch ID"});
    const save = vi.spyOn(api, "saveTouchIdSyncKey").mockResolvedValue({supported:true,saved:true,message:"已保存同步口令"});
    const user = userEvent.setup();
    render(<EncryptedSyncSettings onError={vi.fn()}/>);

    await user.click(screen.getByRole("button", {name:"选择"}));
    await user.type(screen.getByLabelText("同步口令（至少 8 位；默认不保存）"), "vault-password");
    await user.click(await screen.findByRole("button", {name:/用 Touch ID 保存当前口令/}));

    expect(save).toHaveBeenCalledWith("/Users/test/Sync", "vault-password");
    expect(screen.getByLabelText("同步口令（至少 8 位；默认不保存）")).toHaveValue("");
    expect(await screen.findByText("已保存同步口令")).toBeInTheDocument();
  });

  it("runs encrypted sync through the backend Touch ID command without a frontend secret", async () => {
    vi.spyOn(api, "touchIdSyncStatus").mockResolvedValue({supported:true,saved:true,message:"已保存同步口令"});
    const sync = vi.spyOn(api, "writeEncryptedSyncWithTouchId").mockResolvedValue({path:"/Users/test/Sync/cnshell.sync",connectionCount:2,conflictCopy:null,encrypted:true});
    const user = userEvent.setup();
    render(<EncryptedSyncSettings onError={vi.fn()}/>);

    await user.click(screen.getByRole("button", {name:"选择"}));
    await user.click(await screen.findByRole("button", {name:"用 Touch ID 生成"}));

    await waitFor(() => expect(sync).toHaveBeenCalledWith("/Users/test/Sync", {includeHosts:true,includePrivateKeyPaths:false,includeCredentials:false}));
    expect(await screen.findByText("已处理 2 个连接")).toBeInTheDocument();
  });
});
