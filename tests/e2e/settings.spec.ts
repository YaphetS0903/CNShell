import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    localStorage.setItem("cnshell-welcome-seen", "1");
    localStorage.removeItem("cnshell-settings");
  });
});

test("keeps the settings footer reachable and uses compact navigation at narrow widths", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1100, height: 760 });
  await page.goto("/");
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "设置" })
    .click();
  const dialog = page.getByRole("dialog", { name: "设置" });
  await expect(dialog).toBeVisible();
  const automationTab = dialog.getByRole("tab", { name: "自动化与集成" });
  await automationTab.click();
  await expect(automationTab).toHaveAttribute("aria-selected", "true");
  await page.setViewportSize({ width: 700, height: 760 });
  await expect(dialog.locator(".settings-navigation")).toHaveCSS(
    "flex-direction",
    "row",
  );
  await expect(dialog.getByRole("button", { name: "完成" })).toBeInViewport();
  await expect(
    dialog.getByRole("button", { name: "保存基础设置", exact: true }),
  ).toHaveCount(0);
  await expect(dialog.getByRole("button", { name: /^MCP 服务/ })).toBeVisible();
  await expect(
    dialog.getByRole("button", { name: /^MCP 服务/ }),
  ).toBeInViewport();
  await expect(
    dialog.getByRole("button", { name: /^MCP 服务/ }),
  ).toHaveAttribute("aria-expanded", "false");
});

test("renders settings surfaces correctly in explicit light and dark themes", async ({
  page,
}) => {
  await page.goto("/");
  const settingsButton = page
    .getByRole("navigation")
    .getByRole("button", { name: "设置" });
  await settingsButton.click();
  const dialog = page.getByRole("dialog", { name: "设置" });
  await dialog.getByLabel("主题").selectOption("light");
  await dialog
    .getByRole("button", { name: "保存基础设置", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  await expectFaintContrast(page, [
    "--bg",
    "--surface",
    "--surface-2",
    "--surface-3",
  ]);
  await expect(dialog).toBeVisible();
  await dialog.getByRole("tab", { name: "连接与安全" }).click();
  const lightBackground = await dialog
    .locator(".settings-module")
    .first()
    .evaluate((element) => getComputedStyle(element).backgroundColor);
  expect(lightBackground).not.toBe("rgb(7, 16, 29)");
  await dialog.getByRole("tab", { name: "基础设置" }).click();
  await dialog.getByLabel("主题").selectOption("dark");
  await dialog
    .getByRole("button", { name: "保存基础设置", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expectFaintContrast(page, ["--surface-3"]);
});

test("searches metadata and expands only the selected advanced module", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "设置" })
    .click();
  const dialog = page.getByRole("dialog", { name: "设置" });
  await dialog.getByRole("searchbox", { name: "搜索设置" }).fill("Mosh");
  await dialog.getByRole("button", { name: /高级协议与转发/ }).click();
  await expect(dialog.getByRole("tab", { name: "连接与安全" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(
    dialog.getByRole("button", { name: /^高级协议与转发/ }),
  ).toHaveAttribute("aria-expanded", "true");
  await expect(
    dialog.getByRole("button", { name: /^OpenSSH 配置与密钥/ }),
  ).toHaveAttribute("aria-expanded", "false");
  await expect(
    dialog.getByRole("combobox", { name: "按连接配置协议选项" }),
  ).toBeVisible();
});

async function expectFaintContrast(
  page: import("@playwright/test").Page,
  backgrounds: string[],
) {
  const colors = await page.locator("html").evaluate((element, tokens) => {
    const styles = getComputedStyle(element);
    return {
      faint: styles.getPropertyValue("--faint").trim(),
      backgrounds: tokens.map((token) => styles.getPropertyValue(token).trim()),
    };
  }, backgrounds);
  for (const background of colors.backgrounds) {
    expect(contrastRatio(colors.faint, background)).toBeGreaterThanOrEqual(4.5);
  }
}

function contrastRatio(left: string, right: string) {
  const luminance = (color: string) => {
    const normalized =
      color.length === 4
        ? `#${color
            .slice(1)
            .split("")
            .map((value) => value.repeat(2))
            .join("")}`
        : color;
    const channels = normalized
      .slice(1)
      .match(/.{2}/g)!
      .map((value) => Number.parseInt(value, 16) / 255)
      .map((value) =>
        value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
      );
    return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  };
  const [bright, dark] = [luminance(left), luminance(right)].sort(
    (a, b) => b - a,
  );
  return (bright + 0.05) / (dark + 0.05);
}
