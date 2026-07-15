import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { TerminalSession } from "../../types";
import { SerialTransferPanel } from "./SerialTransferPanel";

const session: TerminalSession = {
  id: "serial-session",
  connectionId: "serial-connection",
  sessionType: "serial",
  title: "Console",
  status: "online",
  startedAt: "now",
  lastError: null,
};

describe("SerialTransferPanel", () => {
  afterEach(() => vi.restoreAllMocks());

  it("offers bounded Xmodem and Ymodem modes and renders progress events", async () => {
    const user = userEvent.setup();
    let listener: ((event: import("../../types").SerialTransferEvent) => void) | undefined;
    vi.spyOn(api, "onSerialTransfer").mockImplementation(async (handler) => {
      listener = handler;
      return () => undefined;
    });
    render(<SerialTransferPanel session={session} onError={vi.fn()} />);
    const mode = screen.getByRole("combobox", { name: "协议模式" });
    expect(mode).toHaveValue("xmodem1k");
    await user.selectOptions(mode, "ymodem");
    expect(screen.getByText("批量与文件名")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Kermit Batch" })).toBeInTheDocument();
    listener?.({
      id: "transfer",
      sessionId: session.id,
      protocol: "ymodem",
      direction: "upload",
      status: "running",
      fileName: "firmware.bin",
      totalBytes: 1024,
      transferredBytes: 512,
      error: null,
    });
    expect(await screen.findByText("firmware.bin")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "取消" })).toBeInTheDocument();
  });
});
