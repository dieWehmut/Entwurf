// A run-scoped Web Lock tells Chromium this page has outstanding background
// work. Chromium's freezing policy protects pages holding Web Locks even when
// Windows marks all browser windows occluded at the lock screen. This lock does
// not take focus, bypass sleep, or share any ChatGPT authentication material.
export class BackgroundWork {
  constructor(locks, { leaseMs = 15 * 60 * 1000 } = {}) {
    this.locks = locks;
    this.leaseMs = leaseMs;
    this.held = null;
    this.acquiring = false;
  }
  async acquire(run_id) {
    if (this.held || this.acquiring) throw new Error('background_work_active');
    if (!this.locks?.request) throw new Error('background_work_unavailable');
    if (typeof run_id !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(run_id)) throw new Error('invalid_run');
    let release;
    const finished = new Promise(resolve => { release = resolve; });
    this.acquiring = true;
    try {
      await new Promise((resolve, reject) => {
        this.locks.request(`entwurf-chatgpt:${run_id}`, { mode: 'exclusive', ifAvailable: true }, lock => {
          if (!lock) throw new Error('background_work_unavailable');
          const timer = setTimeout(() => this.release(run_id), this.leaseMs);
          this.held = { run_id, release, timer };
          resolve();
          return finished;
        }).catch(() => reject(new Error('background_work_unavailable')));
      });
    } finally { this.acquiring = false; }
  }
  release(run_id) {
    if (this.held?.run_id !== run_id) return false;
    const held = this.held;
    this.held = null;
    clearTimeout(held.timer);
    held.release();
    return true;
  }
}
