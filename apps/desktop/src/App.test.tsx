import { render, screen, within, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import App from './App';
import { fixture, capabilities, repairFixture, emptyTaskFixture, gptImportFixture } from '../tests/protocol-fixture';
import { versionDiff, type WorkspaceClient, type WorkspaceSnapshot } from './lib/workspace';
function setup(handler?: (command: string, args: any, state: WorkspaceSnapshot) => unknown, initialState = fixture()) {
  let state = initialState;
  const call = vi.fn(async (command: string, args: any) => {
    if (command === 'bootstrap') return capabilities;
    if (command === 'workspace_snapshot') return structuredClone(state);
    const result = await handler?.(command, args, state);
    if (result && typeof result === 'object' && 'project' in result) state = result as WorkspaceSnapshot;
    return result === undefined ? structuredClone(state) : result;
  });
  render(<App client={{ available: true, call } as WorkspaceClient} />);
  return { call, user: userEvent.setup(), state, setState: (next: WorkspaceSnapshot) => { state = next; } };
}
async function navigate(user: ReturnType<typeof userEvent.setup>, name: string) {
  await screen.findByRole('combobox', { name: '当前任务' });
  await user.click(within(screen.getByRole('navigation')).getByRole('button', { name: new RegExp(`^${name}`) }));
}
beforeEach(() => localStorage.clear());
describe('desktop workspace protocol', () => {
  it('browser mode has no fake project, export or validation result', () => {
    render(<App client={{ available: false, call: vi.fn() }} />);
    expect(screen.getByText(/浏览器预览：本地工作区不可用/)).toBeVisible();
    expect(screen.getByRole('button', { name: '打开项目' })).toBeDisabled();
    expect(screen.queryByText(/PASS|已准备 ArtifactSet|内容门禁通过/)).not.toBeInTheDocument();
  });
  it('preserves complete anchor fields and saves only changed nodes with base version and order', async () => {
    const { user, call, state } = setup((command, args, state) => {
      if (command !== 'update_nodes') return;
      const next = structuredClone(state); next.current_version!.id = 'version-2';
      next.current_version!.ir.nodes[0] = { ...next.current_version!.ir.nodes[0], ...args.edits[0] }; return next;
    });
    await navigate(user, '结果');
    await user.clear(screen.getByLabelText('节点标题')); await user.type(screen.getByLabelText('节点标题'), '已编辑的标题');
    expect(screen.getByRole('button', { name: /导出 SVG/ })).toBeDisabled();
    await user.click(screen.getByRole('button', { name: '保存节点修改' }));
    await waitFor(() => expect(call).toHaveBeenCalledWith('update_nodes', expect.objectContaining({ taskId: 'task-1', baseVersionId: 'version-1', nodeOrder: ['node-1', 'node-2'], edits: [expect.objectContaining({ title: '已编辑的标题', anchors: state.current_version!.ir.nodes[0].anchors })] })));
    expect(await screen.findByText('所有编辑已保存')).toBeVisible();
  });
  it('keeps rejected and stale drafts intact and blocks export', async () => {
    const { user, setState, state } = setup(command => { if (command === 'update_nodes') throw new Error('stale baseVersionId'); });
    await navigate(user, '结果'); await user.type(screen.getByLabelText('节点正文'), ' 本地草稿');
    const next = structuredClone(state); next.current_version!.id = 'version-other'; setState(next);
    await user.click(screen.getByRole('button', { name: '保存节点修改' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('stale baseVersionId');
    expect((screen.getByLabelText('节点正文') as HTMLTextAreaElement).value).toContain('本地草稿');
    expect(screen.getByRole('button', { name: '保存节点修改' })).toBeDisabled();
    expect(screen.getByText(/当前版本已改变/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: '放弃草稿' }));
    expect((screen.getByLabelText('节点正文') as HTMLTextAreaElement).value).not.toContain('本地草稿');
  });
  it('rejects malformed anchor JSON without changing saved data', async () => {
    const { user, call } = setup(); await navigate(user, '结果');
    await user.clear(screen.getByLabelText('来源锚点 JSON')); await user.type(screen.getByLabelText('来源锚点 JSON'), 'broken');
    await user.click(screen.getByRole('button', { name: '保存节点修改' }));
    expect(await screen.findByRole('alert')).toBeVisible();
    expect(call.mock.calls.some(([cmd]) => cmd === 'update_nodes')).toBe(false);
    expect(screen.getByLabelText('来源锚点 JSON')).toHaveValue('broken');
  });
  it('imports style images through a separate native command', async () => {
    const { user, call } = setup(command => command === 'choose_sources' ? ['C:\\public\\style.png'] : undefined);
    await navigate(user, '资料与风格'); await user.selectOptions(screen.getByLabelText('导入用途'), 'style');
    await user.click(screen.getByRole('button', { name: '选择本机文件并导入' }));
    await waitFor(() => expect(call).toHaveBeenCalledWith('import_style_images', { paths: ['C:\\public\\style.png'] }));
    expect(call.mock.calls.some(([cmd]) => cmd === 'import_sources')).toBe(false);
  });
  it('shows backend retry state and enforces manual mode at second rejection', async () => {
    let attempts = 0;
    const { user } = setup((command, _args, state) => {
      if (command === 'prepare_handoff') state.handoff = { envelope_json: '{}', markdown: 'public prompt', task_id: 'task-1', revision: 1, correction_attempts: 0, status: 'prepared', last_error: null };
      if (command === 'import_result') { attempts++; state.handoff!.correction_attempts = attempts; state.handoff!.status = attempts === 1 ? 'correction_required' : 'manual_required'; state.handoff!.last_error = 'Invalid JSON'; throw new Error('Invalid JSON'); }
      return structuredClone(state);
    });
    await navigate(user, '任务'); await user.click(screen.getByRole('button', { name: '生成 TaskEnvelope' }));
    await user.type(screen.getByLabelText('ResultEnvelope JSON'), 'bad'); await user.click(screen.getByRole('button', { name: '导入 JSON' }));
    expect(await screen.findByText(/第一次导入失败/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: '导入 JSON' }));
    expect(await screen.findByText(/第二次导入失败/)).toBeVisible();
    expect(screen.getByRole('button', { name: '导入 JSON' })).toBeDisabled();
  });
  it('opens only artifact paths returned by native export', async () => {
    const { user, call } = setup((command, _args, state) => {
      if (command === 'export_current') {
        state.current_version!.artifacts = { svg: ['exports/page-1.svg'], png: ['exports/page-1.png'], pdf: 'exports/result.pdf', manifest: 'exports/manifest.json', ir: 'exports/knowledge.ir.json' };
        state.current_version!.validation = { schema_version: 1, checked_at: '2026-01-01T00:00:00Z', metrics: {}, findings: [] };
        state.project.tasks[0].state = 'EXPORTED';
      }
      return structuredClone(state);
    }); await navigate(user, '结果');
    expect(screen.queryByRole('button', { name: '打开 PDF' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: /导出 SVG/ }));
    expect(await screen.findByText('导出文件完成')).toBeVisible();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
    await user.click(await screen.findByRole('button', { name: '打开 PDF' }));
    await waitFor(() => expect(call).toHaveBeenCalledWith('open_export', { taskId: 'task-1', kind: 'pdf', index: 0 }));
  });
  it('comparison identifies actual additions, removals and changes', () => {
    const before = fixture().current_version!; const after = structuredClone(before);
    after.ir.nodes[0].body = 'changed'; after.ir.nodes.splice(1, 1); after.ir.nodes.push({ ...after.ir.nodes[0], id: 'node-3' });
    const diff = versionDiff(before, after); expect(diff.map(d => d.id)).toEqual(['node-1', 'node-2', 'node-3']);
    expect(diff[1].after).toBeUndefined(); expect(diff[2].before).toBeUndefined();
  });
});

it('creates projects and tasks through native dialogs and commands', async () => {
  const { user, call } = setup(command => command === 'choose_directory' ? 'C:\\项目 空间' : undefined);
  await screen.findByLabelText('当前任务');
  await user.type(screen.getByLabelText('项目名称'), '本地项目'); await user.click(screen.getByRole('button', { name: '新建项目' }));
  await waitFor(() => expect(call).toHaveBeenCalledWith('create_project', { root: 'C:\\项目 空间', name: '本地项目' }));
  await user.click(screen.getByRole('button', { name: '打开项目' }));
  await waitFor(() => expect(call).toHaveBeenCalledWith('load_existing_project', { root: 'C:\\项目 空间' }));
  await user.type(screen.getByLabelText('任务标题'), '新任务'); await user.type(screen.getByLabelText('章节（可选）'), '第一章');
  await user.click(screen.getByRole('button', { name: '创建任务' }));
  await waitFor(() => expect(call).toHaveBeenCalledWith('create_task', { title: '新任务', chapter: '第一章' }));
  expect(screen.getByRole('heading', { name: '先确认来源，再决定构图。' })).toBeVisible();
});
it('requires a review note and sends one explicit review item with its base version', async () => {
  const { user, state } = setup((command, args, state) => {
    if (command === 'resolve_review') {
      expect(args).toEqual({ taskId: 'task-1', baseVersionId: 'version-1', itemId: 'review-1', note: '已对照原件' });
      state.current_version!.ir.unverified = []; return structuredClone(state);
    }
  });
  state.current_version!.ir.unverified = [{ id: 'review-1', node_id: 'node-1', kind: 'formula', text: '公式待确认', reason: '检查来源' }];
  await navigate(user, '结果');
  await user.click(screen.getByRole('button', { name: '刷新工作区' }));
  expect(await screen.findByRole('button', { name: '记录审阅并确认' })).toBeDisabled();
  await user.type(screen.getByLabelText('审阅说明 · review-1'), '已对照原件'); await user.click(screen.getByRole('button', { name: '记录审阅并确认' }));
  await waitFor(() => expect(screen.queryByText('公式待确认')).not.toBeInTheDocument());
});
it('saves formula and order while keeping other canonical node fields intact', async () => {
  const { user, call, state } = setup(); await navigate(user, '结果');
  await user.clear(screen.getByLabelText('公式 LaTeX')); await user.type(screen.getByLabelText('公式 LaTeX'), 'P(A)=0');
  await user.click(screen.getByRole('button', { name: '节点下移' })); await user.click(screen.getByRole('button', { name: '保存节点修改' }));
  await waitFor(() => expect(call).toHaveBeenCalledWith('update_nodes', expect.objectContaining({ edits: [expect.objectContaining({ id: 'node-1', latex: 'P(A)=0', anchors: state.current_version!.ir.nodes[0].anchors })], nodeOrder: ['node-2', 'node-1'] })));
});
it('quick jump works from keyboard and restores focus on Escape', async () => {
  const { user } = setup(); await screen.findByLabelText('当前任务');
  const trigger = screen.getByRole('button', { name: /快速跳转/ }); trigger.focus();
  await user.keyboard('{Control>}k{/Control}'); expect(await screen.findByRole('dialog')).toBeVisible();
  await user.keyboard('{Escape}'); expect(trigger).toHaveFocus();
  await user.click(trigger); await user.type(screen.getByLabelText('搜索页面'), '设置'); await user.keyboard('{Enter}');
  expect(screen.getByRole('heading', { name: '本地、可观测、可回退。' })).toBeVisible();
});

describe('review regressions: authoritative export outcomes', () => {
  it.each([
    ['layout_overlap', '布局存在重叠', null],
    ['export_failed', '磁盘空间不足', '磁盘空间不足'],
  ])('surfaces %s from a resolved snapshot without reporting success', async (code, message, lastError) => {
    const { user, call } = setup((command, _args, state) => {
      if (command === 'export_current') {
        state.current_version!.artifacts = null;
        state.current_version!.validation = { schema_version: 1, checked_at: '2026-01-01T00:00:00Z', metrics: {}, findings: [{ id: 'blocked', code, severity: 'blocking', message, node_id: null, evidence: null }] };
        state.project.tasks[0].state = 'NEEDS_REVIEW'; state.project.tasks[0].last_error = lastError;
      }
      return structuredClone(state);
    });
    await navigate(user, '结果'); await user.click(screen.getByRole('button', { name: /导出 SVG/ }));
    expect(await screen.findByRole('alert')).toHaveTextContent(message);
    expect(screen.queryByText('导出文件完成')).not.toBeInTheDocument();
    expect(screen.getByText(`blocking · ${code}`)).toBeVisible();
    expect(screen.queryByRole('button', { name: '打开 PDF' })).not.toBeInTheDocument();
    expect(call.mock.calls.filter(([command]) => command === 'workspace_snapshot')).toHaveLength(1);
  });
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
describe('review regressions: pending form inputs', () => {
  it.each([
    ['style', 'save_style', '资料与风格', '风格名称', '保存风格', 'saved name'],
    ['ocr', 'save_settings', 'settings', 'OCR 语言', '保存 OCR 设置', 'eng'],
    ['result', 'import_result', '任务', 'ResultEnvelope JSON', '导入 JSON', 'submitted-json'],
    ['task', 'create_task', '项目', '任务标题', '创建任务', '新任务'],
  ])('locks %s fields until the submitted snapshot returns', async (_name, command, page, field, button, value) => {
    const request = deferred<WorkspaceSnapshot>();
    const { user, call } = setup((cmd, _args, state) => {
      if (cmd === command) return request.promise;
      if (cmd === 'prepare_handoff') state.handoff = { envelope_json: '{}', markdown: 'prompt', task_id: 'task-1', revision: 1, correction_attempts: 0, status: 'prepared', last_error: null };
      return structuredClone(state);
    });
    await screen.findByLabelText('当前任务');
    if (page === 'settings') await user.click(screen.getByRole('button', { name: '打开设置' })); else await navigate(user, page);
    if (command === 'import_result') await user.click(screen.getByRole('button', { name: '生成 TaskEnvelope' }));
    const input = screen.getByLabelText(field);
    await user.clear(input); await user.type(input, value); await user.click(screen.getByRole('button', { name: button }));
    await waitFor(() => expect(call.mock.calls.some(([cmd]) => cmd === command)).toBe(true));
    expect(input).toBeDisabled();
    await user.type(input, ' newer unsaved text'); expect(input).toHaveValue(value);
    const next = fixture(); if (command === 'save_style') next.project.style.name = value; if (command === 'save_settings') next.project.settings.ocr_languages = value;
    request.resolve(next);
    if (command === 'create_task') await screen.findByRole('heading', { name: '导入资料' });
    else { await waitFor(() => expect(input).not.toBeDisabled()); expect(input).toHaveValue(command === 'import_result' ? '' : value); }
  });
  it('retains a submitted style draft when the save rejects and prevents pending navigation', async () => {
    const request = deferred<WorkspaceSnapshot>();
    const { user } = setup(command => command === 'save_style' ? request.promise : undefined);
    await navigate(user, '资料与风格'); await user.clear(screen.getByLabelText('风格名称')); await user.type(screen.getByLabelText('风格名称'), 'unsaved style');
    await user.click(screen.getByRole('button', { name: '保存风格' }));
    await user.click(within(screen.getByRole('navigation')).getByRole('button', { name: /^任务/ }));
    expect(screen.getByLabelText('风格名称')).toHaveValue('unsaved style');
    request.reject(new Error('save failed'));
    expect(await screen.findByRole('alert')).toHaveTextContent('save failed');
    expect(screen.getByLabelText('风格名称')).toHaveValue('unsaved style');
    expect(screen.getByLabelText('风格名称')).not.toBeDisabled();
  });
});
it('locks project inputs while a native directory chooser is pending, and dialog failure retains them', async () => {
  const request = deferred<string | null>();
  const { user, call } = setup(command => command === 'choose_directory' ? request.promise : undefined);
  await screen.findByLabelText('当前任务');
  await user.type(screen.getByLabelText('项目名称'), 'submitted project');
  await user.click(screen.getByRole('button', { name: '新建项目' }));
  expect(screen.getByLabelText('项目名称')).toBeDisabled();
  expect(screen.getByLabelText('任务标题')).toBeDisabled();
  expect(screen.getByLabelText('章节（可选）')).toBeDisabled();
  await user.type(screen.getByLabelText('项目名称'), ' newer');
  request.reject(new Error('dialog failed'));
  expect(await screen.findByRole('alert')).toHaveTextContent('dialog failed');
  expect(screen.getByLabelText('项目名称')).toHaveValue('submitted project');
  expect(screen.getByLabelText('项目名称')).not.toBeDisabled();
  expect(call.mock.calls.some(([command]) => command === 'create_project')).toBe(false);
});
it('locks review notes while awaiting confirmation and retains the submitted note on failure', async () => {
  const request = deferred<WorkspaceSnapshot>();
  const { user, state } = setup(command => command === 'resolve_review' ? request.promise : undefined);
  state.current_version!.ir.unverified = [{ id: 'pending-review', node_id: 'node-1', kind: 'formula', text: '待确认', reason: '检查来源' }];
  await navigate(user, '结果'); await user.click(screen.getByRole('button', { name: '刷新工作区' }));
  const note = await screen.findByLabelText('审阅说明 · pending-review');
  await user.type(note, 'submitted review'); await user.click(screen.getByRole('button', { name: '记录审阅并确认' }));
  expect(note).toBeDisabled(); await user.type(note, ' newer'); expect(note).toHaveValue('submitted review');
  request.reject(new Error('review failed'));
  expect(await screen.findByRole('alert')).toHaveTextContent('review failed');
  expect(note).not.toBeDisabled(); expect(note).toHaveValue('submitted review');
});
it.each([false, true])('renders native ChatGPT discovery and four gates (window found: %s) without enabling automation', async found => {
  const detected = { ...capabilities, bridge: {
    status: 'read_only', detail: 'Native parent report', automation_enabled: false,
    capabilities: { app_found: found, web_view_accessibility: found, foreground: false, login_likely: null, app_version: found ? '1.2.3.4' : null, window_count: found ? 1 : 0, accessible_controls: found ? 12 : 0, automation_enabled: false,
      gates: { discover_window: { passed: found, evidence: 'native discovery evidence' }, reliable_input: { passed: false, evidence: 'input unverified' }, reliable_completion: { passed: false, evidence: 'completion unverified' }, reliable_capture: { passed: false, evidence: 'capture unverified' } } },
  } };
  const call = vi.fn(async (command: string) => command === 'bootstrap' ? detected : fixture());
  render(<App client={{ available: true, call } as WorkspaceClient} />);
  await screen.findByLabelText('当前任务'); await userEvent.click(screen.getByRole('button', { name: '打开设置' }));
  expect(screen.getByText(found ? '检测到 ChatGPT 窗口' : '未检测到 ChatGPT 窗口')).toBeVisible();
  const gates = screen.getByRole('list', { name: '结构化规划接口兼容性门槛' });
  expect(within(gates).getAllByRole('listitem')).toHaveLength(4);
  expect(within(gates).getAllByText('未验证')).toHaveLength(found ? 3 : 4);
  expect(within(gates).getByText('可靠完成判定')).toBeVisible();
  expect(within(gates).getByText('capture unverified')).toBeVisible();
  if (found) expect(within(gates).getByText('通过')).toBeVisible();
  expect(screen.queryByRole('switch')).not.toBeInTheDocument();
  expect(screen.queryByRole('button', { name: /开启自动/ })).not.toBeInTheDocument();
});


describe('visual repair and diagnostics release flows', () => {
  it('undoes a repair against its base version and replaces old artifacts, report and preview', async () => {
    const { user, call, state } = setup((command, _args, state) => {
      if (command === 'undo_visual_repair') {
        const next = structuredClone(state);
        next.current_version!.id = 'version-undo'; next.current_version!.revision = 2;
        next.project.tasks[0].current_version = 'version-undo'; next.project.tasks[0].state = 'NEEDS_REVIEW';
        next.current_version!.visual_repairs![0].reverted = true;
        next.current_version!.visual_repair_enabled = false;
        next.current_version!.layout_options = next.current_version!.visual_repairs![0].before;
        next.current_version!.artifacts = null; next.preview_pages = [];
        next.current_version!.validation = { schema_version: 1, checked_at: '2026-01-02T00:00:00Z', metrics: {}, findings: [{ id: 'undo-blocked', code: 'node_overlap', severity: 'blocking', message: '撤销后的版式仍需修复', node_id: null, evidence: null }] };
        return next;
      }
    });
    state.current_version!.visual_repairs = [repairFixture()]; state.current_version!.visual_repair_enabled = true;
    state.current_version!.artifacts = { svg: ['old.svg'], png: ['old.png'], pdf: 'old.pdf', manifest: 'old-manifest.json', ir: 'old-ir.json' };
    state.preview_pages = [{ page: 1, svg: '<svg xmlns="http://www.w3.org/2000/svg" />' }];
    await navigate(user, '结果'); await user.click(screen.getByRole('button', { name: '刷新工作区' }));
    await user.click(screen.getByRole('button', { name: '编译页面' }));
    expect(screen.getByRole('img', { name: '本地编译第 1 页' })).toBeVisible();
    expect(screen.getByText('old.pdf')).toBeVisible();
    expect(screen.getByText('公开测试：缩小卡片间距')).toBeVisible();
    expect(screen.getByText('当轮校验通过')).toBeVisible();
    await user.click(screen.getByRole('button', { name: '撤销第 1 轮版式修复' }));
    await waitFor(() => expect(call).toHaveBeenCalledWith('undo_visual_repair', { taskId: 'task-1', baseVersionId: 'version-1', repairId: 'repair-1' }));
    expect(await screen.findByText('撤销后的版式仍需修复')).toBeVisible();
    expect(screen.getByRole('button', { name: '撤销第 1 轮版式修复' })).toBeDisabled();
    expect(screen.getByText('已撤销')).toBeVisible();
    expect(screen.queryByRole('img', { name: '本地编译第 1 页' })).not.toBeInTheDocument();
    expect(screen.queryByText('old.pdf')).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: '打开 PDF' })).not.toBeInTheDocument();
    await user.click(screen.getByRole('button', { name: '节点编辑' }));
    expect(screen.getByLabelText('节点正文')).toHaveValue(state.current_version!.ir.nodes[0].body);
    expect(screen.getByLabelText('公式 LaTeX')).toHaveValue(state.current_version!.ir.nodes[0].latex);
  });
  it('disables repair undo for unsaved node changes and keeps a rejected repair intact', async () => {
    const { user, call, state } = setup(command => { if (command === 'undo_visual_repair') throw new Error('stale base version'); });
    state.current_version!.visual_repairs = [repairFixture()];
    await navigate(user, '结果'); await user.click(screen.getByRole('button', { name: '刷新工作区' }));
    await user.type(screen.getByLabelText('节点正文'), ' local draft');
    expect(screen.getByRole('button', { name: '撤销第 1 轮版式修复' })).toBeDisabled();
    expect(call.mock.calls.some(([command]) => command === 'undo_visual_repair')).toBe(false);
    await user.click(screen.getByRole('button', { name: '放弃草稿' }));
    await user.click(screen.getByRole('button', { name: '撤销第 1 轮版式修复' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('stale base version');
    expect(screen.getByText('未撤销')).toBeVisible();
    expect(screen.queryByText('撤销版式修复完成')).not.toBeInTheDocument();
  });
  it('exports diagnostics without a project and clears success when the next dialog is cancelled', async () => {
    const call = vi.fn(async (command: string): Promise<unknown> => {
      if (command === 'bootstrap') return capabilities;
      if (command === 'workspace_snapshot') throw new Error('no project is open');
      if (command === 'export_diagnostics') return 'C:\\公开诊断\\report.json';
      throw new Error(`unexpected ${command}`);
    });
    render(<App client={{ available: true, call } as WorkspaceClient} />);
    await waitFor(() => expect(screen.getByRole('button', { name: '打开设置' })).toBeEnabled());
    await userEvent.click(screen.getByRole('button', { name: '打开设置' }));
    const button = screen.getByRole('button', { name: '导出脱敏诊断' });
    expect(button).toBeEnabled(); await userEvent.click(button);
    expect(await screen.findByText('已保存：C:\\公开诊断\\report.json')).toBeVisible();
    expect(call).toHaveBeenCalledWith('export_diagnostics', undefined);
    call.mockResolvedValueOnce(null);
    await userEvent.click(button);
    await waitFor(() => expect(button).toBeEnabled());
    expect(screen.queryByText(/已保存：/)).not.toBeInTheDocument();
    expect(screen.queryByText('导出脱敏诊断完成')).not.toBeInTheDocument();
  });
  it('does not report a diagnostics file when native writing fails', async () => {
    const { user } = setup(command => { if (command === 'export_diagnostics') throw new Error('diagnostics write failed'); });
    await screen.findByLabelText('当前任务'); await user.click(screen.getByRole('button', { name: '打开设置' }));
    await user.click(screen.getByRole('button', { name: '导出脱敏诊断' }));
    expect(await screen.findByRole('alert')).toHaveTextContent('diagnostics write failed');
    expect(screen.queryByText(/已保存：/)).not.toBeInTheDocument();
    expect(screen.queryByText('导出脱敏诊断完成')).not.toBeInTheDocument();
  });
});


describe('Knowledge IR import', () => {
  it('requires a new empty task, then forwards the complete chosen IR unchanged', async () => {
    const ir = fixture().current_version!.ir;
    ir.nodes[0].body = '已审阅的公开正文'; ir.nodes[0].origin = 'user_edit';
    const json = JSON.stringify(ir, null, 2);
    const { user, call } = setup((command, _args, state) => {
      if (command === 'create_task') return emptyTaskFixture();
      if (command === 'choose_ir_file') return json;
      if (command === 'import_ir') {
        const next = structuredClone(state); const version = fixture().current_version!;
        version.id = 'imported-ir-version'; version.ir = { ...ir, task_id: 'empty-task' };
        next.current_version = version; next.project.tasks[1].versions = [version]; next.project.tasks[1].current_version = version.id; next.project.tasks[1].state = 'IR_READY';
        return next;
      }
    });
    await navigate(user, '资料与风格');
    expect(screen.getByRole('button', { name: '导入 Knowledge IR' })).toBeDisabled();
    expect(screen.getByText(/当前任务已有版本，请创建空任务/)).toBeVisible();
    await user.click(screen.getByRole('button', { name: '前往项目创建新任务' }));
    await user.type(screen.getByLabelText('任务标题'), '空任务');
    await user.click(screen.getByRole('button', { name: '创建任务' }));
    expect(await screen.findByRole('button', { name: '导入 Knowledge IR' })).toBeEnabled();
    await user.click(screen.getByRole('button', { name: '导入 Knowledge IR' }));
    await waitFor(() => expect(call).toHaveBeenCalledWith('import_ir', { taskId: 'empty-task', baseVersionId: null, json }));
    expect(call).toHaveBeenCalledWith('choose_ir_file', undefined);
    expect(screen.getByRole('button', { name: '导入 Knowledge IR' })).toBeDisabled();
    await navigate(user, '结果');
    expect(screen.getByLabelText('节点正文')).toHaveValue('已审阅的公开正文');
    expect(screen.getByLabelText('来源锚点 JSON')).toHaveValue(JSON.stringify(ir.nodes[0].anchors, null, 2));
  });
  it('does not import or report completion when the IR chooser is cancelled', async () => {
    const { user, call } = setup(command => command === 'choose_ir_file' ? null : undefined, emptyTaskFixture());
    await navigate(user, '资料与风格'); await user.click(screen.getByRole('button', { name: '导入 Knowledge IR' }));
    await waitFor(() => expect(screen.getByRole('button', { name: '导入 Knowledge IR' })).toBeEnabled());
    expect(call.mock.calls.some(([command]) => command === 'import_ir')).toBe(false);
    expect(screen.queryByText('导入 Knowledge IR完成')).not.toBeInTheDocument();
  });
  it.each(['IR schema invalid', 'Retained source hash not found; import originals first'])('keeps the empty task unchanged and displays %s', async message => {
    const { user, state } = setup(command => {
      if (command === 'choose_ir_file') return '{"invalid":true}';
      if (command === 'import_ir') throw new Error(message);
    }, emptyTaskFixture());
    await navigate(user, '资料与风格'); await user.click(screen.getByRole('button', { name: '导入 Knowledge IR' }));
    expect(await screen.findByRole('alert')).toHaveTextContent(message);
    expect(screen.getByRole('button', { name: '导入 Knowledge IR' })).toBeEnabled();
    expect(state.current_version).toBeNull(); expect(state.project.tasks[1].versions).toHaveLength(0);
    expect(screen.queryByText('导入 Knowledge IR完成')).not.toBeInTheDocument();
  });
});
it('shows the persisted GPT proposal, advisory fallback, ignored edits and unretrieved image honestly', async () => {
  const state = fixture();
  state.current_version!.gpt_import = gptImportFixture();
  const { user, call } = setup(undefined, state);
  await navigate(user, '任务');
  expect(screen.getByRole('heading', { name: '已导入的 GPT 视觉方案' })).toBeVisible();
  expect(screen.getByText('未采用')).toBeVisible();
  expect(screen.getByText('公开提议分组')).toBeVisible();
  expect(screen.getByText('公开方案说明')).toBeVisible();
  expect(screen.getByText('手工修改节点的更新已忽略')).toBeVisible();
  expect(screen.getByText('方案与手工顺序不兼容，使用本地顺序排版')).toBeVisible();
  expect(screen.getByText('结果包含图片引用；图片尚未取回，也未用于当前页面。')).toBeVisible();
  expect(screen.queryByRole('img')).not.toBeInTheDocument();
  expect(call.mock.calls.some(([command]) => command === 'open_export')).toBe(false);
});

it('updates visual-plan adoption only from the latest compiler snapshot', async () => {
  const state = fixture(); state.current_version!.gpt_import = { ...gptImportFixture(), visual_plan_status: 'not_evaluated', warnings: [] };
  const { user } = setup((command, _args, state) => {
    if (command === 'preview_current') {
      const next = structuredClone(state);
      next.current_version!.gpt_import!.visual_plan_status = 'partially_applied';
      next.current_version!.gpt_import!.warnings = [{ code: 'page_hint_ignored', message: '部分分页建议超出页数上限，已使用本地分页' }];
      return next;
    }
  }, state);
  await navigate(user, '任务'); expect(screen.getByText('尚未编译评估')).toBeVisible();
  expect(screen.queryByText('已采用')).not.toBeInTheDocument();
  await navigate(user, '结果'); await user.click(screen.getByRole('button', { name: '编译预览' }));
  await navigate(user, '任务'); expect(screen.getByText('部分采用')).toBeVisible();
  expect(screen.getByText('部分分页建议超出页数上限，已使用本地分页')).toBeVisible();
});
