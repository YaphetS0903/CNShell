import { Profiler } from "react";
import { act, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { ConnectionSidebar } from "./ConnectionSidebar";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";

it("does not rerender the connection list for transfer progress or unrelated errors", async () => {
  vi.spyOn(api, "listFolders").mockResolvedValue([]);
  useAppStore.setState({ connections: [], transfers: [], error: null });
  const renderCount = vi.fn();
  await act(async () => {
    render(
      <Profiler id="connections" onRender={renderCount}>
        <ConnectionSidebar connect={vi.fn()} />
      </Profiler>,
    );
  });
  expect(screen.getByRole("textbox", { name: "搜索连接" })).toBeInTheDocument();
  renderCount.mockClear();
  for (let index = 0; index < 10; index += 1) {
    act(() =>
      useAppStore.setState({ transfers: [], error: `unrelated-${index}` }),
    );
  }
  expect(renderCount).not.toHaveBeenCalled();
  act(() => useAppStore.setState({ connections: [] }));
  expect(renderCount).toHaveBeenCalledOnce();
  vi.restoreAllMocks();
});
