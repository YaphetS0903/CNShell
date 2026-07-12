import { act, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ErrorToast } from "./ErrorToast";

describe("ErrorToast", () => {
  it("closes automatically after five seconds", () => {
    vi.useFakeTimers();
    const onClose = vi.fn();
    render(<ErrorToast message="测试错误" onClose={onClose}/>);
    expect(screen.getByRole("alert")).toHaveTextContent("测试错误");
    act(() => vi.advanceTimersByTime(4_999));
    expect(onClose).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(1));
    expect(onClose).toHaveBeenCalledOnce();
    vi.useRealTimers();
  });
});
