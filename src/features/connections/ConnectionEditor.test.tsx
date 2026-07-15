import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAppStore } from "../../store/app-store";
import { ConnectionEditor } from "./ConnectionEditor";
import type { ConnectionProfile } from "../../types";
import { api } from "../../lib/api";
import { defaultSettings } from "../../types";

const dialog = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

describe("ConnectionEditor", () => {
  beforeEach(() => {
    dialog.open.mockReset();
    vi.restoreAllMocks();
    useAppStore.setState({ connectionEditorOpen: true, editingConnection: null, connections: [], error: null });
    vi.spyOn(api,"listFolders").mockResolvedValue([{id:"root",name:"生产",parentId:null,sortOrder:0},{id:"child",name:"华南",parentId:"root",sortOrder:0}]);
  });
  it("shows secure SSH defaults and switches RDP port", async () => {
    const user = userEvent.setup();
    render(<ConnectionEditor/>);
    expect(screen.getByRole("combobox", { name: "主机密钥策略" })).toHaveValue("strict");
    await user.type(screen.getByPlaceholderText("例如：tmux attach || tmux"), "tmux attach");
    await user.selectOptions(screen.getByRole("combobox", { name: "协议" }), "rdp");
    expect(screen.getByRole("spinbutton", { name: "端口" })).toHaveValue(3389);
    expect(screen.getByLabelText("Windows 密码")).toBeRequired();
    expect(screen.queryByText("主机密钥策略")).not.toBeInTheDocument();
    await user.selectOptions(screen.getByRole("combobox", { name: "协议" }), "ssh");
    expect(screen.getByPlaceholderText("例如：tmux attach || tmux")).toHaveValue("");
  });

  it("fills a persisted connection without exposing its credential", async () => {
    const connection: ConnectionProfile = {
      id: "persisted", folderId: "child", protocol: "ssh", name: "持久化测试机", host: "example.test", port: 2222,
      username: "ubuntu", authType: "password", privateKeyPath: null, certificatePath: null, hostKeyPolicy: "strict", note: "验收", tags: ["云主机"],
      encoding: "UTF-8", startupCommand: null, proxyId: null, environment: {}, hasCredential: true, createdAt: "", updatedAt: "", lastConnectedAt: null
    };
    useAppStore.setState({ connectionEditorOpen: true, editingConnection: connection });

    render(<ConnectionEditor/>);

    expect(screen.getByRole("textbox", { name: "名称" })).toHaveValue("持久化测试机");
    expect(screen.getByRole("textbox", { name: "主机" })).toHaveValue("example.test");
    expect(screen.getByRole("spinbutton", { name: "端口" })).toHaveValue(2222);
    expect(screen.getByLabelText("密码")).toHaveValue("");
    expect(screen.getByPlaceholderText("留空以保留已保存凭据")).toBeInTheDocument();
    expect(await screen.findByRole("option",{name:"生产 / 华南"})).toBeInTheDocument();
    expect(screen.getByRole("combobox",{name:"文件夹"})).toHaveValue("child");
  });

  it("selects an extensionless private key with the native picker", async () => {
    const user = userEvent.setup();
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
    dialog.open.mockResolvedValue("/Users/test/.ssh/id_ed25519");
    render(<ConnectionEditor/>);

    await user.selectOptions(screen.getByRole("combobox", { name: "认证方式" }), "privateKey");
    await user.click(screen.getByRole("button", { name: "选择私钥" }));

    expect(dialog.open).toHaveBeenCalledWith({ multiple: false, directory: false });
    expect(screen.getByRole("textbox", { name: "私钥路径" })).toHaveValue("/Users/test/.ssh/id_ed25519");
  });

  it("validates and previews an OpenSSH user certificate", async () => {
    const user = userEvent.setup();
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
    dialog.open.mockResolvedValue("/Users/test/.ssh/id_ed25519-cert.pub");
    vi.spyOn(api, "inspectSshCertificate").mockResolvedValue({
      path: "/Users/test/.ssh/id_ed25519-cert.pub",
      certificateType: "ssh-ed25519-cert-v01@openssh.com user certificate",
      keyId: "deploy-2026",
      serial: "42",
      signingCa: "ED25519 SHA256:ca",
      validFrom: "2026-07-01T00:00:00",
      validTo: "2026-08-01T00:00:00",
      principals: ["ubuntu", "deploy"],
      validNow: true,
      status: "valid",
    });
    render(<ConnectionEditor/>);

    await user.selectOptions(screen.getByRole("combobox", { name: "认证方式" }), "sshCertificate");
    await user.click(screen.getByRole("button", { name: "选择证书" }));

    expect(api.inspectSshCertificate).toHaveBeenCalledWith("/Users/test/.ssh/id_ed25519-cert.pub");
    expect(screen.getByRole("textbox", { name: "证书路径" })).toHaveValue("/Users/test/.ssh/id_ed25519-cert.pub");
    expect(screen.getByText("deploy-2026")).toBeInTheDocument();
    expect(screen.getByText("ubuntu, deploy")).toBeInTheDocument();
    expect(screen.getByText("有效")).toBeInTheDocument();
  });

  it("detects and displays only the backend-provided FIDO2 identities", async () => {
    const user = userEvent.setup();
    vi.spyOn(api, "listFido2Identities").mockResolvedValue([{keyType:"sk-ssh-ed25519@openssh.com",comment:"YubiKey 5",fingerprint:"SHA256:hardware"}]);
    render(<ConnectionEditor/>);

    await user.selectOptions(screen.getByRole("combobox", { name: "认证方式" }), "fido2Agent");

    expect(await screen.findByText("YubiKey 5")).toBeInTheDocument();
    expect(screen.getByText("sk-ssh-ed25519@openssh.com")).toBeInTheDocument();
    expect(screen.getByText("SHA256:hardware")).toBeInTheDocument();
    expect(screen.queryByLabelText("私钥口令")).not.toBeInTheDocument();
    expect(api.listFido2Identities).toHaveBeenCalledTimes(1);
  });

  it("saves a terminal preference override for one connection",async()=>{const user=userEvent.setup();const connection:ConnectionProfile={id:"persisted",folderId:null,protocol:"ssh",name:"测试机",host:"example.test",port:22,username:"ubuntu",authType:"password",privateKeyPath:null, certificatePath: null,hostKeyPolicy:"strict",note:"",tags:[],encoding:"UTF-8",startupCommand:null,proxyId:null,environment:{},hasCredential:true,createdAt:"",updatedAt:"",lastConnectedAt:null};useAppStore.setState({editingConnection:connection,settings:defaultSettings});vi.spyOn(api,"saveConnection").mockResolvedValue(connection);const save=vi.spyOn(api,"saveSettings").mockImplementation(async(settings)=>settings);vi.spyOn(api,"listConnections").mockResolvedValue([connection]);render(<ConnectionEditor/>);await user.click(screen.getByRole("checkbox",{name:"为此连接覆盖全局终端偏好"}));fireEvent.change(screen.getByLabelText("字号"),{target:{value:"18"}});await user.click(screen.getByRole("button",{name:"保存连接"}));expect(save).toHaveBeenCalledWith(expect.objectContaining({terminalOverrides:{persisted:expect.objectContaining({fontSize:18})}}));});
});
