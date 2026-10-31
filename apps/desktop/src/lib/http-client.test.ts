import { afterEach, expect, it, vi } from "vitest";
import { createHttpClient, restoreHttpSelection } from "./http-client";

afterEach(() => vi.unstubAllGlobals());
it("keeps the session credential in the header and preserves command arguments", async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ root: "/projects/p" }), { headers: { "Content-Type": "application/json" } }));
  vi.stubGlobal("fetch", fetcher);
  const client = createHttpClient({ session_token: "test-secret" } as never);
  await client.call("create_project", { root: "/projects/p", name: "项目" });
  const [url, init] = fetcher.mock.calls[0];
  expect(url).toBe("/api/command");
  expect(init.headers["X-Entwurf-Session"]).toBe("test-secret");
  expect(JSON.parse(init.body)).toEqual({ command: "create_project", args: { root: "/projects/p", name: "项目" } });
  expect(init.body).not.toContain("test-secret");
});
it("reports server rejection without treating it as a saved snapshot", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(new Response(JSON.stringify({ error: "stale version" }), { status: 409 })));
  const client = createHttpClient({ session_token: "secret" } as never);
  await expect(client.call("create_task", { title: "test" })).rejects.toThrow("stale version");
});

it("creates a new document identity even when sessionStorage is cloned", async () => {
  sessionStorage.setItem("entwurf.http-client.v1", "inherited-id");
  const first = (await import("./http-client")).httpClientId();
  vi.resetModules();
  const second = (await import("./http-client")).httpClientId();
  expect(first).not.toBe("inherited-id");
  expect(second).not.toBe(first);
});

it("restores only project selection into the new server session", async () => {
  sessionStorage.setItem("entwurf.http-selection.v1", JSON.stringify({ root: "/projects/p", taskId: "task-a" }));
  const call = vi.fn().mockResolvedValue(null);
  await restoreHttpSelection({ call } as never);
  expect(call.mock.calls).toEqual([["load_existing_project", { root: "/projects/p" }], ["workspace_snapshot", { taskId: "task-a" }]]);
});
