import { AutomationPanel, GeneratedImageGallery } from "../components/AutomationPanel";
import { useState } from "react";
import type { Run } from "../App";
import type { StyleSpec, TaskVersion, Theme } from "../lib/types";
import { type BridgeGate, type BridgeReport, type Capabilities, type WorkspaceClient, type WorkspaceSnapshot } from "../lib/workspace";
interface Props { snapshot: WorkspaceSnapshot; client: WorkspaceClient; run: Run; locked: boolean }
export function SourcesPage({ snapshot, client, run, locked, onCreateTask }: Props & { onCreateTask: () => void }) {
  const taskId = snapshot.selected_task_id;
  const task = snapshot.project.tasks.find(t => t.id === taskId);
  const emptyTask = !!task && task.versions.length === 0;
  const version = snapshot.current_version;
  const [role, setRole] = useState("knowledge");
  const [styleDraft, setStyleDraft] = useState<StyleSpec | null>(null);
  const style = styleDraft ?? snapshot.project.style;
  const changeStyle = (patch: Partial<StyleSpec>) => setStyleDraft({ ...style, ...patch });
  return <div className="workspace-stack"><section className="panel"><h2>导入资料</h2><p className="muted">知识资料用于解析与来源追溯；风格参考图仅保存到风格，不进入知识正文。</p>
    <div className="workspace-toolbar"><label>用途<select disabled={locked} aria-label="导入用途" value={role} onChange={e => setRole(e.target.value)}><option value="knowledge">知识来源 · PDF / Markdown / LaTeX / 图片 OCR</option><option value="style">风格参考图 · 不做知识 OCR</option></select></label>
    <button className="primary-button" disabled={locked || !!styleDraft || (role === "knowledge" && !taskId)} onClick={() => { void run("导入资料", async () => { const paths = await client.call("choose_sources", undefined); if (!paths.length) return; return role === "style" ? client.call("import_style_images", { paths }) : client.call("import_sources", { paths, taskId: taskId! }); }); }}>选择本机文件并导入</button></div>
    {!taskId && <p className="empty-note">导入知识资料前请先创建任务。</p>}
    {version?.ir.sources.map(source => <article className="source-record" key={source.id}><strong>{source.original_name}</strong><p>{source.kind} · {source.byte_len.toLocaleString()} bytes</p><p className="path-text">项目内路径：{source.stored_path}</p><p className="path-text">SHA-256：{source.content_hash}</p>{source.notes.map((note, index) => <p className="muted" key={index}>{note}</p>)}<button className="secondary-button" disabled={locked} onClick={() => { void run("重新解析原件", () => client.call("reimport_source", { taskId: taskId!, baseVersionId: version.id, sourceId: source.id })); }}>重新解析原件</button></article>)}
    {!version?.ir.sources.length && <p className="empty-note">当前任务还没有知识来源。</p>}
  </section><section className="panel"><h2>恢复已审阅的 Knowledge IR</h2>
    <p className="muted">导入已导出的 Knowledge IR（最多 16 MiB），保留节点、来源锚点与纠错记录。来源须按内容哈希匹配本项目已验证的原件。</p>
    <p className="muted">若缺少来源，请先在其他任务导入原件，再创建空任务导入 IR。</p>
    {task && !emptyTask && <p className="callout subtle">当前任务已有版本，请创建空任务以保留现有手工修改。</p>}
    <div className="handoff-actions"><button className="secondary-button" disabled={locked || !!styleDraft || !emptyTask} onClick={() => { void run("导入 Knowledge IR", async () => {
      const json = await client.call("choose_ir_file", undefined);
      if (json === null) return;
      return client.call("import_ir", { taskId: taskId!, baseVersionId: version?.id ?? null, json });
    }); }}>导入 Knowledge IR</button><button className="secondary-button" disabled={locked} onClick={onCreateTask}>前往项目创建新任务</button></div>
  </section><section className="panel"><h2>画布与风格</h2><fieldset disabled={locked} className="form-grid">
    <label className="field">风格名称<input value={style.name} onChange={e => changeStyle({ name: e.target.value })} /></label>
    <label className="field">密度<select value={style.density} onChange={e => changeStyle({ density: e.target.value as StyleSpec["density"] })}><option value="dense">高密度</option><option value="balanced">均衡</option><option value="relaxed">宽松</option></select></label>
    {([['width_mm', '纸张宽度（mm）'], ['height_mm', '纸张高度（mm）'], ['margin_mm', '页边距（mm）'], ['max_pages', '最多页数']] as const).map(([key, label]) => <label className="field" key={key}>{label}<input type="number" min="1" value={style.paper[key]} onChange={e => changeStyle({ paper: { ...style.paper, [key]: Number(e.target.value) } })} /></label>)}
    <label className="field">正文字号（pt）<input type="number" min="8" value={style.typography.body_pt} onChange={e => changeStyle({ typography: { ...style.typography, body_pt: Number(e.target.value) } })} /></label>
    <label className="field">强调色<input value={style.colors.accent} onChange={e => changeStyle({ colors: { ...style.colors, accent: e.target.value } })} /></label>
  </fieldset><p className="muted">保存风格会为已有任务创建新版本，需重新校验与编译。</p><div className="workspace-toolbar"><button className="primary-button" disabled={locked || !styleDraft} onClick={() => { void run("保存风格", () => client.call("save_style", { style }), () => setStyleDraft(null)); }}>保存风格</button>{styleDraft && <button className="secondary-button" disabled={locked} onClick={() => setStyleDraft(null)}>放弃风格修改</button>}</div>
  <h3>已保存的风格参考图</h3>{snapshot.project.style.reference_images.length ? snapshot.project.style.reference_images.map(path => <p className="path-text" key={path}>{path}</p>) : <p className="empty-note">尚未导入风格参考图。</p>}</section></div>;
}
export function TasksPage({ snapshot, client, run, locked }: Props) {
  const task = snapshot.project.tasks.find(t => t.id === snapshot.selected_task_id)!;
  const [json, setJson] = useState("");
  const handoff = snapshot.handoff;
  return <div className="workspace-stack"><section className="panel"><h2>{task.title}</h2><p>{task.state} · {task.chapter ?? "未分章节"}</p>{task.last_error && <p role="alert">{task.last_error}</p>}
  <div className="automation-gate"><div><strong>知识规划与人工导入</strong><p>生成提示后复制到 ChatGPT，再导入返回的 ResultEnvelope JSON。图片可使用下方独立生成入口。</p></div></div>
  <div className="handoff-actions"><button className="primary-button" disabled={locked || !snapshot.current_version} onClick={() => { void run("生成 TaskEnvelope", () => client.call("prepare_handoff", { taskId: task.id })); }}>生成 TaskEnvelope</button><button className="secondary-button" disabled={locked || !handoff} onClick={() => { void run("复制提示", () => client.call("copy_prompt", { taskId: task.id })); }}>复制提示</button><button className="secondary-button" disabled={locked} onClick={() => { void run("打开 ChatGPT", () => client.call("open_chatgpt", undefined)); }}>打开 ChatGPT</button></div>
  {handoff && <><p className="callout subtle">r{handoff.revision} · {handoff.status} · 已纠正 {handoff.correction_attempts} 次</p>{handoff.last_error && <p className="callout warn">{handoff.last_error}</p>}{handoff.status === "manual_required" ? <p role="alert">第二次导入失败：需要人工处理。请检查资料与节点，保存新版本后重新生成提示。</p> : handoff.correction_attempts > 0 ? <p>第一次导入失败。复制当前纠正提示，让 ChatGPT 修正一次。</p> : null}<details><summary>查看当前提示与 TaskEnvelope</summary><pre className="json-view">{handoff.markdown}</pre><pre className="json-view">{handoff.envelope_json}</pre></details></>}
  <label className="field">ResultEnvelope JSON<textarea disabled={locked} rows={7} value={json} onChange={e => setJson(e.target.value)} placeholder="粘贴 ChatGPT 返回的完整 JSON" /></label><div className="handoff-actions"><button className="primary-button" disabled={locked || !handoff || !json.trim() || handoff.status === "manual_required"} onClick={() => { void run("导入 GPT 结果", () => client.call("import_result", { taskId: task.id, json }), () => setJson("")); }}>导入 JSON</button><button className="secondary-button" disabled={locked || !handoff || handoff.status === "manual_required"} onClick={() => { void run("导入结果文件", async () => { const result = await client.call("choose_result_file", undefined); if (result !== null) return client.call("import_result", { taskId: task.id, json: result }); }); }}>选择 JSON 文件</button></div></section>
  <AutomationPanel snapshot={snapshot} client={client} run={run} locked={locked} />
  <GeneratedImageGallery snapshot={snapshot} client={client} />
  {snapshot.current_version?.gpt_import && <ImportedVisualPlan version={snapshot.current_version} />}
  <section className="panel"><h2>不可变版本</h2>{[...task.versions].reverse().map(v => <article className="source-record" key={v.id}><strong>r{v.revision} · {v.reason}</strong><p>{v.created_at} · {v.ir.nodes.length} 个节点</p><button className="secondary-button" disabled={locked || v.id === task.current_version} onClick={() => { void run("回滚版本", () => client.call("rollback_version", { taskId: task.id, versionId: v.id })); }}>从 r{v.revision} 恢复内容</button></article>)}{!task.versions.length && <p className="empty-note">导入资料后建立第一个版本。</p>}</section>
  <section className="panel"><h2>任务审计日志</h2><ol className="audit-list">{snapshot.project.audit.filter(event => event.task_id === task.id).map(event => <li key={event.id}><time>{event.at}</time><strong>{event.kind}</strong><p>{event.detail}</p></li>)}</ol></section></div>;
}
export function SettingsPage({ snapshot, capabilities, client, run, locked, theme, toggleTheme, refreshCapabilities }: Omit<Props, "snapshot"> & { snapshot: WorkspaceSnapshot | null; capabilities: Capabilities | null; theme: Theme; toggleTheme: () => void; refreshCapabilities: () => void }) {
  const [ocrDraft, setOcrDraft] = useState<string | null>(null);
  const [diagnosticsPath, setDiagnosticsPath] = useState<string | null>(null);
  return <div className="workspace-stack"><section className="panel"><h2>外观与项目</h2><div className="setting-row"><div><strong>界面主题</strong><p>当前为{theme === "dark" ? "深色" : "浅色"}。</p></div><button className="secondary-button" onClick={toggleTheme}>切换主题</button></div><p className="path-text">项目目录：{snapshot?.root ?? "未打开"}</p></section>
  <section className="panel"><h2>本机能力报告</h2><button className="secondary-button" disabled={locked} onClick={refreshCapabilities}>重新检测能力</button>{capabilities ? <><dl className="capability-details"><dt>运行平台</dt><dd>{capabilities.platform} · {capabilities.version}</dd>{capabilities.storage && <><dt>项目存储</dt><dd>{capabilities.storage === "indexeddb" ? "此浏览器数据库" : capabilities.storage === "sqlite" ? "本机 SQLite 数据库" : capabilities.storage}</dd></>}<dt>Tesseract</dt><dd>{capabilities.tesseract_available ? "已检测到" : "未检测到；图片文字保留为待确认"}</dd><dt>已安装 OCR 语言</dt><dd>{capabilities.ocr_languages.join(", ") || "未检测到语言包"}</dd><dt>ChatGPT 自动化</dt><dd>{capabilities.runtime === "browser" ? "连接与运行状态见任务页" : `${capabilities.automation_enabled ? "可用" : "关闭"} · ${capabilities.bridge_status}`}</dd></dl><p className="muted">{capabilities.bridge_detail}</p></> : <p className="empty-note">尚未取得本机检测结果。</p>}
  {capabilities?.bridge && <BridgeDiscovery report={capabilities.bridge} />}
  {!capabilities?.automation_enabled && capabilities?.runtime !== "browser" && <div className="callout subtle">自动化尚未连接。可先复制提示与导入结果。</div>}</section>
  <section className="panel"><h2>脱敏诊断</h2><p className="muted">将本机能力与项目统计保存为脱敏诊断 JSON。</p>
    <button className="secondary-button" disabled={locked} onClick={() => { void run("导出脱敏诊断", async () => {
      setDiagnosticsPath(null);
      const savedPath = await client.call("export_diagnostics", undefined);
      if (!savedPath?.trim()) return;
      setDiagnosticsPath(savedPath);
      return null;
    }); }}>导出脱敏诊断</button>
    {diagnosticsPath && <p className="path-text" role="status">已保存：{diagnosticsPath}</p>}
  </section>
  <section className="panel"><h2>OCR 设置</h2><label className="field">OCR 语言<input value={ocrDraft ?? snapshot?.project.settings.ocr_languages ?? ""} placeholder="chi_sim+eng" onChange={e => setOcrDraft(e.target.value)} disabled={locked || !snapshot} /></label><p className="muted">保存后用于后续资料摄取。已有资料请在来源页重新解析原件。</p><button className="primary-button" disabled={locked || !snapshot || !ocrDraft?.trim()} onClick={() => { void run("保存 OCR 设置", () => client.call("save_settings", { ocrLanguages: ocrDraft!.trim() }), () => setOcrDraft(null)); }}>保存 OCR 设置</button></section></div>;
}

const bridgeGateLabels: { id: BridgeGate; label: string }[] = [
  { id: "discover_window", label: "发现窗口" },
  { id: "reliable_input", label: "可靠输入" },
  { id: "reliable_completion", label: "可靠完成判定" },
  { id: "reliable_capture", label: "可靠取回" },
];
function BridgeDiscovery({ report }: { report: BridgeReport }) {
  const detected = report.capabilities;
  return <div className="bridge-discovery"><h3>ChatGPT 本机检测</h3><p className="muted">{report.detail}</p>
    {detected ? <><p>{detected.app_found ? "检测到 ChatGPT 窗口" : "未检测到 ChatGPT 窗口"}</p>
      <dl className="capability-details"><dt>应用版本</dt><dd>{detected.app_version ?? "未确认"}</dd><dt>窗口 / 可访问控件</dt><dd>{detected.window_count} / {detected.accessible_controls}</dd><dt>WebView 可访问性</dt><dd>{detected.web_view_accessibility ? "已检测到" : "未检测到"}</dd><dt>前台窗口</dt><dd>{detected.foreground ? "是" : "否"}</dd></dl>
      {detected.detail && <p className="muted">{detected.detail}</p>}
      <ul className="bridge-gates" aria-label="ChatGPT 自动化兼容性门槛">{bridgeGateLabels.map(({ id, label }) => {
        const gate = detected.gates[id];
        return <li className="source-record" key={id}><div className="result-head"><strong>{label}</strong><span className={`status-pill ${gate?.passed ? "tone-ok" : "tone-muted"}`}>{gate?.passed ? "通过" : "未验证"}</span></div><p className="muted">{gate?.evidence ?? "本次检测没有提供此项证据。"}</p></li>;
      })}</ul></> : <p className="empty-note">未取得 ChatGPT 窗口检测结果。</p>}
  </div>;
}

const visualPlanStatuses = {
  not_evaluated: "尚未编译评估", applied: "已采用", partially_applied: "部分采用", ignored: "未采用",
};
function ImportedVisualPlan({ version }: { version: TaskVersion }) {
  const metadata = version.gpt_import!;
  const plan = metadata.visual_plan;
  const nodeTitles = new Map(version.ir.nodes.map(node => [node.id, node.title]));
  return <section className="panel"><h2>已导入的 GPT 视觉方案</h2>
    <div className="result-head"><p>本地排版采用状态</p><strong>{visualPlanStatuses[metadata.visual_plan_status]}</strong></div>
    <p className="muted">方案建议 {plan.page_count} 页；实际页面以本地编译与校验为准。</p>
    {plan.groups.map(group => <article className="source-record" key={group.id}><strong>{group.title}</strong><p className="muted">建议第 {group.page} 页 · {group.node_ids.length} 个节点</p><p>{group.node_ids.map(id => nodeTitles.get(id) ?? id).join(" · ")}</p></article>)}
    {!!plan.notes.length && <ul>{plan.notes.map((note, index) => <li key={index}>{note}</li>)}</ul>}
    {metadata.warnings.map((warning, index) => <div className="callout warn" key={`${warning.code}-${index}`}><div><strong>{warning.code}</strong><p>{warning.message}</p></div></div>)}
    <p className="callout subtle">{metadata.image_reference_present ? metadata.image_reference_applied ? "本地服务报告已采用图片引用。请以编译页面为准。" : "结果包含图片引用；图片尚未取回，也未用于当前页面。" : "结果未包含图片引用。"}</p>
  </section>;
}
