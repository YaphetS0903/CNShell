import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "../../types";
import { useAppStore } from "../../store/app-store";
import SettingsModal from "./SettingsModal";
import { api } from "../../lib/api";

describe("SettingsModal", () => {
  beforeEach(() => useAppStore.setState({ settingsOpen: true, settings: defaultSettings, connections: [] }));
  it("opens on basic settings and loads advanced sections on demand", async () => {
    const user = userEvent.setup();
    render(<SettingsModal/>);
    expect(screen.getByRole("heading", { name: "基础设置" })).toBeVisible();
    expect(screen.queryByText(/Argon2id/)).not.toBeInTheDocument();
    expect(screen.queryByText("MCP 服务")).not.toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "连接与安全" }));
    expect(await screen.findByText(/Argon2id/)).toBeVisible();
    expect(screen.getByRole("heading", { name: "连接与安全" })).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "关于与支持" }));
    expect(await screen.findByRole("button", { name: "检查更新" })).toBeVisible();
    expect(screen.getByRole("button", { name: "报告问题" })).toBeVisible();
  });
  it("keeps the draft while switching categories and confirms unsaved close", async () => {
    const user = userEvent.setup();
    vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<SettingsModal/>);
    fireEvent.change(screen.getByLabelText("主题"), { target: { value: "light" } });
    expect(screen.getByText("有未保存的修改")).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "关于与支持" }));
    await user.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.getByRole("dialog", { name: "设置" })).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "基础设置" }));
    expect(screen.getByLabelText("主题")).toHaveValue("light");
  });
  it("describes encrypted exports and diagnostics privacy", async () => {
    const user = userEvent.setup();
    render(<SettingsModal/>);
    await user.click(screen.getByRole("tab", { name: "连接与安全" }));
    expect(await screen.findByText(/Argon2id/)).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "关于与支持" }));
    expect(await screen.findByText(/不包含主机、用户名、路径或命令/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "检查更新" })).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "基础设置" }));
    expect(screen.getByRole("button", { name: "清空全部命令历史" })).toBeInTheDocument();
  });
  it("saves font size and terminal color preferences",async()=>{const user=userEvent.setup();const save=vi.spyOn(api,"saveSettings").mockImplementation(async(settings)=>settings);render(<SettingsModal/>);fireEvent.change(screen.getByLabelText("字号"),{target:{value:"18"}});await user.click(screen.getByRole("radio",{name:"Solarized"}));await user.click(screen.getByRole("button",{name:"保存设置"}));await waitFor(()=>expect(save).toHaveBeenCalled());expect(save.mock.calls[0][0].terminal).toMatchObject({fontSize:18,colorScheme:"solarizedDark"});});
});
