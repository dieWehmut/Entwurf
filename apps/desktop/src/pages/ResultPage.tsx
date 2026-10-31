import { GeneratedImageGallery } from "../components/AutomationPanel";
import { useState, type Dispatch, type SetStateAction } from "react";
import type { Run } from "../App";
import type { KnowledgeNode, TaskVersion } from "../lib/types";
import { exportOutcomeError, versionDiff, type WorkspaceClient, type WorkspaceSnapshot } from "../lib/workspace";
export interface Draft { baseVersionId: string; baseNodes: KnowledgeNode[]; nodes: KnowledgeNode[]; anchorText: Record<string, string> }
interface Props {
  snapshot: WorkspaceSnapshot; client: WorkspaceClient; run: Run; locked: boolean; busy: boolean;
  draft: Draft | null; setDraft: Dispatch<SetStateAction<Draft | null>>;
  activeNodeId: string | null; setActiveNodeId: (id: string) => void; saveDraft: () => Promise<void>;
}
export function ResultPage({ snapshot, client, run, locked, busy, draft, setDraft, activeNodeId, setActiveNodeId, saveDraft }: Props) {
  const version = snapshot.current_version!;
  const task = snapshot.project.tasks.find(t => t.id === snapshot.selected_task_id)!;
  const [mode, setMode] = useState("edit");
  const [compareId, setCompareId] = useState("");
  const [reviewNotes, setReviewNotes] = useState<Record<string, string>>({});
  const nodes = draft?.nodes ?? version.ir.nodes;
  const node = nodes.find(n => n.id === activeNodeId) ?? nodes[0];
  const stale = !!draft && version.id !== draft.baseVersionId;
  const report = version.validation;
  const blocking = report?.findings.some(f => f.severity === "blocking");
  const createDraft = (): Draft => ({ baseVersionId: version.id, baseNodes: version.ir.nodes, nodes: structuredClone(version.ir.nodes), anchorText: {} });
  function editNode(patch: Partial<KnowledgeNode>) {
    setDraft(current => { const next = current ?? createDraft(); return { ...next, nodes: next.nodes.map(n => n.id === node.id ? { ...n, ...patch } : n) }; });
  }
  function reorder(offset: number) {
    setDraft(current => { const next = current ?? createDraft(); const reordered = [...next.nodes]; const index = reordered.findIndex(n => n.id === node.id); const target = index + offset; if (target < 0 || target >= reordered.length) return next; [reordered[index], reordered[target]] = [reordered[target], reordered[index]]; return { ...next, nodes: reordered }; });
  }
  const compare = task.versions.find(v => v.id === compareId) ?? task.versions.find(v => v.id !== version.id);
  const actions = { taskId: task.id };
  return <div className="workspace-stack"><section className="panel"><div className="result-head"><div><h2>r{version.revision} · {version.ir.title}</h2><p className="muted">{report ? blocking ? "校验存在阻断项" : "当前版本校验无阻断项" : "当前版本尚未校验"} · {version.layout?.pages.length ?? 0} 页</p></div><div className="handoff-actions"><button className="secondary-button" disabled={locked} onClick={() => { void run("校验当前版本", () => client.call("validate_current", actions)); }}>校验当前版本</button><button className="primary-button" disabled={locked} onClick={() => { void run("编译预览", () => client.call("preview_current", actions)); }}>编译预览</button></div></div><div className="workspace-toolbar segmented" aria-label="结果视图">{[['edit', '节点编辑'], ['preview', '编译页面'], ['compare', '版本比较']].map(([id, label]) => <button key={id} className={mode === id ? "is-active" : ""} aria-pressed={mode === id} onClick={() => setMode(id)}>{label}</button>)}</div></section>
  {mode === "edit" && <div className="result-grid"><section className="panel"><h2>知识节点 · {nodes.length}</h2><div className="node-picker">{nodes.map((n, index) => <button key={n.id} className={`task-card ${node?.id === n.id ? "is-active" : ""}`} onClick={() => setActiveNodeId(n.id)}><strong>{index + 1}. {n.title}</strong><span>{n.kind} · {n.importance} · {n.anchors.length} 个来源锚点</span></button>)}</div>{!nodes.length && <p className="empty-note">没有可编辑节点，请检查解析诊断。</p>}</section>
  <section className="panel node-editor"><h2>节点编辑</h2>{node && <><p className="muted">{node.id} · {node.origin}</p><fieldset disabled={busy || stale}>
    <label className="field">节点标题<input value={node.title} onChange={e => editNode({ title: e.target.value })} /></label>
    <label className="field">节点正文<textarea rows={5} value={node.body} onChange={e => editNode({ body: e.target.value })} /></label>
    <label className="field">公式 LaTeX<textarea className="mono" rows={3} value={node.latex ?? ""} onChange={e => editNode({ latex: e.target.value || null })} /></label>
    <label className="field">重要度<select value={node.importance} onChange={e => editNode({ importance: e.target.value as KnowledgeNode['importance'] })}><option value="critical">核心</option><option value="high">高</option><option value="normal">普通</option><option value="low">低</option></select></label>
    <label className="field">来源锚点 JSON<textarea className="mono" rows={9} spellCheck={false} value={draft?.anchorText[node.id] ?? JSON.stringify(node.anchors, null, 2)} onChange={e => { const text = e.target.value; setDraft(current => { const next = current ?? createDraft(); return { ...next, anchorText: { ...next.anchorText, [node.id]: text } }; }); }} /></label>
    <p className="muted">保留 source_id、页/行、字符范围、quote_hash 与 region 精确坐标。保存时由本地服务校验。</p>
    <div className="handoff-actions"><button className="secondary-button" disabled={nodes[0]?.id === node.id} onClick={() => reorder(-1)}>节点上移</button><button className="secondary-button" disabled={nodes.at(-1)?.id === node.id} onClick={() => reorder(1)}>节点下移</button></div></fieldset>
    <div className="handoff-actions"><button className="primary-button" disabled={busy || !draft || stale} onClick={() => { void saveDraft(); }}>保存节点修改</button><button className="secondary-button" disabled={busy || !draft} onClick={() => setDraft(null)}>放弃草稿</button></div>
    <p className="muted">{draft ? `未保存 · 基于 ${draft.baseVersionId}` : "所有编辑已保存"}</p>
  </>}</section></div>}
  {mode === "preview" && <section className="panel"><h2>本地编译页面</h2>{snapshot.preview_pages.length ? snapshot.preview_pages.map(page => <figure className="svg-page" key={page.page}><img src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(page.svg)}`} alt={`本地编译第 ${page.page} 页`} /><figcaption>第 {page.page} 页 · 当前版本 r{version.revision}</figcaption></figure>) : <p className="empty-note">尚无当前版本的 SVG 预览。点击“编译预览”生成实际页面。</p>}</section>}
  {mode === "compare" && <section className="panel"><h2>版本内容差异</h2>{compare ? <><label className="field">比较版本<select value={compare.id} onChange={e => setCompareId(e.target.value)}>{task.versions.filter(v => v.id !== version.id).map(v => <option value={v.id} key={v.id}>r{v.revision} · {v.reason}</option>)}</select></label><VersionComparison before={compare} after={version} /><button className="secondary-button" disabled={locked} onClick={() => { void run("回滚版本", () => client.call("rollback_version", { taskId: task.id, versionId: compare.id })); }}>从 r{compare.revision} 恢复内容</button></> : <p className="empty-note">需要至少两个版本才能比较。</p>}</section>}
  <section className="panel"><h2>校验与待确认</h2>{report ? <><p className="muted">校验时间：{report.checked_at}</p>{report.findings.map(f => <div className="source-record" key={f.id}><strong>{f.severity} · {f.code}</strong><p>{f.message}</p>{f.evidence && <pre className="json-view">{f.evidence}</pre>}</div>)}{!report.findings.length && <p>本次校验未发现问题。</p>}<details><summary>校验指标</summary><pre className="json-view">{JSON.stringify(report.metrics, null, 2)}</pre></details></> : <p className="empty-note">尚无校验报告。</p>}
    {version.ir.unverified.map(item => <div className="source-record" key={item.id}><strong>{item.kind} · {item.text}</strong><p>{item.reason}</p><label className="field">审阅说明 · {item.id}<textarea disabled={locked} value={reviewNotes[item.id] ?? ""} onChange={e => setReviewNotes(notes => ({ ...notes, [item.id]: e.target.value }))} /></label><button className="secondary-button" disabled={locked || !reviewNotes[item.id]?.trim()} onClick={() => { void run("确认审阅", () => client.call("resolve_review", { ...actions, baseVersionId: version.id, itemId: item.id, note: reviewNotes[item.id].trim() })); }}>记录审阅并确认</button></div>)}
  </section>
  {!!version.visual_repairs?.length && <section className="panel"><h2>版式修复记录</h2>
    <p className="muted">撤销会创建新版本，恢复本轮之前的版式并撤销后续依赖调整；知识内容保持不变。导出文件需重新生成。</p>
    {version.visual_repair_enabled === false && <p className="callout subtle">此版本已关闭自动版式修复。</p>}
    {version.visual_repairs.map(repair => <article className="source-record" key={repair.id}>
      <div className="result-head"><strong>第 {repair.round} 轮</strong><span>{repair.reverted ? "已撤销" : "未撤销"}</span></div>
      <p>{repair.reason}</p><p className="muted">{repair.passed ? "当轮校验通过" : "当轮仍有阻断项"}</p>
      <button className="secondary-button" disabled={locked || repair.reverted} onClick={() => { void run("撤销版式修复", () => client.call("undo_visual_repair", { ...actions, baseVersionId: version.id, repairId: repair.id })); }}>撤销第 {repair.round} 轮版式修复</button>
    </article>)}
  </section>}
  {!!version.ir.corrections.length && <section className="panel"><h2>可撤销纠错记录</h2>{version.ir.corrections.map(c => <article className="source-record" key={c.id}><strong>{c.field} · {c.node_id}</strong><pre className="json-view">{c.before}</pre><pre className="json-view">{c.after}</pre><p>{c.reason}</p><p className="muted">{c.created_at} · {c.reverted ? "已撤销" : "已记录"}</p><button className="secondary-button" disabled={locked || c.reverted || !c.accepted} onClick={() => { void run("撤销纠错", () => client.call("revert_correction", { ...actions, baseVersionId: version.id, correctionId: c.id })); }}>撤销这条纠错</button></article>)}</section>}
  <GeneratedImageGallery snapshot={snapshot} client={client} />
  <section className="panel"><h2>导出文件</h2><p className="muted">导出会重新执行本地编译与校验。仅通过门禁的当前版本可生成文件。</p><button className="primary-button" disabled={locked || !!blocking || !!version.ir.unverified.length} onClick={() => { void run("导出文件", () => client.call("export_current", actions), undefined, exportOutcomeError); }}>导出 SVG / PNG / PDF</button>
  {version.artifacts ? <div className="artifact-list">{(['svg', 'png', 'pdf', 'manifest', 'ir'] as const).flatMap(kind => { const paths = version.artifacts![kind]; return (Array.isArray(paths) ? paths : [paths]).map((path, index) => <div className="source-record" key={`${kind}-${index}`}><strong>{kind.toUpperCase()}</strong><p className="path-text">{path}</p><button className="secondary-button" disabled={busy} onClick={() => { void run("打开导出文件", () => client.call("open_export", { ...actions, kind, index })); }}>打开 {kind.toUpperCase()}{Array.isArray(paths) ? ` ${index + 1}` : ""}</button></div>); })}</div> : <p className="empty-note">当前版本尚未产生导出文件。</p>}</section></div>;
}
function VersionComparison({ before, after }: { before: TaskVersion; after: TaskVersion }) {
  const diff = versionDiff(before, after);
  const orderChanged = JSON.stringify(before.ir.nodes.map(n => n.id)) !== JSON.stringify(after.ir.nodes.map(n => n.id));
  return <><p>r{before.revision} → r{after.revision} · {diff.length} 个节点内容有差异</p>{orderChanged && <div className="source-record"><strong>节点顺序发生变化</strong><p className="path-text">之前：{before.ir.nodes.map(n => n.id).join(" → ")}</p><p className="path-text">当前：{after.ir.nodes.map(n => n.id).join(" → ")}</p></div>}{diff.map(change => <div key={change.id} className="source-record"><h3>{change.after?.title ?? change.before?.title}</h3><div className="compare-view"><div><strong>之前 · r{before.revision}</strong><pre className="json-view">{change.before ? JSON.stringify(change.before, null, 2) : "（新增节点）"}</pre></div><div><strong>当前 · r{after.revision}</strong><pre className="json-view">{change.after ? JSON.stringify(change.after, null, 2) : "（已删除节点）"}</pre></div></div></div>)}{!diff.length && <p className="empty-note">节点内容相同。</p>}<details><summary>完整 IR 差异依据（包含来源、边、纠错与待确认）</summary><div className="compare-view"><pre className="json-view">{JSON.stringify(before.ir, null, 2)}</pre><pre className="json-view">{JSON.stringify(after.ir, null, 2)}</pre></div></details></>;
}
