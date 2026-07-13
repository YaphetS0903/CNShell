import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defaultSettings } from "../../types";
import { useAppStore } from "../../store/app-store";
import SettingsModal from "./SettingsModal";
import { api } from "../../lib/api";

describe("SettingsModal", () => {
  beforeEach(() => useAppStore.setState({ settingsOpen: true, settings: defaultSettings, connections: [] }));
  it("describes encrypted exports and diagnostics privacy", () => {
    render(<SettingsModal/>);
    expect(screen.getByText(/Argon2id/)).toBeInTheDocument();
    expect(screen.getByText(/不包含主机、用户名、路径或命令/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "清空全部命令历史" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "检查更新" })).toBeInTheDocument();
  });
  it("saves font size and terminal color preferences",async()=>{const user=userEvent.setup();const save=vi.spyOn(api,"saveSettings").mockImplementation(async(settings)=>settings);render(<SettingsModal/>);fireEvent.change(screen.getByLabelText("字号"),{target:{value:"18"}});await user.click(screen.getByRole("radio",{name:"Solarized"}));await user.click(screen.getByRole("button",{name:"保存设置"}));await waitFor(()=>expect(save).toHaveBeenCalled());expect(save.mock.calls[0][0].terminal).toMatchObject({fontSize:18,colorScheme:"solarizedDark"});});
});
