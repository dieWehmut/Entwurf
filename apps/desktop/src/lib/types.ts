export type Theme = "dark" | "light";
export type PageId = "project" | "sources" | "tasks" | "result" | "settings";

export interface Anchor {
  sourceId: string;
  page?: number;
  line?: number;
  label: string;
}

export type NodeKind = "chapter" | "section" | "concept" | "definition" | "formula" | "theorem" | "example" | "warning" | "note";
export type Importance = "critical" | "high" | "normal" | "low";
export type Origin = "source" | "gpt_candidate" | "user_edit" | "gpt_correction";

export interface KnowledgeNode {
  id: string;
  kind: NodeKind;
  title: string;
  body: string;
  latex: string | null;
  anchors: Anchor[];
  importance: Importance;
  origin: Origin;
  parentId: string | null;
  tags: string[];
  verified: boolean;
  reviewNote: string | null;
}

export interface SourceFile {
  id: string;
  name: string;
  kind: "pdf" | "markdown" | "image" | "latex";
  pages?: number;
  bytes: number;
  hash: string;
  status: "ready" | "warning" | "ocring";
  note?: string;
}

export interface GraphEdge {
  id: string;
  from: string;
  to: string;
  type: "depends_on" | "related" | "example_of" | "contradicts";
  label?: string;
}

export interface Version {
  id: string;
  revision: number;
  reason: "ingested" | "gpt_imported" | "user_edited" | "correction_reverted" | "manual_snapshot";
  createdAt: string;
  approved: boolean;
  summary: string;
}

export interface ValidationFinding {
  id: string;
  code: string;
  severity: "blocking" | "warning" | "info";
  message: string;
  nodeId?: string;
}

export interface Task {
  id: string;
  title: string;
  chapter: string;
  state: string;
  revision: number;
  pages: number;
  updatedAt: string;
  progress: number;
  automation: "manual" | "waiting" | "paused" | "ready";
  findings: ValidationFinding[];
  versions: Version[];
}

export interface ProjectData {
  id: string;
  name: string;
  subject: string;
  language: string;
  styleName: string;
  updatedAt: string;
  sources: SourceFile[];
  nodes: KnowledgeNode[];
  edges: GraphEdge[];
  tasks: Task[];
}

export interface Capability {
  id: string;
  label: string;
  status: "ready" | "missing" | "manual";
  detail: string;
}
