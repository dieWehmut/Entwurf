import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

test("automation submits once, resumes after refresh, and acknowledges a real persisted image", async ({ page }) => {
  const bytes = await readFile(fileURLToPath(new URL("../../../fixtures/public/ingestion/bilingual.png", import.meta.url)));
  const sha = createHash("sha256").update(bytes).digest("hex");
  let starts = 0, acks = 0, ready = false, storedBeforeAck = false;
  let job: Record<string, unknown> | undefined;
  await page.addInitScript(() => localStorage.setItem("entwurf.automation-access.v1", JSON.stringify({ token: "fixture-only", base_url: "http://127.0.0.1:48123" })));
  await page.route("http://127.0.0.1:48123/api/automation/**", async route => {
    const url = new URL(route.request().url());
    const headers = { "Access-Control-Allow-Origin": "*", "Access-Control-Allow-Headers": "Authorization,Content-Type", "Access-Control-Expose-Headers": "X-Entwurf-Sha256" };
    if (route.request().method() === "OPTIONS") return route.fulfill({ status: 204, headers });
    const json = (value: unknown) => route.fulfill({ status: 200, headers, contentType: "application/json", body: JSON.stringify(value) });
    if (url.pathname.endsWith("/status")) return json({ connected: true, available: true, reason: "本地验收服务" });
    if (url.pathname.endsWith("/runs") && route.request().method() === "POST") {
      starts++; const body = route.request().postDataJSON();
      job = { ...body, prompt_sha256: createHash("sha256").update(body.prompt).digest("hex"), phase: "generating", reason: "本地测试等待" };
      return json(job);
    }
    if (url.pathname.endsWith("/runs")) {
      if (job && ready && job.phase !== "completed") job = { ...job, phase: "awaiting_import", sha256: sha, mime_type: "image/png" };
      return json(job ? [job] : []);
    }
    if (url.pathname.endsWith("/artifact")) return route.fulfill({ status: 200, headers: { ...headers, "X-Entwurf-Sha256": sha }, contentType: "image/png", body: bytes });
    if (url.pathname.endsWith("/ack")) {
      storedBeforeAck = await page.evaluate(async expected => {
        const db = await new Promise<IDBDatabase>((resolve, reject) => { const request = indexedDB.open("entwurf.workspace.v1"); request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
        return new Promise<boolean>(resolve => {
          const request = db.transaction("files", "readonly").objectStore("files").openCursor();
          request.onsuccess = () => { const cursor = request.result; if (!cursor) { db.close(); resolve(false); return; } if (String(cursor.key).endsWith("/project.json")) { const project = JSON.parse(new TextDecoder().decode(cursor.value)); if (project.generated_images?.some((image: { sha256: string }) => image.sha256 === expected)) { db.close(); resolve(true); return; } } cursor.continue(); };
        });
      }, sha);
      acks++; job = { ...job, phase: "completed" }; return json(job);
    }
    return route.fulfill({ status: 404, headers });
  });
  await page.goto("./");
  await page.getByLabel("项目名称").fill("图片自动化验收"); await page.getByRole("button", { name: "新建项目", exact: true }).click();
  await page.getByLabel("任务标题").fill("图片任务"); await page.getByRole("button", { name: "创建任务", exact: true }).click();
  const chooser = page.waitForEvent("filechooser"); await page.getByRole("button", { name: "选择本机文件并导入" }).click();
  await (await chooser).setFiles({ name: "image.md", mimeType: "text/markdown", buffer: Buffer.from("# Probability\n\nProbability measures uncertainty.\n") });
  await expect(page.getByText("image.md", { exact: true })).toBeVisible();
  const tasks = () => page.getByRole("navigation").getByRole("button", { name: /^任务/ }).click();
  await tasks(); await page.getByLabel("图片生成提示").fill("Draw a probability study diagram.");
  await expect(page.getByRole("button", { name: "启动一次图片生成" })).toBeEnabled();
  await page.getByRole("button", { name: "启动一次图片生成" }).click();
  await expect(page.getByText("等待生成", { exact: true })).toBeVisible(); expect(starts).toBe(1);
  await page.reload(); await tasks(); await expect(page.getByText("等待生成", { exact: true })).toBeVisible(); expect(starts).toBe(1);
  ready = true; await page.getByRole("button", { name: "继续导入图片" }).click();
  await expect(page.getByRole("heading", { name: "生成图片结果" })).toBeVisible();
  await expect(page.getByRole("img", { name: /ChatGPT 生成图片/ })).toBeVisible();
  expect(acks).toBe(1); expect(storedBeforeAck).toBe(true);
  await page.reload(); await tasks(); await expect(page.getByRole("img", { name: /ChatGPT 生成图片/ })).toBeVisible();
  expect(starts).toBe(1);
  const download = page.waitForEvent("download"); await page.getByRole("button", { name: "下载原始图片" }).click();
  expect(await readFile((await (await download).path())!)).toEqual(bytes);
  await page.screenshot({ path: "/tmp/entwurf-generated-image-result.png", fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.getByRole("heading", { name: "生成图片结果" }).scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
  await page.screenshot({ path: "/tmp/entwurf-generated-image-mobile.png" });
});
