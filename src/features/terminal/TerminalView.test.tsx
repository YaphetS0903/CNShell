import { act, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { TerminalSession } from "../../types";
import { TerminalView } from "./TerminalView";

const terminalMock = vi.hoisted(() => {
  const dispose = () => ({ dispose: vi.fn() });
  const instance = {
    cols: 80,
    rows: 24,
    options: {} as Record<string, unknown>,
    buffer: {
      active: { baseY: 0, cursorY: 0, viewportY: 0, length: 1, getLine: () => undefined },
      normal: { length: 1, getLine: () => undefined },
    },
    parser: { registerOscHandler: vi.fn(dispose) },
    loadAddon: vi.fn(),
    open: vi.fn(),
    focus: vi.fn(),
    clear: vi.fn(),
    paste: vi.fn(),
    write: vi.fn(),
    writeln: vi.fn(),
    dispose: vi.fn(),
    onData: vi.fn(dispose),
    onBell: vi.fn(dispose),
    onSelectionChange: vi.fn(dispose),
    onScroll: vi.fn(dispose),
    attachCustomKeyEventHandler: vi.fn(),
    hasSelection: vi.fn(() => false),
    getSelection: vi.fn(() => ""),
    clearSelection: vi.fn(),
    selectLines: vi.fn(),
    scrollToLine: vi.fn(),
    registerMarker: vi.fn(() => ({ dispose: vi.fn() })),
    registerDecoration: vi.fn(() => undefined),
  };
  const fit = vi.fn();
  return {
    instance,
    fit,
    Terminal: vi.fn(function (options: Record<string, unknown>) {
      instance.options = { ...options };
      return instance;
    }),
    FitAddon: vi.fn(function () {
      return { fit };
    }),
    SearchAddon: vi.fn(function () {
      return { findNext: vi.fn(() => false) };
    }),
    WebLinksAddon: vi.fn(function () {
      return {};
    }),
  };
});

const resizeMock = vi.hoisted(() => ({
  callback: null as ResizeObserverCallback | null,
  observe: vi.fn(),
  disconnect: vi.fn(),
}));

vi.mock("@xterm/xterm", () => ({ Terminal: terminalMock.Terminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: terminalMock.FitAddon }));
vi.mock("@xterm/addon-search", () => ({ SearchAddon: terminalMock.SearchAddon }));
vi.mock("@xterm/addon-web-links", () => ({ WebLinksAddon: terminalMock.WebLinksAddon }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));

const moshSession: TerminalSession = {
  id: "11111111-1111-4111-8111-111111111111",
  connectionId: "22222222-2222-4222-8222-222222222222",
  sessionType: "mosh",
  title: "Roaming server · Mosh",
  status: "online",
  startedAt: "2026-07-16T00:00:00Z",
  lastError: null,
};

describe("TerminalView resize", () => {
  beforeEach(() => {
    terminalMock.instance.cols = 80;
    terminalMock.instance.rows = 24;
    terminalMock.fit.mockClear();
    resizeMock.callback = null;
    resizeMock.observe.mockClear();
    resizeMock.disconnect.mockClear();
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: ResizeObserverCallback) {
          resizeMock.callback = callback;
        }
        observe = resizeMock.observe;
        disconnect = resizeMock.disconnect;
      },
    );
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
    vi.spyOn(api, "terminalResize").mockResolvedValue();
    vi.spyOn(api, "onTerminalOutput").mockResolvedValue(() => undefined);
    vi.spyOn(api, "onZmodemEvent").mockResolvedValue(() => undefined);
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("forwards fitted Mosh dimensions and disconnects the observer on unmount", async () => {
    const view = render(
      <TerminalView session={moshSession} visible focused />,
    );

    await waitFor(() => expect(resizeMock.observe).toHaveBeenCalledTimes(1));
    vi.mocked(api.terminalResize).mockClear();
    terminalMock.instance.cols = 132;
    terminalMock.instance.rows = 42;

    await act(async () => {
      resizeMock.callback?.([], {} as ResizeObserver);
    });

    expect(terminalMock.fit).toHaveBeenCalled();
    expect(api.terminalResize).toHaveBeenCalledWith(moshSession.id, 132, 42);

    view.unmount();
    expect(resizeMock.disconnect).toHaveBeenCalledTimes(1);
  });
});
