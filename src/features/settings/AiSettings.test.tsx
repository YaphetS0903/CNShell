import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { AiSettings } from "./AiSettings";

describe("AiSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "listAiProviders").mockResolvedValue([{ id: "local", name: "本地", endpoint: "http://127.0.0.1:11434/v1", model: "model", hasApiKey: false }]);
    vi.spyOn(api, "previewAi").mockResolvedValue({ requestId: "request", providerName: "本地", endpoint: "http://127.0.0.1:11434/v1", model: "model", kind: "command", redactedContent: "list [HOST]", redactions: ["hostname"], expiresAt: "later" });
    vi.spyOn(api, "executeAi").mockResolvedValue({ id: "task", kind: "ai-assistant", status: "queued", result: null, error: null, createdAt: "now" });
    vi.spyOn(window, "confirm").mockReturnValue(true);
  });

  it("previews redacted text before execution", async () => {
    const user = userEvent.setup();
    render(<AiSettings onError={vi.fn()} />);
    await user.type(screen.getByLabelText("AI 输入"), "list api.example.test");
    await user.click(screen.getByRole("button", { name: "生成脱敏预览" }));
    expect(await screen.findByText(/将发送的脱敏文本/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "确认发送" }));
    await waitFor(() => expect(api.executeAi).toHaveBeenCalledWith("request"));
  });
});
