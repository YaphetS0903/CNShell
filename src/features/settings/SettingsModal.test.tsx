import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import { defaultSettings } from "../../types";
import SettingsModal from "./SettingsModal";
import { appVersion, releaseDate } from "../../lib/release";

describe("SettingsModal", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    document.documentElement.dataset.interfaceScale = "100";
    document.documentElement.style.setProperty("--interface-scale", "1");
    useAppStore.setState({
      settingsOpen: true,
      settingsTarget: null,
      settings: defaultSettings,
      connections: [],
    });
  });

  it("opens directly on a requested settings module", async () => {
    useAppStore.setState({
      settingsOpen: true,
      settingsTarget: { category: "team", moduleId: "team" },
    });
    render(<SettingsModal />);

    expect(screen.getByRole("tab", { name: "团队与云端" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.getByRole("button", { name: /^团队工作区与在线团队服务/ }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      await screen.findByRole("tab", { name: "本地工作区" }),
    ).toBeVisible();
  });

  it("opens on basic settings and loads advanced modules only after expansion", async () => {
    const user = userEvent.setup();
    const proxies = vi.spyOn(api, "listProxies").mockResolvedValue([]);
    render(<SettingsModal />);

    expect(screen.getByRole("heading", { name: "基础设置" })).toBeVisible();
    expect(screen.queryByText(/Argon2id/)).not.toBeInTheDocument();
    expect(screen.queryByText("MCP 服务")).not.toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "连接与安全" }));
    expect(screen.getByRole("heading", { name: "连接与安全" })).toBeVisible();
    expect(screen.getByRole("button", { name: /^连接库备份/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    expect(proxies).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: /^代理与跳板机/ }));
    await waitFor(() => expect(proxies).toHaveBeenCalledTimes(1));
    await user.click(screen.getByRole("button", { name: /^连接库备份/ }));
    expect(
      screen.getByRole("button", { name: /^代理与跳板机/ }),
    ).toHaveAttribute("aria-expanded", "false");
    expect(await screen.findByText(/Argon2id/)).toBeVisible();

    await user.click(screen.getByRole("tab", { name: "关于与支持" }));
    expect(screen.getByRole("heading", { name: "CNshell" })).toBeVisible();
    expect(screen.getByText(`v${appVersion}`)).toBeVisible();
    expect(screen.getByText(releaseDate)).toBeVisible();
    expect(screen.getByText("Beta 候选版")).toBeVisible();
    await user.click(screen.getByRole("button", { name: /^软件更新/ }));
    expect(
      await screen.findByRole("button", { name: "检查更新" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: /^反馈与诊断/ }));
    expect(
      await screen.findByRole("button", { name: "报告问题" }),
    ).toBeVisible();
  });

  it("keeps the draft while switching categories and confirms unsaved close", async () => {
    const user = userEvent.setup();
    vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<SettingsModal />);
    fireEvent.change(screen.getByLabelText("主题"), {
      target: { value: "light" },
    });
    expect(screen.getByText("基础设置有未保存的修改")).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "关于与支持" }));
    await user.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.getByRole("dialog", { name: "设置" })).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "基础设置" }));
    expect(screen.getByLabelText("主题")).toHaveValue("light");
  });

  it("describes encrypted exports and diagnostics privacy", async () => {
    const user = userEvent.setup();
    render(<SettingsModal />);
    await user.click(screen.getByRole("tab", { name: "连接与安全" }));
    await user.click(screen.getByRole("button", { name: /^连接库备份/ }));
    expect(await screen.findByText(/Argon2id/)).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "关于与支持" }));
    await user.click(screen.getByRole("button", { name: /^反馈与诊断/ }));
    expect(
      await screen.findByText(/不包含主机、用户名、路径或命令/),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "报告问题" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "基础设置" }));
    expect(
      screen.getByRole("button", { name: "清空全部命令历史" }),
    ).toBeInTheDocument();
  });

  it("saves font size and terminal color preferences", async () => {
    const user = userEvent.setup();
    const save = vi
      .spyOn(api, "saveSettings")
      .mockImplementation(async (settings) => settings);
    render(<SettingsModal />);
    fireEvent.change(screen.getByLabelText("字号"), {
      target: { value: "18" },
    });
    await user.click(screen.getByRole("radio", { name: "Solarized" }));
    await user.click(screen.getByRole("button", { name: "保存基础设置" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0][0].terminal).toMatchObject({
      fontSize: 18,
      colorScheme: "solarizedDark",
    });
  });

  it("previews and saves the global interface scale", async () => {
    const user = userEvent.setup();
    const save = vi
      .spyOn(api, "saveSettings")
      .mockImplementation(async (settings) => settings);
    render(<SettingsModal />);

    await user.selectOptions(
      screen.getByRole("combobox", { name: /界面缩放/ }),
      "125",
    );
    expect(document.documentElement).toHaveAttribute(
      "data-interface-scale",
      "125",
    );
    expect(
      document.documentElement.style.getPropertyValue("--interface-scale"),
    ).toBe("1.25");

    await user.click(screen.getByRole("button", { name: "保存基础设置" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0][0].interfaceScalePercent).toBe(125);
  });

  it("searches registered metadata and opens the matching module", async () => {
    const user = userEvent.setup();
    vi.spyOn(api, "protocolCapabilities").mockResolvedValue([]);
    render(<SettingsModal />);

    await user.type(
      screen.getByRole("searchbox", { name: "搜索设置" }),
      "Mosh",
    );
    const result = await screen.findByRole("button", {
      name: /高级协议与转发/,
    });
    expect(result).toHaveTextContent("连接与安全");
    await user.click(result);

    expect(screen.getByRole("tab", { name: "连接与安全" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.getByRole("button", { name: /^高级协议与转发/ }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      await screen.findByRole("combobox", { name: "按连接配置协议选项" }),
    ).toBeVisible();
  });

  it("restores only basic defaults and preserves connection overrides", async () => {
    const user = userEvent.setup();
    const override = { ...defaultSettings.terminal, fontSize: 18 };
    const customized = {
      ...defaultSettings,
      theme: "light" as const,
      monitorIntervalMs: 5000,
      showHiddenFiles: true,
      terminal: { ...defaultSettings.terminal, fontSize: 20 },
      terminalOverrides: { server: override },
    };
    useAppStore.setState({ settingsOpen: true, settings: customized });
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const save = vi
      .spyOn(api, "saveSettings")
      .mockImplementation(async (settings) => settings);
    render(<SettingsModal />);

    await user.click(screen.getByRole("button", { name: "恢复基础默认值" }));
    expect(screen.getByLabelText("主题")).toHaveValue(defaultSettings.theme);
    expect(screen.getByLabelText("字号")).toHaveValue(
      String(defaultSettings.terminal.fontSize),
    );
    await user.click(screen.getByRole("button", { name: "保存基础设置" }));
    await waitFor(() => expect(save).toHaveBeenCalled());
    expect(save.mock.calls[0][0]).toMatchObject({
      theme: defaultSettings.theme,
      monitorIntervalMs: defaultSettings.monitorIntervalMs,
      showHiddenFiles: defaultSettings.showHiddenFiles,
      terminalOverrides: { server: override },
    });
  });

  it("keeps a mounted module draft when it is collapsed and reopened", async () => {
    const user = userEvent.setup();
    vi.spyOn(api, "listProxies").mockResolvedValue([]);
    render(<SettingsModal />);
    await user.click(screen.getByRole("tab", { name: "连接与安全" }));
    const moduleToggle = screen.getByRole("button", { name: /^代理与跳板机/ });
    await user.click(moduleToggle);
    await user.click(await screen.findByRole("button", { name: "SOCKS5" }));
    await user.type(screen.getByRole("textbox", { name: "名称" }), "公司代理");
    await user.click(moduleToggle);
    expect(moduleToggle).toHaveAttribute("aria-expanded", "false");
    await user.click(moduleToggle);
    expect(screen.getByRole("textbox", { name: "名称" })).toHaveValue(
      "公司代理",
    );
  });

  it("protects an unsaved AI provider draft from both close actions", async () => {
    const user = userEvent.setup();
    vi.spyOn(api, "listAiProviders").mockResolvedValue([]);
    const confirmClose = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<SettingsModal />);

    await user.click(screen.getByRole("tab", { name: "自动化与集成" }));
    await user.click(screen.getByRole("button", { name: /^AI Provider/ }));
    await user.type(
      await screen.findByRole("textbox", { name: "Provider 名称" }),
      "本地模型",
    );

    expect(await screen.findByText("AI Provider有未保存的修改")).toBeVisible();
    expect(screen.getByText("未保存")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(confirmClose).toHaveBeenCalledWith(
      "设置中有尚未保存的修改，确定放弃并关闭吗？",
    );
    expect(screen.getByRole("dialog", { name: "设置" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "关闭" }));
    expect(confirmClose).toHaveBeenCalledTimes(2);
  });

  it("opens the independent automation center from settings", async () => {
    const user = userEvent.setup();
    const openAutomation = vi.fn();
    window.addEventListener("cnshell-open-automation-center", openAutomation);
    render(<SettingsModal />);

    await user.click(screen.getByRole("tab", { name: "自动化与集成" }));
    await user.click(screen.getByRole("button", { name: /^自动化与定时任务/ }));
    await user.click(screen.getByRole("button", { name: "打开自动化中心" }));

    expect(openAutomation).toHaveBeenCalledTimes(1);
    expect(useAppStore.getState().settingsOpen).toBe(false);
    window.removeEventListener(
      "cnshell-open-automation-center",
      openAutomation,
    );
  });

  it("supports category Home and End keys and exposes contextual help", async () => {
    const user = userEvent.setup();
    render(<SettingsModal />);
    const basic = screen.getByRole("tab", { name: "基础设置" });
    basic.focus();
    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "关于与支持" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(basic).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "查看外观帮助" }));
    expect(screen.getByText(/跟随系统会自动使用操作系统/)).toBeVisible();
  });

  it("keeps only the active tab panel exposed to WebKit accessibility", async () => {
    const user = userEvent.setup();
    render(<SettingsModal />);

    const activePanel = () => screen.getByRole("tabpanel");
    expect(activePanel()).toHaveAttribute("id", "settings-panel-basic");

    await user.click(screen.getByRole("tab", { name: "连接与安全" }));
    expect(activePanel()).toHaveAttribute("id", "settings-panel-connection");
    expect(activePanel()).not.toHaveAttribute("hidden");
    expect(document.getElementById("settings-panel-basic")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(document.getElementById("settings-panel-basic")).toHaveAttribute(
      "inert",
    );
    expect(
      screen.getByRole("button", { name: /^OpenSSH 配置与密钥/ }),
    ).toBeVisible();

    await user.click(screen.getByRole("tab", { name: "关于与支持" }));
    expect(activePanel()).toHaveAttribute("id", "settings-panel-support");
    expect(screen.getByRole("button", { name: /^软件更新/ })).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent("此页模块分别保存");
  });
});
