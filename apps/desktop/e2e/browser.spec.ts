import { expect, test, type Page } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";

async function createProject(page: Page, name: string) {
  await page.getByLabel("项目名称").fill(name);
  await page.getByRole("button", { name: "新建项目", exact: true }).click();
  await expect(page.getByLabel("任务标题")).toBeEnabled();
}
async function createTask(page: Page, title: string) {
  await page.getByLabel("任务标题").fill(title);
  await page.getByRole("button", { name: "创建任务", exact: true }).click();
  await expect(page.getByRole("heading", { name: "导入资料", exact: true })).toBeVisible();
}
const nav = (page: Page, name: string) => page.getByRole("navigation").getByRole("button", { name: new RegExp(`^${name}`) }).click();

test("real WASM project imports Markdown and survives reload with original source and nodes", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("console", message => { if (message.type() === "error") errors.push(message.text()); });
  await page.goto("./");
  await expect(page).toHaveTitle("Entwurf · 可验证知识图");
  await expect(page.getByRole("heading", { name: "浏览器项目", exact: true })).toBeVisible();
  await createProject(page, "浏览器持久化验收");
  await createTask(page, "集合与映射");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "选择本机文件并导入" }).click();
  await (await chooser).setFiles({ name: "sets.md", mimeType: "text/markdown", buffer: Buffer.from("# 集合\n\n集合是确定对象组成的整体。\n\n## 映射\n\n映射将定义域中的元素对应到值域中的元素。\n") });
  await expect(page.getByText("sets.md", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "重新解析原件" }).click();
  await expect(page.getByText("重新解析原件完成", { exact: true })).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
  await nav(page, "结果");
  await expect(page.getByLabel("节点标题")).toBeVisible();
  await page.getByLabel("节点标题").fill("已保存的集合定义");
  await page.getByRole("button", { name: "保存节点修改" }).click();
  await expect(page.getByLabel("节点标题")).toHaveValue("已保存的集合定义");
  await page.reload();
  await expect(page.getByLabel("当前任务")).toContainText("集合与映射");
  await nav(page, "结果");
  await expect(page.getByLabel("节点标题")).toHaveValue("已保存的集合定义");
  await page.getByRole("button", { name: "编译预览", exact: true }).click();
  await page.getByRole("button", { name: "编译页面", exact: true }).click();
  await expect(page.getByRole("img", { name: /本地编译第 1 页/ })).toBeVisible({ timeout: 90_000 });
  await page.getByRole("button", { name: "导出 SVG / PNG / PDF", exact: true }).click();
  await expect(page.getByRole("button", { name: "打开 PDF", exact: true })).toBeVisible();
  for (const [label, signature] of [["打开 PDF", "%PDF"], ["打开 PNG 1", "PNG"], ["打开 SVG 1", "<svg"]]) {
    const downloading = page.waitForEvent("download");
    await page.getByRole("button", { name: label, exact: true }).click();
    const downloaded = await downloading;
    const bytes = await readFile((await downloaded.path())!);
    expect(bytes.subarray(0, 100).toString("utf8")).toContain(signature);
  }
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: "/tmp/entwurf-browser-persistent-result.png", fullPage: false });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: "/tmp/entwurf-browser-mobile.png", fullPage: false });
  await page.getByRole("button", { name: "打开设置", exact: true }).click();
  const diagnosticsDownload = page.waitForEvent("download");
  await page.getByRole("button", { name: "导出脱敏诊断", exact: true }).click();
  const diagnostics = await readFile((await (await diagnosticsDownload).path())!, "utf8");
  expect(() => JSON.parse(diagnostics)).not.toThrow();
  expect(diagnostics).not.toContain("已保存的集合定义");
  expect(diagnostics).not.toContain("浏览器持久化验收");
  expect(errors).toEqual([]);
});

test("two tabs keep their active projects separate while sharing the database", async ({ page, context }) => {
  await page.goto("./"); await createProject(page, "项目 A");
  const other = await context.newPage();
  await other.goto("./"); await createProject(other, "项目 B");
  await createTask(page, "只属于 A");
  await createTask(other, "只属于 B");
  await nav(page, "项目"); await nav(other, "项目");
  await page.getByRole("button", { name: "刷新工作区" }).click();
  await other.getByRole("button", { name: "刷新工作区" }).click();
  await expect(page.getByLabel("当前任务")).toContainText("只属于 A");
  await expect(page.getByLabel("当前任务")).not.toContainText("只属于 B");
  await expect(other.getByLabel("当前任务")).toContainText("只属于 B");
  await expect(other.getByLabel("当前任务")).not.toContainText("只属于 A");
});

test("real PDF text and image OCR keep original sources and review evidence", async ({ page, baseURL }) => {
  test.setTimeout(240_000);
  const externalRequests: string[] = [];
  const expectedOrigin = new URL(baseURL!).origin;
  page.on("request", request => { if (/^https?:/.test(request.url()) && new URL(request.url()).origin !== expectedOrigin) externalRequests.push(request.url()); });
  await page.goto("./"); await createProject(page, "PDF 与 OCR 验收"); await createTask(page, "真实二进制来源");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "选择本机文件并导入" }).click();
  await (await chooser).setFiles(["two-pages.pdf", "bilingual.png", "scanned.pdf"].map(name => fileURLToPath(new URL(`../../../fixtures/public/ingestion/${name}`, import.meta.url))));
  for (const name of ["two-pages.pdf", "bilingual.png", "scanned.pdf"]) await expect(page.getByText(name, { exact: true })).toBeVisible({ timeout: 180_000 });
  await nav(page, "结果");
  await expect(page.getByLabel("节点标题")).toBeVisible();
  await expect(page.getByRole("button", { name: "记录审阅并确认" }).first()).toBeVisible();
  await page.reload();
  await nav(page, "资料与风格");
  for (const name of ["two-pages.pdf", "bilingual.png", "scanned.pdf"]) await expect(page.getByText(name, { exact: true })).toBeVisible();
  for (const name of ["two-pages.pdf", "bilingual.png"]) {
    await page.locator("article.source-record").filter({ has: page.getByText(name, { exact: true }) }).getByRole("button", { name: "重新解析原件" }).click();
    await expect(page.getByText("重新解析原件完成", { exact: true })).toBeVisible();
    await expect(page.getByRole("alert")).toHaveCount(0);
  }
  expect(externalRequests).toEqual([]);
});

test("a stale tab cannot overwrite a saved node and keeps its unsaved draft", async ({ page, context }) => {
  await page.goto("./"); await createProject(page, "并发编辑验收"); await createTask(page, "同一任务");
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "选择本机文件并导入" }).click();
  await (await chooser).setFiles({ name: "draft.md", mimeType: "text/markdown", buffer: Buffer.from("# Definition\n\nA function maps each input to one output.\n") });
  await expect(page.getByText("draft.md", { exact: true })).toBeVisible(); await nav(page, "结果");
  const other = await context.newPage(); await other.goto("./"); await nav(other, "结果");
  await other.getByLabel("节点标题").fill("保留但不能覆盖的草稿");
  await page.getByLabel("节点标题").fill("已提交的新标题");
  await page.getByRole("button", { name: "保存节点修改" }).click();
  await expect(page.getByRole("button", { name: "保存节点修改" })).toBeDisabled();
  await other.getByRole("button", { name: "保存节点修改" }).click();
  await expect(other.getByRole("alert")).toBeVisible();
  await expect(other.getByLabel("节点标题")).toHaveValue("保留但不能覆盖的草稿");
  await expect(other.getByText(/草稿已过期/)).toBeVisible();
  await other.getByRole("button", { name: "放弃草稿" }).click();
  await expect(other.getByLabel("节点标题")).toHaveValue("已提交的新标题");
});

test("EXIF-rotated JPEGs retain raw dimensions for source anchor validation", async ({ page }) => {
  await page.goto("./"); await createProject(page, "EXIF 来源验收"); await createTask(page, "相机方向");
  const images = await page.evaluate(async () => {
    const canvas = document.createElement("canvas"); canvas.width = 400; canvas.height = 160;
    const draw = canvas.getContext("2d")!; draw.fillStyle = "white"; draw.fillRect(0, 0, 400, 160); draw.fillStyle = "black"; draw.font = "32px sans-serif"; draw.fillText("Probability 123", 20, 90);
    const blob = await new Promise<Blob>(resolve => canvas.toBlob(blob => resolve(blob!), "image/jpeg"));
    const jpeg = new Uint8Array(await blob.arrayBuffer());
    return [6, 8].map(orientation => {
      // APP1 Exif/TIFF: one SHORT Orientation tag. JPEG encoded size stays 400x160.
      const exif = new Uint8Array([255,225,0,34,69,120,105,102,0,0,73,73,42,0,8,0,0,0,1,0,18,1,3,0,1,0,0,0,orientation,0,0,0,0,0,0,0]);
      return { orientation, bytes: [...jpeg.subarray(0,2), ...exif, ...jpeg.subarray(2)] };
    });
  });
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: "选择本机文件并导入" }).click();
  await (await chooser).setFiles(images.map(image => ({ name: `orientation-${image.orientation}.jpg`, mimeType: "image/jpeg", buffer: Buffer.from(image.bytes) })));
  for (const image of images) await expect(page.getByText(`orientation-${image.orientation}.jpg`, { exact: true })).toBeVisible({ timeout: 30_000 });
  await nav(page, "结果");
  await expect(page.getByLabel("来源锚点 JSON")).toContainText('"width": 400');
  await expect(page.getByLabel("来源锚点 JSON")).toContainText('"height": 160');
});
