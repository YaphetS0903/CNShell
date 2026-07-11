import { expect, test } from "@playwright/test";

test("creates a connection and opens browser preview terminal", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "连接" })).toBeVisible();
  await page.getByRole("navigation").getByRole("button", { name: "新建连接" }).click();
  await page.getByRole("textbox", { name: "名称", exact: true }).fill("E2E Server");
  await page.getByRole("textbox", { name: "主机", exact: true }).fill("192.0.2.20");
  await page.getByRole("textbox", { name: "用户名", exact: true }).fill("ops");
  await page.getByRole("button", { name: "保存连接" }).click();
  await expect(page.getByRole("button", { name: /E2E Server ops@192\.0\.2\.20:22/ })).toBeVisible();
  await page.getByRole("button", { name: /演示服务器 developer@127\.0\.0\.1:22/ }).click();
  await expect(page.getByRole("tab", { name: "预览终端" })).toBeVisible();
  await expect(page.getByText("建立真实 SSH 会话。", { exact: false })).toBeVisible();
});

test("opens settings and help with accessible dialogs", async ({ page }) => {
  await page.goto("/");
  const navigation = page.getByRole("navigation");
  await navigation.getByRole("button", { name: "设置" }).click();
  await expect(page.getByRole("dialog", { name: "设置" })).toBeVisible();
  await page.getByLabel("主题").selectOption("highContrast");
  await page.getByRole("dialog", { name: "设置" }).getByRole("button", { name: "保存设置" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "highContrast");
  await navigation.getByRole("button", { name: "设置" }).click();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog", { name: "设置" })).toBeHidden();
  await navigation.getByRole("button", { name: "设置" }).click();
  await page.getByRole("dialog", { name: "设置" }).getByRole("button", { name: "关闭" }).click();
  await navigation.getByRole("button", { name: "使用帮助" }).click();
  await expect(page.getByRole("dialog", { name: "CNshell 使用帮助" })).toContainText("密码不写入数据库和日志");
  await expect(page.getByRole("dialog")).toHaveCount(1);
});

test("follows the macOS light appearance without overriding explicit themes", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");
  await expect(page.locator("html")).not.toHaveAttribute("data-theme", /.+/);
  await expect(page.locator("html")).toHaveCSS("color-scheme", "light");
  await expect(page.locator(".app-shell")).toHaveCSS("background-color", "rgb(237, 242, 248)");

  await page.getByRole("navigation").getByRole("button", { name: "设置" }).click();
  await page.getByLabel("主题").selectOption("dark");
  await page.getByRole("dialog", { name: "设置" }).getByRole("button", { name: "保存设置" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator("html")).toHaveCSS("color-scheme", "dark");
});

test("collapses monitor at compact viewport", async ({ page }) => {
  await page.setViewportSize({ width: 900, height: 700 });
  await page.goto("/");
  await expect(page.locator(".monitor-sidebar")).toBeHidden();
  await expect(page.getByRole("heading", { name: "连接" })).toBeVisible();
});

test("exposes import and keyboard-resizable workspace panels", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("button", { name: "导入连接" })).toBeVisible();
  const connectionsResize=page.getByRole("separator",{name:"调整连接库宽度"});
  await expect(connectionsResize).toHaveAttribute("aria-valuenow","260");
  await connectionsResize.press("ArrowRight");
  await expect(connectionsResize).toHaveAttribute("aria-valuenow","276");
  await page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/}).click();
  const bottomResize=page.getByRole("separator",{name:"调整底部工具区高度"});
  await expect(bottomResize).toHaveAttribute("aria-valuenow","260");
  await bottomResize.press("ArrowUp");
  await expect(bottomResize).toHaveAttribute("aria-valuenow","244");
  await page.reload();
  await page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/}).click();
  await expect(page.getByRole("separator",{name:"调整底部工具区高度"})).toHaveAttribute("aria-valuenow","244");
});

test("moves connections to trash and restores them",async({page})=>{
  await page.goto("/");const row=page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/});await expect(row).toBeVisible();await page.getByRole("button",{name:"演示服务器操作"}).click();page.once("dialog",(dialog)=>dialog.accept());await page.getByRole("button",{name:"删除",exact:true}).click();await expect(row).toBeHidden();await page.getByRole("button",{name:/已删除项目/}).click();await page.getByRole("button",{name:"演示服务器操作"}).click();await page.getByRole("button",{name:"恢复"}).click();await page.getByRole("button",{name:/所有连接/}).click();await expect(row).toBeVisible();
});

test("keeps terminal local echo scheduling below 50 ms per character",async({page})=>{
  await page.goto("/");await page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/}).click();const input=page.locator(".xterm-helper-textarea");await input.focus();const marker="CNshell-latency-probe";const started=Date.now();await page.keyboard.type(marker);const elapsed=Date.now()-started;await expect(page.locator(".xterm-rows")).toContainText(marker);expect(elapsed/marker.length).toBeLessThan(50);
});

test("preserves IME-style UTF-8 input and emoji",async({page})=>{
  await page.goto("/");await page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/}).click();
  const input=page.locator(".xterm-helper-textarea");await input.focus();await page.keyboard.insertText("中文输入🚀");
  await expect(page.locator(".xterm-rows")).toContainText("中文输入🚀");
});

test("keeps built-in commands read-only and allows deleting user commands",async({page})=>{
  await page.goto("/");await page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/}).click();await page.getByRole("tab",{name:"快捷命令",exact:true}).click();
  await expect(page.getByRole("button",{name:"系统概览 uname -a && uptime"})).toBeVisible();await expect(page.getByRole("button",{name:"删除快捷命令 系统概览"})).toHaveCount(0);
  await page.getByPlaceholder("输入命令，Return 执行").fill("echo user-command");page.once("dialog",(dialog)=>dialog.accept("我的命令"));await page.getByRole("button",{name:"保存为快捷命令"}).click();
  const remove=page.getByRole("button",{name:"删除快捷命令 我的命令"});await expect(remove).toBeVisible();page.once("dialog",(dialog)=>dialog.accept());await remove.click();await expect(remove).toHaveCount(0);
});

test("provides keyboard-operable session and tool tabs",async({page})=>{
  await page.goto("/");
  const demo=page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/});
  await demo.click();await demo.click();
  const tabs=page.getByRole("tablist",{name:"打开的会话"}).getByRole("tab");
  await expect(tabs).toHaveCount(2);
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected","true");
  await tabs.nth(1).press("ArrowLeft");
  await expect(tabs.nth(0)).toHaveAttribute("aria-selected","true");
  await expect(tabs.nth(0)).toBeFocused();
  await expect(page.getByRole("tabpanel",{name:/预览终端/}).first()).toBeVisible();

  const files=page.getByRole("tab",{name:"文件",exact:true});
  await files.focus();await files.press("ArrowRight");
  await expect(page.getByRole("tab",{name:"快捷命令",exact:true})).toHaveAttribute("aria-selected","true");
  await expect(page.getByRole("tabpanel",{name:"快捷命令"})).toBeVisible();
  await expect(page.getByRole("button",{name:/预览终端 会话操作/}).first()).toBeVisible();
  await files.click();
  const table=page.getByRole("table",{name:"远程目录 /"});
  await expect(table).toHaveAttribute("aria-colcount","6");
  await expect(table.getByRole("columnheader",{name:/名称/})).toHaveAttribute("aria-sort","ascending");
});

test("expands and navigates the remote directory tree",async({page})=>{
  await page.goto("/");
  await page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/}).click();
  const tree=page.getByRole("navigation",{name:"远端目录树"});
  await expect(tree.getByRole("button",{name:"home",exact:true})).toBeVisible();
  await tree.getByRole("button",{name:"展开 home"}).click();
  await expect(tree.getByRole("button",{name:"developer",exact:true})).toBeVisible();
  await tree.getByRole("button",{name:"developer",exact:true}).click();
  await expect(page.getByLabel("远程路径")).toHaveValue("/home/developer");
  await expect(page.getByRole("table",{name:"远程目录 /home/developer"})).toContainText("README.txt");
  await tree.getByRole("button",{name:"折叠 home"}).click();
  await expect(tree.getByRole("button",{name:"developer",exact:true})).toBeHidden();
});

test("opens file actions from the row context menu",async({page})=>{
  await page.goto("/");
  await page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/}).click();
  await page.getByRole("row",{name:/home/}).click({button:"right"});
  const menu=page.getByRole("menu",{name:"home 文件操作"});
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"复制路径"})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"下载"})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"上传文件到此处"})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"新建文件",exact:true})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"新建文件夹",exact:true})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"压缩为 tar.gz"})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"重命名"})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"修改权限"})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"删除"})).toBeVisible();
  await expect(menu.getByRole("menuitem",{name:"编辑文本"})).toBeDisabled();
  await page.keyboard.press("Escape");
  await expect(menu).toBeHidden();
  await page.getByRole("navigation",{name:"远端目录树"}).getByRole("button",{name:"展开 home"}).click();
  await page.getByRole("navigation",{name:"远端目录树"}).getByRole("button",{name:"developer",exact:true}).click();
  await page.getByRole("row",{name:/README\.txt/}).click({button:"right"});
  const fileMenu=page.getByRole("menu",{name:"README.txt 文件操作"});
  await expect(fileMenu.getByRole("menuitem",{name:"编辑文本"})).toBeEnabled();
  await expect(fileMenu.getByRole("menuitem",{name:"使用默认应用打开"})).toBeVisible();
  await expect(fileMenu.getByRole("menuitem",{name:"选择应用打开…"})).toBeVisible();
});

test("keeps both terminal panes visible and exits split when selecting the secondary tab",async({page})=>{
  await page.goto("/");
  await page.getByRole("button",{name:/演示服务器 developer@127\.0\.0\.1:22/}).click();
  await page.getByRole("button",{name:/预览终端 会话操作/}).click();
  await page.getByRole("menuitem",{name:"左右拆分"}).click();
  const tabs=page.getByRole("tablist",{name:"打开的会话"}).getByRole("tab");
  await expect(tabs).toHaveCount(2);
  await expect(tabs.nth(0)).toHaveAttribute("aria-selected","true");
  await expect(page.locator(".terminal-area")).toHaveClass(/split/);
  await expect(page.locator(".terminal-instance.active.pane-primary")).toHaveCount(1);
  await expect(page.locator(".terminal-instance.active.pane-secondary")).toHaveCount(1);
  await tabs.nth(1).click();
  await expect(page.locator(".terminal-area")).not.toHaveClass(/split/);
  await expect(tabs.nth(1)).toHaveAttribute("aria-selected","true");
  await expect(page.locator(".terminal-instance.active.pane-primary")).toHaveCount(1);
});

test("creates and expands nested connection folders",async({page})=>{
  await page.goto("/");
  page.once("dialog",(dialog)=>dialog.accept("生产"));
  await page.getByRole("button",{name:"新建文件夹"}).click();
  const tree=page.getByRole("tree",{name:"连接文件夹树"});
  await tree.getByRole("button",{name:"生产 0",exact:true}).click();
  page.once("dialog",(dialog)=>dialog.accept("华南"));
  await page.getByRole("button",{name:"新建文件夹"}).click();
  await expect(tree.getByRole("button",{name:"华南 0",exact:true})).toBeVisible();
  await tree.getByRole("button",{name:"折叠 生产"}).click();
  await expect(tree.getByRole("button",{name:"华南 0",exact:true})).toBeHidden();
  await tree.getByRole("button",{name:"展开 生产"}).click();
  await expect(tree.getByRole("button",{name:"华南 0",exact:true})).toBeVisible();
  await page.getByRole("button",{name:/所有连接/}).click();
  await page.getByRole("button",{name:"演示服务器操作"}).click();
  await expect(page.getByRole("button",{name:"移动到文件夹"})).toBeVisible();
});
