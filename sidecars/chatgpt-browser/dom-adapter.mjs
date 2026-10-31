const editorSelector = '#prompt-textarea, textarea[data-id="root"], [contenteditable="true"][role="textbox"]';
const sendSelector = 'button[data-testid="send-button"], button[aria-label="Send prompt"], button[aria-label="Send message"], button[aria-label="发送提示"], button[aria-label="发送"]';
const stopSelector = 'button[data-testid="stop-button"], button[aria-label="Stop streaming"], button[aria-label="Stop generating"], button[aria-label="停止生成"]';
const loginLabels = ['Log in', 'Sign in', 'Sign up', 'Sign up for free', '登录', '注册', '免费注册', '登入', '註冊', '免費註冊'];
const loginSelector = ['[data-testid="login-button"]', '[data-testid="signup-button"]',
  ...loginLabels.flatMap(label => ['button', 'a', '[role="button"]'].map(tag => `${tag}[aria-label="${label}"]`))].join(',');
const loginTextXPath = `//*[self::button or self::a or @role="button"][not(@aria-label) and (${loginLabels.map(label => `normalize-space(.)="${label}"`).join(' or ')})]`;
const normalize = text => text.replace(/\r\n/g, '\n').replace(/\u00a0/g, ' ');
export class ChatGptPage {
  constructor(window, { now = () => Date.now() } = {}) {
    this.window = window; this.document = window.document; this.now = now; this.run = null;
  }
  visible(element) {
    const style = this.window.getComputedStyle(element);
    return !element.hidden && style.display !== 'none' && style.visibility !== 'hidden' && !element.closest('[aria-hidden="true"]');
  }
  one(selector) {
    const elements = [...this.document.querySelectorAll(selector)].filter(e => this.visible(e));
    if (elements.length !== 1) throw new Error(elements.length ? 'controls_ambiguous' : 'controls_unavailable');
    return elements[0];
  }
  assertOrigin() {
    if (this.window.location.origin !== 'https://chatgpt.com') throw new Error('wrong_origin');
  }
  assertNoLoginControls() {
    const controls = [...this.document.querySelectorAll(loginSelector)];
    // Match only fixed auth-control labels, without collecting account or chat text.
    const matches = this.document.evaluate(loginTextXPath, this.document, null, this.window.XPathResult.ORDERED_NODE_SNAPSHOT_TYPE, null);
    for (let i = 0; i < matches.snapshotLength; i++) controls.push(matches.snapshotItem(i));
    const visible = control => {
      for (let element = control; element; element = element.parentElement) if (!this.visible(element)) return false;
      return true;
    };
    if (controls.some(visible)) throw new Error('login_required');
  }
  probe() {
    try {
      this.assertOrigin(); this.assertNoLoginControls(); this.one(editorSelector);
      return { available: true, origin: 'https://chatgpt.com', adapter: 'chatgpt-browser', reason: 'composer_detected' };
    } catch (error) { return { available: false, adapter: 'chatgpt-browser', reason: error.message }; }
  }
  text(editor) { return normalize(editor.tagName === 'TEXTAREA' ? editor.value : editor.innerText ?? editor.textContent ?? ''); }
  prepare({ run_id, prompt }) {
    this.assertOrigin();
    this.assertNoLoginControls();
    if (this.run) throw new Error('run_active');
    const editor = this.one(editorSelector);
    if (this.text(editor).trim()) throw new Error('composer_not_empty');
    const send = this.one(sendSelector);
    const assistantCount = this.document.querySelectorAll('[data-message-author-role="assistant"]').length;
    const userCount = this.document.querySelectorAll('[data-message-author-role="user"]').length;
    editor.focus();
    if (editor.tagName === 'TEXTAREA') {
      Object.getOwnPropertyDescriptor(this.window.HTMLTextAreaElement.prototype, 'value').set.call(editor, prompt);
      editor.dispatchEvent(new this.window.Event('input', { bubbles: true }));
    } else {
      const selection = this.window.getSelection();
      const range = this.document.createRange(); range.selectNodeContents(editor);
      selection.removeAllRanges(); selection.addRange(range);
      if (!this.document.execCommand?.('insertText', false, prompt)) throw new Error('input_unavailable');
      editor.dispatchEvent(new this.window.InputEvent('input', { bubbles: true, inputType: 'insertText', data: prompt }));
    }
    if (this.text(editor) !== normalize(prompt)) throw new Error('input_verification_failed');
    this.run = { run_id, prompt: normalize(prompt), editor, send, assistantCount, userCount,
      path: this.window.location.pathname, submitted: false, response: null, stable: null };
    return { prepared: true };
  }
  requireRun(run_id) {
    this.assertOrigin();
    if (!this.run || this.run.run_id !== run_id) throw new Error('unknown_run');
    return this.run;
  }
  submit(run_id) {
    const run = this.requireRun(run_id);
    this.assertNoLoginControls();
    if (run.submitted) throw new Error('already_submitted');
    if (this.window.location.pathname !== run.path) throw new Error('conversation_changed');
    if (this.one(editorSelector) !== run.editor || this.text(run.editor) !== run.prompt) throw new Error('composer_changed');
    const send = this.one(sendSelector);
    if (send.disabled || send.getAttribute('aria-disabled') === 'true') throw new Error('submit_unavailable');
    run.submitted = true; run.started = this.now();
    send.click();
    return { submitted: true };
  }
  checkConversation(run) {
    const path = this.window.location.pathname;
    if (path !== run.path) {
      if (!(run.path === '/' && /^\/c\/[A-Za-z0-9_-]+$/.test(path) && !run.boundConversation)) throw new Error('conversation_changed');
    }
    const users = this.document.querySelectorAll('[data-message-author-role="user"]');
    if (users.length > run.userCount + 1 || users.length < run.userCount) throw new Error('conversation_changed');
    const user = users[run.userCount];
    if (run.user && run.user !== user) throw new Error('conversation_changed');
    if (!user) return false;
    // Inspect only this new turn, never prior conversation text. A route change
    // alone cannot bind an existing conversation's image to this submission.
    if (normalize(user.innerText ?? user.textContent ?? '') !== run.prompt) throw new Error('submitted_prompt_mismatch');
    run.user = user;
    if (path !== run.path) { run.path = path; run.boundConversation = true; }
    return true;
  }
  downloadButton(response) {
    const buttons = [...response.querySelectorAll('button,a[download]')].filter(button => {
      const label = button.getAttribute('aria-label') || button.getAttribute('title') || button.textContent || '';
      return this.visible(button) && (/^(Download(?: image)?|下载(?:图片)?|保存图片)$/i.test(label.trim()) || button.hasAttribute('download'));
    });
    if (buttons.length > 1) throw new Error('images_ambiguous');
    return buttons[0];
  }
  observe(run_id) {
    const run = this.requireRun(run_id);
    if (!run.submitted) throw new Error('not_submitted');
    if (this.now() - run.started > 10 * 60 * 1000) throw new Error('generation_timeout');
    if (!this.checkConversation(run)) return { phase: 'generating' };
    const responses = [...this.document.querySelectorAll('[data-message-author-role="assistant"]')];
    if (!run.response) run.response = responses[run.assistantCount] ?? null;
    if (!run.response) return { phase: 'generating' };
    if (!run.response.isConnected || !(run.user.compareDocumentPosition(run.response) & this.window.Node.DOCUMENT_POSITION_FOLLOWING)) throw new Error('response_changed');
    if ([...this.document.querySelectorAll(stopSelector)].some(e => this.visible(e)) || run.response.querySelector('[aria-busy="true"]')) {
      run.stable = null; return { phase: 'generating' };
    }
    const images = [...run.response.querySelectorAll('img')].filter(img => this.visible(img) && img.complete && img.naturalWidth >= 128 && img.naturalHeight >= 128);
    if (images.length > 1) throw new Error('images_ambiguous');
    const image = images[0], button = this.downloadButton(run.response);
    if (!image || !button || button.disabled) { run.stable = null; return { phase: 'generating' }; }
    if (image.naturalWidth * image.naturalHeight > 64_000_000) throw new Error('image_too_large');
    const fingerprint = `${image.currentSrc || image.src}|${image.naturalWidth}|${image.naturalHeight}`;
    if (run.stable?.fingerprint !== fingerprint) run.stable = { fingerprint, since: this.now() };
    if (this.now() - run.stable.since < 1500) return { phase: 'generating' };
    return { phase: 'image_ready', conversation_url: this.window.location.href,
      download_url: button.tagName === 'A' && button.hasAttribute('download') ? button.href : null };
  }
  download(run_id) {
    if (this.observe(run_id).phase !== 'image_ready') throw new Error('image_not_ready');
    const run = this.requireRun(run_id);
    if (run.downloadClicked) throw new Error('already_downloading');
    const button = this.downloadButton(run.response);
    run.downloadClicked = true;
    button.click();
    return { downloading: true };
  }
  cancel(run_id) {
    this.requireRun(run_id);
    this.run = null;
    return { cancelled: true, external_generation_stopped: false };
  }
}
