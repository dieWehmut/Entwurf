import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { RunExecutor } from '../executor.mjs';

const prompt = 'Generate a blue circle.';
const request = { run_id: 'run-1', task_id: 'task-1', base_version_id: 'version-1', prompt,
  prompt_sha256: createHash('sha256').update(prompt).digest('hex') };
function harness(records = new Map(), failure = null, options = {}) {
  const events = [], actions = [];
  const transport = {
    async prepare(run) { actions.push(['prepare', run.run_id]); return { target: 7 }; },
    async submit(target, run) {
      assert.equal(records.get(run.run_id).phase, 'submit_intent');
      actions.push(['submit', target]);
      if (failure) throw new Error(failure);
    },
    async observe() { return { phase: 'generating' }; },
    async cancel() { actions.push(['cancel']); },
    async release(target, run_id) { actions.push(['release', target, run_id]); }
  };
  const store = { async get(id) { return structuredClone(records.get(id)); },
    async put(record) { records.set(record.run_id, structuredClone(record)); } };
  return { executor: new RunExecutor({ store, transport, emit: e => events.push(e), ...options }), records, events, actions };
}

test('submission intent survives a lost response and prevents replay after executor restart', async () => {
  const h = harness(new Map(), 'connection_lost');
  await h.executor.start(request);
  assert.equal(h.records.get('run-1').phase, 'submission_uncertain');
  const reopened = harness(h.records);
  await reopened.executor.start(request);
  assert.equal(reopened.actions.length, 0);
  assert.equal(reopened.events.at(-1).phase, 'paused');
  assert.equal(reopened.events.at(-1).detail, 'submission_uncertain');
});

test('a reused run id cannot submit a changed prompt or task', async () => {
  const h = harness();
  await h.executor.start(request);
  await assert.rejects(h.executor.start({ ...request, task_id: 'another-task' }), /run_conflict/);
  assert.equal(h.actions.filter(a => a[0] === 'submit').length, 1);
});

test('prompt hash mismatch is rejected before changing the browser', async () => {
  const h = harness();
  await assert.rejects(h.executor.start({ ...request, prompt: 'Different prompt' }), /prompt_hash_mismatch/);
  assert.equal(h.actions.length, 0);
});

test('concurrent runs cannot share the active browser target', async () => {
  const h = harness();
  await h.executor.start(request);
  await assert.rejects(h.executor.start({ ...request, run_id: 'run-2' }), /run_active/);
});

test('artifact is emitted once and completion requires database acknowledgement', async () => {
  const h = harness();
  h.executor.transport.observe = async () => ({ phase: 'image_ready' });
  h.executor.transport.capture = async () => ({ mime_type: 'image/png', bytes_base64: 'aW1hZ2U=',
    sha256: createHash('sha256').update('image').digest('hex') });
  await h.executor.start(request);
  await h.executor.tick('run-1');
  await h.executor.tick('run-1');
  assert.equal(h.records.get('run-1').phase, 'awaiting_import');
  assert.equal(h.events.filter(e => e.type === 'artifact').length, 1);
  await h.executor.accept('run-1');
  assert.equal(h.records.get('run-1').phase, 'completed');
  assert.deepEqual(h.actions.at(-1), ['release', 7, 'run-1']);
});

test('cancellation releases the run without claiming ChatGPT stopped generating', async () => {
  const h = harness(); await h.executor.start(request);
  await h.executor.cancel('run-1');
  assert.equal(h.records.get('run-1').phase, 'cancelled');
  assert.equal(h.events.at(-1).external_generation_stopped, false);
  await h.executor.start({ ...request, run_id: 'run-2' });
  assert.equal(h.records.get('run-2').phase, 'submitted');
});

test('cancellation during durable intent persistence prevents the submit click', async () => {
  const h = harness();
  let intentReached, finishWrite;
  const reached = new Promise(resolve => { intentReached = resolve; });
  const pendingWrite = new Promise(resolve => { finishWrite = resolve; });
  const put = h.executor.store.put;
  h.executor.store.put = async record => {
    await put(record);
    if (record.phase === 'submit_intent') { intentReached(); await pendingWrite; }
  };
  const start = h.executor.start(request);
  await reached;
  const cancel = h.executor.cancel(request.run_id);
  finishWrite();
  await Promise.all([start, cancel]);
  assert.equal(h.actions.filter(action => action[0] === 'submit').length, 0);
  assert.equal(h.records.get(request.run_id).phase, 'cancelled');
});

test('duplicate acknowledgement for a completed run preserves the next active run', async () => {
  const h = harness();
  h.executor.transport.observe = async () => ({ phase: 'image_ready' });
  h.executor.transport.capture = async () => ({ mime_type: 'image/png', bytes_base64: 'aW1hZ2U=',
    sha256: createHash('sha256').update('image').digest('hex') });
  await h.executor.start(request);
  await h.executor.tick(request.run_id);
  await h.executor.accept(request.run_id);
  await h.executor.start({ ...request, run_id: 'run-2' });
  await h.executor.accept(request.run_id);
  assert.equal(h.executor.active, 'run-2');
  await h.executor.tick(h.executor.active);
  assert.equal(h.records.get('run-2').phase, 'awaiting_import');
});

test('repeated cancellation of an old run preserves the next active run', async () => {
  const h = harness();
  await h.executor.start(request);
  await h.executor.cancel(request.run_id);
  await h.executor.start({ ...request, run_id: 'run-2' });
  await h.executor.cancel(request.run_id);
  assert.equal(h.executor.active, 'run-2');
  assert.equal(h.records.get('run-2').phase, 'submitted');
});

test('a slow background response keeps observing the submitted run without sending again', async () => {
  const h = harness();
  await h.executor.start(request);
  h.executor.transport.observe = async () => { throw new Error('content_timeout'); };
  await h.executor.tick(request.run_id);
  assert.equal(h.records.get(request.run_id).phase, 'generating');
  assert.equal(h.events.at(-1).detail, 'content_timeout');
  h.executor.transport.observe = async () => ({ phase: 'image_ready' });
  h.executor.transport.capture = async () => ({ mime_type: 'image/png', bytes_base64: 'aW1hZ2U=',
    sha256: createHash('sha256').update('image').digest('hex') });
  await h.executor.tick(request.run_id);
  assert.equal(h.records.get(request.run_id).phase, 'awaiting_import');
  assert.equal(h.actions.filter(action => action[0] === 'submit').length, 1);
  await h.executor.cancel(request.run_id);
  assert.equal(h.executor.active, null);
});

test('background response retries stop at the durable generation deadline without resubmission', async () => {
  let now = 1000;
  const h = harness(new Map(), null, { now: () => now });
  await h.executor.start(request);
  assert.equal(h.records.get(request.run_id).submitted_at, 1000);
  let observations = 0;
  h.executor.transport.observe = async () => { observations++; throw new Error('content_timeout'); };
  now += 9 * 60 * 1000;
  await h.executor.tick(request.run_id);
  assert.equal(h.records.get(request.run_id).phase, 'generating');
  now += 60 * 1000;
  await h.executor.tick(request.run_id);
  assert.equal(h.records.get(request.run_id).phase, 'paused');
  assert.equal(h.records.get(request.run_id).detail, 'generation_timeout');
  assert.equal(observations, 1);
  await h.executor.start(request);
  await h.executor.tick(request.run_id);
  assert.equal(observations, 1);
  assert.equal(h.actions.filter(action => action[0] === 'submit').length, 1);
});

test('an observation timeout crossing the deadline pauses during that tick', async () => {
  let now = 1000;
  const h = harness(new Map(), null, { now: () => now });
  await h.executor.start(request);
  h.executor.transport.observe = async () => { now += 10 * 60 * 1000; throw new Error('content_timeout'); };
  await h.executor.tick(request.run_id);
  assert.equal(h.records.get(request.run_id).phase, 'paused');
  assert.equal(h.records.get(request.run_id).detail, 'generation_timeout');
});

test('executor restart preserves the submission deadline instead of granting another ten minutes', async () => {
  let now = 1000;
  const first = harness(new Map(), null, { now: () => now });
  await first.executor.start(request);
  now += 9 * 60 * 1000;
  const reopened = harness(first.records, null, { now: () => now });
  await reopened.executor.start(request);
  now += 60 * 1000;
  await reopened.executor.tick(request.run_id);
  assert.equal(reopened.records.get(request.run_id).phase, 'paused');
  assert.equal(reopened.records.get(request.run_id).detail, 'generation_timeout');
  assert.equal(reopened.actions.length, 0);
  assert.equal(reopened.records.get(request.run_id).submitted_at, 1000);
});

test('legacy submitted records receive one persisted deadline shared by later restarts', async () => {
  let now = 1000;
  const records = new Map([[request.run_id, { ...request, prompt: undefined, phase: 'submitted', target: 7 }]]);
  const first = harness(records, null, { now: () => now });
  first.executor.transport.observe = async () => { throw new Error('content_timeout'); };
  await first.executor.tick(request.run_id);
  assert.equal(records.get(request.run_id).generation_deadline_at, 1000 + 10 * 60 * 1000);
  now += 9 * 60 * 1000;
  const reopened = harness(records, null, { now: () => now });
  await reopened.executor.start(request);
  now += 60 * 1000;
  await reopened.executor.tick(request.run_id);
  assert.equal(records.get(request.run_id).detail, 'generation_timeout');
  assert.equal(records.get(request.run_id).generation_deadline_at, 1000 + 10 * 60 * 1000);
  assert.equal(reopened.actions.length, 0);
});
