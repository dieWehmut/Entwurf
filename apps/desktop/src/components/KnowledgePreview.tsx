import { useMemo } from "react";
import { Icons } from "./Icons";
import type { GraphEdge, KnowledgeNode } from "../lib/types";

interface KnowledgePreviewProps {
  nodes: KnowledgeNode[];
  edges: GraphEdge[];
  activeNodeId: string | null;
  onSelectNode: (id: string) => void;
  compact?: boolean;
}

const cardPositions: Record<string, { x: number; y: number; w: number; h: number }> = {
  "n-chapter": { x: 36, y: 70, w: 720, h: 58 },
  "n-rv": { x: 36, y: 142, w: 348, h: 128 },
  "n-cdf": { x: 402, y: 142, w: 354, h: 128 },
  "n-cond": { x: 36, y: 286, w: 348, h: 154 },
  "n-total": { x: 402, y: 286, w: 354, h: 142 },
  "n-bayes": { x: 36, y: 456, w: 348, h: 154 },
  "n-independence": { x: 402, y: 444, w: 354, h: 112 },
  "n-zero": { x: 402, y: 572, w: 354, h: 72 },
};

const edgePaths: Record<string, string> = {
  e1: "M 384 206 H 402",
  e2: "M 210 440 V 456",
  e3: "M 402 336 H 384",
  e4: "M 384 350 H 402",
  e5: "M 579 556 V 572",
};

const kindLabel: Record<string, string> = {
  chapter: "章节",
  section: "小节",
  concept: "概念",
  definition: "定义",
  formula: "公式",
  theorem: "定理",
  example: "例题",
  warning: "注意",
  note: "笔记",
};

export function KnowledgePreview({ nodes, edges, activeNodeId, onSelectNode, compact = false }: KnowledgePreviewProps) {
  const positions = useMemo(() => {
    return nodes.reduce<Record<string, { x: number; y: number; w: number; h: number }>>((acc, node, index) => {
      acc[node.id] = cardPositions[node.id] ?? {
        x: 36 + (index % 2) * 368,
        y: 142 + Math.floor(index / 2) * 128,
        w: 348,
        h: 112,
      };
      return acc;
    }, {});
  }, [nodes]);

  return (
    <div className={`canvas-wrap ${compact ? "is-compact" : ""}`}>
      <div className="canvas-toolbar">
        <span><Icons.grid size={13} /> A3 · 2 页</span>
        <span><Icons.layers size={13} /> 8 节点 / 5 连线</span>
        <span className="canvas-status"><span className="status-dot" /> 校验通过</span>
      </div>
      <div className="canvas-scroll">
        <svg className="knowledge-canvas" viewBox="0 0 792 680" role="img" aria-label="知识图预览">
          <defs>
            <pattern id="grid" width="18" height="18" patternUnits="userSpaceOnUse">
              <path d="M 18 0 L 0 0 0 18" fill="none" stroke="var(--canvas-grid)" strokeWidth="0.55" />
            </pattern>
            <marker id="arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="5" markerHeight="5" orient="auto-start-reverse">
              <path d="M 0 0 L 10 5 L 0 10 z" fill="var(--accent-muted)" />
            </marker>
          </defs>
          <rect width="792" height="680" fill="url(#grid)" />
          <g className="canvas-title">
            <text x="36" y="36" className="canvas-project">概率论 · 第一章</text>
            <text x="756" y="36" textAnchor="end" className="canvas-meta">Classic · A3 横向</text>
          </g>
          {edges.map((edge) => {
            const path = edgePaths[edge.id] ?? "M 380 340 H 420";
            return (
              <g key={edge.id} className="canvas-edge">
                <path d={path} markerEnd="url(#arrow)" />
                {edge.label && <text x={path.includes("H") ? 396 : 216} y={path.includes("H") ? 332 : 452} className="edge-label">{edge.label}</text>}
              </g>
            );
          })}
          {nodes.map((node) => {
            const pos = positions[node.id];
            if (!pos) return null;
            const isChapter = node.kind === "chapter";
            return (
              <g
                key={node.id}
                className={`canvas-node kind-${node.kind} ${activeNodeId === node.id ? "is-selected" : ""}`}
                onClick={() => onSelectNode(node.id)}
                role="button"
                tabIndex={0}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") onSelectNode(node.id);
                }}
                aria-label={`${kindLabel[node.kind]}：${node.title}`}
              >
                <rect x={pos.x} y={pos.y} width={pos.w} height={pos.h} rx={isChapter ? 6 : 9} className="node-card" />
                {!isChapter && <rect x={pos.x} y={pos.y + 8} width={3} height={pos.h - 16} rx={1.5} className="node-accent" />}
                <text x={pos.x + 16} y={pos.y + 24} className="node-kind">{kindLabel[node.kind]}</text>
                <text x={pos.x + 16} y={isChapter ? pos.y + 26 : pos.y + 44} className={`node-title ${isChapter ? "is-chapter" : ""}`}>{truncate(node.title, isChapter ? 36 : 26)}</text>
                {!isChapter && <text x={pos.x + 16} y={pos.y + 62} className="node-body">{truncate(node.body, 52)}</text>}
                {node.latex && (
                  <text x={pos.x + 16} y={pos.y + 86} className="node-formula">{truncate(node.latex, 34)}</text>
                )}
                {!isChapter && (
                  <text x={pos.x + 16} y={pos.y + pos.h - 12} className="node-anchor">
                    {node.anchors[0]?.label ?? "缺少来源锚点"}
                  </text>
                )}
              </g>
            );
          })}
        </svg>
      </div>
    </div>
  );
}

function truncate(value: string, length: number) {
  return value.length > length ? `${value.slice(0, length - 1)}…` : value;
}
