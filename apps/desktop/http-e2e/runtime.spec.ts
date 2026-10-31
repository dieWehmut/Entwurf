import { expect, test, type Page } from "@playwright/test";

async function createProject(page: Page, name: string) {
  await page.getByLabel("项目名称").fill(name);
  await page.getByRole("button", { name: "新建项目", exact: true }).click();
  await expect(page.getByLabel("任务标题")).toBeEnabled();
}
async function createTask(page: Page, title: string) {
  await page.getByLabel("任务标题").fill(title);
  await page.getByRole("button", { name: "创建任务", exact: true }).click();
  await expect(page.getByLabel("当前任务")).toContainText(title);
}
test("window.open clones selection without sharing the server client, and reload restores selection", async ({ page }) => {
  const ids: string[] = [];
  page.on("request", request => { if (new URL(request.url()).pathname === "/api/bootstrap") ids.push(request.headers()["x-entwurf-client"]); });
  await page.goto("/"); await createProject(page, "HTTP A");
  const selection = await page.evaluate(() => sessionStorage.getItem("entwurf.http-selection.v1"));
  expect(selection).toBeTruthy();
  const opened = page.waitForEvent("popup");
  await page.evaluate(() => { window.open(location.href, "_blank"); });
  const other = await opened;
  await expect(other.getByLabel("任务标题")).toBeEnabled();
  expect(await other.evaluate(() => sessionStorage.getItem("entwurf.http-selection.v1"))).toBe(selection);
  await createProject(other, "HTTP B");
  await createTask(page, "只属于 HTTP A");
  await createTask(other, "只属于 HTTP B");
  await page.reload();
  await expect(page.getByLabel("当前任务")).toContainText("只属于 HTTP A");
  await expect(page.getByLabel("当前任务")).not.toContainText("只属于 HTTP B");
  await other.getByRole("button", { name: "刷新工作区" }).click();
  await expect(other.getByLabel("当前任务")).toContainText("只属于 HTTP B");
  await expect(other.getByLabel("当前任务")).not.toContainText("只属于 HTTP A");
  expect(new Set(ids).size).toBe(2);
});

test("a three-second local bootstrap keeps the HTTP database", async ({ page }) => {
  let delayed = false;
  await page.route("**/api/bootstrap", async route => {
    if (!delayed) { delayed = true; await new Promise(resolve => setTimeout(resolve, 3000)); }
    await route.continue();
  });
  await page.goto("/");
  await expect(page.getByRole("heading", { name: "本地项目", exact: true })).toBeVisible({ timeout: 12_000 });
  await expect(page.getByRole("heading", { name: "浏览器项目", exact: true })).toHaveCount(0);
  await createProject(page, "延迟服务项目");
  expect(await page.evaluate(() => sessionStorage.getItem("entwurf.http-selection.v1"))).toBeTruthy();
});

test("an interrupted bootstrap shows retry without switching to browser storage", async ({ page }) => {
  await page.route("**/api/bootstrap", route => route.abort("connectionrefused"));
  await page.goto("/");
  await expect(page.getByRole("alert")).toContainText("项目存储未切换");
  await expect(page.getByRole("heading", { name: "浏览器项目", exact: true })).toHaveCount(0);
  await page.unroute("**/api/bootstrap");
  await page.getByRole("button", { name: "重试连接", exact: true }).click();
  await expect(page.getByRole("heading", { name: "本地项目", exact: true })).toBeVisible();
});
