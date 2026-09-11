import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { AiSettings } from "./AiSettings";

describe("AiSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "listAiProviders").mockResolvedValue([
      {
        id: "local",
        name: "本地",
        endpoint: "http://127.0.0.1:11434/v1",
        model: "model",
        hasApiKey: false,
      },
    ]);
    vi.spyOn(api, "saveAiProvider").mockImplementation(async (input) => ({
      id: input.id,
      name: input.name,
      endpoint: input.endpoint,
      model: input.model,
      hasApiKey: Boolean(input.apiKey),
    }));
  });

  it("keeps provider configuration separate from terminal AI requests", async () => {
    const user = userEvent.setup();
    render(<AiSettings onError={vi.fn()} />);
    const model = await screen.findByRole("textbox", { name: "模型" });
    await waitFor(() => expect(model).toHaveValue("model"));
    await user.clear(model);
    await user.type(model, "model-v2");
    await user.click(screen.getByRole("button", { name: "保存 Provider" }));
    await waitFor(() =>
      expect(api.saveAiProvider).toHaveBeenCalledWith(
        expect.objectContaining({ id: "local", model: "model-v2" }),
      ),
    );
    expect(screen.queryByLabelText("AI 输入")).not.toBeInTheDocument();
  });
});
