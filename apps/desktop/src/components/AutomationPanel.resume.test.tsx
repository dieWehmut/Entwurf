import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { beforeEach, afterEach, expect, it, vi } from "vitest";
import { createHash, webcrypto } from "node:crypto";
import { AutomationPanel } from "./AutomationPanel";
import { fixture } from "../../tests/protocol-fixture";
import type { AutomationRun } from "../lib/automation";

const prompt = "Public fixture diagram.";
const base: AutomationRun = { run_id: "fixture-resume", task_id: "task-1", base_version_id: "version-1", prompt_sha256: createHash("sha256").update(prompt).digest("hex"), phase: "paused", pre_submit_retryable: true };
beforeEach(() => { localStorage.clear(); vi.stubGlobal("crypto", webcrypto); });
afterEach(() => vi.unstubAllGlobals());
function setup(item: AutomationRun = base, provider = "windows", uncertain = false, globallyBusy = false) {
  const posts: { url: string; body: any }[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, options: RequestInit) => {
    if (options.method === "POST") {
      const body = JSON.parse(options.body as string); posts.push({ url, body });
      if (uncertain) throw new TypeError("fixture response lost");
      if (url.endsWith("/runs")) return new Response(JSON.stringify({ ...item, ...body, phase: "queued", pre_submit_retryable: false, pre_submit_parked: false }));
      return new Response(JSON.stringify({ ...item, phase: "queued", pre_submit_retryable: false, resume_request_id: body.request_id }));
    }
    return new Response(JSON.stringify(url.endsWith("/status") ? { connected: true, available: true, reason: "ready", provider, busy: globallyBusy } : [item]));
  }));
  const client = { runtime: "native", available: true, call: vi.fn().mockResolvedValue({ token: "fixture", base_url: "http://127.0.0.1:48123" }) } as any;
  const view = render(<AutomationPanel snapshot={fixture()} client={client} run={vi.fn()} locked={false} />);
  return { ...view, posts, client };
}

it("recovers only through an explicit click with the original prompt and keeps Start disabled", async () => {
  const { posts } = setup();
  const button = await screen.findByRole("button", { name: "继续未发送的任务" });
  expect(button).toBeDisabled();
  expect(screen.getByLabelText("图片生成提示")).toBeEnabled();
  fireEvent.change(screen.getByLabelText("图片生成提示"), { target: { value: prompt } });
  fireEvent.click(button);
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0].url).toContain("/runs/fixture-resume/resume-pre-submit");
  expect(posts[0].body.prompt).toBe(prompt);
  expect(screen.getByRole("button", { name: "启动一次图片生成" })).toBeDisabled();
});

it.each([
  [{ ...base, pre_submit_retryable: undefined }, "windows"],
  [{ ...base, base_version_id: "older-version" }, "windows"],
  [base, "browser"],
] as const)("does not offer recovery without native evidence and current task/version", async (item, provider) => {
  setup(item, provider);
  await screen.findByText("观察已暂停");
  expect(screen.queryByRole("button", { name: "继续未发送的任务" })).not.toBeInTheDocument();
});

it("remounts an unknown recovery as observation only without a second POST", async () => {
  const { posts, unmount, client } = setup(base, "windows", true);
  const button = await screen.findByRole("button", { name: "继续未发送的任务" });
  fireEvent.change(screen.getByLabelText("图片生成提示"), { target: { value: prompt } });
  fireEvent.click(button);
  await screen.findByRole("alert");
  expect(posts).toHaveLength(1);
  unmount();
  render(<AutomationPanel snapshot={fixture()} client={client} run={vi.fn()} locked={false} />);
  expect(await screen.findByRole("button", { name: "继续未发送的任务" })).toBeDisabled();
  expect(screen.getByText(/恢复请求尚未确认/)).toBeVisible();
  expect(posts).toHaveLength(1);
});

it("starts a separate run while retaining a freshly parked native task", async () => {
  const item = { ...base, pre_submit_parked: true };
  const { posts } = setup(item);
  await screen.findByText("观察已暂停");
  fireEvent.change(screen.getByLabelText("图片生成提示"), { target: { value: "A different fixture diagram." } });
  const start = screen.getByRole("button", { name: "启动一次图片生成" });
  expect(start).toBeEnabled();
  fireEvent.click(start);
  await waitFor(() => expect(posts).toHaveLength(1));
  expect(posts[0].url.endsWith("/runs")).toBe(true);
  expect(posts[0].body.run_id).not.toBe(item.run_id);
  expect(posts[0].body.prompt).toBe("A different fixture diagram.");
  expect(screen.getByText("观察已暂停")).toBeVisible();
  expect(screen.getByText(/运行：fixture-resume/)).toBeVisible();
  await screen.findByText("等待执行");
  expect(start).toBeDisabled();
  expect(posts.some(post => post.url.includes("resume-pre-submit"))).toBe(false);
});

it.each([
  [{ ...base, pre_submit_parked: true }, "browser", false],
  [base, "windows", false],
  [{ ...base, pre_submit_parked: true, sha256: "retained", mime_type: "image/png" }, "windows", false],
  [{ ...base, pre_submit_parked: true }, "windows", true],
] as const)("keeps new Start blocked without a free native execution slot", async (item, provider, globallyBusy) => {
  setup(item, provider, false, globallyBusy);
  await screen.findByText("观察已暂停");
  fireEvent.change(screen.getByLabelText("图片生成提示"), { target: { value: prompt } });
  expect(screen.getByRole("button", { name: "启动一次图片生成" })).toBeDisabled();
});
