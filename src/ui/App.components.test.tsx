import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BulkCommandConfirmation, CommandPalette, LogViewerPanel, QuickCommandPanel } from "./App";
import { quickCommands } from "../domain/seed";

vi.mock("@xterm/addon-fit", () => ({ FitAddon: class FitAddon {} }));
vi.mock("@xterm/addon-search", () => ({ SearchAddon: class SearchAddon {} }));
vi.mock("@xterm/xterm", () => ({ Terminal: class Terminal {} }));

afterEach(() => {
  cleanup();
});

describe("renderer workflow components", () => {
  it("executes a quick command from the operations panel", () => {
    const onExecute = vi.fn();
    render(<QuickCommandPanel quickCommands={quickCommands.slice(0, 2)} onExecute={onExecute} />);

    fireEvent.click(screen.getByRole("button", { name: /Restart service/i }));

    expect(onExecute).toHaveBeenCalledWith("sudo systemctl restart ${service}");
  });

  it("filters and executes commands from the command palette", () => {
    const onExecute = vi.fn();
    const onClose = vi.fn();
    render(
      <CommandPalette
        commands={quickCommands}
        query="disk"
        onQueryChange={vi.fn()}
        onExecute={onExecute}
        onClose={onClose}
      />
    );

    expect(screen.queryByText("Restart service")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Disk usage/i }));

    expect(onExecute).toHaveBeenCalledWith("df -h");
  });

  it("confirms or cancels a bulk command before dispatch", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <BulkCommandConfirmation
        command="systemctl restart api"
        targets={[
          { id: "tab-1", title: "prod-web-01", status: "connected" },
          { id: "tab-2", title: "stage-db-01", status: "disconnected" }
        ]}
        onConfirm={onConfirm}
        onCancel={onCancel}
      />
    );

    expect(screen.getByText("2 sessions")).toBeInTheDocument();
    expect(screen.getByText("systemctl restart api")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Send to all" }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("shows log empty and populated states", () => {
    const onRefresh = vi.fn();
    const { rerender } = render(
      <LogViewerPanel
        title="Audit"
        refreshLabel="Refresh audit log"
        emptyText="No audit entries"
        query=""
        lines={[]}
        status="idle"
        onQueryChange={vi.fn()}
        onRefresh={onRefresh}
      />
    );

    expect(screen.getByText("No audit entries")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Refresh audit log" }));
    expect(onRefresh).toHaveBeenCalledOnce();

    rerender(
      <LogViewerPanel
        title="Audit"
        refreshLabel="Refresh audit log"
        emptyText="No audit entries"
        query=""
        lines={['{"action":"terminal.start","status":"ok"}']}
        status="idle"
        onQueryChange={vi.fn()}
        onRefresh={onRefresh}
      />
    );

    expect(screen.getByText(/terminal.start/)).toBeInTheDocument();
  });
});
