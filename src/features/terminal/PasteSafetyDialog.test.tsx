import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { PasteHistoryDialog } from "./PasteSafetyDialog";

describe("PasteHistoryDialog", () => {
  it("explains how temporary paste history is created", () => {
    render(
      <PasteHistoryDialog
        items={[]}
        onSelect={() => undefined}
        onClear={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(screen.getByRole("status")).toHaveTextContent(
      "本次运行还没有粘贴内容",
    );
    expect(screen.getByText(/任意终端使用粘贴后/)).toBeVisible();
    expect(screen.getByText(/关闭应用后自动清除/)).toBeVisible();
  });

  it("returns a selected history item", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(
      <PasteHistoryDialog
        items={["echo first\necho second"]}
        onSelect={onSelect}
        onClear={() => undefined}
        onClose={() => undefined}
      />,
    );

    await user.click(screen.getByRole("button", { name: /echo first/ }));
    expect(onSelect).toHaveBeenCalledWith("echo first\necho second");
  });
});
