import type { WorkspaceClient, WorkspaceSnapshot } from "./workspace";
export interface AutomationAccess { token: string; base_url: string }
export type AutomationPhase = "queued" | "preparing" | "submitting" | "generating" | "downloading" | "awaiting_import" | "completed" | "paused" | "failed" | "cancelled";
export interface AutomationRun {
  run_id: string; task_id: string; base_version_id: string; prompt_sha256: string;
  phase: AutomationPhase; reason?: string; sha256?: string; mime_type?: string;
  pre_submit_retryable?: boolean; pre_submit_parked?: boolean; resume_request_id?: string;
  stage?: string; exception_type?: string; exception_types?: string[]; failure_method?: string;
}
export const automationOrigin = "http://127.0.0.1:48123";
const reasonLabels: Record<string, string> = {
  ready: "ChatGPT 已就绪，可以开始生成。",
  composer_detected: "已找到 ChatGPT 输入框；发送前仍会在独立窗口检查会话。",
  existing_session_requires_verification: "已找到 ChatGPT 网页，但原页面尚未响应。可准备新任务，发送前会在独立窗口检查会话。",
  open_existing_chatgpt_session: "请在已登录的浏览器中打开 ChatGPT 网页。",
  reload_chatgpt_tab: "请刷新已打开的 ChatGPT 网页，让扩展建立连接。",
  existing_session_unavailable: "未找到可用的 ChatGPT 网页，请打开已登录的页面并刷新。",
  login_required: "ChatGPT 需要登录，请在浏览器中登录后重新检查连接。",
  background_window_unavailable: "无法打开独立后台窗口，尚未发送提示。请检查浏览器和扩展。",
  background_work_unavailable: "无法启用浏览器后台运行，尚未发送提示。请检查浏览器和扩展后重新连接。",
  background_work_active: "此网页已有后台任务，请先处理原运行。",
  content_timeout: "ChatGPT 网页响应超时，请查看当前运行状态。不会自动重发提示。",
  windows_locked: "连接程序报告旧版锁屏限制，请更新本机连接程序后重新连接。",
  chatgpt_not_found: "未找到 ChatGPT 客户端，请先打开并登录。",
  ambiguous_chatgpt_windows: "检测到多个可用客户端窗口，无法确定新任务的来源，请检查客户端窗口后刷新连接。",
  chatgpt_mode_required: "请在客户端切换到 ChatGPT 模式。",
  image_surface_unavailable: "无法识别 ChatGPT 的图片生成页面，已暂停。",
  independent_window_unavailable: "当前客户端无法打开独立窗口，尚未发送图片生成提示。",
  foreground_unavailable: "连接程序报告旧版前台限制，请更新本机连接程序后重新连接。",
  user_changed_composer: "输入内容已被修改，观察已暂停，请检查 ChatGPT。",
  conversation_changed: "ChatGPT 会话已切换，观察已暂停。",
  generation_timeout: "等待图片生成超时，请查看 ChatGPT 中的进度。",
  download_unavailable: "尚未找到图片下载入口，请查看 ChatGPT。",
  download_dialog_unavailable: "未找到保存窗口，请查看 ChatGPT 的下载状态。",
  download_failed: "图片下载失败，可检查后继续处理。",
  artifact_invalid: "图片文件校验失败，尚未保存到项目。",
  submission_uncertain: "无法确认提示是否已发送，请先查看 ChatGPT，避免重复提交。",
  adapter_restarted: "连接程序已重启，请检查这次运行的状态。",
  cancelled: "观察已停止。",
  resume_not_allowed: "尚未确认这次运行从未发送，请继续查询原运行。",
  resume_limit_reached: "这次运行已达到恢复次数上限。",
  run_identity_conflict: "恢复请求与原任务或提示不一致，已停止。",
};
export function automationReason(reason?: string): string {
  if (!reason) return "等待连接状态。";
  return reasonLabels[reason] ?? (/^[a-z][a-z0-9_]*$/.test(reason) ? `连接状态：${reason}` : reason);
}
export class AutomationHttpError extends Error {
  constructor(message: string, readonly status: number) { super(message); }
}
const accessKey = "entwurf.automation-access.v1";
function checkedAccess(value: unknown, allowLocalPort = false): AutomationAccess {
  const access = value as AutomationAccess;
  const invalid = () => new Error("自动化配对返回了无效的服务地址。");
  if (!access || typeof access.token !== "string" || !access.token || typeof access.base_url !== "string") throw invalid();
  let address: URL;
  try { address = new URL(access.base_url); } catch { throw invalid(); }
  if (address.protocol !== "http:" || !["127.0.0.1", "localhost", "[::1]"].includes(address.hostname)
    || !/^http:\/\/[^/\\?#]+\/?$/i.test(access.base_url)
    || address.username || address.password || address.pathname !== "/" || access.base_url.includes("?") || access.base_url.includes("#")
    || (!allowLocalPort && address.origin !== automationOrigin)) throw invalid();
  return { token: access.token, base_url: address.origin };
}
export async function getAutomationAccess(client: WorkspaceClient): Promise<AutomationAccess | null> {
  if (client.runtime !== "browser") return checkedAccess(await client.call("automation_access", undefined), client.runtime === "local-http");
  const stored = localStorage.getItem(accessKey);
  if (!stored) return null;
  try { return checkedAccess(JSON.parse(stored)); } catch { localStorage.removeItem(accessKey); return null; }
}
export function pairAutomation(): Promise<AutomationAccess> {
  const popup = window.open(`${automationOrigin}/pair.html?origin=${encodeURIComponent(location.origin)}`, "entwurf-automation-pair", "popup,width=600,height=600");
  if (!popup) return Promise.reject(new Error("请允许配对窗口弹出，然后重新连接。"));
  return new Promise((resolve, reject) => {
    const cleanup = () => { clearInterval(closed); window.removeEventListener("message", receive); };
    const receive = (event: MessageEvent) => {
      if (event.origin !== automationOrigin || event.source !== popup || event.data?.type !== "entwurf:automation-paired") return;
      try { const access = checkedAccess(event.data); localStorage.setItem(accessKey, JSON.stringify(access)); cleanup(); popup.close(); resolve(access); }
      catch (error) { cleanup(); reject(error); }
    };
    const closed = setInterval(() => { if (popup.closed) { cleanup(); reject(new Error("配对窗口已关闭，尚未建立连接。")); } }, 500);
    window.addEventListener("message", receive);
  });
}
export function createAutomationService(access: AutomationAccess) {
  access = checkedAccess(access, true);
  async function request(path: string, body?: unknown): Promise<unknown> {
    const response = await fetch(`${access.base_url}/api/automation${path}`, { method: body === undefined ? "GET" : "POST", headers: { Authorization: `Bearer ${access.token}`, ...(body === undefined ? {} : { "Content-Type": "application/json" }) }, ...(body === undefined ? {} : { body: JSON.stringify(body) }), signal: AbortSignal.timeout(15_000) });
    if (!response.ok) { const error = await response.json().catch(() => ({})); throw new AutomationHttpError(error.error ?? `自动化服务返回 ${response.status}`, response.status); }
    return response.status === 204 ? null : response.json();
  }
  return {
    status: () => request("/status") as Promise<{ connected: boolean; available: boolean; reason: string; provider?: string; busy?: boolean }>,
    list: (taskId: string) => request(`/runs?task_id=${encodeURIComponent(taskId)}`) as Promise<AutomationRun[]>,
    get: (id: string) => request(`/runs/${encodeURIComponent(id)}`) as Promise<AutomationRun>,
    start: (run: { run_id: string; task_id: string; base_version_id: string; prompt: string }) => request("/runs", run) as Promise<AutomationRun>,
    cancel: (id: string) => request(`/runs/${encodeURIComponent(id)}/cancel`, {}) as Promise<AutomationRun>,
    resume: (id: string, body: { request_id: string; task_id: string; base_version_id: string; prompt: string }) => request(`/runs/${encodeURIComponent(id)}/resume-pre-submit`, body) as Promise<AutomationRun>,
    ack: (id: string, body: { sha256: string; image_id: string }) => request(`/runs/${encodeURIComponent(id)}/ack`, body),
    artifact: async (id: string) => {
      const response = await fetch(`${access.base_url}/api/automation/runs/${encodeURIComponent(id)}/artifact`, { headers: { Authorization: `Bearer ${access.token}` }, signal: AbortSignal.timeout(30_000) });
      if (!response.ok) throw new Error("生成图片下载失败，可以稍后重新取回。");
      const bytes = new Uint8Array(await response.arrayBuffer());
      const sha256 = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), value => value.toString(16).padStart(2, "0")).join("");
      if (response.headers.get("X-Entwurf-Sha256") !== sha256) throw new Error("生成图片的下载校验未通过，请重新取回。");
      return { bytes, sha256, mimeType: response.headers.get("Content-Type")?.split(";")[0].trim() };
    },
  };
}
export type AutomationService = ReturnType<typeof createAutomationService>;

// Review is deliberately read-only: bind the authenticated response to the
// selected immutable run, without touching the workspace or acknowledging it.
export async function reviewAutomationImage(service: AutomationService, run: Pick<AutomationRun, "run_id" | "sha256" | "mime_type">) {
  if (!run.sha256 || !run.mime_type || !["image/png", "image/jpeg", "image/webp"].includes(run.mime_type)) throw new Error("原始图片类型无效，无法查看。");
  const artifact = await service.artifact(run.run_id);
  if (artifact.sha256 !== run.sha256 || artifact.mimeType !== run.mime_type
    || !artifact.bytes.length || artifact.bytes.length > 32 * 1024 * 1024) throw new Error("原始图片与生成任务的哈希或类型不一致，已停止查看。");
  return artifact.bytes;
}

interface ResumeIntent { request_id: string; prompt_sha256: string; task_id: string; base_version_id: string; pending: boolean }
const resumeKey = (run: AutomationRun) => `entwurf.resume-pre-submit.v1:${run.run_id}`;
function readResumeIntent(run: AutomationRun): ResumeIntent | null {
  const raw = localStorage.getItem(resumeKey(run));
  if (!raw) return null;
  const value = JSON.parse(raw) as ResumeIntent;
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value.request_id)
    || value.task_id !== run.task_id || value.base_version_id !== run.base_version_id
    || value.prompt_sha256 !== run.prompt_sha256 || typeof value.pending !== "boolean") throw new Error("恢复请求记录与原运行不一致。");
  return value;
}
export function resumePending(run: AutomationRun): boolean {
  try { return readResumeIntent(run)?.pending ?? false; } catch { return true; }
}
export function reconcileResumeIntent(run: AutomationRun): void {
  try {
    const intent = readResumeIntent(run);
    if (intent?.pending && intent.request_id === run.resume_request_id) {
      localStorage.setItem(resumeKey(run), JSON.stringify({ ...intent, pending: false }));
    }
  } catch { /* Keep an unreadable/unsaved intent blocked instead of replacing its ID. */ }
}
export async function resumeUnsubmittedRun(service: AutomationService, run: AutomationRun, prompt: string): Promise<AutomationRun> {
  if (run.phase !== "paused" || run.pre_submit_retryable !== true || run.sha256 || run.mime_type) throw new Error("尚未确认这次运行从未发送。");
  if (resumePending(run)) throw new Error("上次恢复请求尚未确认，将继续查询原运行，不会重新发送。");
  const digest = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(prompt))), v => v.toString(16).padStart(2, "0")).join("");
  if (digest !== run.prompt_sha256) throw new Error("请输入与这次运行完全相同的原提示，校验通过后才能继续。");
  // Recheck after hashing, so simultaneous clicks in this document cannot both claim.
  if (resumePending(run)) throw new Error("上次恢复请求尚未确认，将继续查询原运行。");
  const intent: ResumeIntent = { request_id: crypto.randomUUID(), task_id: run.task_id, base_version_id: run.base_version_id, prompt_sha256: run.prompt_sha256, pending: true };
  try { localStorage.setItem(resumeKey(run), JSON.stringify(intent)); }
  catch { throw new Error("无法保存恢复请求记录，尚未发送恢复请求。"); }
  try {
    const updated = await service.resume(run.run_id, { request_id: intent.request_id, task_id: run.task_id, base_version_id: run.base_version_id, prompt });
    reconcileResumeIntent(updated);
    return updated;
  } catch (cause) {
    if (cause instanceof AutomationHttpError && cause.status >= 400 && cause.status < 500) {
      try { localStorage.setItem(resumeKey(run), JSON.stringify({ ...intent, pending: false })); } catch { /* Retain the conservative pending guard. */ }
    }
    throw cause;
  }
}
export async function importAutomationImage(service: AutomationService, run: AutomationRun, client: WorkspaceClient): Promise<WorkspaceSnapshot> {
  if (!run.sha256 || !run.mime_type) throw new Error("图片尚未准备好，等待下载完成后再导入。");
  let snapshot = await client.call("workspace_snapshot", { taskId: run.task_id });
  let stored = snapshot.project.generated_images?.find(image => image.run_id === run.run_id);
  if (!stored) {
    const artifact = await service.artifact(run.run_id);
    if (artifact.sha256 !== run.sha256) throw new Error("生成任务与图片哈希不一致，已停止导入。");
    snapshot = await client.call("import_generated_image", { taskId: run.task_id, baseVersionId: run.base_version_id, runId: run.run_id, promptSha256: run.prompt_sha256, bytes: Array.from(artifact.bytes), expectedSha256: run.sha256, mimeType: run.mime_type });
    stored = snapshot.project.generated_images?.find(image => image.run_id === run.run_id);
  }
  if (!stored || stored.sha256 !== run.sha256 || stored.task_id !== run.task_id || stored.base_version_id !== run.base_version_id || stored.prompt_sha256 !== run.prompt_sha256) throw new Error("项目没有返回匹配的已保存图片，未确认导入。");
  // Metadata alone cannot prove that durable image bytes still exist. The core
  // validates ownership, size and hash; independently bind returned bytes to ACK.
  const retained = new Uint8Array(await client.call("generated_image_bytes", { taskId: run.task_id, imageId: stored.id }));
  const retainedSha = Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", retained)), value => value.toString(16).padStart(2, "0")).join("");
  if (retainedSha !== run.sha256) throw new Error("已保存图片的字节校验失败，未确认导入；服务端副本仍保留。");
  await service.ack(run.run_id, { sha256: run.sha256, image_id: stored.id });
  return snapshot;
}
