import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createHash, webcrypto } from "node:crypto";
import { AutomationPanel } from "./AutomationPanel";
import { fixture } from "../../tests/protocol-fixture";
import { createAutomationService, importAutomationImage, type AutomationRun } from "../lib/automation";
import type { WorkspaceClient } from "../lib/workspace";
import type { Run } from "../App";

const bytes = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 1]);
const sha = createHash("sha256").update(bytes).digest("hex");
const original: AutomationRun = { run_id: "saved-run", task_id: "task-1", base_version_id: "version-1", prompt_sha256: "a".repeat(64), phase: "awaiting_import", sha256: sha, mime_type: "image/png" };
const access = { token: "fixture-only", base_url: "http://127.0.0.1:48123" };
beforeEach(() => { localStorage.clear(); sessionStorage.clear(); vi.stubGlobal("crypto", webcrypto); });
afterEach(() => vi.unstubAllGlobals());

async function savedButUnacknowledged() {
  const snapshot = fixture();
  const item = { ...original };
  let ackAttempts = 0;
  let retained = bytes;
  const requests: { url: string; method: string; body?: string }[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, options: RequestInit) => {
    requests.push({ url, method: options.method ?? "GET", body: options.body as string | undefined });
    if (url.endsWith("/artifact")) return new Response(bytes, { headers: { "Content-Type": "image/png", "X-Entwurf-Sha256": sha } });
    if (url.endsWith("/ack")) {
      if (++ackAttempts === 1) return new Response(JSON.stringify({ error: "ACK transport interrupted" }), { status: 503 });
      item.phase = "completed";
      return new Response(JSON.stringify(item));
    }
    return new Response(JSON.stringify(url.endsWith("/status") ? { connected: true, available: true, reason: "ready" } : [item]));
  }));
  const call = vi.fn(async (name: string) => {
    if (name === "automation_access") return access;
    if (name === "workspace_snapshot") return structuredClone(snapshot);
    if (name === "import_generated_image") {
      snapshot.project.generated_images = [{
        id: "saved-image", run_id: original.run_id, task_id: original.task_id, base_version_id: original.base_version_id,
        prompt_sha256: original.prompt_sha256, sha256: sha, mime_type: "image/png", stored_path: "generated/saved-image.png",
        byte_len: bytes.length, width: 1, height: 1, created_at: "2026-11-01T00:00:00Z", origin: "chatgpt_ui",
      }];
      return structuredClone(snapshot);
    }
    if (name === "generated_image_bytes") return Array.from(retained);
    throw new Error(`Unexpected workspace call: ${name}`);
  });
  const client = { runtime: "native", available: true, call } as unknown as WorkspaceClient;
  await expect(importAutomationImage(createAutomationService(access), item, client)).rejects.toThrow("ACK transport interrupted");
  expect(snapshot.project.generated_images).toHaveLength(1); expect(ackAttempts).toBe(1);
  snapshot.current_version = structuredClone(snapshot.current_version!);
  snapshot.current_version.id = "version-2"; snapshot.current_version.revision = 2;
  snapshot.current_version.ir.nodes[0].title = "User's newer knowledge";
  snapshot.project.tasks[0].versions.push(snapshot.current_version);
  snapshot.project.tasks[0].current_version = "version-2";
  call.mockClear(); requests.length = 0;
  const failures: string[] = [];
  const run: Run = async (_label, action) => {
    try { await action(); return true; }
    catch (error) { failures.push(String(error)); return false; }
  };
  return { snapshot, item, requests, call, failures, client, run, corrupt: () => { retained = new Uint8Array([0]); } };
}

it("explicitly confirms previously saved bytes after a failed ACK and a newer knowledge version", async () => {
  const state = await savedButUnacknowledged(); const before = structuredClone(state.snapshot);
  render(<AutomationPanel snapshot={state.snapshot} client={state.client} run={state.run} locked={false} />);
  const confirm = await screen.findByRole("button", { name: "确认已保存图片" });
  expect(screen.queryByRole("button", { name: "继续导入图片" })).not.toBeInTheDocument();
  expect(state.requests.every(request => request.method === "GET" && !request.url.endsWith("/artifact"))).toBe(true);
  fireEvent.click(confirm);
  await waitFor(() => expect(state.requests.filter(request => request.url.endsWith("/ack"))).toHaveLength(1));
  expect(state.requests.filter(request => request.method !== "GET")).toEqual([{
    url: `${access.base_url}/api/automation/runs/saved-run/ack`, method: "POST", body: JSON.stringify({sha256:sha, image_id:"saved-image"}),
  }]);
  expect(state.requests.some(request => request.url.endsWith("/artifact"))).toBe(false);
  expect(state.call.mock.calls.map(([name]) => name)).toEqual(["automation_access", "workspace_snapshot", "generated_image_bytes"]);
  expect(state.call).toHaveBeenLastCalledWith("generated_image_bytes", {taskId:"task-1", imageId:"saved-image"});
  expect(state.snapshot).toEqual(before); expect(state.failures).toEqual([]);
});

it("refuses confirmation when the already saved bytes changed", async () => {
  const state = await savedButUnacknowledged(); state.corrupt();
  render(<AutomationPanel snapshot={state.snapshot} client={state.client} run={state.run} locked={false} />);
  fireEvent.click(await screen.findByRole("button", { name: "确认已保存图片" }));
  await waitFor(() => expect(state.failures).toHaveLength(1));
  expect(state.failures[0]).toContain("字节校验失败");
  expect(state.requests.some(request => request.method !== "GET" || request.url.endsWith("/artifact"))).toBe(false);
});

it.each(["run_id", "task_id", "base_version_id", "prompt_sha256", "sha256", "mime_type"] as const)("does not offer stale confirmation for mismatched stored %s", async field => {
  const state = await savedButUnacknowledged();
  if (field === "mime_type") state.snapshot.project.generated_images![0].mime_type = "image/jpeg";
  else state.snapshot.project.generated_images![0][field] = "different";
  render(<AutomationPanel snapshot={state.snapshot} client={state.client} run={state.run} locked={false} />);
  await screen.findByRole("button", { name: "查看原始图片" });
  expect(screen.queryByRole("button", { name: "确认已保存图片" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "继续导入图片" })).not.toBeInTheDocument();
  expect(state.requests.every(request => request.method === "GET")).toBe(true);
});

it("keeps a cancelled saved result read-only", async () => {
  const state = await savedButUnacknowledged(); state.item.phase = "cancelled";
  render(<AutomationPanel snapshot={state.snapshot} client={state.client} run={state.run} locked={false} />);
  await screen.findByRole("button", { name: "查看原始图片" });
  expect(screen.queryByRole("button", { name: "确认已保存图片" })).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "继续导入图片" })).not.toBeInTheDocument();
  expect(state.requests.every(request => request.method === "GET")).toBe(true);
});
