import { expect, test, type Page } from "@playwright/test";

async function openPreviewFile(page: Page, secondSession = false) {
  await page.goto("/");
  // Only the remote I/O is stubbed; exercise the real App, file browser and CodeMirror.
  await page.evaluate(async () => {
    const modulePath = "/src/lib/api.ts";
    const { api } = await import(modulePath);
    let content = "remote baseline";
    let modifiedAt = 10;
    api.openText = async (sessionId: string) => {
      localStorage.setItem("test-editor-origin", sessionId);
      return { content, modifiedAt };
    };
    api.saveText = async (sessionId: string, path: string, value: string) => {
      content = value;
      modifiedAt += 1;
      localStorage.setItem(
        "test-editor-save",
        JSON.stringify({ sessionId, path, content }),
      );
    };
  });
  const demo = page.getByRole("button", {
    name: /演示服务器 developer@127\.0\.0\.1:22/,
  });
  await demo.click();
  if (secondSession) {
    await demo.click();
    await page
      .getByRole("tablist", { name: "打开的会话" })
      .getByRole("tab")
      .first()
      .click();
  }
  const tree = page.getByRole("navigation", { name: "远端目录树" });
  await tree.getByRole("button", { name: "展开 home" }).click();
  await tree.getByRole("button", { name: "developer", exact: true }).click();
  await page.getByRole("row", { name: /README\.txt/ }).dblclick();
  await expect(
    page.getByRole("dialog", { name: "预览终端 · README.txt" }),
  ).toBeVisible();
  await expect(page.getByLabel("远程文本内容")).toBeVisible();
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    localStorage.setItem("cnshell-welcome-seen", "1"),
  );
});

test("keeps a dirty CodeMirror document when collapsing the panel and switching sessions", async ({
  page,
}) => {
  await openPreviewFile(page, true);
  const editor = page.getByLabel("远程文本内容");
  await editor.fill("未保存的编辑 🚀");
  const origin = await page.evaluate(() =>
    localStorage.getItem("test-editor-origin"),
  );
  await page.keyboard.press("Meta+j");
  await expect(page.locator(".bottom-panel")).toHaveCount(0);
  await expect(editor).toHaveText("未保存的编辑 🚀");
  await page.keyboard.press("Meta+2");
  await expect(
    page.getByRole("tablist", { name: "打开的会话" }).getByRole("tab").nth(1),
  ).toHaveAttribute("aria-selected", "true");
  await expect(editor).toHaveText("未保存的编辑 🚀");
  await page.getByRole("button", { name: "保存到服务器" }).click();
  await expect(page.getByText(/· 已保存/)).toBeVisible();
  const saved = await page.evaluate(() =>
    JSON.parse(localStorage.getItem("test-editor-save")!),
  );
  expect(saved).toEqual({
    sessionId: origin,
    path: "/home/developer/README.txt",
    content: "未保存的编辑 🚀",
  });
});

test("restores a retained draft after reloading and reconnecting", async ({
  page,
}) => {
  await openPreviewFile(page);
  await page.getByLabel("远程文本内容").fill("recovered after restart");
  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "保留草稿并关闭" }).click();
  await expect(
    page.getByRole("dialog", { name: "预览终端 · README.txt" }),
  ).toHaveCount(0);
  await openPreviewFile(page);
  await expect(page.getByLabel("远程文本内容")).toHaveText(
    "recovered after restart",
  );
  await expect(page.getByRole("dialog").getByRole("status")).toContainText(
    "已恢复本地草稿",
  );
  page.once("dialog", (dialog) => dialog.accept());
  await page
    .getByRole("dialog")
    .getByRole("button", { name: "关闭", exact: true })
    .last()
    .click();
  expect(
    await page.evaluate(() =>
      Object.keys(localStorage).filter((key) =>
        key.startsWith("cnshell-text-draft-v1:"),
      ),
    ),
  ).toEqual([]);
});
