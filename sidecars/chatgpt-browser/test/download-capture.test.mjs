import { test } from 'node:test';
import assert from 'node:assert/strict';
import { captureDownload } from '../download-capture.mjs';
function events() {
  const listeners = new Set();
  return { addListener: f => listeners.add(f), removeListener: f => listeners.delete(f),
    fire: event => listeners.forEach(f => f(event)), count: () => listeners.size };
}
test('only the task conversation download is captured and listeners are removed', async () => {
  const event = events(), read = [];
  const result = await captureDownload({ onCreated: event }, 'https://chatgpt.com/c/task', async () => {
    event.fire({ id: 1, referrer: 'https://elsewhere.test/', url: 'https://chatgpt.com/unrelated.png' });
    event.fire({ id: 2, referrer: 'https://chatgpt.com/c/task', url: 'blob:https://chatgpt.com/original' });
  }, async url => { read.push(url); return 'original-bytes'; }, { settleMs: 5, timeoutMs: 50 });
  assert.equal(result, 'original-bytes');
  assert.deepEqual(read, ['blob:https://chatgpt.com/original']);
  assert.equal(event.count(), 0);
});
test('two matching downloads cannot silently assign the wrong image', async () => {
  const event = events();
  await assert.rejects(captureDownload({ onCreated: event }, 'https://chatgpt.com/c/task', async () => {
    event.fire({ id: 1, referrer: 'https://chatgpt.com/c/task', url: 'blob:https://chatgpt.com/a' });
    event.fire({ id: 2, referrer: 'https://chatgpt.com/c/task', url: 'blob:https://chatgpt.com/b' });
  }, async () => 'bytes', { settleMs: 5, timeoutMs: 50 }), /download_ambiguous/);
  assert.equal(event.count(), 0);
});
test('a blob download with no referrer must match the exact scoped download anchor URL', async () => {
  const event = events(), read = [];
  const result = await captureDownload({ onCreated: event }, 'https://chatgpt.com/c/task', async () => {
    event.fire({ id: 1, referrer: '', url: 'blob:https://chatgpt.com/unrelated' });
    event.fire({ id: 2, referrer: '', url: 'blob:https://chatgpt.com/exact-download' });
  }, async url => { read.push(url); return 'original'; }, { settleMs: 5, timeoutMs: 50, expectedUrl: 'blob:https://chatgpt.com/exact-download' });
  assert.equal(result, 'original');
  assert.deepEqual(read, ['blob:https://chatgpt.com/exact-download']);
});
