import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { defaultSettings } from "../../types";
import { useAppStore } from "../../store/app-store";
import SettingsModal from "./SettingsModal";

describe("SettingsModal", () => {
  beforeEach(() => useAppStore.setState({ settingsOpen: true, settings: defaultSettings, connections: [] }));
  it("describes encrypted exports and diagnostics privacy", () => {
    render(<SettingsModal/>);
    expect(screen.getByText(/Argon2id/)).toBeInTheDocument();
    expect(screen.getByText(/不包含主机、用户名、路径或命令/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "清空全部命令历史" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "检查更新" })).toBeInTheDocument();
  });
});
