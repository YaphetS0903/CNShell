import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() =>
    localStorage.setItem("cnshell-welcome-seen", "1"),
  );
});

test("creates a connection and opens browser preview terminal", async ({
  page,
}) => {
  const consoleErrors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(message.text());
  });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "连接" })).toBeVisible();
  const titlebar = page.locator(".titlebar");
  await expect(
    titlebar.getByRole("button", { name: "连接管理器" }),
  ).toHaveCount(0);
  await expect(
    titlebar.getByRole("button", { name: "新建连接" }),
  ).toHaveCount(0);
  await page
    .locator(".connections-sidebar")
    .getByRole("button", { name: "新建连接" })
    .click();
  await page
    .getByRole("textbox", { name: "名称", exact: true })
    .fill("E2E Server");
  await page
    .getByRole("textbox", { name: "主机", exact: true })
    .fill("192.0.2.20");
  await page.getByRole("textbox", { name: "用户名", exact: true }).fill("ops");
  await page.getByRole("button", { name: "保存连接" }).click();
  await expect(
    page.getByRole("button", { name: /E2E Server ops@192\.0\.2\.20:22/ }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  await expect(page.getByRole("tab", { name: "预览终端" })).toBeVisible();
  await expect(page.locator(".titlebar-context")).toContainText("预览终端");
  await expect(page.locator(".titlebar-context")).toContainText(
    "SSH · developer@127.0.0.1",
  );
  await expect(
    page.getByText("建立真实 SSH 会话。", { exact: false }),
  ).toBeVisible();
  expect(consoleErrors).toEqual([]);
});

test("opens automation as an independent task center", async ({ page }) => {
  await page.goto("/");
  const navigation = page.getByRole("navigation");
  await navigation.getByRole("button", { name: "自动化中心" }).click();

  const center = page.getByRole("dialog", { name: "自动化中心" });
  await expect(center).toBeVisible();
  await expect(center.getByRole("tab", { name: "任务编排" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(center.getByRole("tab", { name: "运行记录" })).toBeVisible();
  await center.getByRole("button", { name: "关闭" }).click();

  await navigation.getByRole("button", { name: "设置" }).click();
  const settings = page.getByRole("dialog", { name: "设置" });
  await settings.getByRole("tab", { name: "自动化与集成" }).click();
  await settings.getByRole("button", { name: /^自动化与定时任务/ }).click();
  await settings.getByRole("button", { name: "打开自动化中心" }).click();
  await expect(settings).toBeHidden();
  await expect(page.getByRole("dialog", { name: "自动化中心" })).toBeVisible();
});

test("previews system information and network diagnostics without desktop IPC", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  await page.getByRole("tab", { name: "系统信息", exact: true }).click();

  await expect(page.getByText("preview-host")).toBeVisible();
  await expect(page.getByText(/^采集于 /)).toBeVisible();
  await page.getByRole("tab", { name: "磁盘", exact: true }).click();
  await expect(page.getByText("/dev/vda1")).toBeVisible();
  await expect(page.getByText("/run", { exact: true })).toBeHidden();
  await page.getByRole("button", { name: "显示全部挂载点（另 1 项）" }).click();
  await expect(page.getByText("/run", { exact: true })).toBeVisible();

  await page.getByRole("tab", { name: "端口与连接", exact: true }).click();
  await expect(page.getByText("显示 3/3 条，共 3 条")).toBeVisible();
  await page
    .getByRole("combobox", { name: "连接状态筛选" })
    .selectOption("LISTEN");
  await expect(page.getByText("显示 2/2 条，共 3 条")).toBeVisible();
  await page
    .getByRole("combobox", { name: "连接状态筛选" })
    .selectOption("all");
  await page.getByRole("textbox", { name: "搜索端口与连接" }).fill("nginx");
  await expect(page.getByText("显示 1/1 条，共 3 条")).toBeVisible();

  await page.getByRole("textbox", { name: "网络诊断目标" }).fill("example.com");
  await page.getByRole("button", { name: "Ping" }).click();
  await expect(page.getByText("Ping · example.com")).toBeVisible();
  await expect(page.getByText(/18\.4\/18\.4\/18\.4 ms/)).toBeVisible();
  await expect(page.getByRole("button", { name: "关闭错误" })).toHaveCount(0);
});

test("opens settings and help with accessible dialogs", async ({ page }) => {
  await page.goto("/");
  const navigation = page.getByRole("navigation");
  await navigation.getByRole("button", { name: "设置" }).click();
  await expect(page.getByRole("dialog", { name: "设置" })).toBeVisible();
  await page.getByLabel("主题").selectOption("highContrast");
  await page
    .getByRole("dialog", { name: "设置" })
    .getByRole("button", { name: "保存基础设置", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute(
    "data-theme",
    "highContrast",
  );
  await expect(page.getByRole("dialog", { name: "设置" })).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "设置" })).toBeHidden();
  await navigation.getByRole("button", { name: "设置" }).click();
  await page
    .getByRole("dialog", { name: "设置" })
    .getByRole("button", { name: "关闭" })
    .click();
  await navigation.getByRole("button", { name: "使用帮助" }).click();
  await expect(
    page.getByRole("dialog", { name: "CNshell 使用帮助" }),
  ).toContainText("密码不写入数据库和日志");
  await expect(page.getByRole("dialog")).toHaveCount(1);
});

test("applies terminal preferences to an open session without reconnecting", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  await expect(page.getByRole("tab", { name: "预览终端" })).toBeVisible();
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "设置" })
    .click();
  const dialog = page.getByRole("dialog", { name: "设置" });
  await dialog.getByLabel("字体").selectOption("menlo");
  await dialog.getByLabel("字号").fill("18");
  await dialog.getByRole("radio", { name: "Solarized" }).click();
  await dialog
    .getByRole("button", { name: "保存基础设置", exact: true })
    .click();
  await expect(page.locator(".terminal-instance.active")).toHaveCSS(
    "background-color",
    "rgb(0, 43, 54)",
  );
  await expect(page.getByRole("tab", { name: "预览终端" })).toBeVisible();
  await dialog.getByRole("button", { name: "关闭" }).click();
  await page.getByRole("textbox", { name: "Terminal input" }).click();
  await page.keyboard.press("Meta+=");
  await page
    .getByRole("navigation")
    .getByRole("button", { name: "设置" })
    .click();
  await expect(
    page.getByRole("dialog", { name: "设置" }).getByLabel("字号"),
  ).toHaveValue("19");
});

test("follows the macOS light appearance without overriding explicit themes", async ({
  page,
}) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");
  await expect(page.locator("html")).not.toHaveAttribute("data-theme", /.+/);
  await expect(page.locator("html")).toHaveCSS("color-scheme", "light");
  await expect(page.locator(".app-shell")).toHaveCSS(
    "background-color",
    "rgb(237, 242, 248)",
  );

  await page
    .getByRole("navigation")
    .getByRole("button", { name: "设置" })
    .click();
  await page.getByLabel("主题").selectOption("dark");
  await page
    .getByRole("dialog", { name: "设置" })
    .getByRole("button", { name: "保存基础设置", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator("html")).toHaveCSS("color-scheme", "dark");
});

test("collapses monitor at compact viewport", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 700 });
  await page.goto("/");
  await expect(page.locator(".monitor-sidebar")).toBeHidden();
  await expect(page.getByRole("heading", { name: "连接" })).toBeVisible();
});

test("keeps connection actions visible while optional fields scroll", async ({
  page,
}) => {
  await page.setViewportSize({ width: 700, height: 760 });
  await page.goto("/");
  await page
    .locator(".connections-sidebar")
    .getByRole("button", { name: "新建连接" })
    .click();

  const dialog = page.getByRole("dialog", { name: "新建连接" });
  const footer = dialog.locator(".modal-footer");
  await expect(footer).toBeVisible();
  await expect(
    dialog.getByRole("button", { name: "保存并连接" }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);

  await dialog.getByText("连接选项", { exact: true }).click();
  const footerBox = await footer.boundingBox();
  expect(footerBox).not.toBeNull();
  expect((footerBox?.y ?? 0) + (footerBox?.height ?? 0)).toBeLessThanOrEqual(
    760,
  );
  await expect(
    dialog.getByRole("combobox", { name: "主机密钥策略" }),
  ).toBeVisible();
});

test("exposes import and keyboard-resizable workspace panels", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "导入连接" })).toBeVisible();
  const connectionsResize = page.getByRole("separator", {
    name: "调整连接库宽度",
  });
  await expect(connectionsResize).toHaveAttribute("aria-valuenow", "260");
  await connectionsResize.press("ArrowRight");
  await expect(connectionsResize).toHaveAttribute("aria-valuenow", "276");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  const bottomResize = page.getByRole("separator", {
    name: "调整底部工具区高度",
  });
  await expect(bottomResize).toHaveAttribute("aria-valuenow", "260");
  await bottomResize.press("ArrowUp");
  await expect(bottomResize).toHaveAttribute("aria-valuenow", "244");
  await page.reload();
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  await expect(
    page.getByRole("separator", { name: "调整底部工具区高度" }),
  ).toHaveAttribute("aria-valuenow", "244");
});

test("moves connections to trash and restores them", async ({ page }) => {
  await page.goto("/");
  const row = page.getByRole("button", {
    name: /演示服务器 developer@127\.0\.0\.1:22/,
  });
  await expect(row).toBeVisible();
  await page.getByRole("button", { name: "演示服务器操作" }).click();
  page.once("dialog", (dialog) => dialog.accept());
  await page.getByRole("button", { name: "删除", exact: true }).click();
  await expect(row).toBeHidden();
  await page.getByRole("button", { name: /已删除项目/ }).click();
  await expect(page.getByText(/^删除于 /)).toBeVisible();
  await page.getByRole("button", { name: "恢复 演示服务器" }).click();
  await page.getByRole("button", { name: /所有连接/ }).click();
  await expect(row).toBeVisible();
});

test("pins favorite connections and keeps the choice after reload", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByRole("button", { name: "收藏 演示服务器" }).click();
  await expect(
    page.getByRole("button", { name: "取消收藏 演示服务器" }),
  ).toHaveClass(/is-active/);
  await page.reload();
  await page.getByRole("button", { name: /^收藏/ }).first().click();
  await expect(
    page.getByRole("button", {
      name: /演示服务器 developer@127\.0\.0\.1:22/,
    }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "取消收藏 演示服务器" }),
  ).toBeVisible();
});

test("keeps terminal local echo scheduling below 50 ms per character", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  const input = page.locator(".xterm-helper-textarea");
  await input.focus();
  const marker = "CNshell-latency-probe";
  const started = Date.now();
  await page.keyboard.type(marker);
  const elapsed = Date.now() - started;
  await expect(page.locator(".xterm-rows")).toContainText(marker);
  expect(elapsed / marker.length).toBeLessThan(50);
});

test("preserves IME-style UTF-8 input and emoji", async ({ page }) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  const input = page.locator(".xterm-helper-textarea");
  await input.focus();
  await page.keyboard.insertText("中文输入🚀");
  await expect(page.locator(".xterm-rows")).toContainText("中文输入🚀");
});

test("keeps built-in commands read-only and allows deleting user commands", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  await page.getByRole("tab", { name: "快捷命令", exact: true }).click();
  const builtIn = page.getByRole("button", {
    name: "系统概览，填入命令",
  });
  await expect(builtIn).toBeVisible();
  await expect(
    page.getByRole("button", { name: "执行快捷命令 系统概览" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "删除快捷命令 系统概览" }),
  ).toHaveCount(0);
  await builtIn.click();
  await expect(
    page.getByRole("combobox", { name: "智能命令输入" }),
  ).toHaveValue("uname -a && uptime");
  await page
    .getByRole("combobox", { name: "智能命令输入" })
    .fill("echo user-command");
  await page.getByRole("button", { name: "保存" }).click();
  const snippetDialog = page.getByRole("dialog", { name: "保存快捷命令" });
  await snippetDialog.getByRole("textbox", { name: "名称" }).fill("我的命令");
  await snippetDialog.getByRole("button", { name: "保存" }).click();
  const remove = page.getByRole("button", { name: "删除快捷命令 我的命令" });
  await expect(remove).toBeVisible();
  page.once("dialog", (dialog) => dialog.accept());
  await remove.click();
  await expect(remove).toHaveCount(0);
});

test("provides keyboard-operable session and tool tabs", async ({ page }) => {
  await page.goto("/");
  const demo = page.getByRole("button", {
    name: /演示服务器 developer@127\.0\.0\.1:22/,
  });
  await demo.click();
  await demo.click();
  const tabs = page
    .getByRole("tablist", { name: "打开的会话" })
    .getByRole("tab");
  await expect(tabs).toHaveCount(2);
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await tabs.nth(1).press("ArrowLeft");
  await expect(tabs.nth(0)).toHaveAttribute("aria-selected", "true");
  await expect(tabs.nth(0)).toBeFocused();
  await expect(
    page.getByRole("tabpanel", { name: /预览终端/ }).first(),
  ).toBeVisible();

  const files = page.getByRole("tab", { name: "文件", exact: true });
  await files.focus();
  await files.press("ArrowRight");
  await expect(
    page.getByRole("tab", { name: "快捷命令", exact: true }),
  ).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "快捷命令" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: /预览终端 会话操作/ }).first(),
  ).toBeVisible();
  await files.click();
  const table = page.getByRole("table", { name: "远程目录 /" });
  await expect(table).toHaveAttribute("aria-colcount", "6");
  await expect(
    table.getByRole("columnheader", { name: /名称/ }),
  ).toHaveAttribute("aria-sort", "ascending");
});

test("expands and navigates the remote directory tree", async ({ page }) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  const tree = page.getByRole("navigation", { name: "远端目录树" });
  await expect(
    tree.getByRole("button", { name: "home", exact: true }),
  ).toBeVisible();
  await tree.getByRole("button", { name: "展开 home" }).click();
  await expect(
    tree.getByRole("button", { name: "developer", exact: true }),
  ).toBeVisible();
  await tree.getByRole("button", { name: "developer", exact: true }).click();
  const currentPath = page.getByRole("navigation", {
    name: "当前远程路径",
  });
  await expect(
    currentPath.getByRole("button", { name: "developer", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await page.getByRole("button", { name: "编辑远程路径" }).click();
  await expect(page.getByRole("textbox", { name: "远程路径" })).toHaveValue(
    "/home/developer",
  );
  await page.getByRole("textbox", { name: "远程路径" }).press("Escape");
  await expect(currentPath).toBeVisible();
  await expect(
    page.getByRole("table", { name: "远程目录 /home/developer" }),
  ).toContainText("README.txt");
  await page.getByRole("tab", { name: "快捷命令", exact: true }).click();
  await page.getByRole("tab", { name: "文件", exact: true }).click();
  await expect(
    currentPath.getByRole("button", { name: "developer", exact: true }),
  ).toHaveAttribute("aria-current", "page");
  await expect(tree.getByRole("button", { name: "折叠 home" })).toBeVisible();
  await expect(
    tree.getByRole("button", { name: "developer", exact: true }),
  ).toBeVisible();
  await tree.getByRole("button", { name: "折叠 home" }).click();
  await expect(
    tree.getByRole("button", { name: "developer", exact: true }),
  ).toBeHidden();
});

test("opens file actions from the row context menu", async ({ page }) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  await page.getByRole("row", { name: /home/ }).click({ button: "right" });
  const menu = page.getByRole("menu", { name: "home 文件操作" });
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "复制路径" })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "下载" })).toBeVisible();
  await expect(
    menu.getByRole("menuitem", { name: "上传文件到此处" }),
  ).toBeVisible();
  await expect(
    menu.getByRole("menuitem", { name: "新建文件", exact: true }),
  ).toBeVisible();
  await expect(
    menu.getByRole("menuitem", { name: "新建文件夹", exact: true }),
  ).toBeVisible();
  await expect(
    menu.getByRole("menuitem", { name: "压缩为 tar.gz" }),
  ).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "重命名" })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "修改权限" })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "删除" })).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "编辑文本" })).toBeDisabled();
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await page
    .getByRole("navigation", { name: "远端目录树" })
    .getByRole("button", { name: "展开 home" })
    .click();
  await page
    .getByRole("navigation", { name: "远端目录树" })
    .getByRole("button", { name: "developer", exact: true })
    .click();
  await page
    .getByRole("row", { name: /README\.txt/ })
    .click({ button: "right" });
  const fileMenu = page.getByRole("menu", { name: "README.txt 文件操作" });
  await expect(
    fileMenu.getByRole("menuitem", { name: "编辑文本" }),
  ).toBeEnabled();
  await expect(
    fileMenu.getByRole("menuitem", { name: "使用默认应用打开" }),
  ).toBeVisible();
  await expect(
    fileMenu.getByRole("menuitem", { name: "选择应用打开…" }),
  ).toBeVisible();
});

test("keeps the split tree while selecting its secondary tab", async ({
  page,
}) => {
  await page.goto("/");
  await page
    .getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ })
    .click();
  await page.getByRole("button", { name: /预览终端 会话操作/ }).click();
  await page.getByRole("menuitem", { name: "左右拆分" }).click();
  const tabs = page
    .getByRole("tablist", { name: "打开的会话" })
    .getByRole("tab");
  await expect(tabs).toHaveCount(2);
  await expect(tabs.nth(0)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".terminal-instance.active")).toHaveCount(2);
  await expect(
    page.getByRole("separator", { name: "调整左右终端窗格" }),
  ).toBeVisible();
  await tabs.nth(1).click();
  await expect(
    page.getByRole("separator", { name: "调整左右终端窗格" }),
  ).toBeVisible();
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".terminal-instance.active")).toHaveCount(2);
});

test("keeps the inactive connection-folder shortcut hidden", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "新建文件夹" })).toHaveCount(0);
  await expect(page.getByRole("tree", { name: "连接文件夹树" })).toHaveCount(1);
});
