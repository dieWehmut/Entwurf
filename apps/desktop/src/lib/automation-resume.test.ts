import { webcrypto, createHash } from "node:crypto";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createAutomationService, resumeUnsubmittedRun, resumePending, reconcileResumeIntent, type AutomationRun } from "./automation";

const prompt = "A public probability diagram.";
const item: AutomationRun = { run_id: "run-resume", task_id: "task-1", base_version_id: "version-1", prompt_sha256: createHash("sha256").update(prompt).digest("hex"), phase: "paused", pre_submit_retryable: true };
const service = () => createAutomationService({ token: "fixture", base_url: "http://127.0.0.1:48123" });
beforeEach(() => { localStorage.clear(); vi.stubGlobal("crypto", webcrypto); });
afterEach(() => vi.unstubAllGlobals());

it("persists a recovery ID before sending exactly the original identity and prompt", async () => {
  const fetcher = vi.fn(async (_url, options) => {
    const body = JSON.parse(options.body);
    expect(resumePending(item)).toBe(true);
    expect(body).toMatchObject({ task_id: item.task_id, base_version_id: item.base_version_id, prompt });
    expect(body.request_id).toMatch(/^[0-9a-f-]{36}$/);
    return new Response(JSON.stringify({ ...item, resume_request_id: body.request_id, phase: "queued", pre_submit_retryable: false }));
  });
  vi.stubGlobal("fetch", fetcher);
  const result = await resumeUnsubmittedRun(service(), item, prompt);
  expect(result.run_id).toBe(item.run_id);
  expect(fetcher).toHaveBeenCalledTimes(1);
  expect(fetcher.mock.calls[0][0]).toContain(`/runs/${item.run_id}/resume-pre-submit`);
  expect(resumePending(item)).toBe(false);
});

it("retains uncertain recovery across a recreated client and only reconciles the same accepted ID", async () => {
  const fetcher = vi.fn().mockRejectedValue(new TypeError("connection interrupted"));
  vi.stubGlobal("fetch", fetcher);
  await expect(resumeUnsubmittedRun(service(), item, prompt)).rejects.toThrow("connection interrupted");
  expect(resumePending(item)).toBe(true);
  await expect(resumeUnsubmittedRun(service(), item, prompt)).rejects.toThrow("继续查询");
  expect(fetcher).toHaveBeenCalledTimes(1);
  reconcileResumeIntent({ ...item, resume_request_id: "another-request" });
  expect(resumePending(item)).toBe(true);
  const sent = JSON.parse(fetcher.mock.calls[0][1].body);
  reconcileResumeIntent({ ...item, resume_request_id: sent.request_id });
  expect(resumePending(item)).toBe(false);
});

it("never sends a changed prompt or a run without authoritative eligibility", async () => {
  const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
  await expect(resumeUnsubmittedRun(service(), item, "changed")).rejects.toThrow("原提示");
  await expect(resumeUnsubmittedRun(service(), { ...item, pre_submit_retryable: false }, prompt)).rejects.toThrow("尚未确认");
  await expect(resumeUnsubmittedRun(service(), { ...item, phase: "generating" }, prompt)).rejects.toThrow("尚未确认");
  expect(fetcher).not.toHaveBeenCalled();
});

it("fails closed if a recovery intent cannot be saved", async () => {
  const fetcher = vi.fn(); vi.stubGlobal("fetch", fetcher);
  const save = vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("quota"); });
  try { await expect(resumeUnsubmittedRun(service(), item, prompt)).rejects.toThrow("保存恢复请求"); }
  finally { save.mockRestore(); }
  expect(fetcher).not.toHaveBeenCalled();
});
