// Public synthetic protocol fixture. This simulates the IPC boundary only; it is not a native filesystem/compiler test.
import type { KnowledgeNode, StyleSpec, TaskVersion, VisualRepairRecord } from '../src/lib/types';
import type { Capabilities, WorkspaceSnapshot } from '../src/lib/workspace';
export const capabilities: Capabilities = { version: '0.1.0', platform: 'fixture', tesseract_available: false, ocr_languages: [], automation_enabled: false, bridge_status: 'manual_only', bridge_detail: 'Deterministic protocol fixture; no native services running.', bridge: {
  status: 'read_only', detail: 'Public fixture for native discovery report rendering.', automation_enabled: false,
  capabilities: { app_found: false, web_view_accessibility: false, foreground: false, login_likely: null, app_version: null, window_count: 0, accessible_controls: 0, automation_enabled: false,
    gates: { discover_window: { passed: false, evidence: 'No visible ChatGPT window found.' }, reliable_input: { passed: false, evidence: 'Input unverified.' }, reliable_completion: { passed: false, evidence: 'Completion unverified.' }, reliable_capture: { passed: false, evidence: 'Capture unverified.' } } },
} };
export const node: KnowledgeNode = { id: 'node-1', kind: 'definition', title: '公开测试定义', body: '用于交互测试的公开合成正文。', latex: 'P(A)=1', anchors: [{ source_id: 'source-1', label: '公开样本第 3 行', page: 2, line: 3, char_start: 10, char_end: 24, quote_hash: 'a'.repeat(64), region: { left: 12, top: 34, width: 56, height: 78, unit: 'pdf_millipoints' } }], importance: 'critical', origin: 'source', parent_id: null, tags: ['public'], verified: true, review_note: null };
export const style: StyleSpec = { id: 'classic', name: 'Classic', paper: { width_mm: 420, height_mm: 297, margin_mm: 12, max_pages: 3 }, colors: { background: '#ffffff', surface: '#ffffff', text: '#000000', muted: '#555555', accent: '#8f35ff', border: '#888888' }, typography: { family: 'LXGW WenKai', title_pt: 14, body_pt: 10, caption_pt: 8, line_height: 1.4 }, density: 'dense', reference_images: [], rules: [] };
export function fixture(): WorkspaceSnapshot {
  const at = '2026-01-01T00:00:00Z';
  const version: TaskVersion = { id: 'version-1', revision: 1, created_at: at, reason: 'ingested', ir: { schema_version: 1, id: 'ir-1', project_id: 'project-1', task_id: 'task-1', title: '公开协议样本', language: 'zh-CN', sources: [{ id: 'source-1', original_name: 'public.md', kind: 'markdown', content_hash: 'a'.repeat(64), stored_path: 'sources/公开 sample.md', byte_len: 42, imported_at: at, notes: [] }], nodes: [structuredClone(node), { ...structuredClone(node), id: 'node-2', title: '第二个公开节点' }], edges: [], corrections: [], unverified: [], updated_at: at }, layout: null, validation: null, artifacts: null, source_prompt_hash: null };
  return { root: 'C:\\Entwurf 测试\\public', selected_task_id: 'task-1', current_version: version, preview_pages: [], handoff: null, project: { schema_version: 1, id: 'project-1', name: '公开测试项目', created_at: at, updated_at: at, style: structuredClone(style), settings: { automation_enabled: false, tesseract_available: false, ocr_languages: 'chi_sim+eng', locale: 'zh-CN' }, audit: [], tasks: [{ schema_version: 1, id: 'task-1', title: '公开测试任务', chapter: '协议测试', state: 'IR_READY', created_at: at, updated_at: at, versions: [version], current_version: version.id, paused_from: null, last_error: null }] } };
}

// Canonical repair wire fixture; hashes/options are public synthetic test data.
export function repairFixture(): VisualRepairRecord {
  const before = { max_pages: 3, min_font_pt: 8, max_font_pt: 11, column_gap_pt: 12, node_gap_pt: 6, card_padding_pt: 8, preferred_columns: 3, obstacle_routing: false, max_heading_font_pt: 28 };
  return { id: 'repair-1', reason: '公开测试：缩小卡片间距', ir_hash: 'a'.repeat(64), style_hash: 'b'.repeat(64), round: 1, before, after: { ...before, node_gap_pt: 4, obstacle_routing: true }, passed: true, reverted: false, before_geometry_hash: 'c'.repeat(64), after_geometry_hash: 'd'.repeat(64) };
}
