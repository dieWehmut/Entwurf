import { useEffect, useRef, useState } from "react";
import type { Run } from "../App";
import type { GeneratedImage } from "../lib/types";
import { errorMessage, type WorkspaceClient, type WorkspaceSnapshot } from "../lib/workspace";
import { createAutomationService, getAutomationAccess, importAutomationImage, pairAutomation, type AutomationRun, type AutomationService } from "../lib/automation";
import { download } from "../lib/web-files";

const phases = { queued: "等待执行", preparing: "准备 ChatGPT", submitting: "正在提交", generating: "等待生成", downloading: "取回图片", awaiting_import: "等待保存到项目", completed: "图片已入库", paused: "观察已暂停", failed: "执行失败", cancelled: "观察已停止" };
const activePhases = new Set(["queued", "preparing", "submitting", "generating", "downloading"]);
interface Props { snapshot: WorkspaceSnapshot; client: WorkspaceClient; run: Run; locked: boolean }
export function AutomationPanel({ snapshot, client, run, locked }: Props) {
  const taskId = snapshot.selected_task_id!;
  const version = snapshot.current_version;
  const [service, setService] = useState<AutomationService | null>(null);
  const [status, setStatus] = useState<{ connected: boolean; available: boolean; reason: string } | null>(null);
  const [runs, setRuns] = useState<AutomationRun[]>([]);
  const [prompt, setPrompt] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const submitting = useRef(false);
  const provisional = useRef<AutomationRun | null>(null);
  useEffect(() => {
    let alive = true;
    void getAutomationAccess(client).then(access => { if (alive && access) setService(createAutomationService(access)); }).catch(cause => { if (alive) setError(errorMessage(cause)); });
    return () => { alive = false; };
  }, [client]);
  useEffect(() => {
    if (!service) return;
    let alive = true;
    let timer: ReturnType<typeof setTimeout>;
    const refresh = async () => {
      try {
        const [nextStatus, nextRuns] = await Promise.all([service.status(), service.list(taskId)]);
        if (!alive) return;
        setStatus(nextStatus);
        const ownRuns = nextRuns.filter(item => item.task_id === taskId);
        if (provisional.current && !ownRuns.some(item => item.run_id === provisional.current!.run_id)) ownRuns.push(provisional.current);
        else provisional.current = null;
        setRuns(ownRuns);
      } catch (cause) { if (alive) { setStatus(null); setError(errorMessage(cause)); } }
      finally { if (alive) timer = setTimeout(() => { void refresh(); }, 2000); }
    };
    void refresh();
    return () => { alive = false; clearTimeout(timer); };
  }, [service, taskId]);
  async function connect() {
    if (submitting.current) return;
    submitting.current = true; setBusy(true); setError(null);
    try { const access = client.runtime === "browser" ? await pairAutomation() : await getAutomationAccess(client); if (access) setService(createAutomationService(access)); }
    catch (cause) { setError(errorMessage(cause)); }
    finally { submitting.current = false; setBusy(false); }
  }
  async function start() {
    if (!service || !version || submitting.current || runs.some(item => activePhases.has(item.phase))) return;
    submitting.current = true; setBusy(true); setError(null);
    const pending: AutomationRun = { run_id: crypto.randomUUID(), task_id: taskId, base_version_id: version.id, prompt_sha256: "", phase: "queued", reason: "提交结果尚未确认，请勿重复发送。" };
    provisional.current = pending; setRuns(items => [...items, pending]);
    try { const started = await service.start({ run_id: pending.run_id, task_id: taskId, base_version_id: version.id, prompt }); provisional.current = started; setRuns(items => items.map(item => item.run_id === started.run_id ? started : item)); }
    catch (cause) { setError(`${errorMessage(cause)} 将继续查询这次提交，不会自动重发。`); }
    finally { submitting.current = false; setBusy(false); }
  }
  async function cancel(item: AutomationRun) {
    if (!service) return;
    setBusy(true); setError(null);
    try { const updated = await service.cancel(item.run_id); setRuns(items => items.map(run => run.run_id === updated.run_id ? updated : run)); if (provisional.current?.run_id === updated.run_id) provisional.current = updated; }
    catch (cause) { setError(errorMessage(cause)); }
    finally { setBusy(false); }
  }
  const running = runs.some(item => activePhases.has(item.phase));
  return <section className="panel"><h2>ChatGPT 图片生成</h2>
    <p className="muted">使用已登录的 ChatGPT 会话。图片独立保存为任务结果，知识节点与来源保持原有审阅流程。</p>
    <div className="handoff-actions"><button className="secondary-button" disabled={locked || busy} onClick={() => { void connect(); }}>{service ? "重新连接自动化" : "连接 ChatGPT 自动化"}</button><span role="status">{status ? `${status.connected ? "已连接" : "未连接"} · ${status.reason}` : "尚未连接自动化服务"}</span></div>
    <button className="text-button" disabled={locked || busy} onClick={() => { void run("打开连接设置", () => client.call("open_automation_setup", undefined)); }}>安装扩展与连接设置</button>
    {error && <p className="callout warn" role="alert">{error}</p>}
    <label className="field">图片生成提示<textarea rows={5} disabled={locked || busy || running} value={prompt} onChange={event => setPrompt(event.target.value)} placeholder="描述这次希望 ChatGPT 生成的图片" /></label>
    <p className="muted">{version ? `本次生成绑定任务 ${taskId} 的 r${version.revision}。` : "导入资料建立知识版本后可开始生成。"}</p>
    <button className="primary-button" disabled={locked || busy || running || !version || !prompt.trim() || !status?.connected || !status.available} onClick={() => { void start(); }}>启动一次图片生成</button>
    {runs.map(item => <article className="source-record" key={item.run_id}><strong>{phases[item.phase] ?? item.phase}</strong><p>{item.reason}</p><p className="path-text">运行：{item.run_id} · 基础版本：{item.base_version_id}</p>
      {version && item.base_version_id !== version.id && <p className="muted">此运行绑定其他知识版本；导入时会校验版本，已保存图片仍可查看。</p>}
      <div className="handoff-actions">{activePhases.has(item.phase) && <button className="secondary-button" disabled={locked || busy} onClick={() => { void cancel(item); }}>停止观察</button>}
      {(item.phase === "awaiting_import" || item.sha256) && item.phase !== "completed" && <button className="primary-button" disabled={locked || busy} onClick={() => { if (service) void run("保存生成图片", () => importAutomationImage(service, item, client)); }}>继续导入图片</button>}</div>
    </article>)}
    <p className="muted">停止观察不会撤回已发送到 ChatGPT 的请求。刷新页面只查询已有运行，不重复提交；已取回的图片可继续导入。</p>
  </section>;
}
export function GeneratedImageGallery({ snapshot, client }: Pick<Props, "snapshot" | "client">) {
  const images = snapshot.project.generated_images?.filter(image => image.task_id === snapshot.selected_task_id) ?? [];
  return images.length ? <section className="panel"><h2>生成图片结果</h2>{images.map(image => <GeneratedImageCard key={image.id} image={image} client={client} />)}</section> : null;
}
function GeneratedImageCard({ image, client }: { image: GeneratedImage; client: WorkspaceClient }) {
  const [url, setUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let alive = true; let objectUrl: string | undefined;
    void client.call("generated_image_bytes", { taskId: image.task_id, imageId: image.id }).then(bytes => { if (!alive) return; objectUrl = URL.createObjectURL(new Blob([new Uint8Array(bytes)], { type: image.mime_type })); setUrl(objectUrl); }).catch(cause => { if (alive) setError(errorMessage(cause)); });
    return () => { alive = false; if (objectUrl) URL.revokeObjectURL(objectUrl); };
  }, [client, image.id, image.task_id, image.mime_type]);
  return <article className="source-record"><h3>ChatGPT 生成图片</h3>{url && <img className="generated-image" src={url} alt={`ChatGPT 生成图片 ${image.run_id}`} />}{error && <p role="alert">{error}</p>}
    <p>{image.width} × {image.height} · {image.mime_type} · {image.created_at}</p><p className="path-text">来源：ChatGPT · 运行：{image.run_id} · 基础版本：{image.base_version_id}</p><p className="path-text">SHA-256：{image.sha256}</p>
    <button className="secondary-button" onClick={() => { void client.call("generated_image_bytes", { taskId: image.task_id, imageId: image.id }).then(bytes => download(new Uint8Array(bytes), `${image.id}.${image.mime_type === "image/jpeg" ? "jpg" : image.mime_type.split("/")[1]}`, image.mime_type)).catch(cause => setError(errorMessage(cause))); }}>下载原始图片</button>
  </article>;
}
