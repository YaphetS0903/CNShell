import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  BulkCommandConfirmation,
  CommandPalette,
  ConnectionEditorDialog,
  FilePanel,
  LogViewerPanel,
  QuickCommandManagerDialog,
  QuickCommandPanel,
  RemoteOperationDialog,
  ServerStatusRail,
  SettingsDialog,
  SystemInfoWorkspace,
  TabStrip
} from "./App";
import { connectionProfiles, quickCommands, serverMetrics, systemInfo } from "../domain/seed";

vi.mock("@xterm/addon-fit", () => ({ FitAddon: class FitAddon {} }));
vi.mock("@xterm/addon-search", () => ({ SearchAddon: class SearchAddon {} }));
vi.mock("@xterm/xterm", () => ({ Terminal: class Terminal {} }));

afterEach(() => {
  cleanup();
});

describe("renderer workflow components", () => {
  it("executes a quick command from the operations panel", () => {
    const onExecute = vi.fn();
    render(<QuickCommandPanel quickCommands={quickCommands.slice(0, 2)} onExecute={onExecute} onManage={vi.fn()} />);

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

  it("edits and validates a connection profile dialog", () => {
    const onChange = vi.fn();
    const onSave = vi.fn();
    const onDelete = vi.fn();
    render(
      <ConnectionEditorDialog
        draft={{
          id: "prod-web-01",
          name: "prod-web-01",
          group: "Production",
          protocol: "ssh",
          host: "10.24.18.11",
          port: "22",
          username: "deploy",
          authMethod: "privateKey",
          password: "",
          privateKey: "",
          passphrase: "",
          saveCredential: false,
          color: "#2f9e44",
          tags: "nginx, api"
        }}
        error="Enter a host."
        canDelete
        onChange={onChange}
        onSave={onSave}
        onDelete={onDelete}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByRole("alert")).toHaveTextContent("Enter a host.");
    fireEvent.change(screen.getByDisplayValue("prod-web-01"), { target: { value: "prod-web-02" } });
    fireEvent.click(screen.getByRole("button", { name: /Save connection/i }));
    fireEvent.click(screen.getByRole("button", { name: /Delete connection/i }));

    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ name: "prod-web-02" }));
    expect(onSave).toHaveBeenCalledOnce();
    expect(onDelete).toHaveBeenCalledOnce();
  });

  it("accepts SSH password input in the connection profile dialog", () => {
    const onChange = vi.fn();
    render(
      <ConnectionEditorDialog
        draft={{
          name: "new-host",
          group: "Staging",
          protocol: "ssh",
          host: "124.223.19.107",
          port: "22",
          username: "ubuntu",
          authMethod: "password",
          password: "",
          privateKey: "",
          passphrase: "",
          saveCredential: false,
          color: "#2f9e44",
          tags: ""
        }}
        error=""
        canDelete={false}
        onChange={onChange}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onClose={vi.fn()}
      />
    );

    fireEvent.change(screen.getByLabelText("Password"), { target: { value: "secret-pass" } });

    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ password: "secret-pass" }));
  });

  it("splits host and port input in the connection profile dialog", () => {
    const onChange = vi.fn();
    render(
      <ConnectionEditorDialog
        draft={{
          name: "new-host",
          group: "Staging",
          protocol: "ssh",
          host: "124.223.19.107:2222",
          port: "22",
          username: "ubuntu",
          authMethod: "password",
          password: "",
          privateKey: "",
          passphrase: "",
          saveCredential: false,
          color: "#2f9e44",
          tags: ""
        }}
        error=""
        canDelete={false}
        onChange={onChange}
        onSave={vi.fn()}
        onDelete={vi.fn()}
        onClose={vi.fn()}
      />
    );

    fireEvent.blur(screen.getByLabelText("Host"));

    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({ host: "124.223.19.107", port: "2222" }));
  });

  it("changes theme preferences from the settings dialog", () => {
    const onThemeModeChange = vi.fn();
    const onThemeAccentChange = vi.fn();
    render(
      <SettingsDialog
        language="zh-CN"
        themeMode="light"
        themeAccent="green"
        onLanguageChange={vi.fn()}
        onThemeModeChange={onThemeModeChange}
        onThemeAccentChange={onThemeAccentChange}
        onClose={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: /Dark/i }));
    fireEvent.click(screen.getByRole("button", { name: /Blue/i }));

    expect(onThemeModeChange).toHaveBeenCalledWith("dark");
    expect(onThemeAccentChange).toHaveBeenCalledWith("blue");
  });

  it("manages quick command drafts", () => {
    const onDraftChange = vi.fn();
    const onEdit = vi.fn();
    const onSave = vi.fn();
    const onDelete = vi.fn();
    render(
      <QuickCommandManagerDialog
        commands={quickCommands}
        draft={{ id: "qc-disk", title: "Disk usage", command: "df -h", scope: "global" }}
        error=""
        onDraftChange={onDraftChange}
        onNew={vi.fn()}
        onEdit={onEdit}
        onSave={onSave}
        onDelete={onDelete}
        onClose={vi.fn()}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: /Nginx errors/i }));
    fireEvent.change(screen.getByDisplayValue("Disk usage"), { target: { value: "Disk free" } });
    fireEvent.click(screen.getByRole("button", { name: /Save command/i }));
    fireEvent.click(screen.getByRole("button", { name: /Delete command/i }));

    expect(onEdit).toHaveBeenCalledWith(expect.objectContaining({ id: "qc-nginx" }));
    expect(onDraftChange).toHaveBeenCalledWith(expect.objectContaining({ title: "Disk free" }));
    expect(onSave).toHaveBeenCalledOnce();
    expect(onDelete).toHaveBeenCalledWith("qc-disk");
  });

  it("closes session tabs from the tab strip", () => {
    const onClose = vi.fn();
    render(
      <TabStrip
        tabs={[
          {
            id: "tab-1",
            connectionId: "prod-web-01",
            title: "prod-web-01",
            cwd: "/",
            status: "connected",
            startedAt: new Date().toISOString()
          }
        ]}
        activeTabId="tab-1"
        onSelect={vi.fn()}
        onCreate={vi.fn()}
        onClose={onClose}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "Close session tab" }));
    expect(onClose).toHaveBeenCalledWith("tab-1");
  });

  it("opens the FinalShell-style system information entry from the server rail", () => {
    const onOpenSystemInfo = vi.fn();
    render(
      <ServerStatusRail
        connection={connectionProfiles[0]}
        metrics={serverMetrics}
        systemInfo={systemInfo}
        processes={[{ pid: 101, ppid: 1, cpu: 20.3, memory: 5.1, command: "nginx", args: "worker" }]}
        status="idle"
        onOpenSystemInfo={onOpenSystemInfo}
      />
    );

    expect(screen.getByText(connectionProfiles[0].host)).toBeInTheDocument();
    expect(screen.getByText("enp0s6")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "System information" }));

    expect(onOpenSystemInfo).toHaveBeenCalledOnce();
  });

  it("hides seeded system metrics while the ssh session is disconnected", () => {
    render(
      <ServerStatusRail
        connection={connectionProfiles[0]}
        metrics={serverMetrics}
        systemInfo={systemInfo}
        processes={[{ pid: 101, ppid: 1, cpu: 20.3, memory: 5.1, command: "nginx", args: "worker" }]}
        status="idle"
        isConnected={false}
        hasMetrics={false}
        onOpenSystemInfo={vi.fn()}
      />
    );

    expect(screen.getAllByText("No system information yet. Connect and refresh metrics to collect it.").length).toBeGreaterThan(0);
    expect(screen.queryByText("enp0s6")).not.toBeInTheDocument();
    expect(screen.queryByText("nginx")).not.toBeInTheDocument();
  });

  it("renders a detailed system information workspace", () => {
    const onRefresh = vi.fn();
    render(
      <SystemInfoWorkspace
        connection={connectionProfiles[0]}
        metrics={serverMetrics}
        systemInfo={systemInfo}
        processes={[{ pid: 101, ppid: 1, cpu: 20.3, memory: 5.1, command: "nginx", args: "worker" }]}
        status="idle"
        error=""
        onRefresh={onRefresh}
      />
    );

    expect(screen.getByText("Ubuntu 22.04 LTS")).toBeInTheDocument();
    expect(screen.getByText("5.15.0")).toBeInTheDocument();
    expect(screen.getByText("nginx")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /Refresh metrics/i }));

    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("does not show system information details before the ssh session connects", () => {
    const onRefresh = vi.fn();
    render(
      <SystemInfoWorkspace
        connection={connectionProfiles[0]}
        metrics={serverMetrics}
        systemInfo={systemInfo}
        processes={[{ pid: 101, ppid: 1, cpu: 20.3, memory: 5.1, command: "nginx", args: "worker" }]}
        status="idle"
        error=""
        isConnected={false}
        hasMetrics={false}
        onRefresh={onRefresh}
      />
    );

    expect(screen.getAllByText("No system information yet. Connect and refresh metrics to collect it.").length).toBeGreaterThan(0);
    expect(screen.queryByText("Ubuntu 22.04 LTS")).not.toBeInTheDocument();
    expect(screen.queryByText("5.15.0")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Refresh metrics/i })).toBeDisabled();
  });

  it("exposes SFTP file management actions", () => {
    const onCreateDirectory = vi.fn();
    const onRenamePath = vi.fn();
    const onDeletePath = vi.fn();
    const onOpenFile = vi.fn();
    const onNavigatePath = vi.fn();
    render(
      <FilePanel
        remoteFiles={[
          {
            id: "/var/www/logs",
            name: "logs",
            path: "/var/www/logs",
            type: "directory",
            size: 0,
            modifiedAt: "2026-06-18T00:00:00.000Z",
            mode: "drwxr-xr-x"
          },
          {
            id: "/var/www/app.log",
            name: "app.log",
            path: "/var/www/app.log",
            type: "file",
            size: 1024,
            modifiedAt: "2026-06-18T00:00:00.000Z",
            mode: "-rw-r--r--"
          }
        ]}
        path="/var/www"
        status="idle"
        error=""
        localPath=""
        transferRemotePath=""
        transferJobs={[]}
        onPathChange={vi.fn()}
        onNavigatePath={onNavigatePath}
        onLocalPathChange={vi.fn()}
        onTransferRemotePathChange={vi.fn()}
        onRefresh={vi.fn()}
        onTransfer={vi.fn()}
        onOpenFile={onOpenFile}
        onCreateDirectory={onCreateDirectory}
        onRenamePath={onRenamePath}
        onDeletePath={onDeletePath}
      />
    );

    fireEvent.click(screen.getByRole("button", { name: "New directory" }));
    fireEvent.click(screen.getAllByRole("button", { name: "logs" })[0]);
    fireEvent.click(screen.getAllByRole("button", { name: /Open editor/i }).at(-1)!);
    fireEvent.click(screen.getAllByRole("button", { name: "Rename" }).at(-1)!);
    fireEvent.click(screen.getAllByRole("button", { name: "Delete" }).at(-1)!);

    expect(screen.getByRole("columnheader", { name: "File name" })).toBeInTheDocument();
    expect(screen.getByText("Folder")).toBeInTheDocument();
    expect(onCreateDirectory).toHaveBeenCalledOnce();
    expect(onNavigatePath).toHaveBeenCalledWith("/var/www/logs");
    expect(onOpenFile).toHaveBeenCalledWith("/var/www/app.log");
    expect(onRenamePath).toHaveBeenCalledWith("/var/www/app.log");
    expect(onDeletePath).toHaveBeenCalledWith("/var/www/app.log");
  });

  it("confirms remote delete operations", () => {
    const onConfirm = vi.fn();
    render(
      <RemoteOperationDialog
        draft={{ type: "delete", targetPath: "/var/www/app.log", value: "" }}
        error=""
        onChange={vi.fn()}
        onConfirm={onConfirm}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByText("Delete /var/www/app.log?")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });
});
