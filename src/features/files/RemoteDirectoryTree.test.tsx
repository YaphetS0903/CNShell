import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RemoteDirectoryTree } from "./RemoteDirectoryTree";

describe("RemoteDirectoryTree", () => {
  it("loads nested folders when their disclosure button is expanded", async () => {
    const listDirectories = vi.fn(async (path: string) => path === "/"
      ? [{ name: "home", path: "/home" }]
      : path === "/home"
        ? [{ name: "ubuntu", path: "/home/ubuntu" }]
        : []);
    const onNavigate = vi.fn();
    render(<RemoteDirectoryTree activePath="/" listDirectories={listDirectories} onNavigate={onNavigate} onError={vi.fn()} />);

    await screen.findByText("home");
    await userEvent.click(screen.getByRole("button", { name: "home" }));
    expect(await screen.findByText("ubuntu")).toBeInTheDocument();
    expect(listDirectories).toHaveBeenCalledWith("/home");
    expect(onNavigate).toHaveBeenCalledWith("/home");

    await userEvent.click(screen.getByRole("button", { name: "ubuntu" }));
    expect(onNavigate).toHaveBeenCalledWith("/home/ubuntu");
    expect(screen.getByRole("button", { name: "ubuntu" })).toHaveAttribute("title", "/home/ubuntu");
    window.dispatchEvent(new Event("cnshell-refresh-directory-tree"));
    expect(await screen.findByText("ubuntu")).toBeVisible();
    expect(screen.getByRole("button", { name: "折叠 home" })).toBeVisible();
  });

  it("reveals the active directory hierarchy", async () => {
    const listDirectories = vi.fn(async (path: string) => path === "/"
      ? [{ name: "home", path: "/home" }]
      : path === "/home"
        ? [{ name: "ubuntu", path: "/home/ubuntu" }]
        : []);

    render(<RemoteDirectoryTree activePath="/home/ubuntu" listDirectories={listDirectories} onNavigate={vi.fn()} onError={vi.fn()} />);

    expect(await screen.findByText("ubuntu")).toBeVisible();
    expect(screen.getByRole("button", { name: "折叠 home" })).toBeVisible();
    expect(screen.getByTitle("/home/ubuntu").closest('[role="treeitem"]')).toHaveAttribute("aria-selected", "true");
  });
});
