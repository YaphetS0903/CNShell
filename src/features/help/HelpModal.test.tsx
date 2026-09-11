import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { useAppStore } from "../../store/app-store";
import { defaultSettings } from "../../types";
import HelpModal from "./HelpModal";

describe("HelpModal", () => {
  beforeEach(() => {
    useAppStore.setState({
      helpOpen: true,
      connectionEditorOpen: false,
      editingConnection: null,
      settings: defaultSettings,
    });
  });

  it("searches shortcuts and opens the first connection workflow", async () => {
    const user = userEvent.setup();
    render(<HelpModal />);

    const search = screen.getByRole("searchbox", {
      name: "搜索帮助与快捷键",
    });
    expect(search).toHaveFocus();
    await user.type(search, "跨标签");
    expect(screen.getByRole("row", { name: /跨标签搜索/ })).toBeVisible();
    expect(
      screen.queryByRole("row", { name: /关闭当前会话/ }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "新建第一个连接" }));
    expect(useAppStore.getState()).toMatchObject({
      helpOpen: false,
      connectionEditorOpen: true,
      editingConnection: null,
    });
  });
});
