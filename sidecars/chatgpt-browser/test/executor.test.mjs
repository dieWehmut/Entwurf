import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { RunExecutor } from '../executor.mjs';

const prompt = 'Generate a blue circle.';
const request = { run_id: 'run-1', task_id: 'task-1', base_version_id: 'version-1', prompt,
  prompt_sha256: createHash('sha256').update(prompt).digest('hex') };
function harness(records = new Map(), failure = null) {
  const events = [], actions = [];
  const transport = {
    async prepare(run) { actions.push(['prepare', run.run_id]); return { target: 7 }; },
    async submit(target, run) {
      assert.equal(records.get(run.run_id).phase, 'submit_intent');
      actions.push(['submit', target]);
      if (failure) throw new Error(failure);
    },
    async observe() { return { phase: 'generating' }; },
    async cancel() { actions.push(['cancel']); }
  };
  const store = { async get(id) { return structuredClone(records.get(id)); },
    async put(record) { records.set(record.run_id, structuredClone(record)); } };
  return { executor: new RunExecutor({ store, transport, emit: e => events.push(e) }), records, events, actions };
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
});

test('cancellation releases the run without claiming ChatGPT stopped generating', async () => {
  const h = harness(); await h.executor.start(request);
  await h.executor.cancel('run-1');
  assert.equal(h.records.get('run-1').phase, 'cancelled');
  assert.equal(h.events.at(-1).external_generation_stopped, false);
  await h.executor.start({ ...request, run_id: 'run-2' });
  assert.equal(h.records.get('run-2').phase, 'submitted');
});
