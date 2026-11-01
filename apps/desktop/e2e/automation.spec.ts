import { expect, test } from "@playwright/test";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

for (const scenario of ["success", "corrupt", "stale", "reject", "paused", "unknown"] as const) test(`automation recovery ${scenario}: real persistence and bounded retries`, async ({ page }) => {
  const pageErrors: string[] = [];
  if (scenario === "stale") {
    page.on("pageerror", error => pageErrors.push(error.message));
    page.on("console", message => { if (message.type() === "error") pageErrors.push(message.text()); });
  }
  const bytes = await readFile(fileURLToPath(new URL("../../../fixtures/public/ingestion/bilingual.png", import.meta.url)));
  const sha = createHash("sha256").update(bytes).digest("hex");
  let starts = 0, acks = 0, ready = false, storedBeforeAck = false, artifactRequests = 0, lists = 0;
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
      if(scenario === "reject") return route.fulfill({status:409,headers,contentType:"application/json",body:JSON.stringify({error:"run_active"})});
      job = { ...body, prompt_sha256: createHash("sha256").update(body.prompt).digest("hex"), phase: scenario === "paused" ? "paused" : "generating", reason: "本地测试等待" };
      if(scenario === "unknown") return route.abort("failed");
      return json(job);
    }
    if (url.pathname.endsWith("/runs")) {
      lists++;
      if (job && ready && job.phase !== "completed" && job.phase !== "cancelled") job = { ...job, phase: "awaiting_import", sha256: sha, mime_type: "image/png" };
      return json(job ? [job] : []);
    }
    if (url.pathname.endsWith("/artifact")) { artifactRequests++; return route.fulfill({ status: 200, headers: { ...headers, "X-Entwurf-Sha256": sha }, contentType: "image/png", body: bytes }); }
    if (url.pathname.endsWith("/cancel")) { job = { ...job, phase: "cancelled" }; return json(job); }
    if (url.pathname.endsWith("/ack")) {
      storedBeforeAck = await page.evaluate(async expected => {
        const db = await new Promise<IDBDatabase>((resolve, reject) => { const request = indexedDB.open("entwurf.workspace.v1"); request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
        return new Promise<boolean>(resolve => {
          const request = db.transaction("files", "readonly").objectStore("files").openCursor();
          request.onsuccess = () => { const cursor = request.result; if (!cursor) { db.close(); resolve(false); return; } if (String(cursor.key).endsWith("/project.json")) { const project = JSON.parse(new TextDecoder().decode(cursor.value)); if (project.generated_images?.some((image: { sha256: string }) => image.sha256 === expected)) { db.close(); resolve(true); return; } } cursor.continue(); };
        });
      }, sha);
      acks++; if (scenario === "corrupt") return route.fulfill({status:503,headers,contentType:"application/json",body:JSON.stringify({error:"fixture acknowledgment interrupted"})});
      job = { ...job, phase: "completed" }; return json(job);
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
  if (scenario === "reject") {
    await expect(page.getByRole("alert")).toContainText("本次提交已被拒绝");
    const before=lists; await expect.poll(()=>lists,{timeout:10_000}).toBeGreaterThanOrEqual(before+3);
    await expect(page.getByText("等待执行",{exact:true})).toHaveCount(0);
    await expect(page.getByRole("button",{name:"启动一次图片生成"})).toBeEnabled(); expect(starts).toBe(1); return;
  }
  if (scenario === "paused") {
    await expect(page.getByText("观察已暂停",{exact:true})).toBeVisible();
    await expect(page.getByRole("button",{name:"启动一次图片生成"})).toBeDisabled();
    await page.getByRole("button",{name:"停止观察"}).click();
    await expect(page.getByText("观察已停止",{exact:true})).toBeVisible();
    await expect(page.getByRole("button",{name:"启动一次图片生成"})).toBeEnabled(); expect(starts).toBe(1); return;
  }
  await expect(page.getByText("等待生成", { exact: true })).toBeVisible(); expect(starts).toBe(1);
  await page.reload(); await tasks(); await expect(page.getByText("等待生成", { exact: true })).toBeVisible(); expect(starts).toBe(1);
  if (scenario === "stale") {
    await page.getByRole("navigation").getByRole("button", { name: /^结果/ }).click();
    await page.getByLabel("节点标题").fill("Newer knowledge version");
    await page.getByRole("button", {name: "保存节点修改"}).click();
    await expect(page.getByText("保存节点完成", {exact: true})).toBeVisible();
    await tasks(); ready = true;
    await expect(page.getByRole("button", {name: "查看原始图片"})).toBeVisible();
    await expect(page.getByRole("button", {name: "继续导入图片"})).toHaveCount(0);
    const persistedFiles = () => page.evaluate(async () => {
      const db = await new Promise<IDBDatabase>(resolve => {const req=indexedDB.open("entwurf.workspace.v1");req.onsuccess=()=>resolve(req.result);});
      return new Promise<Record<string, number[]>>(resolve => {
        const files: Record<string, number[]> = {};
        const req = db.transaction("files").objectStore("files").openCursor();
        req.onsuccess = () => { const entry = req.result; if (!entry) {db.close();resolve(files);return;} files[String(entry.key)] = Array.from(entry.value as Uint8Array); entry.continue(); };
      });
    });
    const before = await persistedFiles(); expect(artifactRequests).toBe(0);
    const review = async () => {
      await page.getByRole("button", {name: "查看原始图片"}).click();
      const preview = page.getByRole("img", {name: /原始生成图片/});
      await expect(preview).toBeVisible();
      await expect.poll(() => preview.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBeGreaterThan(0);
      await expect(page.getByText(`任务：${job!.task_id} · SHA-256：${sha}`, {exact:true})).toBeVisible();
      await expect(page.getByText(`运行：${job!.run_id} · 基础版本：${job!.base_version_id}`, {exact:true})).toBeVisible();
      const download = page.waitForEvent("download");
      await page.getByRole("link", {name: "下载原始图片"}).click();
      expect(await readFile((await (await download).path())!)).toEqual(bytes);
    };
    await review();
    const initialLists = lists;
    await expect.poll(() => lists, {timeout: 10_000}).toBeGreaterThanOrEqual(initialLists + 2);
    expect(artifactRequests).toBe(1);
    await page.reload(); await tasks(); await review();
    expect(artifactRequests).toBe(2);
    await page.getByRole("button", {name:"停止观察"}).click();
    await expect(page.getByText("观察已停止",{exact:true})).toBeVisible();
    await expect(page.getByRole("link", {name:"下载原始图片"})).toBeVisible();
    expect(artifactRequests).toBe(2);
    await page.reload(); await tasks(); await review();
    expect(artifactRequests).toBe(3);
    expect(await persistedFiles()).toEqual(before);
    expect(acks).toBe(0); expect(starts).toBe(1);
    await expect(page.getByRole("button", {name:"继续导入图片"})).toHaveCount(0);
    await page.getByLabel("图片生成提示").fill("another explicit prompt");
    await expect(page.getByRole("button",{name:"启动一次图片生成"})).toBeEnabled();
    expect(page.url()).toContain("http://127.0.0.1:");
    await expect(page).toHaveTitle(/Entwurf/);
    await expect(page.locator("vite-error-overlay")).toHaveCount(0);
    await page.getByRole("img", {name:/原始生成图片/}).scrollIntoViewIfNeeded();
    await page.screenshot({ path: "/tmp/entwurf-retained-original-desktop.png" });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.getByRole("button", {name:"关闭任务侧栏"}).click();
    await page.getByRole("img", {name:/原始生成图片/}).scrollIntoViewIfNeeded();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth + 1)).toBe(true);
    await page.screenshot({ path: "/tmp/entwurf-retained-original-mobile.png" });
    expect(pageErrors).toEqual([]);
    return;
  }
  ready = true;
  await expect(page.getByRole("heading", { name: "生成图片结果" })).toBeVisible();
  await expect(page.getByRole("img", { name: /ChatGPT 生成图片/ })).toBeVisible();
  expect(acks).toBe(1); expect(storedBeforeAck).toBe(true);
  if (scenario === "corrupt") {
    const initialLists=lists;
    await expect.poll(()=>lists,{timeout:10_000}).toBeGreaterThanOrEqual(initialLists+3);
    expect(artifactRequests).toBe(1); expect(acks).toBe(1);
    await page.evaluate(async()=>{
      const db=await new Promise<IDBDatabase>(resolve=>{const req=indexedDB.open("entwurf.workspace.v1");req.onsuccess=()=>resolve(req.result);});
      await new Promise<void>((resolve,reject)=>{
        const tx=db.transaction(["files","meta"],"readwrite"); const cursor=tx.objectStore("files").openCursor();
        cursor.onsuccess=()=>{const entry=cursor.result;if(entry){if(String(entry.key).includes("/generated/"))entry.update(new Uint8Array([0]));entry.continue();}};
        const meta=tx.objectStore("meta").get("state");meta.onsuccess=()=>tx.objectStore("meta").put({...meta.result,revision:meta.result.revision+1},"state");
        tx.oncomplete=()=>{db.close();resolve();};tx.onabort=()=>reject(tx.error);
      });
    });
    await page.getByRole("button",{name:"继续导入图片"}).click();
    await expect(page.getByRole("alert").filter({hasText:/retained image|image.*changed/i})).toBeVisible();
    expect(acks).toBe(1); expect(artifactRequests).toBe(1);
    return;
  }
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
