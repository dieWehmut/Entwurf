import { expect, test } from "@playwright/test";
import { createHash, randomUUID } from "node:crypto";
import { access as fileAccess, readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";

test("real HTTP coordinator automatically persists a fixture extension image then acknowledges", async ({ page, baseURL }) => {
  test.skip(new URL(baseURL!).port !== "48123", "Requires the isolated companion on port 48123.");
  const clientId = randomUUID();
  const bootstrap = await fetch(`${baseURL}/api/bootstrap`, { headers: { "X-Entwurf-Client": clientId } });
  expect(bootstrap.ok).toBe(true);
  const { session_token } = await bootstrap.json();
  const pairing = await fetch(`${baseURL}/api/automation/pairing`, { headers: { "X-Entwurf-Session": session_token, "X-Entwurf-Client": clientId } });
  expect(pairing.ok).toBe(true);
  const access = await pairing.json();
  const bytes = await readFile(fileURLToPath(new URL("../../../fixtures/public/ingestion/bilingual.png", import.meta.url)));
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  const socket = new WebSocket(access.websocket_url);
  let starts = 0, acknowledged = false, holdArtifact = false;
  let heldRun: {run_id:string;task_id:string;base_version_id:string} | null = null;
  const uploads: string[] = [];
  page.on("response", async response => {if(new URL(response.url()).pathname === "/api/uploads" && response.request().method() === "POST" && response.ok()) uploads.push(...(await response.json()).map((file:{path:string})=>file.path));});
  const errors: string[] = [];
  try {
    await new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error("Fixture extension authentication timed out")), 10_000);
      socket.onopen = () => socket.send(JSON.stringify({ type: "authenticate", token: access.extension_token }));
      socket.onerror = () => { clearTimeout(timer); reject(new Error("Fixture extension connection failed")); };
      socket.onmessage = event => {
        const message = JSON.parse(String(event.data));
        if (message.type === "error") { errors.push(message.code); clearTimeout(timer); reject(new Error(`Fixture extension: ${message.code}`)); }
        if (message.type === "authenticated") { socket.send(JSON.stringify({ type: "probe", available: true, reason: "fixture_ready" })); clearTimeout(timer); resolve(); }
        if (message.type === "start") {
          starts++; const run = message.run;
          socket.send(JSON.stringify({ type: "status", run_id: run.run_id, phase: "generating", detail: "fixture_generating" }));
          if(holdArtifact) {heldRun=run;return;}
          socket.send(JSON.stringify({ type: "artifact", run_id: run.run_id, task_id: run.task_id, base_version_id: run.base_version_id, mime_type: "image/png", sha256, bytes_base64: bytes.toString("base64") }));
        }
        if (message.type === "artifact_accepted") acknowledged = true;
      };
    });
    await page.goto("/");
    await page.getByLabel("项目名称").fill("真实 HTTP 自动入库"); await page.getByRole("button", { name: "新建项目", exact: true }).click();
    await page.getByLabel("任务标题").fill("本地扩展夹具"); await page.getByRole("button", { name: "创建任务", exact: true }).click();
    const chooser = page.waitForEvent("filechooser"); await page.getByRole("button", { name: "选择本机文件并导入" }).click();
    await (await chooser).setFiles({ name: "fixture.md", mimeType: "text/markdown", buffer: Buffer.from("# Test\n\nA local fixture verifies image storage.\n") });
    await expect(page.getByText("fixture.md", { exact: true })).toBeVisible();
    const tasks = () => page.getByRole("navigation").getByRole("button", { name: /^任务/ }).click();
    await tasks(); await page.getByLabel("图片生成提示").fill("Local fixture only, no ChatGPT request.");
    await expect(page.getByRole("button", { name: "启动一次图片生成" })).toBeEnabled();
    await page.getByRole("button", { name: "启动一次图片生成" }).click();
    await expect(page.getByRole("img", { name: /ChatGPT 生成图片/ })).toBeVisible({ timeout: 20_000 });
    await expect.poll(() => acknowledged).toBe(true);
    await page.reload(); await tasks();
    await expect(page.getByRole("img", { name: /ChatGPT 生成图片/ })).toBeVisible();
    expect(starts).toBe(1); expect(errors).toEqual([]);
    const download = page.waitForEvent("download"); await page.getByRole("button", { name: "下载原始图片" }).click();
    expect(await readFile((await (await download).path())!)).toEqual(bytes);
    holdArtifact=true;
    await page.getByLabel("图片生成提示").fill("Another local fixture for stale rejection.");
    await expect(page.getByRole("button",{name:"启动一次图片生成"})).toBeEnabled();
    await page.getByRole("button",{name:"启动一次图片生成"}).click();
    await expect.poll(()=>heldRun !== null).toBe(true);
    await page.getByRole("navigation").getByRole("button",{name:/^结果/}).click();
    await page.getByLabel("节点标题").fill("Changed while fixture generation waits");
    await page.getByRole("button",{name:"保存节点修改"}).click();
    await expect(page.getByText("保存节点完成",{exact:true})).toBeVisible();
    const alternate = await readFile(fileURLToPath(new URL("../src-tauri/icons/icon.png", import.meta.url)));
    socket.send(JSON.stringify({type:"artifact",...heldRun!,mime_type:"image/png",sha256:createHash("sha256").update(alternate).digest("hex"),bytes_base64:alternate.toString("base64")}));
    await tasks();
    await expect(page.getByRole("button",{name:"继续导入图片"})).toBeVisible();
    for(let attempt=0;attempt<2;attempt++) {
      await page.getByRole("button",{name:"继续导入图片"}).click();
      await expect(page.getByRole("alert").filter({hasText:/stale|version/i})).toBeVisible();
    }
    if(process.env.ENTWURF_HTTP_TEST_DATA_DIR) for(const path of uploads) {
      let exists=true;try{await fileAccess(process.env.ENTWURF_HTTP_TEST_DATA_DIR+path);}catch{exists=false;}
      expect(exists).toBe(false);
    }
    await page.getByRole("button",{name:"停止观察"}).click();
    await expect(page.getByText("观察已停止",{exact:true})).toBeVisible();
    heldRun=null;

  } finally {
    if (heldRun) {
      const result = await fetch(`${baseURL}/api/command`,{method:"POST",headers:{"Content-Type":"application/json","X-Entwurf-Session":session_token,"X-Entwurf-Client":clientId},body:JSON.stringify({command:"automation_access",args:{}})}).then(response=>response.json());
      await fetch(`${baseURL}/api/automation/runs/${heldRun.run_id}/cancel`,{method:"POST",headers:{"Content-Type":"application/json",Authorization:`Bearer ${result.token}`},body:"{}"});
    }
    socket.close();
  }
});
