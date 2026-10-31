import { useRef, useState } from "react";
import { Icons } from "../components/Icons";
import { Panel, StatusPill } from "../components/Panel";
import type { KnowledgeNode, ProjectData, SourceFile } from "../lib/types";

interface SourcesPageProps {
  project: ProjectData;
  activeNodeId: string | null;
  onSelectNode: (id: string) => void;
  onAddSource: (source: SourceFile) => void;
}

export function SourcesPage({ project, activeNodeId, onSelectNode, onAddSource }: SourcesPageProps) {
  const [dragging, setDragging] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const activeNode = project.nodes.find((node) => node.id === activeNodeId) ?? project.nodes[1];

  function addFiles(files: FileList | null) {
    if (!files) return;
    for (const file of Array.from(files)) {
      const kind = file.name.endsWith(".pdf") ? "pdf" : file.name.match(/\.(png|jpg|jpeg|webp)$/i) ? "image" : "markdown";
      onAddSource({
        id: `src-${Date.now()}-${file.name}`,
        name: file.name,
        kind,
        bytes: file.size,
        hash: "pending…",
        status: kind === "image" ? "warning" : "ready",
        note: kind === "image" ? "OCR 待确认" : "等待本地解析",
      });
    }
  }

  return (
    <div className="two-column align-start">
      <div className="side-stack">
        <Panel eyebrow="输入" title="资料库" action={<span className="count-tag">{project.sources.length} 个来源</span>}>
          <div
            className={`drop-zone ${dragging ? "is-dragging" : ""}`}
            onDragOver={(event) => { event.preventDefault(); setDragging(true); }}
            onDragLeave={() => setDragging(false)}
            onDrop={(event) => { event.preventDefault(); setDragging(false); addFiles(event.dataTransfer.files); }}
            onClick={() => fileRef.current?.click()}
            role="button"
            tabIndex={0}
            onKeyDown={(event) => { if (event.key === "Enter") fileRef.current?.click(); }}
          >
            <Icons.upload size={20} />
            <strong>拖入 PDF、Markdown、图片或 LaTeX</strong>
            <span>源文件复制进项目并按 SHA-256 去重</span>
            <input ref={fileRef} hidden type="file" multiple onChange={(event) => addFiles(event.target.files)} />
          </div>
          <div className="source-list">
            {project.sources.map((source) => (
              <div className="source-row" key={source.id}>
                <div className={`file-badge kind-${source.kind}`}>
                  {source.kind === "image" ? <Icons.image size={16} /> : <Icons.file size={16} />}
                </div>
                <div className="source-main">
                  <strong>{source.name}</strong>
                  <span>{source.kind.toUpperCase()} · {formatBytes(source.bytes)} · {source.hash}</span>
                  {source.note && <em>{source.note}</em>}
                </div>
                <StatusPill tone={source.status === "ready" ? "ok" : source.status === "warning" ? "warn" : "muted"}>
                  {source.status === "ready" ? "就绪" : source.status === "warning" ? "待确认" : "OCR 中"}
                </StatusPill>
              </div>
            ))}
          </div>
        </Panel>

        <Panel eyebrow="视觉语言" title="风格采样">
          <div className="style-sample-grid">
            <div className="style-card is-selected">
              <div className="style-swatch classic-swatch"><span /><span /><span /></div>
              <strong>Classic 高密度笔记</strong>
              <span>米白纸面 · 紫色强调 · 细线分组</span>
              <StatusPill tone="accent">使用中</StatusPill>
            </div>
            <div className="style-card">
              <div className="style-swatch mono-swatch"><span /><span /><span /></div>
              <strong>单色板书</strong>
              <span>纯黑纸面 · 白色正文 · 手绘标注</span>
            </div>
          </div>
          <div className="callout subtle">
            <Icons.info size={15} />
            <span>参考图片只影响视觉计划，不会作为知识断言进入 IR。</span>
          </div>
        </Panel>
      </div>

      <div className="side-stack">
        <Panel eyebrow="知识 IR" title="节点与来源锚点" action={<span className="count-tag">{project.nodes.length} 节点</span>}>
          <div className="node-table" role="table" aria-label="知识节点与来源锚点">
            <div className="node-table-head" role="row">
              <span>节点</span><span>类型</span><span>来源锚点</span><span>状态</span>
            </div>
            {project.nodes.map((node) => (
              <button className={`node-table-row ${activeNode?.id === node.id ? "is-active" : ""}`} key={node.id} onClick={() => onSelectNode(node.id)} role="row">
                <span className="node-title-cell"><strong>{node.title}</strong><small>{node.body.slice(0, 42)}…</small></span>
                <span className="kind-chip">{node.kind}</span>
                <span className="anchor-cell">{node.anchors[0]?.label ?? "—"}</span>
                <StatusPill tone={node.anchors.length > 0 ? "ok" : "warn"}>{node.anchors.length > 0 ? "有来源" : "缺少"}</StatusPill>
              </button>
            ))}
          </div>
        </Panel>
        <Panel eyebrow="所选节点" title={activeNode?.title ?? "未选择"} action={<StatusPill tone="ok"><Icons.ok size={12} /> 可追溯</StatusPill>}>
          <NodeDetail node={activeNode} project={project} />
        </Panel>
      </div>
    </div>
  );
}

function NodeDetail({ node, project }: { node: KnowledgeNode; project: ProjectData }) {
  const source = project.sources.find((item) => item.id === node.anchors[0]?.sourceId);
  return (
    <div className="node-detail">
      <div className="detail-grid">
        <div><span>类型</span><strong>{node.kind}</strong></div>
        <div><span>重要度</span><strong>{node.importance}</strong></div>
        <div><span>来源</span><strong>{source?.name ?? "未知"}</strong></div>
        <div><span>位置</span><strong>{node.anchors[0]?.page ? `第 ${node.anchors[0].page} 页` : node.anchors[0]?.line ? `第 ${node.anchors[0].line} 行` : "—"}</strong></div>
      </div>
      <p>{node.body}</p>
      {node.latex && <pre className="latex-block">{node.latex}</pre>}
      <div className="anchor-quote">
        <Icons.book size={14} />
        <span>{node.anchors[0]?.label ?? "尚未建立来源锚点"}</span>
        <button className="text-button">在来源中定位 <Icons.chevron size={12} /></button>
      </div>
    </div>
  );
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
