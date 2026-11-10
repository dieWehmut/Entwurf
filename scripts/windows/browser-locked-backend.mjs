// Optional real local HTTP/SQLite and frontend fixture for the locked browser
// runner. It refuses the installed service and nonempty application stores.
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { readFile, realpath } from 'node:fs/promises';
import path from 'node:path';

export async function prepareBackend(args, receipt, originalBytes, sha256) {
  if (!args['backend-url']) {
    assert.ok(!args['backend-data-dir'], '--backend-data-dir requires --backend-url');
    return null;
  }
  assert.ok(args['backend-data-dir'], 'Real backend acceptance requires its explicit isolated --backend-data-dir.');
  const address = new URL(args['backend-url']);
  assert.equal(address.protocol, 'http:');
  assert.equal(address.hostname, '127.0.0.1');
  assert.equal(address.pathname, '/');
  assert.ok(address.port && address.port !== '48123');
  assert.ok(!address.username && !address.password && !address.search && !address.hash);
  const base = address.origin;
  const data = path.resolve(args['backend-data-dir']);
  assert.ok(data.split(path.sep).includes('artifacts'), 'Only an explicit artifacts fixture data directory is accepted.');
  const database = JSON.parse(await readFile(path.join(data, 'automation', 'automation.json'), 'utf8'));
  assert.equal(database.schema_version, 2);
  assert.equal(database.provider, 'browser');
  assert.equal(Object.keys(database.runs).length, 0, 'Never attach this fixture to existing runs.');
  const client = randomUUID();
  const bootstrap = await json('/api/bootstrap', { headers: { 'X-Entwurf-Client': client } });
  assert.equal(bootstrap.runtime, 'local-http');
  assert.equal(bootstrap.storage, 'sqlite');
  const headers = { 'X-Entwurf-Client': client, 'X-Entwurf-Session': bootstrap.session_token };
  const pairing = await json('/api/automation/pairing', { headers });
  assert.ok(pairing.extension_token === database.extension_token, 'The server must own exactly the explicit fixture data directory.');
  assert.equal(new URL(pairing.websocket_url).origin, `ws://${address.host}`);
  assert.deepEqual(await json('/api/projects', { headers }), [], 'Never attach this fixture to existing projects.');
  const access = await command('automation_access', {});
  const automationHeaders = { Authorization: `Bearer ${access.token}` };
  const root = `/projects/${randomUUID()}`;
  await command('create_project', { root, name: 'Public locked Windows integration fixture' });
  const task = await command('create_task', { title: 'Real browser and SQLite fixture' });
  const body = new FormData();
  body.append('files', new Blob(['# Public fixture\n\nA sparse source for locked Windows image acceptance.\n'], { type: 'text/markdown' }), 'locked-fixture.md');
  const uploads = await json('/api/uploads', { method: 'POST', headers, body });
  const snapshot = await command('import_sources', { taskId: task.selected_task_id, paths: uploads.map(file => file.path) });
  await command('discard_uploads', { paths: uploads.map(file => file.path) });
  assert.ok(snapshot.current_version?.id);
  const expected = { task_id: snapshot.selected_task_id, base_version_id: snapshot.current_version.id };
  const baseVersion = snapshot.current_version;
  receipt.scope = 'real locked Windows browser adapter, local HTTP/SQLite and frontend automatic import; public ChatGPT fixture, not live-account acceptance';
  receipt.backend = { origin: base, data_directory: data, runtime: bootstrap.runtime, storage: bootstrap.storage, platform: bootstrap.platform, project_root: root, ...expected };

  async function json(url, init = {}) {
    const response = await fetch(base + url, { ...init, signal: AbortSignal.timeout(30_000) });
    const result = await response.json();
    assert.ok(response.ok, `Fixture backend ${url}: ${response.status} ${result?.error ?? ''}`);
    return result;
  }
  async function command(name, args) {
    return json('/api/command', { method: 'POST', headers: { ...headers, 'Content-Type': 'application/json' }, body: JSON.stringify({ command: name, args }) });
  }
  async function verifyImport(run, artifact) {
    // Reopen the real project from SQLite rather than trusting the frontend's
    // return value or the coordinator ACK alone.
    await command('load_existing_project', { root });
    const persisted = await command('workspace_snapshot', { taskId: run.task_id });
    assert.deepEqual(persisted.current_version, baseVersion, 'Image import must preserve the knowledge version.');
    const images = persisted.project.generated_images ?? [];
    assert.equal(images.length, 1);
    const image = images[0];
    for (const name of ['run_id', 'task_id', 'base_version_id', 'prompt_sha256']) assert.equal(image[name], run[name]);
    assert.equal(image.sha256, sha256(originalBytes));
    assert.equal(image.mime_type, 'image/png');
    const ownedPath = path.join(data, root.slice(1), 'generated', image.sha256);
    assert.equal(await realpath(image.stored_path), await realpath(ownedPath));
    assert.deepEqual(await readFile(ownedPath), originalBytes);
    assert.deepEqual(Buffer.from(await command('generated_image_bytes', { taskId: run.task_id, imageId: image.id })), originalBytes);
    assert.equal(artifact.sha256, image.sha256);
    const storedRun = JSON.parse(await readFile(path.join(data, 'automation', 'automation.json'), 'utf8')).runs[run.run_id];
    assert.equal(storedRun.phase, 'completed');
    assert.equal(storedRun.image_id, image.id);
    receipt.checks.real_sqlite_project_import_before_ack_delivery = true;
    receipt.checks.knowledge_version_preserved = true;
    receipt.backend.image_sha256 = image.sha256;
  }
  async function openFrontend(attachPage, send, until, prompt, started) {
    const page = await attachPage((await send('Target.createTarget', { url: 'about:blank' })).targetId);
    await send('Page.addScriptToEvaluateOnNewDocument', { source: `sessionStorage.setItem('entwurf.http-selection.v1',${JSON.stringify(JSON.stringify({ root, taskId: expected.task_id }))});` }, page.sessionId);
    await send('Page.navigate', { url: base + '/' }, page.sessionId);
    await until(() => page.evaluate(`(() => { const button=[...document.querySelectorAll('nav button')].find(e=>/^任务/.test(e.textContent.trim()));if(!button || button.disabled)return false;button.click();return true; })()`), 'real frontend task navigation');
    await until(() => page.evaluate(`Boolean([...document.querySelectorAll('label')].find(e=>e.textContent.includes('图片生成提示'))?.querySelector('textarea'))`), 'real frontend image composer');
    await page.evaluate(`(() => { const editor=[...document.querySelectorAll('label')].find(e=>e.textContent.includes('图片生成提示')).querySelector('textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(editor,${JSON.stringify(prompt)});editor.dispatchEvent(new Event('input',{bubbles:true}));return true; })()`);
    await until(() => page.evaluate(`(() => { const button=[...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='启动一次图片生成');return Boolean(button && !button.disabled); })()`), 'real frontend ready for the fixture run');
    await send('Fetch.disable', {}, page.sessionId);
    await page.evaluate(`(() => { [...document.querySelectorAll('button')].find(e=>e.textContent.trim()==='启动一次图片生成').click();return true; })()`);
    await until(started, 'real frontend run accepted by the coordinator');
    await send('Target.detachFromTarget', { sessionId: page.sessionId });
    receipt.checks.frontend_debugger_detached_after_start = true;
    receipt.backend.frontend_target_id = page.targetId;
    return page;
  }
  return { pairing, expected, openFrontend, verifyImport, getRun: id => json(`/api/automation/runs/${id}`, { headers: automationHeaders }), cancel: id => json(`/api/automation/runs/${id}/cancel`, { method: 'POST', headers: { ...automationHeaders, 'Content-Type': 'application/json' }, body: '{}' }) };
}
