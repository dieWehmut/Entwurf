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

test('an unresponsive tab cannot hang discovery or observation', { timeout: 250 }, async () => {
  const api = browser({});
  api.tabs.sendMessage = () => new Promise(() => {});
  const transport = new BrowserTransport(api, { rpcTimeoutMs: 5, probeTimeoutMs: 5 });
  assert.equal((await transport.probe()).reason, 'existing_session_requires_verification');
  await assert.rejects(transport.observe(2, 'run-1'), /content_timeout/);
});

test('a frozen existing tab permits preparation only after the new owned page verifies readiness', { timeout: 250 }, async () => {
  const api = browser({ 2: { available: true, reason: 'composer_detected' } });
  const sendMessage = api.tabs.sendMessage;
  api.tabs.sendMessage = (id, message) => id === 1 ? new Promise(() => {}) : sendMessage(id, message);
  const transport = new BrowserTransport(api, { probeTimeoutMs: 5 });
  assert.deepEqual(await transport.probe(), { available: true, adapter: 'chatgpt-browser', origin: 'https://chatgpt.com', reason: 'existing_session_requires_verification' });
  assert.deepEqual(await transport.prepare({ run_id: 'run-1' }), { target: 2 });
  assert.deepEqual(api.actions, [
    ['window-create', { url: 'https://chatgpt.com/', focused: false }],
    ['update', 2, { autoDiscardable: false }], 'probe', 'prepare'
  ]);
  assert.equal(api.actions.includes('submit'), false);
});

test('a frozen existing tab never permits Send when the new owned page requires login', { timeout: 250 }, async () => {
  const api = browser({ 2: { available: false, reason: 'login_required' } });
  const sendMessage = api.tabs.sendMessage;
  api.tabs.sendMessage = (id, message) => id === 1 ? new Promise(() => {}) : sendMessage(id, message);
  const transport = new BrowserTransport(api, { probeTimeoutMs: 5 });
  await assert.rejects(transport.prepare({ run_id: 'run-1' }), /login_required/);
  assert.equal(api.actions.includes('prepare'), false);
  assert.equal(api.actions.includes('submit'), false);
  assert.deepEqual(api.actions.at(-1), ['window-remove', 9]);
});

test('preparation requires an existing ChatGPT browser session tab', async () => {
  const api = browser({});
  api.tabs.query = async () => [];
  await assert.rejects(new BrowserTransport(api).prepare({ run_id: 'run-1' }), /existing_session_unavailable/);
  assert.deepEqual(api.actions, []);
});
