import { expect, it, vi } from "vitest";
import { createAutomationService, importAutomationImage, pairAutomation, type AutomationRun } from "./automation";

const run = { run_id: "run-1", task_id: "task-1", base_version_id: "version-1", prompt_sha256: "p", phase: "awaiting_import", sha256: "hash", mime_type: "image/png" } as AutomationRun;
it("does not acknowledge a failed project import", async () => {
  const ack = vi.fn();
  const service = { artifact: vi.fn().mockResolvedValue({ bytes: new Uint8Array([1]), sha256: "hash" }), ack };
  const call = vi.fn().mockResolvedValueOnce({ project: { generated_images: [] } }).mockRejectedValueOnce(new Error("quota exceeded"));
  await expect(importAutomationImage(service as never, run, { call } as never)).rejects.toThrow("quota exceeded");
  expect(ack).not.toHaveBeenCalled();
});
it("acknowledges only a matching image in the committed snapshot", async () => {
  const ack = vi.fn().mockResolvedValue(undefined);
  const image = { id: "image-1", run_id: "run-1", task_id: "task-1", base_version_id: "version-1", prompt_sha256: "p", sha256: "hash" };
  const service = { artifact: vi.fn(), ack };
  const call = vi.fn().mockResolvedValue({ project: { generated_images: [image] } });
  await importAutomationImage(service as never, run, { call } as never);
  expect(ack).toHaveBeenCalledWith("run-1", { sha256: "hash", image_id: "image-1" });
  expect(service.artifact).not.toHaveBeenCalled();
  expect(call).toHaveBeenCalledTimes(1);
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
