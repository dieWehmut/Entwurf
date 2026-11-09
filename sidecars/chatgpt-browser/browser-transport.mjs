import { captureDownload } from './download-capture.mjs';
import { captureOriginal } from './artifacts.mjs';
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
export class BrowserTransport {
  constructor(browser) { this.browser = browser; }
  async call(tabId, type, args = {}) {
    const reply = await this.browser.tabs.sendMessage(tabId, { scope: 'entwurf-chatgpt', type, ...args });
    if (!reply?.ok) throw new Error(reply?.error || 'content_unavailable');
    return reply.result;
  }
  async probe() {
    const tabs = await this.browser.tabs.query({ url: 'https://chatgpt.com/*' });
    let loginRequired = false;
    for (const tab of tabs) {
      try {
        const report = await this.call(tab.id, 'probe');
        if (report.available) return report;
        if (report.reason === 'login_required') loginRequired = true;
      } catch { /* An existing tab may predate extension installation. */ }
    }
    return { available: false, adapter: 'chatgpt-browser', reason: loginRequired ? 'login_required' : tabs.length ? 'reload_chatgpt_tab' : 'open_existing_chatgpt_session' };
  }
  async prepare(run) {
    const report = await this.probe();
    if (!report.available) throw new Error(report.reason === 'login_required' ? 'login_required' : 'existing_session_unavailable');
    // The task tab is active inside its own window, so Chrome does not freeze it
    // as an inactive tab. The window itself is never focused and shares the
    // already-authenticated browser profile without disturbing the user's tabs.
    const taskWindow = await this.browser.windows.create({ url: 'https://chatgpt.com/', focused: false });
    const windowId = taskWindow?.id;
    if (!Number.isInteger(windowId)) throw new Error('background_window_unavailable');
    let retained = false;
    try {
      const tabs = await this.browser.tabs.query({ windowId });
      const active = tabs.filter(tab => tab.active && Number.isInteger(tab.id));
      if (active.length !== 1) throw new Error('background_window_unavailable');
      const tab = active[0];
      await this.browser.tabs.update(tab.id, { autoDiscardable: false });
      for (let attempt = 0; attempt < 100; attempt++) {
        const current = await this.browser.tabs.get(tab.id);
        if (current.url && new URL(current.url).origin !== 'https://chatgpt.com') throw new Error('login_required');
        let ready;
        try { ready = await this.call(tab.id, 'probe'); } catch { /* Only readiness checks retry. */ }
        if (ready?.reason === 'login_required') throw new Error('login_required');
        if (ready?.available) {
          await this.call(tab.id, 'prepare', { run });
          retained = true;
          return { target: tab.id };
        }
        await delay(200);
      }
      throw new Error('composer_timeout');
    } finally {
      if (!retained) try { await this.browser.windows.remove(windowId); } catch { /* The failed task window may already be gone. */ }
    }
  }
  submit(tabId, run) { return this.call(tabId, 'submit', { run_id: run.run_id }); }
  observe(tabId, run_id) { return this.call(tabId, 'observe', { run_id }); }
  async closeOwned(tabId, run_id) {
    // The content adapter validates the bound run before we remove anything.
    // Therefore a stale/reused tab id can never close an unrelated user tab.
    const result = await this.call(tabId, 'cancel', { run_id });
    await this.browser.tabs.remove(tabId);
    return result;
  }
  cancel(tabId, run_id) { return this.closeOwned(tabId, run_id); }
  release(tabId, run_id) { return this.closeOwned(tabId, run_id); }
  capture(tabId, run_id, observation) {
    return captureDownload(this.browser.downloads, observation.conversation_url,
      () => this.call(tabId, 'download', { run_id }),
      url => url.startsWith('blob:') || new URL(url).origin === 'https://chatgpt.com'
        ? this.call(tabId, 'capture', { run_id, url }) : captureOriginal(url),
      { expectedUrl: observation.download_url });
  }
}
