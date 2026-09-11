import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import { useAppStore } from "../../store/app-store";
import type { TerminalSession } from "../../types";
import { AiAssistantDialog } from "./AiAssistantDialog";

const session: TerminalSession = {
  id: "ai-session",
  connectionId: "connection-1",
  sessionType: "terminal",
  title: "测试终端",
  status: "online",
  startedAt: "",
  lastError: null,
};

describe("AiAssistantDialog", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    workspaceRuntime.terminalSelectionBySession.clear();
    useAppStore.getState().setSettingsOpen(false);
    vi.spyOn(api, "listAiProviders").mockResolvedValue([
      {
        id: "local",
        name: "本地",
        endpoint: "http://127.0.0.1:11434/v1",
        model: "model",
        hasApiKey: false,
      },
    ]);
    vi.spyOn(api, "previewAi").mockResolvedValue({
      requestId: "request",
      providerName: "本地",
      endpoint: "http://127.0.0.1:11434/v1",
      model: "model",
      kind: "explain",
      redactedContent: "failed at [HOST]",
      redactions: ["hostname"],
      expiresAt: "later",
    });
    vi.spyOn(api, "executeAi").mockResolvedValue({
      id: "task",
      kind: "ai-assistant",
      status: "queued",
      result: null,
      error: null,
      createdAt: "now",
    });
    vi.spyOn(window, "confirm").mockReturnValue(true);
  });

  it("prefills the terminal selection and previews redaction before sending", async () => {
    workspaceRuntime.terminalSelectionBySession.set(
      session.id,
      "failed at api.example.test",
    );
    const user = userEvent.setup();
    render(
      <AiAssistantDialog
        session={session}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );

    const input = await screen.findByRole("textbox", { name: "AI 输入" });
    expect(input).toHaveValue("failed at api.example.test");
    expect(screen.getByRole("combobox", { name: "请求类型" })).toHaveValue(
      "explain",
    );
    await user.click(screen.getByRole("button", { name: "生成脱敏预览" }));
    expect(await screen.findByText(/将发送的脱敏文本/)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "确认发送" }));
    await waitFor(() => expect(api.executeAi).toHaveBeenCalledWith("request"));
  });

  it("routes an unconfigured user to the provider settings module", async () => {
    vi.mocked(api.listAiProviders).mockResolvedValue([]);
    const close = vi.fn();
    const user = userEvent.setup();
    render(
      <AiAssistantDialog session={session} onClose={close} onError={vi.fn()} />,
    );

    await user.click(
      await screen.findByRole("button", { name: "前往 AI 设置" }),
    );
    expect(close).toHaveBeenCalled();
    expect(useAppStore.getState()).toMatchObject({
      settingsOpen: true,
      settingsTarget: { category: "automation", moduleId: "ai" },
    });
  });
});
