import { expect, test, type Page } from '@playwright/test';
import { fixture, capabilities } from '../tests/protocol-fixture';
async function nativeFixture(page: Page, exportFailure = false, delayedStyle = false) {
  // Explicit browser-only Tauri IPC mock, never shipped in the application.
  await page.addInitScript(({ snapshot, capabilities, exportFailure, delayedStyle }) => {
    Object.assign(window, { isTauri: true });
    let current = snapshot;
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: async (command: string, args: any) => {
      if (command === 'bootstrap') return capabilities;
      if (command === 'workspace_snapshot') return structuredClone(current);
      const version = current.current_version!;
      if (command === 'save_style' && delayedStyle) return new Promise(resolve => { Object.assign(window, { finishStyleSave: () => { current.project.style = args.style; resolve(structuredClone(current)); } }); });
      if (command === 'update_nodes' || command === 'rollback_version') {
        const old = structuredClone(version);
        const next = command === 'rollback_version' ? structuredClone(current.project.tasks[0].versions.find(v => v.id === args.versionId)!) : structuredClone(version);
        next.id = `version-${old.revision + 1}`; next.revision = old.revision + 1;
        if (command === 'update_nodes') { if (args.baseVersionId !== version.id) throw 'stale version'; next.ir.nodes = args.nodeOrder.map((id: string) => ({ ...next.ir.nodes.find(n => n.id === id)!, ...args.edits.find((n: { id: string }) => n.id === id) })); }
        next.artifacts = null; next.validation = null;
        current = { ...current, current_version: next, preview_pages: [] };
        current.project.tasks[0].versions.push(next); current.project.tasks[0].current_version = next.id;
      } else if (command === 'preview_current' || command === 'validate_current' || command === 'export_current') {
        version.validation = { schema_version: 1, checked_at: '2026-01-01T00:00:00Z', findings: [], metrics: { fixture: 1 } };
        current.preview_pages = [{ page: 1, svg: '<svg xmlns="http://www.w3.org/2000/svg" width="842" height="595"><rect width="842" height="595" fill="white"/><rect x="40" y="40" width="762" height="180" rx="8" fill="#f8f3ff" stroke="#8f35ff"/><text x="60" y="90" font-size="24">Public deterministic IPC fixture</text><text x="60" y="135" font-size="18">UI rendering test, not a native compiler test.</text></svg>' }];
        if (command === 'export_current') {
          if (exportFailure) {
            version.artifacts = null;
            version.validation.findings = [{ id: 'export-error', code: 'export_failed', severity: 'blocking', message: '测试磁盘写入失败', node_id: null, evidence: null }];
            current.project.tasks[0].state = 'NEEDS_REVIEW'; current.project.tasks[0].last_error = '测试磁盘写入失败';
          } else {
            version.artifacts = { svg: ['fixture/page.svg'], png: ['fixture/page.png'], pdf: 'fixture/result.pdf', manifest: 'fixture/manifest.json', ir: 'fixture/knowledge.ir.json' };
            current.project.tasks[0].state = 'EXPORTED';
          }
        }
      } else if (command === 'open_export' || command === 'copy_prompt' || command === 'open_chatgpt') return null;
      else if (command === 'prepare_handoff') current.handoff = { envelope_json: '{}', markdown: 'Public fixture TaskEnvelope', task_id: 'task-1', revision: version.revision, correction_attempts: 0, status: 'prepared', last_error: null };
      else throw `Unmocked IPC command: ${command}`;
      return structuredClone(current);
    } } });
  }, { snapshot: fixture(), capabilities, exportFailure, delayedStyle });
}
const nav = (page: Page, label: string) => page.getByRole('navigation').getByRole('button', { name: new RegExp(`^${label}`) }).click();
test('browser without IPC explicitly disables native workspace', async ({ page }) => {
  await page.goto('/'); await expect(page).toHaveTitle('Entwurf · 可验证知识图');
  await expect(page.getByText(/浏览器预览：本地工作区不可用/)).toBeVisible();
  await expect(page.getByRole('button', { name: '打开项目' })).toBeDisabled();
});
test('five pages, themes, desktop sizes and zooms render without errors', async ({ page }) => {
  test.setTimeout(120_000);
  const errors: string[] = [];
  page.on('console', msg => { if (msg.type() === 'error') errors.push(msg.text()); }); page.on('pageerror', err => errors.push(err.message));
  await nativeFixture(page); await page.goto('/'); await expect(page.getByLabel('当前任务')).toHaveValue('task-1');
  await expect(page).toHaveTitle('Entwurf · 可验证知识图');
  for (const size of [{ width: 1280, height: 800 }, { width: 1920, height: 1080 }]) {
    await page.setViewportSize(size);
    for (const theme of ['dark', 'light']) {
      const actual = await page.locator('html').getAttribute('data-theme');
      if (actual !== theme) await page.getByRole('main').getByRole('button', { name: /切换到.*主题/ }).click();
      for (const zoom of [1, 1.25, 1.5]) {
        await page.evaluate(zoom => { document.documentElement.style.zoom = String(zoom); }, zoom);
        for (const [id, label] of [['project', '项目'], ['sources', '资料与风格'], ['tasks', '任务'], ['result', '结果'], ['settings', '打开设置']]) {
          if (id === 'settings') await page.getByRole('button', { name: label }).click(); else await nav(page, label);
          await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
          await expect(page.locator('vite-error-overlay')).toHaveCount(0);
          await page.screenshot({ path: `/tmp/entwurf-frontend-${id}-${theme}-${size.width}-${zoom}.png` });
          expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
        }
      }
    }
  }
  await page.evaluate(() => { document.documentElement.style.zoom = '1'; });
  await page.setViewportSize({ width: 390, height: 844 }); await nav(page, '结果');
  await expect(page.getByLabel('节点标题')).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1)).toBe(true);
  await page.screenshot({ path: '/tmp/entwurf-frontend-mobile.png', fullPage: true });
  expect(errors).toEqual([]);
});
test('edit, compare, rollback, preview and export follow returned snapshots', async ({ page }) => {
  await nativeFixture(page); await page.goto('/'); await nav(page, '结果');
  await page.getByLabel('节点标题').fill('保存后的公开标题');
  await expect(page.getByRole('button', { name: /导出 SVG/ })).toBeDisabled();
  await page.getByRole('button', { name: '保存节点修改' }).click(); await expect(page.getByText('所有编辑已保存')).toBeVisible();
  await page.getByRole('button', { name: '版本比较', exact: true }).click(); await expect(page.getByText('r1 → r2 · 1 个节点内容有差异')).toBeVisible();
  await page.getByRole('button', { name: '从 r1 恢复内容' }).click();
  await page.getByRole('button', { name: '节点编辑', exact: true }).click(); await expect(page.getByLabel('节点标题')).toHaveValue('公开测试定义');
  await page.getByRole('button', { name: '编译预览', exact: true }).click(); await page.getByRole('button', { name: '编译页面', exact: true }).click();
  await expect(page.getByRole('img', { name: '本地编译第 1 页' })).toBeVisible();
  await page.getByRole('button', { name: /导出 SVG/ }).click(); await expect(page.getByText('导出文件完成', { exact: true })).toBeVisible(); await expect(page.getByText('fixture/result.pdf', { exact: true })).toBeVisible();
  await page.getByRole('button', { name: '打开 PDF', exact: true }).click(); await expect(page.getByText('打开导出文件完成')).toBeVisible();
  await page.screenshot({ path: '/tmp/entwurf-frontend-export.png', fullPage: true });
});
test('resolved export failure displays report and error without success or artifact links', async ({ page }) => {
  await nativeFixture(page, true); await page.goto('/'); await nav(page, '结果');
  await page.getByRole('button', { name: /导出 SVG/ }).click();
  await expect(page.getByRole('alert')).toContainText('导出未完成：测试磁盘写入失败');
  await expect(page.getByText('blocking · export_failed')).toBeVisible();
  await expect(page.getByText('导出文件完成', { exact: true })).toHaveCount(0);
  await expect(page.getByRole('button', { name: '打开 PDF' })).toHaveCount(0);
  await page.screenshot({ path: '/tmp/entwurf-frontend-export-failed.png', fullPage: true });
});
test('pending style save disables fields and navigation until the matching response', async ({ page }) => {
  await nativeFixture(page, false, true); await page.goto('/'); await nav(page, '资料与风格');
  await page.getByLabel('风格名称').fill('submitted style'); await page.getByRole('button', { name: '保存风格', exact: true }).click();
  await expect(page.getByLabel('风格名称')).toBeDisabled();
  await expect(page.getByRole('combobox', { name: '密度', exact: true })).toBeDisabled();
  await expect(page.getByRole('navigation').getByRole('button', { name: /^任务/ })).toBeDisabled();
  await page.screenshot({ path: '/tmp/entwurf-frontend-style-pending.png', fullPage: true });
  await page.evaluate(() => (window as unknown as { finishStyleSave: () => void }).finishStyleSave());
  await expect(page.getByLabel('风格名称')).toBeEnabled(); await expect(page.getByLabel('风格名称')).toHaveValue('submitted style');
  await expect(page.getByRole('navigation').getByRole('button', { name: /^任务/ })).toBeEnabled();
});

test('settings renders native discovery evidence and four unverified gates', async ({ page }) => {
  await nativeFixture(page); await page.goto('/');
  await expect(page.getByLabel('当前任务')).toHaveValue('task-1');
  await page.getByRole('button', { name: '打开设置', exact: true }).click();
  await expect(page.getByText('未检测到 ChatGPT 窗口', { exact: true })).toBeVisible();
  const gates = page.getByRole('list', { name: 'ChatGPT 自动化兼容性门槛' });
  await expect(gates.getByRole('listitem')).toHaveCount(4);
  await expect(gates.getByText('未验证', { exact: true })).toHaveCount(4);
  await expect(page.getByRole('switch')).toHaveCount(0);
  await page.screenshot({ path: '/tmp/entwurf-frontend-bridge-report.png', fullPage: true });
});
