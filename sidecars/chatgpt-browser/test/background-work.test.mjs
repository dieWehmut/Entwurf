import { test } from 'node:test';
import assert from 'node:assert/strict';
import { BackgroundWork } from '../background-work.mjs';

function lockManager() {
  const held = new Set();
  return { held, async request(name, options, action) {
    assert.deepEqual(options, { mode: 'exclusive', ifAvailable: true });
    held.add(name);
    try { return await action({ name }); }
    finally { held.delete(name); }
  } };
}

test('explicit background work holds a scoped lock until the matching run releases it', async () => {
  const locks = lockManager();
  const work = new BackgroundWork(locks);
  await work.acquire('run-1');
  assert.deepEqual([...locks.held], ['entwurf-chatgpt:run-1']);
  assert.equal(work.release('another-run'), false);
  assert.equal(locks.held.size, 1);
  await assert.rejects(work.acquire('run-2'), /background_work_active/);
  assert.equal(work.release('run-1'), true);
  await Promise.resolve();
  assert.equal(locks.held.size, 0);
  await work.acquire('run-2');
  work.release('run-2');
});

test('background work refuses unavailable or contended locks before preparation', async () => {
  await assert.rejects(new BackgroundWork(null).acquire('run-1'), /background_work_unavailable/);
  await assert.rejects(new BackgroundWork({ request: async (_, __, action) => action(null) }).acquire('run-1'), /background_work_unavailable/);
});

test('two preparations cannot acquire separate locks while the first request is pending', async () => {
  let grant;
  const granted = new Promise(resolve => { grant = resolve; });
  const work = new BackgroundWork({ request: async (_, __, action) => { await granted; return action({}); } });
  const first = work.acquire('run-1');
  await assert.rejects(work.acquire('run-2'), /background_work_active/);
  grant();
  await first;
  work.release('run-1');
});

test('abandoned background work releases its lease without another submission', async () => {
  const locks = lockManager();
  const work = new BackgroundWork(locks, { leaseMs: 5 });
  await work.acquire('run-1');
  await new Promise(resolve => setTimeout(resolve, 15));
  assert.equal(work.held, null);
  assert.equal(locks.held.size, 0);
});
