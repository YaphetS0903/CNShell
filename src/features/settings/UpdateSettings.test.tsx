import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { UpdateSettings } from "./UpdateSettings";

const updater = vi.hoisted(() => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: updater.check }));

describe("UpdateSettings", () => {
  beforeEach(() => updater.check.mockReset());

  it("does not contact an updater endpoint in browser preview", async () => {
    const user = userEvent.setup();
    render(<UpdateSettings onError={vi.fn()}/>);
    await user.click(screen.getByRole("button", { name: "检查更新" }));
    expect(await screen.findByRole("status")).toHaveTextContent("仅在 CNshell 桌面版");
    expect(updater.check).not.toHaveBeenCalled();
  });

  it("explains that candidate builds intentionally have no release endpoint", () => {
    render(<UpdateSettings onError={vi.fn()}/>);
    expect(screen.getByRole("status")).toHaveTextContent("候选版未配置正式更新通道");
  });
});
