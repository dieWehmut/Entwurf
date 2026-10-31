const identity = ['run_id', 'task_id', 'base_version_id', 'prompt_sha256'];
const terminal = new Set(['completed', 'cancelled', 'failed']);
export const MAX_BYTES = 32 * 1024 * 1024;
export async function sha256(bytes) {
  return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), b => b.toString(16).padStart(2, '0')).join('');
}
export function safeCode(error) {
  const code = error?.message;
  return typeof code === 'string' && /^[a-z][a-z0-9_]{1,60}$/.test(code) ? code : 'adapter_failed';
}
export class RunExecutor {
  constructor({ store, transport, emit, artifactStore }) {
    Object.assign(this, { store, transport, emit, artifactStore });
    this.queue = Promise.resolve();
    this.active = null;
    this.cancelled = new Set();
  }
  exclusive(action) {
    const next = this.queue.then(action);
    this.queue = next.catch(() => {});
    return next;
  }
  async report(record) {
    await this.store.put(record);
    this.emitStatus(record);
    return record;
  }
  emitStatus(record) {
    const aliases = { submit_intent: 'submitting', submitted: 'generating', submission_uncertain: 'paused' };
    this.emit({ type: 'status', run_id: record.run_id, phase: aliases[record.phase] ?? record.phase,
      ...(record.phase === 'submission_uncertain' ? { detail: 'submission_uncertain' } : record.detail ? { detail: record.detail } : {}) });
  }
  tick(run_id) { return this.exclusive(async () => {
    const record = await this.store.get(run_id);
    if (!record) throw new Error('unknown_run');
    if (!['submitted', 'generating'].includes(record.phase)) return this.report(record);
    try {
      const observation = await this.transport.observe(record.target, run_id);
      if (this.cancelled.has(run_id)) return record;
      if (observation.phase === 'image_ready') {
        record.phase = 'downloading'; await this.report(record);
        const artifact = await this.transport.capture(record.target, run_id, observation);
        if (this.cancelled.has(run_id)) return record;
        if (!['image/png', 'image/jpeg', 'image/webp'].includes(artifact.mime_type) || typeof artifact.bytes_base64 !== 'string' || artifact.bytes_base64.length > Math.ceil(MAX_BYTES / 3) * 4) throw new Error('invalid_artifact');
        const bytes = Uint8Array.from(atob(artifact.bytes_base64), c => c.charCodeAt(0));
        if (!bytes.length || bytes.length > MAX_BYTES || await sha256(bytes) !== artifact.sha256) throw new Error('invalid_artifact');
        const message = { type: 'artifact', run_id, task_id: record.task_id, base_version_id: record.base_version_id, ...artifact };
        await this.artifactStore?.put(message);
        record.phase = 'awaiting_import'; await this.report(record);
        this.emit(message);
      } else { record.phase = 'generating'; await this.report(record); }
    } catch (error) {
      record.phase = 'paused'; record.detail = safeCode(error); await this.report(record);
    }
    return record;
  }); }
  cancel(run_id) {
    this.cancelled.add(run_id);
    return this.exclusive(async () => {
      const record = await this.store.get(run_id);
      if (!record) throw new Error('unknown_run');
      if (record.phase === 'completed') return this.report(record);
      try { await this.transport.cancel(record.target, run_id); } catch { /* The tab may already be closed. */ }
      record.phase = 'cancelled'; delete record.detail;
      if (this.active === run_id) this.active = null;
      await this.report(record);
      this.emit({ type: 'status', run_id, phase: 'cancelled', external_generation_stopped: false });
      return record;
    });
  }
  accept(run_id) { return this.exclusive(async () => {
    const record = await this.store.get(run_id);
    if (!record || !['awaiting_import', 'completed'].includes(record.phase)) throw new Error('invalid_acknowledgement');
    record.phase = 'completed';
    if (this.active === run_id) this.active = null;
    await this.report(record);
    await this.artifactStore?.remove(run_id);
    return record;
  }); }
  async status(run_id) {
    const record = await this.store.get(run_id);
    if (!record) throw new Error('unknown_run');
    this.emitStatus(record);
    if (record.phase === 'awaiting_import') {
      const artifact = await this.artifactStore?.get(run_id);
      if (artifact) this.emit(artifact);
    }
    return record;
  }
  start(run) { return this.exclusive(() => this.startOnce(run)); }
  async startOnce(run) {
    for (const key of identity.slice(0, 3)) {
      if (typeof run?.[key] !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(run[key])) throw new Error('invalid_run');
    }
    if (typeof run.prompt !== 'string' || !run.prompt.trim() || new TextEncoder().encode(run.prompt).length > 256 * 1024) throw new Error('invalid_prompt');
    if (await sha256(new TextEncoder().encode(run.prompt)) !== run.prompt_sha256) throw new Error('prompt_hash_mismatch');
    const old = await this.store.get(run.run_id);
    if (old) {
      if (identity.some(key => old[key] !== run[key])) throw new Error('run_conflict');
      // A process can die between recording intent and observing the click. Never replay it.
      if (['preparing', 'submit_intent'].includes(old.phase)) {
        old.phase = old.phase === 'submit_intent' ? 'submission_uncertain' : 'failed';
        old.detail = 'executor_restarted';
      }
      if (!terminal.has(old.phase)) this.active = old.run_id;
      return this.report(old);
    }
    const persistedActive = (await this.store.list?.() ?? []).find(r => !terminal.has(r.phase));
    if (this.active || persistedActive) throw new Error('run_active');
    this.active = run.run_id;
    const record = Object.fromEntries(identity.map(key => [key, run[key]]));
    record.phase = 'preparing';
    await this.report(record);
    try {
      const prepared = await this.transport.prepare(run);
      record.target = prepared.target;
      if (this.cancelled.has(run.run_id)) throw new Error('cancelled');
      record.phase = 'submit_intent';
      await this.report(record);
      if (this.cancelled.has(run.run_id)) throw new Error('cancelled');
      await this.transport.submit(record.target, run);
      record.phase = 'submitted';
      delete record.detail;
      if (this.cancelled.has(run.run_id)) record.phase = 'cancelled';
    } catch (error) {
      record.detail = safeCode(error);
      record.phase = this.cancelled.has(run.run_id) ? 'cancelled' : record.phase === 'submit_intent' ? 'submission_uncertain' : 'failed';
    }
    if (terminal.has(record.phase)) this.active = null;
    return this.report(record);
  }
}
