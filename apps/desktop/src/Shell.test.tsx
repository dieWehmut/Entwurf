import { render, screen, within, waitFor, act } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import App from './App';
import { fixture, capabilities } from '../tests/protocol-fixture';
import type { WorkspaceClient } from './lib/workspace';

const windowApi = vi.hoisted(() => ({ minimize: vi.fn(), toggleMaximize: vi.fn(), close: vi.fn(), onCloseRequested: vi.fn(async (_handler: (event: { preventDefault: () => void }) => void) => () => {}) }));
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => windowApi }));

function mount(runtime: WorkspaceClient['runtime'] = 'browser') {
  const state = fixture();
  state.project.tasks.push({ ...structuredClone(state.project.tasks[0]), id: 'task-2', title: '第二个真实任务' });
  const call = vi.fn(async (command: string, args: any) => {
    if (command === 'bootstrap') return capabilities;
    if (command === 'workspace_snapshot' && args?.taskId) state.selected_task_id = args.taskId;
    return structuredClone(state);
  });
  render(<App client={{ available: true, runtime, call } as WorkspaceClient} />);
  return { user: userEvent.setup(), call };
}
beforeEach(() => { localStorage.clear(); vi.clearAllMocks(); });

it('toggles the real task sidebar and navigates backwards and forwards without breadcrumbs', async () => {
  const { user } = mount();
  await screen.findByLabelText('当前任务');
  expect(document.querySelector('.breadcrumb')).not.toBeInTheDocument();
  expect(screen.getByRole('complementary', { name: '任务侧栏' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: '切换任务侧栏' }));
  expect(screen.queryByRole('complementary', { name: '任务侧栏' })).not.toBeInTheDocument();
  await user.click(within(screen.getByRole('navigation')).getByRole('button', { name: '资料与风格' }));
  await user.click(screen.getByRole('button', { name: '后退' }));
  expect(screen.getByRole('heading', { name: '你的知识工作台' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: '前进' }));
  expect(screen.getByRole('heading', { name: '导入资料' })).toBeVisible();
  expect(screen.queryByRole('button', { name: '关闭窗口' })).not.toBeInTheDocument();
});

it('selects actual task IDs and blocks sidebar changes while a node draft exists', async () => {
  const { user, call } = mount();
  await screen.findByLabelText('当前任务');
  await user.click(screen.getByRole('button', { name: '切换到任务 第二个真实任务' }));
  await waitFor(() => expect(call).toHaveBeenCalledWith('workspace_snapshot', { taskId: 'task-2' }));
  await user.click(within(screen.getByRole('navigation')).getByRole('button', { name: '结果' }));
  await user.type(screen.getByLabelText('节点正文'), ' 保留草稿');
  expect(screen.getByRole('button', { name: '切换到任务 第二个真实任务' })).toBeDisabled();
  expect(screen.getByRole('button', { name: '新建任务' })).toBeDisabled();
  await user.click(screen.getByRole('button', { name: '后退' }));
  await user.click(screen.getByRole('button', { name: '前进' }));
  expect((screen.getByLabelText('节点正文') as HTMLTextAreaElement).value).toContain('保留草稿');
});

it('uses keyboard menus for real task creation navigation and exposes theme in the rail', async () => {
  const { user } = mount();
  await screen.findByLabelText('当前任务');
  await user.click(screen.getByRole('button', { name: '文件' }));
  await user.click(screen.getByRole('menuitem', { name: '新建任务' }));
  await waitFor(() => expect(screen.getByLabelText('任务标题')).toHaveFocus());
  await user.click(screen.getByRole('button', { name: '视图' }));
  expect(screen.getByRole('menu', { name: '视图菜单' })).toBeVisible();
  await user.keyboard('{Escape}');
  expect(screen.queryByRole('menu')).not.toBeInTheDocument();
  expect(screen.getByRole('button', { name: '视图' })).toHaveFocus();
  await user.click(screen.getByRole('button', { name: '切换到浅色主题' }));
  expect(document.documentElement.dataset.theme).toBe('light');
});

it('connects native window controls to Tauri and refuses close with an unsaved draft', async () => {
  const { user } = mount('native');
  await screen.findByLabelText('当前任务');
  await user.click(screen.getByRole('button', { name: '最小化窗口' }));
  expect(windowApi.minimize).toHaveBeenCalledTimes(1);
  expect(screen.getByRole('button', { name: '最小化窗口' }).closest('[data-tauri-drag-region]')).toBeNull();
  await user.click(screen.getByRole('button', { name: '切换窗口最大化' }));
  expect(windowApi.toggleMaximize).toHaveBeenCalledTimes(1);
  await user.click(within(screen.getByRole('navigation')).getByRole('button', { name: '结果' }));
  await user.type(screen.getByLabelText('节点正文'), ' 未保存');
  expect(screen.getByRole('button', { name: '关闭窗口' })).toBeDisabled();
  expect(windowApi.close).not.toHaveBeenCalled();
  const prevented = vi.fn();
  act(() => windowApi.onCloseRequested.mock.calls.at(-1)![0]({ preventDefault: prevented }));
  expect(prevented).toHaveBeenCalledOnce();
});

it('temporarily hides the task sidebar in settings and restores its workspace preference', async () => {
  const { user } = mount();
  await screen.findByLabelText('当前任务');
  await user.click(screen.getByRole('button', { name: '打开设置' }));
  expect(screen.queryByRole('complementary', { name: '任务侧栏' })).not.toBeInTheDocument();
  expect(screen.getByRole('complementary', { name: '设置分类侧栏' })).toBeVisible();
  await user.click(screen.getByRole('button', { name: '后退' }));
  expect(screen.getByRole('complementary', { name: '任务侧栏' })).toBeVisible();
});

it('traps focus in help and restores the help menu trigger after closing', async () => {
  const { user } = mount();
  await screen.findByLabelText('当前任务');
  await user.click(screen.getByRole('button', { name: '帮助' }));
  await user.click(screen.getByRole('menuitem', { name: '使用说明' }));
  const close = screen.getByRole('button', { name: '关闭使用说明' });
  expect(close).toHaveFocus();
  await user.tab();
  expect(close).toHaveFocus();
  await user.keyboard('{Escape}');
  await waitFor(() => expect(screen.getByRole('button', { name: '帮助' })).toHaveFocus());
});

it('opens the collapsed project form from the File menu and focuses its input', async () => {
  const { user } = mount();
  await screen.findByLabelText('当前任务');
  expect(document.querySelector('.project-manager')).not.toHaveAttribute('open');
  await user.click(screen.getByRole('button', { name: '文件' }));
  await user.click(screen.getByRole('menuitem', { name: '新建项目' }));
  await waitFor(() => expect(screen.getByLabelText('项目名称')).toHaveFocus());
  expect(document.querySelector('.project-manager')).toHaveAttribute('open');
});

it('searches settings content across categories, handles no results and clears with Escape', async () => {
  const { user, call } = mount();
  await screen.findByLabelText('当前任务');
  await user.click(screen.getByRole('button', { name: '打开设置' }));
  const search = screen.getByRole('searchbox', { name: '搜索设置' });
  await user.type(search, 'tesseract');
  expect(screen.getByRole('heading', { name: '文字识别能力' })).toBeVisible();
  expect(screen.queryByLabelText('界面主题')).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: '重新检测能力' }));
  expect(call).toHaveBeenCalledWith('bootstrap', undefined);
  await user.clear(search); await user.type(search, '没有这样的设置');
  expect(screen.getByRole('heading', { name: '没有找到相关设置' })).toBeVisible();
  await user.keyboard('{Escape}');
  expect(search).toHaveValue('');
  expect(screen.getByLabelText('界面主题')).toBeVisible();
  await user.type(search, 'ocr');
  await user.click(screen.getByRole('button', { name: 'OCR' }));
  expect(search).toHaveValue('');
  expect(screen.getByRole('button', { name: 'OCR' })).toHaveAttribute('aria-pressed', 'true');
  await user.clear(screen.getByLabelText('OCR 语言')); await user.type(screen.getByLabelText('OCR 语言'), 'eng');
  await user.type(search, '主题');
  await user.selectOptions(screen.getByLabelText('界面主题'), 'light');
  expect(localStorage.getItem('entwurf.theme')).toBe('light');
  await user.clear(search);
  expect(screen.getByLabelText('OCR 语言')).toHaveValue('eng');
  await user.click(screen.getByRole('button', { name: '保存 OCR 设置' }));
  expect(call).toHaveBeenCalledWith('save_settings', { ocrLanguages: 'eng' });
});
