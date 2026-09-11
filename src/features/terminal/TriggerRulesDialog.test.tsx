import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { TerminalSession } from "../../types";
import { TriggerRulesDialog } from "./TriggerRulesDialog";

const session: TerminalSession = {
  id: "trigger-session",
  connectionId: "connection-1",
  sessionType: "terminal",
  title: "测试终端",
  status: "online",
  startedAt: "",
  lastError: null,
};

describe("TriggerRulesDialog", () => {
  beforeEach(() => {
    localStorage.removeItem("cnshell-terminal-triggers-v1");
  });

  it("separates display, permission and notification trigger settings", () => {
    render(
      <TriggerRulesDialog
        session={session}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );

    expect(
      screen.getByRole("heading", { name: "文字高亮" }),
    ).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "通知权限" }),
    ).toBeVisible();
    expect(
      screen.getByRole("heading", { name: "触发条件" }),
    ).toBeVisible();
    expect(screen.getByRole("checkbox", { name: "终端 Bell" })).toBeDisabled();
    expect(screen.getByRole("spinbutton", { name: /阈值/ })).toBeDisabled();
  });

  it("previews regular-expression matches while editing a rule", async () => {
    const user = userEvent.setup();
    render(
      <TriggerRulesDialog
        session={session}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: "新建规则" }));
    await user.type(screen.getByRole("textbox", { name: "正则表达式" }), "failed");

    expect(screen.getByText("已匹配 1 处")).toBeVisible();
    expect(screen.getByText("failed", { selector: "mark" })).toBeVisible();
    await user.clear(screen.getByRole("textbox", { name: "规则样例文本" }));
    await user.type(
      screen.getByRole("textbox", { name: "规则样例文本" }),
      "service is healthy",
    );
    expect(screen.getByText("样例中没有匹配")).toBeVisible();
  });
});
