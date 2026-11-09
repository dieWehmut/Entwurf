import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BrowserTransport } from '../browser-transport.mjs';

function browser(reports) {
  const actions = [];
  return { actions, tabs: {
    query: async options => options.windowId === 9
      ? [{ id: 2, windowId: 9, active: true }]
      : [{ id: 1, windowId: 3, active: true }],
    create: async () => { throw new Error('task preparation must use an isolated background window'); },
    update: async (id, options) => { actions.push(['update', id, options]); },
    remove: async id => { actions.push(['tab-remove', id]); },
    get: async () => ({ url: 'https://chatgpt.com/' }),
    sendMessage: async (id, message) => { actions.push(message.type); return { ok: true, result: reports[id] }; },
  }, windows: {
    create: async options => { actions.push(['window-create', options]); return { id: 9, focused: false }; },
    update: async () => { throw new Error('background preparation must not focus a browser window'); },
    remove: async id => { actions.push(['window-remove', id]); }
  } };
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
  assert.deepEqual(api.actions, [
    'probe',
    ['window-create', { url: 'https://chatgpt.com/', focused: false }],
    ['update', 2, { autoDiscardable: false }],
    'probe',
    ['window-remove', 9]
  ]);
});

test('cancelling a bound background task closes only its owned tab after page validation', async () => {
  const api = browser({ 2: { cancelled: true } });
  const transport = new BrowserTransport(api);
  await transport.cancel(2, 'run-1');
  assert.deepEqual(api.actions, ['cancel', ['tab-remove', 2]]);
});
