import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createHash, webcrypto } from "node:crypto";
import { AutomationPanel } from "./AutomationPanel";
import { fixture } from "../../tests/protocol-fixture";
import type { AutomationRun } from "../lib/automation";

const bytes = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10, 1]);
const sha = createHash("sha256").update(bytes).digest("hex");
const base: AutomationRun = { run_id: "retained-run", task_id: "task-1", base_version_id: "older-version", prompt_sha256: "prompt-hash", phase: "awaiting_import", sha256: sha, mime_type: "image/png" };
beforeEach(() => {
  localStorage.clear(); sessionStorage.clear(); vi.stubGlobal("crypto", webcrypto);
  vi.stubGlobal("URL", Object.assign(URL, { createObjectURL: vi.fn(() => "blob:retained"), revokeObjectURL: vi.fn() }));
});
afterEach(() => vi.unstubAllGlobals());
function setup(item = base, mime = "image/png", responseSha = sha, body = bytes) {
  const requests: { url: string; method: string; authorization: string }[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, options: RequestInit) => {
    requests.push({ url, method: options.method ?? "GET", authorization: (options.headers as any).Authorization });
    if (url.endsWith("/artifact")) return new Response(body, { headers: { "Content-Type": mime, "X-Entwurf-Sha256": responseSha } });
    return new Response(JSON.stringify(url.endsWith("/status") ? { connected: true, available: true, reason: "ready" } : [item]));
  }));
  const client = { runtime: "native", available: true, call: vi.fn().mockResolvedValue({ token: "private-fixture", base_url: "http://127.0.0.1:48123" }) } as any;
  const run = vi.fn();
  const view = render(<AutomationPanel snapshot={fixture()} client={client} run={run} locked={false} />);
  return { ...view, requests, client, run };
}
it.each(["awaiting_import", "cancelled"] as const)("reviews %s originals without import, ACK, send, or background download", async phase => {
  const { requests, client, run, unmount } = setup({ ...base, phase });
  const preview = await screen.findByRole("button", { name: "查看原始图片" });
  expect(screen.queryByRole("button", { name: "继续导入图片" })).not.toBeInTheDocument();
  expect(requests.filter(r => r.url.endsWith("/artifact"))).toHaveLength(0);
  fireEvent.click(preview);
  expect(await screen.findByRole("img", { name: /原始生成图片/ })).toHaveAttribute("src", "blob:retained");
  expect(screen.getByRole("link", { name: "下载原始图片" })).toHaveAttribute("download", "retained-run.png");
  expect(screen.getByText(`任务：task-1 · SHA-256：${sha}`)).toBeVisible();
  const initial = requests.length;
  await waitFor(() => expect(requests.length).toBeGreaterThan(initial), { timeout: 3500 });
  expect(requests.filter(r => r.url.endsWith("/artifact"))).toHaveLength(1);
  expect(requests.every(r => r.method === "GET" && r.authorization === "Bearer private-fixture")).toBe(true);
  expect(client.call.mock.calls.map((call: any[]) => call[0])).toEqual(["automation_access"]);
  expect(run).not.toHaveBeenCalled();
  unmount(); expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:retained");
});
it.each([
  ["run hash", { ...base, sha256: "0".repeat(64) }, "image/png", sha, bytes],
  ["response hash", base, "image/png", "0".repeat(64), bytes],
  ["MIME mismatch", base, "image/jpeg", sha, bytes],
  ["unsafe MIME", { ...base, mime_type: "image/svg+xml" }, "image/svg+xml", sha, bytes],
] as const)("refuses preview on %s without creating a download", async (_, item, mime, responseSha, body) => {
  const { run } = setup(item, mime, responseSha, body);
  fireEvent.click(await screen.findByRole("button", { name: "查看原始图片" }));
  await screen.findByRole("alert");
  expect(screen.queryByRole("img")).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: "下载原始图片" })).not.toBeInTheDocument();
  expect(URL.createObjectURL).not.toHaveBeenCalled(); expect(run).not.toHaveBeenCalled();
});
