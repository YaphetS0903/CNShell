import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("cnshell-welcome-seen", "1");
    localStorage.removeItem("cnshell-settings");
  });
});

test("keeps the settings footer reachable and uses compact navigation at narrow widths", async ({ page }) => {
  await page.setViewportSize({ width: 1100, height: 760 });
  await page.goto("/");
  await page.getByRole("navigation").getByRole("button", { name: "设置" }).click();
  const dialog = page.getByRole("dialog", { name: "设置" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("tab", { name: "自动化与集成" }).click();
  await page.setViewportSize({ width: 700, height: 760 });
  await expect(dialog.locator(".settings-navigation")).toHaveCSS("flex-direction", "row");
  await expect(dialog.getByRole("button", { name: "取消" })).toBeInViewport();
  await expect(dialog.getByRole("button", { name: "保存设置" })).toBeInViewport();
  await expect(dialog.getByRole("button", { name: /^MCP 服务/ })).toBeVisible();
  await expect(dialog.getByRole("button", { name: /^MCP 服务/ })).toBeInViewport();
  await expect(dialog.getByRole("button", { name: /^MCP 服务/ })).toHaveAttribute("aria-expanded", "false");
});

test("renders settings surfaces correctly in explicit light and dark themes", async ({ page }) => {
  await page.goto("/");
  const settingsButton = page.getByRole("navigation").getByRole("button", { name: "设置" });
  await settingsButton.click();
  let dialog = page.getByRole("dialog", { name: "设置" });
  await dialog.getByLabel("主题").selectOption("light");
  await dialog.getByRole("button", { name: "保存设置" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await settingsButton.click();
  dialog = page.getByRole("dialog", { name: "设置" });
  await dialog.getByRole("tab", { name: "连接与安全" }).click();
  const lightBackground = await dialog.locator(".settings-module").first().evaluate((element) => getComputedStyle(element).backgroundColor);
  expect(lightBackground).not.toBe("rgb(7, 16, 29)");
  await dialog.getByRole("tab", { name: "基础设置" }).click();
  await dialog.getByLabel("主题").selectOption("dark");
  await dialog.getByRole("button", { name: "保存设置" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});

test("searches metadata and expands only the selected advanced module", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("navigation").getByRole("button", { name: "设置" }).click();
  const dialog = page.getByRole("dialog", { name: "设置" });
  await dialog.getByRole("searchbox", { name: "搜索设置" }).fill("Mosh");
  await dialog.getByRole("button", { name: /高级协议与转发/ }).click();
  await expect(dialog.getByRole("tab", { name: "连接与安全" })).toHaveAttribute("aria-selected", "true");
  await expect(dialog.getByRole("button", { name: /^高级协议与转发/ })).toHaveAttribute("aria-expanded", "true");
  await expect(dialog.getByRole("button", { name: /^OpenSSH 配置与密钥/ })).toHaveAttribute("aria-expanded", "false");
  await expect(dialog.getByRole("combobox", { name: "按连接配置协议选项" })).toBeVisible();
});
