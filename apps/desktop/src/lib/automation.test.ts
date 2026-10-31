import { createHash, webcrypto } from "node:crypto";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createAutomationService, getAutomationAccess, importAutomationImage, pairAutomation, type AutomationRun } from "./automation";

beforeEach(() => vi.stubGlobal("crypto", webcrypto));
afterEach(() => vi.unstubAllGlobals());
const sha = createHash("sha256").update(new Uint8Array([1])).digest("hex");
const run = { run_id: "run-1", task_id: "task-1", base_version_id: "version-1", prompt_sha256: "p", phase: "awaiting_import", sha256: sha, mime_type: "image/png" } as AutomationRun;
it("does not acknowledge a failed project import", async () => {
  const ack = vi.fn();
  const service = { artifact: vi.fn().mockResolvedValue({ bytes: new Uint8Array([1]), sha256: sha }), ack };
  const call = vi.fn().mockResolvedValueOnce({ project: { generated_images: [] } }).mockRejectedValueOnce(new Error("quota exceeded"));
  await expect(importAutomationImage(service as never, run, { call } as never)).rejects.toThrow("quota exceeded");
  expect(ack).not.toHaveBeenCalled();
});
it("acknowledges only a matching image in the committed snapshot", async () => {
  const ack = vi.fn().mockResolvedValue(undefined);
  const image = { id: "image-1", run_id: "run-1", task_id: "task-1", base_version_id: "version-1", prompt_sha256: "p", sha256: sha };
  const service = { artifact: vi.fn(), ack };
  const call = vi.fn().mockResolvedValueOnce({ project: { generated_images: [image] } }).mockResolvedValueOnce([1]);
  await importAutomationImage(service as never, run, { call } as never);
  expect(ack).toHaveBeenCalledWith("run-1", { sha256: sha, image_id: "image-1" });
  expect(service.artifact).not.toHaveBeenCalled();
  expect(call).toHaveBeenNthCalledWith(2, "generated_image_bytes", { taskId: "task-1", imageId: "image-1" });
});
it("does not acknowledge metadata when the retained image bytes are corrupt", async () => {
  const image = { id: "image-1", run_id: run.run_id, task_id: run.task_id, base_version_id: run.base_version_id, prompt_sha256: run.prompt_sha256, sha256: sha };
  const ack = vi.fn();
  const call = vi.fn().mockResolvedValueOnce({ project: { generated_images: [image] } }).mockRejectedValueOnce(new Error("retained image size changed"));
  await expect(importAutomationImage({ ack } as never, run, { call } as never)).rejects.toThrow("retained image size changed");
  expect(ack).not.toHaveBeenCalled();
});

it("rejects pairing messages from the wrong origin or window", async () => {
  localStorage.clear();
  const frame = document.createElement("iframe"); document.body.append(frame);
  const popup = frame.contentWindow!;
  const opened = vi.spyOn(window, "open").mockReturnValue(popup);
  vi.spyOn(popup, "close").mockImplementation(() => {});
  try {
    const paired = pairAutomation();
    const data = { type: "entwurf:automation-paired", token: "fixture", base_url: "http://127.0.0.1:48123" };
    window.dispatchEvent(new MessageEvent("message", { origin: "https://other.example", source: popup, data }));
    window.dispatchEvent(new MessageEvent("message", { origin: "http://127.0.0.1:48123", source: window, data }));
    expect(localStorage.getItem("entwurf.automation-access.v1")).toBeNull();
    window.dispatchEvent(new MessageEvent("message", { origin: "http://127.0.0.1:48123", source: popup, data }));
    await expect(paired).resolves.toEqual({ token: "fixture", base_url: "http://127.0.0.1:48123" });
  } finally { opened.mockRestore(); frame.remove(); localStorage.clear(); }
});

it("cancels observation without submitting another run", async () => {
  const fetcher = vi.fn().mockResolvedValue(new Response(JSON.stringify({ ...run, phase: "cancelled" })));
  vi.stubGlobal("fetch", fetcher);
  try {
    const service = createAutomationService({ token: "fixture", base_url: "http://127.0.0.1:48123" });
    await service.cancel("run-1");
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(fetcher.mock.calls[0][0]).toBe("http://127.0.0.1:48123/api/automation/runs/run-1/cancel");
    expect(fetcher.mock.calls[0][1].method).toBe("POST");
  } finally { vi.unstubAllGlobals(); }
});

it.each(["http://127.0.0.1:49124", "http://localhost:49124/", "http://[::1]:49124"])("uses the validated local HTTP service address %s", async base_url => {
  const fetcher = vi.fn().mockResolvedValue(new Response("{}"));
  vi.stubGlobal("fetch", fetcher);
  const access = await getAutomationAccess({ runtime: "local-http", call: vi.fn().mockResolvedValue({ token: "fixture", base_url }) } as never);
  await createAutomationService(access!).status();
  expect(fetcher.mock.calls[0][0]).toBe(`${new URL(base_url).origin}/api/automation/status`);
});

it.each([
  "https://127.0.0.1:48123", "http://other.example:48123", "http://127.0.0.2:48123", "http://localhost.example:48123",
  "http://user@127.0.0.1:48123", "http://user:password@localhost:48123", "http://127.0.0.1:48123/other",
  "http://127.0.0.1:48123/other/..", "http://127.0.0.1:48123\\other\\..",
  "http://127.0.0.1:48123/?token=other", "http://127.0.0.1:48123/?", "http://127.0.0.1:48123/#other", "http://127.0.0.1:48123/#", "invalid",
])("rejects automation addresses with a nonlocal origin or URL components: %s", base_url => {
  expect(() => createAutomationService({ token: "fixture", base_url })).toThrow("无效的服务地址");
});

it("keeps native access and stored browser pairing on the default companion", async () => {
  const access = { token: "fixture", base_url: "http://127.0.0.1:49124" };
  await expect(getAutomationAccess({ runtime: "native", call: vi.fn().mockResolvedValue(access) } as never)).rejects.toThrow("无效的服务地址");
  localStorage.setItem("entwurf.automation-access.v1", JSON.stringify(access));
  expect(await getAutomationAccess({ runtime: "browser" } as never)).toBeNull();
  expect(localStorage.getItem("entwurf.automation-access.v1")).toBeNull();
});
