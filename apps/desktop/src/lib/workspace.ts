import { invoke, isTauri } from "@tauri-apps/api/core";
import type { KnowledgeNode, Project, StyleSpec, TaskVersion } from "./types";

export interface Handoff {
  envelope_json: string; markdown: string; task_id: string; revision: number;
  correction_attempts: number; status: string; last_error: string | null;
}
export interface WorkspaceSnapshot {
  root: string; project: Project; selected_task_id: string | null;
  current_version: TaskVersion | null; preview_pages: { page: number; svg: string }[];
  handoff: Handoff | null;
}
export type BridgeGate = "discover_window" | "reliable_input" | "reliable_completion" | "reliable_capture";
export interface BridgeReport {
  status: string; detail: string; automation_enabled: boolean;
  capabilities?: {
    app_found: boolean; web_view_accessibility: boolean; foreground: boolean;
    login_likely: boolean | null; app_version: string | null;
    window_count: number; accessible_controls: number; automation_enabled: boolean;
    detail?: string;
    gates: Partial<Record<BridgeGate, { passed: boolean; evidence: string }>>;
  } | null;
}
export interface Capabilities {
  version: string; platform: string; tesseract_available: boolean; ocr_languages: string[];
  automation_enabled: boolean; bridge_status: string; bridge_detail: string;
  runtime?: "native" | "browser" | "local-http";
  storage?: string;
  bridge?: BridgeReport | null;
}
export type NodeEdit = Pick<KnowledgeNode, "id" | "title" | "body" | "latex" | "anchors" | "importance">;
type TaskArgs = { taskId: string };
type VersionArgs = TaskArgs & { baseVersionId: string };
export interface Commands {
  automation_access: [undefined, { token: string; base_url: string }];
  open_automation_setup: [undefined, null];
  import_generated_image: [{ taskId: string; baseVersionId: string; runId: string; promptSha256: string; bytes: number[]; expectedSha256: string; mimeType: string }, WorkspaceSnapshot];
  generated_image_bytes: [{ taskId: string; imageId: string }, number[]];
  bootstrap: [undefined, Capabilities];
  workspace_snapshot: [{ taskId?: string }, WorkspaceSnapshot];
  create_project: [{ root: string; name: string }, WorkspaceSnapshot];
  load_existing_project: [{ root: string }, WorkspaceSnapshot];
  create_task: [{ title: string; chapter?: string }, WorkspaceSnapshot];
  import_sources: [TaskArgs & { paths: string[] }, WorkspaceSnapshot];
  import_ir: [TaskArgs & { baseVersionId: string | null; json: string }, WorkspaceSnapshot];
  import_style_images: [{ paths: string[] }, WorkspaceSnapshot];
  reimport_source: [VersionArgs & { sourceId: string }, WorkspaceSnapshot];
  update_nodes: [VersionArgs & { edits: NodeEdit[]; nodeOrder?: string[] }, WorkspaceSnapshot];
  resolve_review: [VersionArgs & { itemId: string; note: string }, WorkspaceSnapshot];
  rollback_version: [TaskArgs & { versionId: string }, WorkspaceSnapshot];
  revert_correction: [VersionArgs & { correctionId: string }, WorkspaceSnapshot];
  undo_visual_repair: [VersionArgs & { repairId: string }, WorkspaceSnapshot];
  save_style: [{ style: StyleSpec }, WorkspaceSnapshot];
  save_settings: [{ ocrLanguages: string }, WorkspaceSnapshot];
  prepare_handoff: [TaskArgs, WorkspaceSnapshot];
  import_result: [TaskArgs & { json: string }, WorkspaceSnapshot];
  validate_current: [TaskArgs, WorkspaceSnapshot];
  preview_current: [TaskArgs, WorkspaceSnapshot];
  export_current: [TaskArgs & { outputDir?: string }, WorkspaceSnapshot];
  export_diagnostics: [undefined, string | null];
  choose_directory: [undefined | { purpose: "create" | "open"; name?: string }, string | null];
  choose_sources: [undefined, string[]];
  choose_result_file: [undefined, string | null];
  choose_ir_file: [undefined, string | null];
  copy_prompt: [TaskArgs, null];
  open_chatgpt: [undefined, null];
  open_export: [TaskArgs & { kind: "svg" | "png" | "pdf" | "ir" | "manifest"; index?: number }, null];
}
export interface WorkspaceClient {
  available: boolean;
  runtime?: "native" | "browser" | "local-http";
  projects?(): Promise<{ root: string; name: string; id: string }[]>;
  call<K extends keyof Commands>(command: K, args: Commands[K][0]): Promise<Commands[K][1]>;
}
export const nativeClient: WorkspaceClient = {
  available: isTauri(),
  runtime: "native",
  call: (command, args) => invoke(command, args),
};
export function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : typeof error === "string" ? error : JSON.stringify(error);
}
export function nodeChanges(before: KnowledgeNode[], after: KnowledgeNode[]): NodeEdit[] {
  const previous = new Map(before.map(node => [node.id, node]));
  return after.filter(node => JSON.stringify(node) !== JSON.stringify(previous.get(node.id))).map(({ id, title, body, latex, anchors, importance }) => ({ id, title, body, latex, anchors, importance }));
}
export function versionDiff(before: TaskVersion, after: TaskVersion) {
  const a = new Map(before.ir.nodes.map(n => [n.id, n]));
  const b = new Map(after.ir.nodes.map(n => [n.id, n]));
  return [...new Set([...a.keys(), ...b.keys()])].flatMap(id => {
    const old = a.get(id); const current = b.get(id);
    if (JSON.stringify(old) === JSON.stringify(current)) return [];
    return [{ id, before: old, after: current }];
  });
}

// Export can resolve with a blocked snapshot: applying it is necessary to show
// the persisted report, but resolving IPC alone does not establish success.
export function exportOutcomeError(snapshot: WorkspaceSnapshot | null): string | null {
  const version = snapshot?.current_version;
  const task = snapshot?.project.tasks.find(t => t.id === snapshot.selected_task_id);
  const blockers = version?.validation?.findings.filter(f => f.severity === "blocking") ?? [];
  const artifacts = version?.artifacts;
  const completeArtifacts = artifacts && artifacts.svg.length > 0 && artifacts.svg.length === artifacts.png.length
    && [...artifacts.svg, ...artifacts.png, artifacts.pdf, artifacts.manifest, artifacts.ir].every(path => path.trim().length > 0);
  if (task?.state === "EXPORTED" && task.current_version === version?.id && version?.validation
      && !task.last_error && blockers.length === 0 && version.ir.unverified.length === 0 && completeArtifacts) return null;
  const details = [...new Set([task?.last_error, ...blockers.map(f => f.message)].filter(Boolean))];
  return `导出未完成：${details.join("；") || "本地服务未返回当前版本的完整、已批准导出文件。"}`;
}
