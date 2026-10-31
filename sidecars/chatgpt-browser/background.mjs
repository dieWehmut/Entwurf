import { RunExecutor, safeCode } from './executor.mjs';
import { BrowserTransport } from './browser-transport.mjs';
import { CompanionConnection, validateConnection } from './connection.mjs';
import { artifactStore, runStore } from './persistence.mjs';

const store = runStore(chrome.storage.local);
const transport = new BrowserTransport(chrome);
let connection, reconnect, connecting = false, ticking = false;
const emit = message => connection?.send(message);
const executor = new RunExecutor({ store, transport, artifactStore, emit });
const state = (phase, reason = null) => chrome.storage.local.set({ connection_status: { phase, reason } });
async function probe() { emit({ type: 'probe', ...await transport.probe() }); }
async function restore() {
  for (const record of await store.list()) {
    if (record.phase === 'submit_intent') { record.phase = 'submission_uncertain'; record.detail = 'executor_restarted'; await store.put(record); }
    if (record.phase === 'preparing') { record.phase = 'failed'; record.detail = 'executor_restarted'; await store.put(record); }
    if (record.phase === 'downloading') {
      record.phase = await artifactStore.get(record.run_id) ? 'awaiting_import' : 'paused';
      record.detail = 'capture_interrupted'; await store.put(record);
    }
    if (!['completed', 'cancelled', 'failed'].includes(record.phase)) {
      executor.active = record.run_id;
      await executor.status(record.run_id);
    }
  }
  await probe();
}
async function command(message) {
  try {
    switch (message.type) {
      case 'start': await executor.start(message.run); break;
      case 'status': await executor.status(message.run_id); break;
      case 'cancel': await executor.cancel(message.run_id); break;
      case 'artifact_accepted': await executor.accept(message.run_id); break;
      case 'probe': await probe(); break;
    }
  } catch (error) {
    const run_id = message.run?.run_id ?? message.run_id;
    if (typeof run_id === 'string' && /^[A-Za-z0-9_-]{1,128}$/.test(run_id)) emit({ type: 'status', run_id, phase: 'failed', detail: safeCode(error) });
  }
}
async function connect() {
  if (connecting || [0, 1].includes(connection?.socket.readyState)) return;
  connecting = true;
  try {
    const { companion_config } = await chrome.storage.local.get('companion_config');
    if (!companion_config) { await state('unpaired'); return; }
    validateConnection(companion_config);
    await state('connecting');
    connection = new CompanionConnection(companion_config, {
      onCommand: command,
      onAuthenticated: async () => { await state('connected'); await restore(); },
      onDisconnected: () => { void state('disconnected'); clearTimeout(reconnect); reconnect = setTimeout(connect, 3000); },
      onError: reason => { void state('error', reason); }
    });
  } catch (error) { await state('error', safeCode(error)); }
  finally { connecting = false; }
}
chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (sender.id !== chrome.runtime.id || sender.url !== chrome.runtime.getURL('options.html') || message?.type !== 'configure') return false;
  (async () => {
    const config = validateConnection(message.config);
    await chrome.storage.local.set({ companion_config: config });
    if (connection) { connection.socket.onclose = null; connection.close(); }
    clearTimeout(reconnect); await connect(); reply({ ok: true });
  })().catch(error => reply({ ok: false, error: safeCode(error) }));
  return true;
});
setInterval(async () => {
  if (ticking || !connection?.authenticated || !executor.active) return;
  ticking = true;
  try { await executor.tick(executor.active); } catch { /* Each run exposes bounded status through its executor. */ }
  finally { ticking = false; }
}, 1000);
// Chrome 116+ keeps a service worker with an active WebSocket alive when messages flow.
setInterval(() => { if (connection?.authenticated) void probe(); }, 20000);
chrome.runtime.onStartup.addListener(() => { void connect(); });
chrome.runtime.onInstalled.addListener(() => { void connect(); });
void connect();
