import { useState } from "react";
import { Icons } from "../components/Icons";
import { KnowledgePreview } from "../components/KnowledgePreview";
import { Panel, StatusPill } from "../components/Panel";
import type { KnowledgeNode, ProjectData, ValidationFinding, Version } from "../lib/types";

interface ResultPageProps {
  project: ProjectData;
  findings: ValidationFinding[];
  activeNodeId: string | null;
  onSelectNode: (id: string) => void;
  onEditNode: (node: KnowledgeNode) => void;
  versions: Version[];
  onExport: () => void;
  exportNotice: string | null;
}

export function ResultPage({ project, findings, activeNodeId, onSelectNode, onEditNode, versions, onExport, exportNotice }: ResultPageProps) {
  const [tab, setTab] = useState<"preview" | "compare">("preview");
  const node = project.nodes.find((item) => item.id === activeNodeId) ?? project.nodes[1];
  const blocking = findings.filter((item) => item.severity === "blocking");
  return (
    <div className="page-stack">
      <div className="result-head">
        <div className="result-status">
          <StatusPill tone={blocking.length ? "warn" : "ok"}>{blocking.length ? `${blocking.length} 个阻断项` : "全部门禁通过"}</StatusPill>
          <span>内容优先 · 公式可追溯 · 无重叠 / 无裁切</span>
        </div>
        <div className="segmented">
          <button className={tab === "preview" ? "is-active" : ""} onClick={() => setTab("preview")}><Icons.grid size={13} /> 版面</button>
          <button className={tab === "compare" ? "is-active" : ""} onClick={() => setTab("compare")}><Icons.layers size={13} /> 版本对比</button>
        </div>
        <button className="primary-button" onClick={onExport}><Icons.download size={14} /> 导出 SVG / PNG / PDF</button>
      </div>
      {exportNotice && <div className="toast" role="status"><Icons.ok size={14} /> {exportNotice}</div>}

      <div className="result-grid">
        <Panel className="result-canvas-panel" eyebrow={tab === "preview" ? "A3 矢量页面" : "版本对比"} title={tab === "preview" ? "知识图 · 第 1 / 2 页" : "r3 手工编辑 vs r2 GPT 导入"}>
          {tab === "preview" ? (
            <KnowledgePreview nodes={project.nodes} edges={project.edges} activeNodeId={activeNodeId} onSelectNode={onSelectNode} compact />
          ) : (
            <div className="compare-view">
              <div className="compare-column"><span className="compare-label">当前 · r3</span><div className="compare-card"><strong>条件概率</strong><p>条件概率仍然是概率：它对互斥事件可加，并满足 P(Ω | B) = 1。</p><pre>{"P(A \\mid B) = \\frac{P(A \\cap B)}{P(B)}"}</pre><span className="diff-added">+ 手工加宽公式卡片</span></div></div>
              <div className="compare-column"><span className="compare-label">基线 · r2</span><div className="compare-card"><strong>条件概率</strong><p>条件概率仍然是概率：它对互斥事件可加，并满足 P(Ω | B) = 1。</p><pre>{"P(A \\mid B)=P(A \\cap B)/P(B)"}</pre><span className="diff-removed">− 原始自动排版</span></div></div>
            </div>
          )}
          <div className="preview-foot">
            <StatusPill tone="ok"><Icons.ok size={12} /> 页面边界正常</StatusPill>
            <span>最小字号 8.5pt</span><span>对比度 13.8:1</span><span>PDF / SVG 同场景</span>
          </div>
        </Panel>

        <div className="side-stack">
          <Panel eyebrow="校验报告" title="内容与版面门禁" action={<StatusPill tone="ok">PASS</StatusPill>}>
            <div className="finding-list">
              <div className="finding-row"><span className="finding-icon ok"><Icons.ok size={13} /></span><div><strong>来源一致性</strong><p>8/8 节点带来源锚点；所有关键节点可追溯。</p></div></div>
              <div className="finding-row"><span className="finding-icon ok"><Icons.ok size={13} /></span><div><strong>公式完整性</strong><p>4 个公式定界符平衡；无静默改动。</p></div></div>
              <div className="finding-row"><span className="finding-icon ok"><Icons.ok size={13} /></span><div><strong>版面安全</strong><p>2 页 A3；无节点重叠、无页面越界、无文本裁切。</p></div></div>
              {findings.map((finding) => (
                <div className="finding-row" key={finding.id}><span className={`finding-icon ${finding.severity === "warning" ? "warn" : "info"}`}>{finding.severity === "warning" ? <Icons.alert size={13} /> : <Icons.info size={13} />}</span><div><strong>{finding.code}</strong><p>{finding.message}</p></div></div>
              ))}
            </div>
          </Panel>
          <Panel eyebrow="节点编辑" title={node.title} action={<StatusPill tone="accent">已锚定</StatusPill>}>
            <label className="field"><span>标题</span><input value={node.title} onChange={(event) => onEditNode({ ...node, title: event.target.value })} /></label>
            <label className="field"><span>正文</span><textarea rows={4} value={node.body} onChange={(event) => onEditNode({ ...node, body: event.target.value })} /></label>
            <label className="field"><span>LaTeX</span><textarea rows={2} className="mono" value={node.latex ?? ""} onChange={(event) => onEditNode({ ...node, latex: event.target.value })} /></label>
            <div className="field-row"><span className="field-label">重要度</span>{(["critical", "high", "normal", "low"] as const).map((value) => <button key={value} className={`chip ${node.importance === value ? "is-active" : ""}`} onClick={() => onEditNode({ ...node, importance: value })}>{value}</button>)}</div>
            <div className="anchor-quote"><Icons.book size={14} /><span>{node.anchors[0]?.label}</span></div>
          </Panel>
        </div>
      </div>

      <Panel eyebrow="导出" title="已批准版本产物">
        <div className="export-row">
          <div className="export-file"><Icons.file size={16} /><div><strong>entwurf-a3.pdf</strong><span>A3 矢量 · 2 页 · 共享同一场景</span></div><StatusPill tone="ok">可导出</StatusPill></div>
          <div className="export-file"><Icons.grid size={16} /><div><strong>page-01.svg / page-02.svg</strong><span>权威矢量页面 · 重新导入 IR</span></div><StatusPill tone="ok">可导出</StatusPill></div>
          <div className="export-file"><Icons.image size={16} /><div><strong>preview-01.png</strong><span>300 DPI 预览，仅用于检查</span></div><StatusPill tone="muted">预览</StatusPill></div>
          <div className="export-file"><Icons.file size={16} /><div><strong>manifest.json</strong><span>哈希与来源清单</span></div><StatusPill tone="ok">可导出</StatusPill></div>
        </div>
        <div className="version-mini">{versions.slice(0, 3).map((version) => <span key={version.id}>r{version.revision} · {version.reason}</span>)}</div>
      </Panel>
    </div>
  );
}
