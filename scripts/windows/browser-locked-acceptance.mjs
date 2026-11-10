// Real headed Chromium on an already locked Windows session. Everything is a
// public localhost fixture; no account, existing profile, installed companion,
// Windows input, or host/trust configuration is used. Only page CDP sessions are
// attached: attaching DevTools to the worker would itself keep MV3 alive.
import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync, randomBytes, X509Certificate } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { cp, mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import http from 'node:http';
import https from 'node:https';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const args = Object.fromEntries(Array.from({ length: (process.argv.length - 2) / 2 }, (_, i) => [process.argv[2 + i * 2].replace(/^--/, ''), process.argv[3 + i * 2]]));
for (const key of ['chromium', 'websocket-module', 'tls-cert', 'tls-key', 'artifacts', 'extension', 'image', 'windows-probe']) assert.ok(args[key], `Missing --${key}`);
assert.equal(process.platform, 'win32', 'Run through native Windows Node, not WSL or headless Linux.');
const generationWaitMs = Number(args['generation-wait-ms'] ?? 65_000);
assert.ok(Number.isInteger(generationWaitMs) && generationWaitMs >= 65_000 && generationWaitMs <= 540_000, '--generation-wait-ms must be an integer from 65000 to 540000.');
const artifacts = path.resolve(args.artifacts);
const profile = path.join(artifacts, 'chromium-profile');
await mkdir(artifacts, { recursive: true });
const { WebSocketServer } = await import(pathToFileURL(path.resolve(args['websocket-module'])).href);
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const started = Date.now();
const receipt = { fixture_only: true, scope: 'real browser adapter and localhost WS protocol; not Rust/frontend or real ChatGPT acceptance', worker_debugger_attached: false, windows: [], messages: [], blocked_requests: [], chromium_flags: [], checks: {} };
receipt.generation_wait_ms = generationWaitMs;
const fixtureErrors = [];
function windowsProbe() {
  const shell = path.join(process.env.SystemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe');
  const result = spawnSync(shell, ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', args['windows-probe'], '-ProbeOnly', ...(browser?.pid ? ['-BrowserProcessId', String(browser.pid)] : [])], { encoding: 'utf8', windowsHide: true });
  assert.equal(result.status, 0, 'Read-only Windows session probe failed.');
  const probe = JSON.parse(result.stdout.trim());
  assert.equal(probe.locked, true, 'Windows must remain locked throughout acceptance.');
  assert.equal(probe.input_desktop_accessible, false);
  assert.equal(probe.session_id, probe.probe_session);
  if (browser?.pid) { assert.equal(probe.browser_process_id, browser.pid); assert.equal(probe.browser_session, probe.session_id); }
  receipt.windows.push(probe); return probe;
}
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check, label, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { if (fixtureErrors.length) throw new Error(fixtureErrors.join('; ')); const value = await check(); if (value) return value; await pause(100); }
  throw new Error(`Timed out: ${label}`);
}
const listen = server => new Promise((resolve, reject) => { server.once('error', reject); server.listen(0, '127.0.0.1', () => resolve(server.address().port)); });
const fixtureImage = await readFile(args.image);
const originalHash = sha256(fixtureImage);
const prompt = 'Public locked-session fixture; generate no real account request.';
const run = { run_id: 'locked-fixture-run', task_id: 'locked-fixture-task', base_version_id: 'locked-fixture-version', prompt, prompt_sha256: sha256(prompt) };
let submissions = 0, downloads = 0, socket, browser, client, httpsServer, httpServer, wsServer, lockMonitor;
let artifact, submittedAt, acknowledgedAt;
let browserStderr = '';
try {
  windowsProbe();
  await mkdir(profile); // Refuse reuse before touching an earlier run's evidence.
  // Copy the current production bytes. A fixture-only public key gives a stable
  // extension id without ever opening or attaching to its service worker.
  const extension = path.join(artifacts, 'companion');
  await cp(path.resolve(args.extension), extension, { recursive: true, filter: source => !source.split(path.sep).some(part => part === 'node_modules' || part === 'test') });
  const manifestPath = path.join(extension, 'manifest.json');
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  const sourceManifest = JSON.parse(await readFile(path.join(args.extension, 'manifest.json'), 'utf8'));
  assert.deepEqual(manifest, sourceManifest, 'Loaded manifest matches source before the fixture id key.');
  const { publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const publicBytes = publicKey.export({ type: 'spki', format: 'der' });
  manifest.key = publicBytes.toString('base64');
  await writeFile(manifestPath, JSON.stringify(manifest, null, 2));
  const extensionId = [...sha256(publicBytes).slice(0, 32)].map(x => String.fromCharCode(97 + parseInt(x, 16))).join('');
  const runtimeNames = (await readdir(args.extension, { withFileTypes: true })).filter(entry=>entry.isFile() && /\.(?:mjs|js)$/.test(entry.name)).map(entry=>entry.name);
  receipt.production_sources = Object.fromEntries(await Promise.all(runtimeNames.map(async name => [name, sha256(await readFile(path.join(args.extension, name)))])));
  receipt.loaded_sources = Object.fromEntries(await Promise.all(runtimeNames.map(async name => [name, sha256(await readFile(path.join(extension, name)))])));
  assert.deepEqual(receipt.loaded_sources, receipt.production_sources, 'Every loaded runtime module matches the current source byte for byte.');
  receipt.production_sources['manifest.json'] = sha256(await readFile(path.join(args.extension, 'manifest.json')));
  const loadedManifest = JSON.parse(await readFile(manifestPath, 'utf8')); delete loadedManifest.key;
  const expectedManifest = { ...sourceManifest }; delete expectedManifest.key;
  assert.deepEqual(loadedManifest, expectedManifest);
  receipt.checks.loaded_manifest_only_fixture_id_key_changed = true;
  const html = `<!doctype html><html><body><main></main><textarea id="prompt-textarea"></textarea><button data-testid="send-button">Send</button><script>
  const editor=document.querySelector('textarea');
  if(location.pathname==='/existing') editor.value='Existing public fixture draft, preserve exactly.';
  document.querySelector('button').onclick=async()=>{
    const prompt=editor.value;
    await fetch('/fixture/submit',{method:'POST',body:prompt});
    history.pushState(null,'','/c/locked-fixture');
    const user=document.createElement('div');user.dataset.messageAuthorRole='user';user.textContent=prompt;document.querySelector('main').append(user);
    const response=document.createElement('div');response.dataset.messageAuthorRole='assistant';response.setAttribute('aria-busy','true');document.querySelector('main').append(response);
    const stream=await fetch('/fixture/generate'); await stream.text();
    response.removeAttribute('aria-busy');response.innerHTML='<img src="/fixture/original.png"><a href="/fixture/original.png" download="fixture-original.png">Download image</a>';
  };
  </script></body></html>`;
  httpsServer = https.createServer({ cert: await readFile(args['tls-cert']), key: await readFile(args['tls-key']) }, (request, response) => {
    if (request.headers.host !== 'chatgpt.com') { response.writeHead(403); response.end(); return; }
    if (request.url === '/fixture/submit' && request.method === 'POST') {
      let body=''; request.on('data', chunk => { body += chunk; }); request.on('end', () => { if (body !== prompt) fixtureErrors.push('Unexpected fixture prompt'); submissions++; submittedAt = Date.now(); response.end('ok'); });
    } else if (request.url === '/fixture/generate') {
      // This server-side wait stays live for > two MV3 idle windows while all
      // Chromium windows are occluded by the real Windows lock screen.
      response.writeHead(200, { 'Content-Type': 'text/plain' }); response.write('waiting');
      setTimeout(() => { if (!response.destroyed) response.end('ready'); }, generationWaitMs).unref();
    } else if (request.url === '/fixture/original.png') {
      downloads++; response.writeHead(200, { 'Content-Type': 'image/png' }); response.end(fixtureImage);
    } else if (request.url === '/' || request.url === '/existing' || request.url === '/inspector' || request.url === '/c/locked-fixture') {
      response.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); response.end(html);
    } else { response.writeHead(404); response.end(); }
  });
  const httpsPort = await listen(httpsServer);
  httpServer = http.createServer((_, response) => { response.writeHead(404); response.end(); });
  const wsPort = await listen(httpServer);
  assert.notEqual(wsPort, 48123);
  wsServer = new WebSocketServer({ server: httpServer, path: '/api/automation/extension' });
  const token = randomBytes(32).toString('hex');
  let authenticated = false;
  wsServer.on('connection', peer => {
    if (socket !== undefined) { fixtureErrors.push('Worker reconnected/restarted unexpectedly'); peer.close(); return; }
    socket = peer;
    peer.on('message', data => {
      let message;
      try { message = JSON.parse(String(data)); } catch { fixtureErrors.push('Invalid extension message'); return; }
      receipt.messages.push({ type: message.type, phase: message.phase, detail: message.detail, at_ms: Date.now() - started });
      if (message.type === 'authenticate') { if (message.token !== token) { fixtureErrors.push('Wrong fixture token'); peer.close(); return; } authenticated = true; peer.send(JSON.stringify({ type: 'authenticated' })); }
      if (message.type === 'artifact') artifact = message;
    });
  });
  const cert = new X509Certificate(await readFile(args['tls-cert']));
  const spki = createHash('sha256').update(cert.publicKey.export({ type: 'spki', format: 'der' })).digest('base64');
  const chromiumFlags = [
    `--user-data-dir=${profile}`, '--remote-debugging-port=0', '--remote-debugging-address=127.0.0.1',
    `--load-extension=${extension}`, `--disable-extensions-except=${extension}`,
    '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--disable-component-update', '--disable-sync', '--disable-default-apps', '--no-proxy-server',
    `--host-resolver-rules=MAP chatgpt.com 127.0.0.1:${httpsPort},MAP * ~NOTFOUND,EXCLUDE 127.0.0.1,EXCLUDE localhost`,
    `--ignore-certificate-errors-spki-list=${spki}`, 'about:blank'
  ];
  assert.ok(!chromiumFlags.some(arg => /headless|disable-background-timer-throttling|disable-backgrounding-occluded-windows|disable-renderer-backgrounding/.test(arg)));
  receipt.chromium_flags = chromiumFlags;
  browser = spawn(args.chromium, chromiumFlags, { stdio: ['ignore', 'ignore', 'pipe'] });
  browser.on('error', error => { fixtureErrors.push(`Chromium spawn failed: ${error.message}`); });
  browser.stderr.on('data', bytes => { browserStderr += bytes; });
  const portFile = path.join(profile, 'DevToolsActivePort');
  const portInfo = await until(async () => { try { return (await readFile(portFile, 'utf8')).trim().split(/\r?\n/); } catch { return null; } }, 'isolated headed Chromium debugger endpoint');
  const version = await (await fetch(`http://127.0.0.1:${portInfo[0]}/json/version`)).json();
  receipt.browser_version = version.Browser;
  receipt.browser_pid = browser.pid;
  windowsProbe();
  receipt.lock_sampling_interval_ms = 5000;
  receipt.lock_evidence_scope = 'Repeated WTS samples bound to the actual browser PID/session; no unlock was observed between startup and completion.';
  lockMonitor = setInterval(() => { try { windowsProbe(); } catch (error) { fixtureErrors.push(`Windows lock monitor: ${error.message}`); } }, receipt.lock_sampling_interval_ms);
  const ws = new WebSocket(version.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { ws.addEventListener('open', resolve, { once: true }); ws.addEventListener('error', reject, { once: true }); });
  let nextId = 0;
  const pending = new Map();
  const events = new Set();
  ws.addEventListener('message', event => {
    const message = JSON.parse(String(event.data));
    if (message.id) { const call = pending.get(message.id); pending.delete(message.id); clearTimeout(call?.timer); if (message.error) call?.reject(new Error(message.error.message)); else call?.resolve(message.result); }
    else for (const handle of events) handle(message);
  });
  const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++nextId;
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15_000);
    pending.set(id, { resolve, reject, timer }); ws.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  ws.addEventListener('close', () => { for (const call of pending.values()) { clearTimeout(call.timer); call.reject(new Error('Browser CDP disconnected')); } pending.clear(); });
  client = { send, ws };
  async function attachPage(targetId) {
    const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
    await send('Runtime.enable', {}, sessionId);
    await send('Page.enable', {}, sessionId);
    await send('Fetch.enable', { patterns: [{ urlPattern: '*' }] }, sessionId);
    events.add(message => {
      if (message.sessionId !== sessionId || message.method !== 'Fetch.requestPaused') return;
      const { requestId, request } = message.params;
      const url = new URL(request.url);
      const operation = (url.origin === 'https://chatgpt.com' && ['/','/existing','/inspector','/c/locked-fixture','/fixture/submit','/fixture/generate','/fixture/original.png','/favicon.ico'].includes(url.pathname)) || ['chrome-extension:', 'chrome:', 'data:', 'blob:'].includes(url.protocol)
        ? send('Fetch.continueRequest', { requestId }, sessionId)
        : (receipt.blocked_requests.push({ origin: url.origin, path: url.pathname }), send('Fetch.failRequest', { requestId, errorReason: 'BlockedByClient' }, sessionId));
      operation.catch(error => { if (!/Session.*not found|Target closed|CDP disconnected|Invalid InterceptionId/i.test(error.message)) fixtureErrors.push(`Fixture request interception: ${error.message}`); });
    });
    const evaluate = async expression => {
      const result = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true }, sessionId);
      if (result.exceptionDetails) throw new Error(result.exceptionDetails.text + ': ' + result.result?.description);
      return result.result.value;
    };
    return { targetId, sessionId, evaluate };
  }
  const original = await attachPage((await send('Target.createTarget', { url: 'about:blank' })).targetId);
  await send('Page.navigate', { url: 'https://chatgpt.com/existing' }, original.sessionId);
  await until(() => original.evaluate("document.querySelector('textarea')?.value==='Existing public fixture draft, preserve exactly.'"), 'original fixture draft');
  const options = await attachPage((await send('Target.createTarget', { url: `chrome-extension://${extensionId}/options.html` })).targetId);
  await until(() => options.evaluate("Boolean(document.querySelector('#settings') && document.querySelector('#status').textContent)"), 'extension configuration module ready');
  await options.evaluate(`document.querySelector('#settings').value=${JSON.stringify(JSON.stringify({ websocket_url: `ws://127.0.0.1:${wsPort}/api/automation/extension`, extension_token: token }))};document.querySelector('#pair').dispatchEvent(new Event('submit',{bubbles:true,cancelable:true}));true`);
  try { await until(() => authenticated, 'extension WebSocket authentication'); }
  catch (error) { receipt.options_status = await options.evaluate("document.querySelector('#status')?.textContent"); throw error; }
  await until(() => receipt.messages.some(message => message.type === 'probe'), 'logged-in fixture probe');
  await send('Target.closeTarget', { targetId: options.targetId });
  // Freeze only this disposable profile's pre-existing fixture tab. The
  // production adapter must start after lock without reloading/activating it.
  await send('Page.setWebLifecycleState', { state: 'frozen' }, original.sessionId);
  await send('Target.detachFromTarget', { sessionId: original.sessionId });
  receipt.checks.existing_tab_frozen_before_start = true;
  socket.send(JSON.stringify({ type: 'start', run }));
  await until(() => submissions === 1, 'single fixture submission');
  const taskTarget = await until(async () => (await send('Target.getTargets')).targetInfos.find(info => info.type === 'page' && info.url === 'https://chatgpt.com/c/locked-fixture'), 'isolated task page');
  // Never attach DevTools to the task: a task debugger would itself prevent
  // freezing and hide a missing Web Lock. Query origin locks from another page.
  const inspector = await attachPage((await send('Target.createTarget', { url: 'about:blank' })).targetId);
  await send('Page.navigate', { url: 'https://chatgpt.com/inspector' }, inspector.sessionId);
  await until(() => inspector.evaluate('Boolean(document.querySelector("textarea"))'), 'origin lock inspector');
  assert.notEqual(taskTarget.targetId, original.targetId);
  const lockName = `entwurf-chatgpt:${run.run_id}`;
  await until(() => inspector.evaluate(`navigator.locks.query().then(info=>info.held.some(lock=>lock.name===${JSON.stringify(lockName)}&&lock.mode==='exclusive'))`), 'Web Lock held by content page');
  receipt.checks.web_lock_held = true;
  const diagnostic = await attachPage((await send('Target.createTarget', { url: 'chrome://discards/' })).targetId);
  await send('Page.navigate', { url: 'chrome://discards/' }, diagnostic.sessionId);
  await until(() => diagnostic.evaluate('document.readyState==="complete" && document.body?.children.length>0'), 'Chromium diagnostics loaded');
  const collectDiagnosticDom = String.raw`(() => { const collect = root => (root.innerHTML || root.documentElement?.outerHTML || '') + '\n' + [...root.querySelectorAll('*')].map(element=>element.shadowRoot ? collect(element.shadowRoot) : '').join('\n'); return collect(document); })()`;
  if (await diagnostic.evaluate('/内部调试页面目前已停用|Internal debugging pages.*disabled/i.test(document.body.innerText)')) {
    await send('Page.navigate', { url: 'chrome://chrome-urls/' }, diagnostic.sessionId);
    await until(() => diagnostic.evaluate('document.readyState==="complete" && document.body?.children.length>0'), 'isolated-profile internal debug setting');
    await writeFile(path.join(artifacts, 'chrome-urls-dom.html'), await diagnostic.evaluate(collectDiagnosticDom));
    await until(() => diagnostic.evaluate(String.raw`(() => { const roots=[document];for(let i=0;i<roots.length;i++)for(const e of roots[i].querySelectorAll('*'))if(e.shadowRoot)roots.push(e.shadowRoot);for(const root of roots)for(const button of root.querySelectorAll('button,[role="button"]'))if(/启用.*调试|Enable.*debug/i.test(button.innerText)){button.click();return true;}return false; })()`), 'enable internal diagnostics only in disposable Chromium profile');
    await send('Page.navigate', { url: 'chrome://discards/' }, diagnostic.sessionId);
    await until(() => diagnostic.evaluate('document.readyState==="complete" && document.body?.children.length>0'), 'enabled Chromium diagnostics loaded');
    receipt.checks.diagnostic_pages_enabled_in_test_profile = true;
  }
  const diagnosticDom = await diagnostic.evaluate(collectDiagnosticDom);
  await writeFile(path.join(artifacts, 'discards-dom.html'), diagnosticDom);
  const diagnosticExpression = String.raw`(() => {
    const roots = [document];
    for (let i=0;i<roots.length;i++) for (const element of roots[i].querySelectorAll('*')) if (element.shadowRoot) roots.push(element.shadowRoot);
    for (const root of roots) for (const row of root.querySelectorAll('tr, [role="row"], .table-row')) {
      const bound = [...row.querySelectorAll('a')].some(link=>link.href==='https://chatgpt.com/c/locked-fixture') || Boolean(row.querySelector('[title="https://chatgpt.com/c/locked-fixture"]'));
      if (!bound && !(row.innerText||'').includes('https://chatgpt.com/c/locked-fixture')) continue;
      const header=root.querySelector('[data-sort-key="canFreeze"]');
      if (!header) continue;
      const index=[...header.parentElement.children].indexOf(header);
      const freezeCell=row.children[index];
      if (!freezeCell) continue;
      return {row_text:row.innerText||'', task_url:'https://chatgpt.com/c/locked-fixture', can_freeze_cell:freezeCell.textContent.trim()};
    }
    return null;
  })()`;
  let diagnosticText;
  try { diagnosticText = await until(() => diagnostic.evaluate(diagnosticExpression), 'Chromium diagnostic row bound to owned task URL'); }
  catch (error) { await writeFile(path.join(artifacts, 'discards-dom.html'), await diagnostic.evaluate(collectDiagnosticDom)); throw error; }
  await writeFile(path.join(artifacts, 'cannot-freeze-diagnostic.json'), JSON.stringify(diagnosticText, null, 2));
  assert.match(diagnosticText.can_freeze_cell, /holding Web Lock/i, 'The owned task row Can Freeze cell must identify its held Web Lock.');
  receipt.checks.chromium_cannot_freeze_web_lock = true;
  await send('Target.closeTarget', { targetId: diagnostic.targetId });
  const taskWindow = await send('Browser.getWindowForTarget', { targetId: taskTarget.targetId });
  const originalWindow = await send('Browser.getWindowForTarget', { targetId: original.targetId });
  assert.notEqual(taskWindow.windowId, originalWindow.windowId);
  receipt.checks.independent_window = true;
  // Repeated delivery of the same start exercises the real persisted guard.
  socket.send(JSON.stringify({ type: 'start', run }));
  await pause(32_000);
  windowsProbe();
  assert.equal(artifact, undefined, 'The held fixture stream has not completed yet.');
  assert.equal(submissions, 1);
  assert.ok(receipt.messages.some(message => message.type === 'keepalive'), 'The worker must send a heartbeat independently of DOM probing.');
  assert.ok((await send('Target.getTargets')).targetInfos.some(info => info.type === 'service_worker' && info.url === `chrome-extension://${extensionId}/background.mjs`));
  assert.equal(await inspector.evaluate(`navigator.locks.query().then(info=>info.held.some(lock=>lock.name===${JSON.stringify(lockName)}))`), true);
  await until(() => artifact, 'original image captured after configured background interval', generationWaitMs + 30_000);
  assert.equal(artifact.run_id, run.run_id); assert.equal(artifact.task_id, run.task_id); assert.equal(artifact.base_version_id, run.base_version_id);
  assert.equal(artifact.mime_type, 'image/png'); assert.equal(artifact.sha256, originalHash);
  assert.deepEqual(Buffer.from(artifact.bytes_base64, 'base64'), fixtureImage);
  // This protocol fixture persists and independently rereads the exact original
  // before sending ACK. It is intentionally not a Rust/project import claim.
  const retainedPath = path.join(artifacts, 'persisted-original.png');
  await writeFile(retainedPath, Buffer.from(artifact.bytes_base64, 'base64'));
  assert.equal(sha256(await readFile(retainedPath)), originalHash);
  receipt.checks.exact_original_persisted_before_ack = true;
  assert.equal(await inspector.evaluate(`navigator.locks.query().then(info=>info.held.some(lock=>lock.name===${JSON.stringify(lockName)}))`), true, 'Lock remains held until ACK.');
  acknowledgedAt = Date.now();
  socket.send(JSON.stringify({ type: 'artifact_accepted', run_id: run.run_id }));
  await until(async () => !(await send('Target.getTargets')).targetInfos.some(info => info.targetId === taskTarget.targetId), 'owned task window removed after ACK');
  assert.equal(await inspector.evaluate(`navigator.locks.query().then(info=>info.held.some(lock=>lock.name===${JSON.stringify(lockName)}))`), false);
  const restoredOriginal = await attachPage(original.targetId);
  await send('Page.setWebLifecycleState', { state: 'active' }, restoredOriginal.sessionId);
  assert.equal(await restoredOriginal.evaluate("document.querySelector('textarea').value"), 'Existing public fixture draft, preserve exactly.');
  receipt.checks.original_draft_preserved = true;
  receipt.checks.web_lock_released = true; receipt.checks.owned_window_cleanup = true;
  assert.equal(submissions, 1);
  receipt.checks.exactly_one_submission = true;
  receipt.checks.keepalive_count = receipt.messages.filter(message => message.type === 'keepalive').length;
  assert.ok(receipt.checks.keepalive_count >= 3);
  receipt.checks.locked_generation_ms = acknowledgedAt - submittedAt;
  assert.ok(receipt.checks.locked_generation_ms >= generationWaitMs);
  assert.equal(receipt.blocked_requests.length, 0);
  windowsProbe();
  receipt.passed = true;
  console.log(JSON.stringify({ passed: true, checks: receipt.checks, artifacts }));
} catch (error) {
  receipt.passed = false; receipt.error = String(error.stack || error);
  console.error(receipt.error); process.exitCode = 1;
} finally {
  clearInterval(lockMonitor);
  if (client) { try { await client.send('Browser.close'); } catch {} client.ws.close(); }
  if (browser && browser.exitCode === null) {
    await Promise.race([new Promise(resolve => browser.once('exit', resolve)), pause(5000)]);
    if (browser.exitCode === null) browser.kill();
    await Promise.race([new Promise(resolve => browser.once('exit', resolve)), pause(5000)]);
  }
  receipt.checks.browser_exit_verified = !browser?.pid || browser.exitCode !== null || browser.signalCode !== null;
  if (!receipt.checks.browser_exit_verified) { receipt.passed = false; receipt.error = (receipt.error || '') + '\nOwned Chromium did not confirm exit.'; process.exitCode = 1; }
  socket?.close(); wsServer?.close();
  httpsServer?.closeAllConnections(); httpServer?.closeAllConnections();
  await Promise.all([httpsServer, httpServer].filter(Boolean).map(server => new Promise(resolve => server.close(resolve))));
  receipt.elapsed_ms = Date.now() - started;
  receipt.fixture_errors = fixtureErrors;
  await writeFile(path.join(artifacts, 'receipt.json'), JSON.stringify(receipt, null, 2));
  await writeFile(path.join(artifacts, 'chromium.stderr.log'), browserStderr);
}
