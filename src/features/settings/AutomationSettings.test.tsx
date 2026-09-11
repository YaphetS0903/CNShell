import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type {
  AutomationRunRecord,
  AutomationSchedule,
  ConnectionProfile,
} from "../../types";
import { formatAutomationSchedule } from "./automation-format";
import { AutomationSettings } from "./AutomationSettings";

const connection: ConnectionProfile = {
  id: "server",
  folderId: null,
  protocol: "ssh",
  name: "服务器",
  host: "example",
  port: 22,
  username: "root",
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

const savedSchedule: AutomationSchedule = {
  id: "daily-check",
  plan: {
    id: "plan",
    name: "每日检查",
    connectionId: "server",
    steps: [
      {
        id: "step",
        kind: "command",
        command: "uptime",
        pattern: null,
        timeoutSeconds: 30,
        action: null,
        direction: null,
        localPath: null,
        remotePath: null,
      },
    ],
  },
  scheduleType: "daily",
  expression: "09:15",
  enabled: true,
  misfirePolicy: "skip",
  timeZone: "Asia/Shanghai",
  nextRunAt: "2026-07-17T01:15:00+00:00",
  lastRunAt: null,
  lastOccurrenceKey: null,
};

const savedRun: AutomationRunRecord = {
  id: "run-1",
  planId: "plan",
  planName: "每日检查",
  connectionId: "server",
  source: "scheduled",
  scheduleId: "daily-check",
  startedAt: "2026-07-17T01:15:00+00:00",
  finishedAt: "2026-07-17T01:15:01+00:00",
  status: "completed",
  results: [
    {
      stepId: "step",
      kind: "command",
      status: "completed",
      startedAt: "2026-07-17T01:15:00+00:00",
      durationMs: 900,
      output: "up 10 days",
      error: null,
    },
  ],
  error: null,
};

describe("AutomationSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "listAutomationSchedules").mockResolvedValue([]);
    vi.spyOn(api, "listAutomationRuns").mockResolvedValue([]);
    vi.spyOn(api, "validateAutomation").mockImplementation(
      async (plan) => plan,
    );
    vi.spyOn(api, "startAutomation").mockResolvedValue({
      id: "task",
      kind: "automation",
      status: "queued",
      result: null,
      error: null,
      createdAt: "now",
    });
    vi.spyOn(window, "confirm").mockReturnValue(true);
  });

  it("previews the final workflow before starting", async () => {
    const user = userEvent.setup();
    render(
      <AutomationSettings
        connections={[connection]}
        onError={() => undefined}
      />,
    );
    await user.type(screen.getByRole("textbox", { name: "计划名称" }), "检查");
    await user.selectOptions(
      screen.getByRole("combobox", { name: "目标连接" }),
      "server",
    );
    await user.type(screen.getByRole("textbox", { name: "命令" }), "uname -a");
    expect(screen.getByLabelText("自动化预览")).toHaveTextContent(
      "执行 uname -a",
    );
    await user.click(screen.getByRole("button", { name: "预览并运行" }));
    expect(window.confirm).toHaveBeenCalledWith(
      expect.stringContaining("uname -a"),
    );
    expect(api.startAutomation).toHaveBeenCalled();
  });

  it("shows an actionable empty state for saved tasks", async () => {
    render(
      <AutomationSettings
        connections={[connection]}
        onError={() => undefined}
      />,
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "还没有定时任务",
    );
  });

  it("saves an explicit daily schedule and IANA time zone", async () => {
    const user = userEvent.setup();
    const save = vi
      .spyOn(api, "saveAutomationSchedule")
      .mockImplementation(async (schedule) => ({
        ...schedule,
        nextRunAt: "2026-07-17T01:00:00+00:00",
      }));
    render(
      <AutomationSettings
        connections={[connection]}
        onError={() => undefined}
      />,
    );
    await user.type(
      screen.getByRole("textbox", { name: "计划名称" }),
      "每日检查",
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "目标连接" }),
      "server",
    );
    await user.type(screen.getByRole("textbox", { name: "命令" }), "uptime");
    await user.selectOptions(
      screen.getByRole("combobox", { name: "类型" }),
      "daily",
    );
    const dailyTime = screen.getByRole("textbox", { name: "每日时间" });
    await user.clear(dailyTime);
    await user.type(dailyTime, "09:15");
    const timeZone = screen.getByRole("textbox", { name: "IANA 时区" });
    await user.clear(timeZone);
    await user.type(timeZone, "Asia/Shanghai");
    await user.click(screen.getByRole("button", { name: "保存定时任务" }));

    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({
        scheduleType: "daily",
        expression: "09:15",
        timeZone: "Asia/Shanghai",
        nextRunAt: null,
        lastRunAt: null,
        lastOccurrenceKey: null,
      }),
    );
    expect(await screen.findByText(/下次/)).toBeInTheDocument();
    expect(screen.getByText("每天 09:15")).toBeVisible();
  });

  it("tracks a manually started saved task", async () => {
    const user = userEvent.setup();
    vi.mocked(api.listAutomationSchedules).mockResolvedValue([savedSchedule]);
    vi.spyOn(api, "runAutomationScheduleNow").mockResolvedValue({
      id: "scheduled-run",
      kind: "automation-scheduled",
      status: "queued",
      result: null,
      error: null,
      createdAt: "now",
    });
    render(
      <AutomationSettings
        connections={[connection]}
        onError={() => undefined}
      />,
    );

    expect(await screen.findByText("每天 09:15")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "立即运行" }));
    expect(await screen.findByText(/任务状态：queued/)).toBeVisible();
  });

  it("separates workflow, Python and recording into focused tabs", async () => {
    const user = userEvent.setup();
    render(
      <AutomationSettings
        connections={[connection]}
        onError={() => undefined}
      />,
    );

    expect(screen.getByRole("tab", { name: "任务编排" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("textbox", { name: "计划名称" })).toBeVisible();
    await user.click(screen.getByRole("tab", { name: "Python" }));
    expect(screen.getByRole("tab", { name: "Python" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.queryByRole("textbox", { name: "计划名称" }),
    ).not.toBeInTheDocument();
  });

  it("shows persistent run history with expandable step results", async () => {
    const user = userEvent.setup();
    vi.mocked(api.listAutomationRuns).mockResolvedValue([savedRun]);
    render(
      <AutomationSettings
        connections={[connection]}
        onError={() => undefined}
      />,
    );

    await user.click(screen.getByRole("tab", { name: "运行记录" }));
    expect(await screen.findByText("每日检查")).toBeVisible();
    expect(screen.getByText(/服务器 · 定时触发 · 1.0 秒/)).toBeVisible();
    await user.click(screen.getByText("查看 1 个步骤"));
    expect(screen.getByText("up 10 days")).toBeVisible();
  });
});

describe("formatAutomationSchedule", () => {
  it("turns interval and weekly expressions into readable labels", () => {
    expect(
      formatAutomationSchedule({
        ...savedSchedule,
        scheduleType: "interval",
        expression: "3600",
      }),
    ).toBe("每 1 小时");
    expect(
      formatAutomationSchedule({
        ...savedSchedule,
        scheduleType: "weekly",
        expression: "fri@18:30",
      }),
    ).toBe("每周五 18:30");
  });
});
