import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/");
  await page.evaluate(() => localStorage.clear());
  await page.reload();
});

test("five-page workspace renders without console errors", async ({ page }) => {
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await expect(page.getByRole("heading", { name: /把教材编译成可追溯的知识图/ })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  await expect(page.locator(".node-formula").first()).toHaveCSS("font-family", /Entwurf UI/);
  await page.screenshot({ path: "test-results/project-dark.png", fullPage: true });
  for (const label of ["资料与风格", "任务", "结果"]) {
    await page.getByRole("navigation").getByRole("button", { name: new RegExp(`^${label}`) }).click();
    await expect(page.locator("main")).toBeVisible();
  }
  await page.getByRole("button", { name: "打开设置" }).click();
  await expect(page.getByRole("heading", { name: "本地、可观测、可回退。" })).toBeVisible();
  await page.screenshot({ path: "test-results/settings-dark.png", fullPage: true });
  expect(errors).toEqual([]);
});

test("theme and mobile layout remain usable", async ({ page }) => {
  const initialTheme = await page.locator("html").getAttribute("data-theme");
  const nextTheme = initialTheme === "dark" ? "light" : "dark";
  await page.getByRole("main").getByRole("button", { name: nextTheme === "light" ? "切换到浅色主题" : "切换到深色主题" }).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", nextTheme);
  await page.screenshot({ path: `test-results/project-${nextTheme}.png`, fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await expect(page.getByRole("navigation")).toBeVisible();
  await page.screenshot({ path: "test-results/project-mobile.png", fullPage: true });
});

test("result export is reachable after review", async ({ page }) => {
  await page.getByRole("navigation").getByRole("button", { name: /^结果/ }).click();
  await expect(page.getByRole("heading", { name: "内容通过后才允许导出。" })).toBeVisible();
  await page.getByRole("button", { name: /导出 SVG/ }).click();
  await expect(page.getByText(/已准备 ArtifactSet/)).toBeVisible();
  await page.screenshot({ path: "test-results/result-export.png", fullPage: true });
});
