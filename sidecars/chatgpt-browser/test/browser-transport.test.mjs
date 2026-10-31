import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BrowserTransport } from '../browser-transport.mjs';

function browser(reports) {
  const actions = [];
  return { actions, tabs: {
    query: async () => [{ id: 1, windowId: 3, active: true }],
    create: async () => { actions.push('create'); return { id: 2 }; },
    get: async () => ({ url: 'https://chatgpt.com/' }),
    sendMessage: async (id, message) => { actions.push(message.type); return { ok: true, result: reports[id] }; },
  }, windows: { update: async () => {} } };
}

test('anonymous existing tabs expose login_required without opening or preparing another tab', async () => {
  const api = browser({ 1: { available: false, reason: 'login_required' } });
  const transport = new BrowserTransport(api);
  assert.equal((await transport.probe()).reason, 'login_required');
  await assert.rejects(transport.prepare({ run_id: 'r' }), /login_required/);
  assert.deepEqual(api.actions, ['probe', 'probe']);
});

test('a newly opened anonymous tab fails preparation promptly without a submit', async () => {
  const api = browser({ 1: { available: true }, 2: { available: false, reason: 'login_required' } });
  const transport = new BrowserTransport(api);
  await assert.rejects(transport.prepare({ run_id: 'r' }), /login_required/);
  assert.deepEqual(api.actions, ['probe', 'create', 'probe']);
});
