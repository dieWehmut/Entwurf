// Drives the actual installed WebView2/Tauri app. No IPC mocks or production hooks.
// Run with native Windows Node and configuration from reinstall-acceptance.ps1.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import path from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import { constants as fsConstants } from 'node:fs';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const require = createRequire(new URL('../../apps/desktop/package.json', import.meta.url));
const { chromium, expect } = require('@playwright/test');
const execute = promisify(execFile);
const config = JSON.parse(await fs.readFile(process.argv[2], 'utf8'));
const evidence = path.resolve(config.evidence);
await fs.mkdir(evidence, { recursive: true });
const report = { cycle: config.cycle, started_at: new Date().toISOString(), checks: [], errors: [] };
const sha = data => createHash('sha256').update(data).digest('hex');
const headingFixtureSuffix = '公开概率论'.repeat(4);
let browser;
let page;
let primaryExport;
let requestedVisualPlan;
async function step(name, action) {
  const start = Date.now();
  try {
    await action();
    report.checks.push({ name, passed: true, elapsed_ms: Date.now() - start });
    console.log(`PASS: ${name}`);
  } catch (error) {
    report.checks.push({ name, passed: false, error: String(error), elapsed_ms: Date.now() - start });
    throw error;
  }
}
const invoke = (command, args = {}) => page.evaluate(
  ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args), { command, args });
const snapshot = () => invoke('workspace_snapshot');
const idle = () => expect(page.getByRole('main')).toHaveAttribute('aria-busy', 'false', { timeout: 120_000 });
const navigate = label => page.getByRole('navigation').getByRole('button', { name: new RegExp(`^${label}`) }).click();
async function nativeDialog(arguments_) {
  const script = fileURLToPath(new URL('./select-native-dialog.ps1', import.meta.url));
  await execute('powershell.exe', ['-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass',
    '-File', script, '-AppProcessId', String(config.process_id), ...arguments_],
  { timeout: 40_000, windowsHide: true });
}
const chooseNativePath = (selected, kind) => nativeDialog(['-SelectedPath', selected, '-Kind', kind]);
const cancelNativeDialog = () => nativeDialog(['-Cancel']);
async function capture(filename, fullPage = false) {
  const metrics = await page.evaluate(() => ({
    innerWidth: window.innerWidth, innerHeight: window.innerHeight,
    outerWidth: window.outerWidth, outerHeight: window.outerHeight,
    devicePixelRatio: window.devicePixelRatio,
    screenWidth: window.screen.width, screenHeight: window.screen.height,
    visualViewportScale: window.visualViewport?.scale ?? null,
  }));
  const bytes = await page.screenshot({ path: path.join(evidence, filename), fullPage });
  (report.screenshots ??= []).push({ filename, fullPage, ...metrics,
    pixelWidth: bytes.readUInt32BE(16), pixelHeight: bytes.readUInt32BE(20) });
  report.display_scope = 'Measured WebView CSS viewport, devicePixelRatio and PNG pixels; OS display scaling was not changed or independently verified. No 100/125/150 percent scale coverage is claimed.';
}
async function refreshWorkspace() {
  await page.getByRole('button', { name: '刷新工作区', exact: true }).click();
  await idle();
}
function assertPassing(current) {
  assert(current.current_version.validation, 'native compiler must return a validation report');
  assert.deepEqual(current.current_version.validation.findings.filter(f => f.severity === 'blocking'), []);
  assert(current.preview_pages.length > 0, 'passing preview must contain actual rendered pages');
}
async function verifyBundle(version, expectedPages) {
  const artifacts = version.artifacts;
  assert(artifacts, 'current version must have real artifacts');
  const manifest = JSON.parse(await fs.readFile(artifacts.manifest, 'utf8'));
  assert.equal(manifest.ir_id, version.ir.id);
  assert.equal(manifest.project_id, version.ir.project_id);
  assert.equal(manifest.task_id, version.ir.task_id);
  assert(manifest.page_count >= 1 && manifest.page_count <= 3);
  if (expectedPages !== undefined) assert.equal(manifest.page_count, expectedPages);
  assert.equal(artifacts.svg.length, manifest.page_count);
  assert.equal(artifacts.png.length, manifest.page_count);
  assert.equal(manifest.width_mm, 420);
  assert.equal(manifest.height_mm, 297);
  const exportDirectory = path.dirname(artifacts.manifest);
  const layoutPath = path.join(exportDirectory, 'layout.json');
  const expectedFiles = [...artifacts.svg, ...artifacts.png, artifacts.pdf, artifacts.ir, layoutPath];
  const expectedNames = expectedFiles.map(filename => {
    const relative = path.relative(exportDirectory, filename);
    assert(relative && !path.isAbsolute(relative) && relative !== '..' && !relative.startsWith(`..${path.sep}`),
      'each artifact must stay inside its export directory');
    return relative;
  });
  assert.equal(new Set(expectedNames).size, expectedNames.length, 'artifact paths must be distinct');
  assert.deepEqual(manifest.files.map(file => file.path).sort(), [...expectedNames].sort(),
    'manifest must cover every actual SVG, PNG, PDF, IR and layout exactly once');
  for (const file of manifest.files) {
    const bytes = await fs.readFile(path.join(exportDirectory, file.path));
    assert.equal(bytes.length, file.bytes);
    assert.equal(sha(bytes), file.sha256);
  }
  for (const filename of artifacts.svg) {
    assert.match(await fs.readFile(filename, 'utf8'), /<svg\b/, 'each vector page must contain an SVG root');
  }
  const irBytes = await fs.readFile(artifacts.ir);
  assert.equal(manifest.ir_hash, sha(irBytes));
  const ir = JSON.parse(irBytes.toString('utf8'));
  assert.deepEqual(ir, version.ir);
  assert.deepEqual(JSON.parse(await fs.readFile(layoutPath, 'utf8')), version.layout);
  assert.deepEqual([...manifest.block_ids].sort(), version.ir.nodes.map(node => node.id).sort());
  const pdf = await fs.readFile(artifacts.pdf);
  assert.equal(pdf.subarray(0, 5).toString(), '%PDF-');
  for (const filename of artifacts.png) {
    const bytes = await fs.readFile(filename);
    assert.equal(bytes.readUInt32BE(16), 4961);
    assert.equal(bytes.readUInt32BE(20), 3508);
    let found = false;
    for (let offset = 8; offset + 12 <= bytes.length;) {
      const size = bytes.readUInt32BE(offset);
      const kind = bytes.toString('ascii', offset + 4, offset + 8);
      if (kind === 'pHYs') {
        assert.equal(bytes.readUInt32BE(offset + 8), 11811);
        assert.equal(bytes.readUInt32BE(offset + 12), 11811);
        assert.equal(bytes[offset + 16], 1);
        found = true;
      }
      offset += 12 + size;
    }
    assert(found, '300 DPI PNG physical resolution must be present');
  }
  return { pages: manifest.page_count, files: manifest.files.length, ir_hash: manifest.ir_hash };
}
const without = (object, keys) => Object.fromEntries(Object.entries(object).filter(([key]) => !keys.includes(key)));
const filesystemIdentity = filename => path.normalize(filename.replace(/^\\\\\?\\/, '')).toLowerCase();
async function verifyRestoredIr(original, restored, projectBeforeImport) {
  // The destination IR identity/touch timestamp change. Knowledge and provenance
  // stay exact; source audit metadata is rebound to verified retained originals.
  const retargeted = ['id', 'project_id', 'task_id', 'updated_at', 'sources'];
  assert.deepEqual(without(restored, retargeted), without(original, retargeted));
  assert.notEqual(restored.id, original.id);
  assert.notEqual(restored.task_id, original.task_id);
  assert.equal(restored.project_id, projectBeforeImport.id);
  assert.equal(restored.sources.length, original.sources.length);
  const retained = projectBeforeImport.tasks.flatMap(task => [...task.versions].reverse())
    .flatMap(version => version.ir.sources);
  for (const [index, source] of restored.sources.entries()) {
    const incoming = original.sources[index];
    assert.deepEqual(without(source, ['original_name', 'stored_path', 'imported_at']),
      without(incoming, ['original_name', 'stored_path', 'imported_at']));
    const candidates = retained.filter(candidate => candidate.content_hash === incoming.content_hash
      && candidate.kind === incoming.kind && candidate.byte_len === incoming.byte_len);
    const trusted = candidates.find(candidate => JSON.stringify(candidate.notes) === JSON.stringify(incoming.notes)) ?? candidates[0];
    assert(trusted, 'IR source must bind to an existing retained original');
    assert.deepEqual(without(source, ['id', 'stored_path']), without(trusted, ['id', 'stored_path']));
    assert.equal(filesystemIdentity(await fs.realpath(source.stored_path)), filesystemIdentity(await fs.realpath(trusted.stored_path)));
    const bytes = await fs.readFile(source.stored_path);
    assert.equal(bytes.length, source.byte_len);
    assert.equal(sha(bytes), source.content_hash);
  }
}
function verifyDiagnostics(diagnostics, current) {
  const keys = ['schema_version', 'application_version', 'platform', 'architecture', 'test_status',
    'network_upload', 'project_open', 'capabilities', 'counts', 'source_kinds', 'task_states', 'report_codes'];
  assert.deepEqual(Object.keys(diagnostics).sort(), keys.sort());
  assert.equal(diagnostics.schema_version, 1);
  assert.equal(diagnostics.application_version, report.capabilities.version);
  assert.equal(diagnostics.platform, 'windows');
  assert.equal(diagnostics.architecture, 'x86_64');
  assert.equal(diagnostics.test_status, 'not_run_in_this_export');
  assert.equal(diagnostics.network_upload, false);
  assert.equal(diagnostics.project_open, true);
  assert.deepEqual(Object.keys(diagnostics.capabilities).sort(),
    ['pdfium_available', 'tesseract_available', 'bridge_available', 'chatgpt_detected', 'automation_enabled'].sort());
  for (const value of Object.values(diagnostics.capabilities)) assert.equal(typeof value, 'boolean');
  assert.equal(diagnostics.capabilities.automation_enabled, false);
  const versions = current.project.tasks.flatMap(task => task.versions);
  assert.deepEqual(diagnostics.counts, {
    tasks: current.project.tasks.length, versions: versions.length,
    sources: versions.reduce((n, version) => n + version.ir.sources.length, 0),
    nodes: versions.reduce((n, version) => n + version.ir.nodes.length, 0),
    edges: versions.reduce((n, version) => n + version.ir.edges.length, 0),
    corrections: versions.reduce((n, version) => n + version.ir.corrections.length, 0),
    findings: versions.reduce((n, version) => n + (version.validation?.findings.length ?? 0), 0),
    audit_events: current.project.audit.length,
    state_changes: current.project.audit.filter(event => event.kind === 'state_changed').length,
  });
  const allowed = {
    source_kinds: ['pdf', 'markdown', 'image', 'latex'],
    task_states: ['DRAFT', 'INGESTING', 'IR_READY', 'PROMPT_READY', 'SENDING', 'WAITING', 'IMPORTING', 'COMPILING', 'VERIFYING', 'NEEDS_REVIEW', 'APPROVED', 'EXPORTED', 'PAUSED', 'FAILED', 'CANCELLED'],
    report_codes: ['missing_critical_anchor', 'unresolved_review', 'node_overlap', 'out_of_bounds', 'small_font', 'formula_mismatch', 'other'],
  };
  for (const [field, names] of Object.entries(allowed)) {
    for (const [name, count] of Object.entries(diagnostics[field])) {
      assert(names.includes(name), `diagnostic ${field} must use fixed labels`);
      assert(Number.isSafeInteger(count) && count >= 0, 'diagnostic values must be nonnegative counts');
    }
  }
  // Besides the fixed literals above, the schema accepts only booleans/counts.
  // This rejects arbitrary nested payloads, paths, names and prompt/source text.
}
try {
  await step('installed WebView2 connects and exposes real native IPC', async () => {
    const deadline = Date.now() + 45_000;
    while (!browser && Date.now() < deadline) {
      try { browser = await chromium.connectOverCDP(config.endpoint); }
      catch { await new Promise(resolve => setTimeout(resolve, 250)); }
    }
    assert(browser, 'installed app did not open its acceptance-only CDP endpoint');
    page = browser.contexts()[0].pages().find(p => /tauri\.localhost/.test(p.url())) ?? browser.contexts()[0].pages()[0];
    assert(page);
    page.setDefaultTimeout(30_000);
    page.on('pageerror', error => report.errors.push(error.message));
    await expect(page).toHaveTitle('Entwurf · 可验证知识图');
    await idle();
    assert(await page.evaluate(() => !!window.__TAURI_INTERNALS__));
    const capabilities = await invoke('bootstrap');
    assert.equal(capabilities.platform, 'windows');
    assert.equal(capabilities.automation_enabled, false);
    assert.equal(capabilities.tesseract_available, true);
    assert(capabilities.ocr_languages.includes('chi_sim'));
    assert(capabilities.ocr_languages.includes('eng'));
    report.capabilities = capabilities;
  });
  await step('installed desktop serves the local WebUI before a project is opened', async () => {
    const base = 'http://127.0.0.1:48123';
    await expect.poll(async () => {
      try {
        const response = await fetch(`${base}/api/bootstrap`, { signal: AbortSignal.timeout(4000) });
        const info = await response.json();
        return response.ok && info.runtime === 'local-http' && info.storage === 'sqlite';
      } catch { return false; }
    }, { timeout: 20_000 }).toBe(true);
    const owner = await execute('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command',
      '(Get-NetTCPConnection -LocalAddress 127.0.0.1 -LocalPort 48123 -State Listen).OwningProcess | ConvertTo-Json -Compress'],
      { timeout: 15_000, windowsHide: true });
    assert.equal(JSON.parse(owner.stdout.trim()), config.process_id, 'the installed desktop must own the companion listener');
    const response = await fetch(`${base}/`);
    assert(response.ok);
    const html = await response.text();
    assert.match(html, /Entwurf/);
    const script = html.match(/<script[^>]+src="([^"]+)"/);
    assert(script, 'embedded frontend must expose its production script');
    const asset = await fetch(new URL(script[1], base));
    assert(asset.ok);
    assert((await asset.arrayBuffer()).byteLength > 1000);
  });
  await step('offline WebView2 keeps local native workflows available', async () => {
    const session = await page.context().newCDPSession(page);
    await session.send('Network.enable');
    await session.send('Network.emulateNetworkConditions', {
      offline: true, latency: 0, downloadThroughput: 0, uploadThroughput: 0,
    });
    report.offline_scope = 'WebView2 network disabled by CDP; host adapters and native networking unchanged';
    if (config.cycle >= 1) {
      report.offline_ui_reload = { passed: false, navigator_online: null };
      await page.reload({ waitUntil: 'domcontentloaded', timeout: 30_000 });
      await idle();
      await expect(page).toHaveTitle('Entwurf · 可验证知识图');
      await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
      assert.equal(await page.evaluate(() => navigator.onLine), false);
      report.offline_ui_reload = { passed: true, navigator_online: false };
    }
    assert.equal((await invoke('bootstrap')).platform, 'windows');
  });
  await step(config.cycle === 0 ? 'create or reopen project through native folder dialog' : 'reinstall restores existing project and edits', async () => {
    if (config.cycle === 0) {
      await fs.mkdir(config.project, { recursive: true });
      const existing = await fs.access(path.join(config.project, 'project.json')).then(() => true, () => false);
      if (existing) await page.getByRole('button', { name: '打开项目', exact: true }).click();
      else {
        await page.getByLabel('项目名称').fill('概率论 桌面重装验收');
        await page.getByRole('button', { name: '新建项目', exact: true }).click();
      }
      await chooseNativePath(config.project, 'directory');
      await idle();
    }
    const current = await snapshot();
    assert.equal(path.resolve(current.root).toLowerCase(), path.resolve(config.project).toLowerCase());
    if (config.cycle > 0) {
      const retained = current.project.tasks.flatMap(task => task.versions).flatMap(version => version.ir.nodes);
      assert(retained.some(node => node.title === `本地保留编辑 ${config.cycle - 1}`));
      assert(current.project.tasks.length >= config.cycle);
    }
  });
  await step('create a new task and import a Chinese spaced path through native dialog', async () => {
    await page.getByLabel('任务标题').fill(config.cycle >= 1
      ? `安装验收 ${config.cycle} · ${headingFixtureSuffix}` : `安装验收 ${config.cycle}`);
    await page.getByLabel('章节（可选）').fill('概率论公开合成章节');
    await page.getByRole('button', { name: '创建任务', exact: true }).click();
    await idle();
    const original = path.join(evidence, `中文 概率论章节-${randomUUID()}.md`);
    await fs.copyFile(config.source, original, fsConstants.COPYFILE_EXCL);
    await page.getByRole('button', { name: '选择本机文件并导入', exact: true }).click();
    await chooseNativePath(original, 'file');
    await idle();
    const current = await snapshot();
    assert.equal(current.current_version.ir.nodes.length, 18);
    assert.equal(current.current_version.ir.unverified.length, 4);
    assert.equal(current.current_version.ir.sources[0].content_hash, sha(await fs.readFile(original)));
    await fs.unlink(original); // Remove only this test-created copy, never the source fixture.
    assert((await fs.stat(current.current_version.ir.sources[0].stored_path)).isFile());
  });
  await step('four public formulas need explicit reviewed source evidence', async () => {
    await navigate('结果');
    await expect(page.getByRole('button', { name: /导出 SVG/ })).toBeDisabled();
    const first = await snapshot();
    for (const review of first.current_version.ir.unverified) {
      const current = await snapshot();
      const node = current.current_version.ir.nodes.find(n => n.id === review.node_id);
      assert(node?.latex);
      let exact = false;
      for (const anchor of node.anchors) {
        const source = current.current_version.ir.sources.find(s => s.id === anchor.source_id);
        const bytes = await fs.readFile(source.stored_path);
        const quote = bytes.subarray(anchor.char_start, anchor.char_end);
        assert.equal(sha(quote), anchor.quote_hash);
        if (quote.toString().replace(/^\s*\$\$|\$\$\s*$/g, '').trim() === node.latex.trim()) exact = true;
      }
      assert(exact, 'only a literal source-matched public fixture formula may be acknowledged');
      // Controlled textarea text can enter the wrapping label's DOM text after
      // typing; the immutable review ID keeps this partial label unique.
      const field = page.getByLabel(`审阅说明 · ${review.id}`);
      await field.fill('本地安装验收：逐字核对公开样本公式与原始来源哈希。');
      await page.locator('.source-record').filter({ has: field })
        .getByRole('button', { name: '记录审阅并确认', exact: true }).click();
      await idle();
    }
    assert.equal((await snapshot()).current_version.ir.unverified.length, 0);
  });
  await step('save manual edit and import a synthetic result without overwriting it', async () => {
    await page.getByLabel('节点标题').fill(`本地保留编辑 ${config.cycle}`);
    await page.getByRole('button', { name: '保存节点修改', exact: true }).click();
    await idle();
    await navigate('任务');
    await page.getByRole('button', { name: '生成 TaskEnvelope', exact: true }).click();
    await idle();
    await page.getByRole('button', { name: '复制提示', exact: true }).click();
    await idle();
    const current = await snapshot();
    requestedVisualPlan = config.cycle >= 1 ? {
      page_count: 2,
      groups: [0, 1].map(index => ({
        id: `acceptance-page-${index + 1}`, title: `公开样本第 ${index + 1} 组`,
        node_ids: current.current_version.ir.nodes.slice(index * 9, (index + 1) * 9).map(node => node.id),
        page: index + 1,
      })), notes: ['Contiguous public-fixture groups; keep all 18 nodes in their existing manual order.'],
    } : { page_count: 1, groups: [], notes: [] };
    const json = {
      schema_version: 1, task_id: current.selected_task_id, revision: current.current_version.revision,
      ir_patch: { add_nodes: [], update_nodes: [{ id: current.current_version.ir.nodes[0].id, title: '不得覆盖人工标题', body: null, latex: null, importance: null }], add_edges: [], remove_node_ids: [] },
      visual_plan: requestedVisualPlan, corrections: [], image_path: null,
      notes: ['Synthetic local acceptance ResultEnvelope; no real ChatGPT generation claimed.'],
    };
    await page.getByLabel('ResultEnvelope JSON', { exact: true }).fill(JSON.stringify(json));
    await page.getByRole('button', { name: '导入 JSON', exact: true }).click();
    await idle();
    const imported = await snapshot();
    assert.equal(imported.current_version.ir.nodes[0].title, `本地保留编辑 ${config.cycle}`);
    if (config.cycle >= 1) {
      assert.deepEqual(imported.current_version.gpt_import.visual_plan, requestedVisualPlan);
      assert.equal(imported.current_version.gpt_import.visual_plan_status, 'not_evaluated');
      assert(imported.current_version.gpt_import.warnings.some(warning => warning.code === 'manual_update_ignored'));
      await expect(page.getByRole('heading', { name: '已导入的 GPT 视觉方案', exact: true })).toBeVisible();
      await expect(page.getByText('尚未编译评估', { exact: true })).toBeVisible();
      await expect(page.getByText('manual_update_ignored', { exact: true })).toBeVisible();
    }
  });
  await step('installed compiler previews and exports verified SVG PNG PDF offline', async () => {
    await navigate('结果');
    await page.getByRole('button', { name: '编译预览', exact: true }).click();
    await idle();
    await page.getByRole('button', { name: '编译页面', exact: true }).click();
    await expect(page.getByRole('img', { name: '本地编译第 1 页' })).toBeVisible();
    if (config.cycle >= 1) {
      await expect(page.getByRole('img', { name: /^本地编译第 \d+ 页$/ })).toHaveCount(2);
      await expect(page.getByRole('img', { name: '本地编译第 2 页', exact: true })).toBeVisible();
    }
    await page.getByRole('button', { name: /导出 SVG/ }).click();
    await idle();
    await expect(page.getByText('导出文件完成', { exact: true })).toBeVisible();
    const current = await snapshot();
    assert.equal(current.project.tasks.find(t => t.id === current.selected_task_id).state, 'EXPORTED');
    assert.equal(current.current_version.validation.findings.filter(f => f.severity === 'blocking').length, 0);
    report.export = await verifyBundle(current.current_version, config.cycle >= 1 ? 2 : undefined);
    // Preserve the first export before creating another task or style versions.
    primaryExport = { version: current.current_version, irPath: current.current_version.artifacts.ir,
      irSha256: sha(await fs.readFile(current.current_version.artifacts.ir)) };
    report.primary_export = { ...report.export, ir_path: primaryExport.irPath, task_id: current.selected_task_id, version_id: current.current_version.id };
    if (config.cycle >= 1) {
      assert.deepEqual(current.current_version.gpt_import.visual_plan, requestedVisualPlan);
      assert.equal(current.current_version.gpt_import.visual_plan_status, 'applied');
      assert.equal(current.preview_pages.length, 2);
      for (const [index, group] of requestedVisualPlan.groups.entries()) {
        assert.deepEqual(current.current_version.layout.pages[index].nodes.map(node => node.node_id), group.node_ids);
      }
      await navigate('任务');
      await expect(page.getByText('已采用', { exact: true })).toBeVisible();
      await capture('installed-visual-plan-applied.png');
      await navigate('结果');
      await page.getByRole('button', { name: '编译页面', exact: true }).click();
    }
    await capture('installed-export.png', true);
  });
  if (config.cycle >= 1) {
    await step('native IR chooser restores reviewed knowledge into a new empty task and exports it', async () => {
      const before = await snapshot();
      const exportedIr = JSON.parse(await fs.readFile(primaryExport.irPath, 'utf8'));
      assert.deepEqual(exportedIr, primaryExport.version.ir);
      await navigate('资料与风格');
      await expect(page.getByRole('button', { name: '导入 Knowledge IR', exact: true })).toBeDisabled();
      await page.getByRole('button', { name: '前往项目创建新任务', exact: true }).click();
      await page.getByLabel('任务标题').fill(`IR 恢复验收 ${config.cycle} · ${headingFixtureSuffix}`);
      await page.getByLabel('章节（可选）').fill('从真实导出恢复已审阅公开样本');
      await page.getByRole('button', { name: '创建任务', exact: true }).click();
      await idle();
      const empty = await snapshot();
      assert.notEqual(empty.selected_task_id, before.selected_task_id);
      assert.equal(empty.current_version, null);
      assert.deepEqual(empty.project.tasks.find(task => task.id === empty.selected_task_id).versions, []);
      const importButton = page.getByRole('button', { name: '导入 Knowledge IR', exact: true });
      await expect(importButton).toBeEnabled();
      await importButton.click();
      await chooseNativePath(primaryExport.irPath, 'file');
      await idle();
      const restored = await snapshot();
      assert.equal(restored.selected_task_id, empty.selected_task_id);
      assert.equal(restored.current_version.ir.task_id, empty.selected_task_id);
      await verifyRestoredIr(exportedIr, restored.current_version.ir, before.project);
      assert.equal(restored.current_version.ir.unverified.length, 0);
      await expect(importButton).toBeDisabled();
      await navigate('结果');
      await page.getByRole('button', { name: '编译预览', exact: true }).click();
      await idle();
      await page.getByRole('button', { name: '校验当前版本', exact: true }).click();
      await idle();
      assertPassing(await snapshot());
      await page.getByRole('button', { name: '编译页面', exact: true }).click();
      await expect(page.getByRole('img', { name: '本地编译第 1 页', exact: true })).toBeVisible();
      await page.getByRole('button', { name: /导出 SVG/ }).click();
      await idle();
      await expect(page.getByText('导出文件完成', { exact: true })).toBeVisible();
      const exported = await snapshot();
      assert.equal(exported.project.tasks.find(task => task.id === exported.selected_task_id).state, 'EXPORTED');
      await verifyRestoredIr(exportedIr, exported.current_version.ir, before.project);
      assert.equal(sha(await fs.readFile(primaryExport.irPath)), primaryExport.irSha256, 'IR reimport must not alter the primary export');
      report.ir_reimport = { ...(await verifyBundle(exported.current_version)),
        task_id: exported.selected_task_id, ir_path: exported.current_version.artifacts.ir,
        primary_ir_unchanged: true,
        preserved: 'Exact nodes, order, edges, corrections, review status and source evidence; destination IR/task IDs and touch timestamp change; original filename, path and import audit bind to verified retained originals.' };
      await capture('installed-ir-reimport-export.png', true);
    });
    await step('real native heading repair can be undone without changing knowledge or retaining exports', async () => {
      const original = await snapshot();
      const originalStyle = structuredClone(original.project.style);
      const controlledStyle = structuredClone(originalStyle);
      controlledStyle.typography.title_pt = 56;
      // IR import preserves the exported title, not the destination task label.
      assert.equal(original.current_version.ir.title, primaryExport.version.ir.title);
      assert(original.current_version.ir.title.endsWith(headingFixtureSuffix));
      // The UI exposes body size only. Set this controlled fixture through the
      // real native service, then use rendered UI for compile/selected undo.
      // 56pt is valid. The public long heading wraps past the 56pt header;
      // the normal 30pt heading and repaired 24pt heading both fit one line.
      try {
        await invoke('save_style', { style: controlledStyle });
        await refreshWorkspace();
        assert.equal((await snapshot()).selected_task_id, original.selected_task_id);
        await page.getByRole('button', { name: '编译预览', exact: true }).click();
        await idle();
        const prepared = await snapshot();
        assertPassing(prepared);
        const repair = prepared.current_version.visual_repairs?.find(record => !record.reverted && record.reason.includes('heading') && record.after.max_heading_font_pt === 24);
        assert(repair, 'valid title_pt=56 with the public long heading must produce the real first-round 24pt cap repair');
        assert.equal(repair.round, 1);
        assert.equal(repair.passed, true);
        // Export the repaired version first, so undo's artifact invalidation is
        // proven against an actual saved bundle, not an already-empty field.
        await page.getByRole('button', { name: /导出 SVG/ }).click();
        await idle();
        await expect(page.getByText('导出文件完成', { exact: true })).toBeVisible();
        const before = (await snapshot()).current_version;
        await verifyBundle(before);
        await expect(page.getByRole('heading', { name: '版式修复记录', exact: true })).toBeVisible();
        await page.getByRole('button', { name: `撤销第 ${repair.round} 轮版式修复`, exact: true }).click();
        await idle();
        const undone = await snapshot();
        const version = undone.current_version;
        assert.notEqual(version.id, before.id);
        assert.deepEqual(version.ir, before.ir);
        assert.equal(version.source_prompt_hash, before.source_prompt_hash);
        assert.equal(version.visual_repair_enabled, false);
        assert.equal(version.artifacts, null);
        assert.deepEqual(version.layout_options, repair.before);
        const selectedIndex = before.visual_repairs.findIndex(record => record.id === repair.id);
        assert.deepEqual(version.visual_repairs, before.visual_repairs.map((record, index) => index < selectedIndex ? record : { ...record, reverted: true }));
        const old = undone.project.tasks.find(task => task.id === undone.selected_task_id).versions.find(candidate => candidate.id === before.id);
        assert.deepEqual(old, before, 'selected repair undo must not rewrite historical versions');
        assert(version.validation.findings.some(finding => finding.severity === 'blocking'), 'undoing the required heading repair must restore the blocking report');
        assert.equal(undone.project.tasks.find(task => task.id === undone.selected_task_id).state, 'NEEDS_REVIEW');
        assert.deepEqual(undone.preview_pages, []);
        await expect(page.getByText('此版本已关闭自动版式修复。', { exact: true })).toBeVisible();
        await expect(page.getByRole('button', { name: `撤销第 ${repair.round} 轮版式修复`, exact: true })).toBeDisabled();
        await expect(page.getByRole('button', { name: /导出 SVG/ })).toBeDisabled();
        await expect(page.getByText('当前版本尚未产生导出文件。', { exact: true })).toBeVisible();
        report.visual_repair_undo = { repair_id: repair.id, round: repair.round, heading_cap_pt: repair.after.max_heading_font_pt,
          fixture_title: before.ir.title, fixture_title_pt: controlledStyle.typography.title_pt,
          before_version: before.id, after_version: version.id, exact_ir_preserved: true,
          auto_repair_enabled: version.visual_repair_enabled, artifacts: version.artifacts,
          blocking_codes: version.validation.findings.filter(finding => finding.severity === 'blocking').map(finding => finding.code) };
        await capture('installed-repair-undone.png', true);
      } finally {
        await invoke('save_style', { style: originalStyle });
        await refreshWorkspace();
        await page.getByRole('button', { name: '编译预览', exact: true }).click();
        await idle();
        const restored = await snapshot();
        assert.deepEqual(restored.project.style, originalStyle);
        assert.equal(restored.selected_task_id, original.selected_task_id);
        assertPassing(restored);
        assert.equal(sha(await fs.readFile(primaryExport.irPath)), primaryExport.irSha256, 'visual repair fixture must not alter the original IR export');
        report.repair_fixture_style_restored = true;
      }
    });
    await step('Settings exports only sanitized diagnostics through native Save As and cancels without false success', async () => {
      await page.getByRole('button', { name: '打开设置', exact: true }).click();
      const current = await snapshot();
      const filename = `diagnostics-cycle-${config.cycle}-${Date.now()}.json`;
      const diagnosticsPath = path.join(evidence, filename);
      assert.equal(path.dirname(diagnosticsPath), evidence, 'diagnostics evidence must stay in the owned cycle folder');
      await assert.rejects(fs.access(diagnosticsPath), { code: 'ENOENT' });
      const exportButton = page.getByRole('button', { name: '导出脱敏诊断', exact: true });
      await exportButton.click();
      await chooseNativePath(diagnosticsPath, 'file');
      await idle();
      await expect(page.getByText(`已保存：${diagnosticsPath}`, { exact: true })).toBeVisible();
      const bytes = await fs.readFile(diagnosticsPath);
      verifyDiagnostics(JSON.parse(bytes.toString('utf8')), current);
      await capture('installed-diagnostics-saved.png');
      const filesBeforeCancel = (await fs.readdir(evidence)).sort();
      await exportButton.click();
      await cancelNativeDialog();
      await idle();
      await expect(page.getByText(/^已保存：/)).toHaveCount(0);
      await expect(page.getByText('导出脱敏诊断完成', { exact: true })).toHaveCount(0);
      await expect(exportButton).toBeEnabled();
      assert.deepEqual((await fs.readdir(evidence)).sort(), filesBeforeCancel);
      assert.equal(sha(await fs.readFile(diagnosticsPath)), sha(bytes));
      report.diagnostics = { filename, sha256: sha(bytes), allowlist_verified: true, cancellation_no_success_or_path: true, cancellation_no_file_changes: true };
      await capture('installed-diagnostics-cancelled.png');
    });
  }
  await step('installed five pages render with both themes', async () => {
    for (const theme of ['dark', 'light']) {
      if (await page.locator('html').getAttribute('data-theme') !== theme) {
        await page.getByRole('main').getByRole('button', { name: /切换到.*主题/ }).click();
      }
      for (const [id, label] of [['project', '项目'], ['sources', '资料与风格'], ['tasks', '任务'], ['result', '结果'], ['settings', '打开设置']]) {
        if (id === 'settings') await page.getByRole('button', { name: label, exact: true }).click();
        else await navigate(label);
        await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
        await capture(`installed-${id}-${theme}.png`);
      }
    }
    assert.deepEqual(report.errors, []);
  });
  report.passed = true;
} catch (error) {
  report.passed = false;
  report.failure = String(error.stack ?? error);
  if (page) await capture('failure.png', true).catch(() => {});
  process.exitCode = 1;
} finally {
  report.finished_at = new Date().toISOString();
  await fs.writeFile(path.join(evidence, 'acceptance.json'), JSON.stringify(report, null, 2));
  if (browser) await browser.close();
  console.log(JSON.stringify({ passed: report.passed, cycle: report.cycle, evidence, failure: report.failure }));
}
