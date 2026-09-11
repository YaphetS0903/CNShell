import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import AutomationCenter from "./AutomationCenter";

describe("AutomationCenter", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "listAutomationSchedules").mockResolvedValue([]);
    vi.spyOn(api, "listAutomationRuns").mockResolvedValue([]);
  });

  it("focuses task editing and protects an unsaved plan", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    const confirmClose = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(
      <AutomationCenter
        open
        connections={[]}
        onClose={onClose}
        onError={vi.fn()}
      />,
    );

    const taskTab = screen.getByRole("tab", { name: "任务编排" });
    await waitFor(() => expect(taskTab).toHaveFocus());
    await user.type(screen.getByLabelText("计划名称"), "发布前检查");
    await user.click(screen.getByRole("button", { name: "关闭" }));

    expect(confirmClose).toHaveBeenCalledWith(
      "自动化计划有尚未保存的修改，确定放弃并关闭吗？",
    );
    expect(onClose).not.toHaveBeenCalled();

    confirmClose.mockReturnValue(true);
    await user.click(screen.getByRole("button", { name: "关闭" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("supports keyboard navigation across automation tools", async () => {
    const user = userEvent.setup();
    render(
      <AutomationCenter
        open
        connections={[]}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );

    const taskTab = screen.getByRole("tab", { name: "任务编排" });
    await waitFor(() => expect(taskTab).toHaveFocus());
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "运行记录" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });
});
