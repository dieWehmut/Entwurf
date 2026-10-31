import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { ChatGptPage } from '../dom-adapter.mjs';

const prompt = 'Generate a blue circle.';
function fixture() {
  const dom = new JSDOM(`<main><div data-message-author-role="assistant"><img id="old-image"></div>
    <textarea id="prompt-textarea"></textarea><button data-testid="send-button">Send</button></main>`, { url: 'https://chatgpt.com/' });
  const win = dom.window;
  let now = 0;
  const adapter = new ChatGptPage(win, { now: () => now });
  const advance = ms => { now += ms; };
  function response() {
    const user = win.document.createElement('div'); user.dataset.messageAuthorRole = 'user';
    user.textContent = prompt;
    win.document.querySelector('main').append(user);
    const message = win.document.createElement('div'); message.dataset.messageAuthorRole = 'assistant';
    message.innerHTML = '<img id="generated" src="blob:https://chatgpt.com/original"><button aria-label="Download image">Download</button>';
    for (const [key, value] of Object.entries({ complete: true, naturalWidth: 1024, naturalHeight: 1024 })) {
      Object.defineProperty(message.querySelector('img'), key, { value });
    }
    win.document.querySelector('main').append(message);
    return message;
  }
  return { win, adapter, advance, response };
}

test('does not overwrite an existing user draft', () => {
  const { win, adapter } = fixture();
  win.document.querySelector('textarea').value = 'An unsent user draft';
  assert.throws(() => adapter.prepare({ run_id: 'r', prompt }), /composer_not_empty/);
  assert.equal(win.document.querySelector('textarea').value, 'An unsent user draft');
});

test('a user edit after preparation prevents submission', () => {
  const { win, adapter } = fixture();
  adapter.prepare({ run_id: 'r', prompt });
  win.document.querySelector('textarea').value = 'Edited by user';
  let clicks = 0; win.document.querySelector('button').onclick = () => clicks++;
  assert.throws(() => adapter.submit('r'), /composer_changed/);
  assert.equal(clicks, 0);
});

test('an old image or a temporarily absent stop button cannot complete a new run', () => {
  const { adapter, advance } = fixture();
  adapter.prepare({ run_id: 'r', prompt }); adapter.submit('r'); advance(10000);
  assert.equal(adapter.observe('r').phase, 'generating');
});

test('waits for the new response image, generation end and image stability', () => {
  const { win, adapter, advance, response } = fixture();
  adapter.prepare({ run_id: 'r', prompt }); adapter.submit('r');
  const stop = win.document.createElement('button'); stop.dataset.testid = 'stop-button'; win.document.body.append(stop);
  response(); advance(2000);
  assert.equal(adapter.observe('r').phase, 'generating');
  stop.remove();
  assert.equal(adapter.observe('r').phase, 'generating');
  advance(1600);
  assert.equal(adapter.observe('r').phase, 'image_ready');
});

test('navigation after preparation prevents sending into another conversation', () => {
  const { win, adapter } = fixture();
  adapter.prepare({ run_id: 'r', prompt });
  win.history.pushState({}, '', '/c/other');
  assert.throws(() => adapter.submit('r'), /conversation_changed/);
});

test('a successful run permits one submission only', () => {
  const { win, adapter } = fixture();
  let clicks = 0; win.document.querySelector('button').onclick = () => clicks++;
  adapter.prepare({ run_id: 'r', prompt }); adapter.submit('r');
  assert.throws(() => adapter.submit('r'), /already_submitted/);
  assert.equal(clicks, 1);
});

test('an old conversation reached after submit cannot provide the generated response', () => {
  const { win, adapter, response } = fixture();
  adapter.prepare({ run_id: 'r', prompt }); adapter.submit('r');
  win.history.pushState({}, '', '/c/previous-conversation');
  response();
  win.document.querySelector('[data-message-author-role="user"]').textContent = 'An unrelated earlier prompt';
  assert.throws(() => adapter.observe('r'), /submitted_prompt_mismatch/);
});

test('an image cannot complete before the submitted user turn appears', () => {
  const { win, adapter, advance, response } = fixture();
  adapter.prepare({ run_id: 'r', prompt }); adapter.submit('r');
  response();
  win.document.querySelector('[data-message-author-role="user"]').remove();
  assert.equal(adapter.observe('r').phase, 'generating');
  advance(1600);
  assert.equal(adapter.observe('r').phase, 'generating');
});

test('the bound response must follow the exact submitted user turn', () => {
  const { win, adapter, response } = fixture();
  adapter.prepare({ run_id: 'r', prompt }); adapter.submit('r');
  const message = response();
  const user = win.document.querySelector('[data-message-author-role="user"]');
  user.before(message);
  assert.throws(() => adapter.observe('r'), /response_changed/);
});

test('new conversation routing preserves binding and replacing its user turn is rejected', () => {
  const { win, adapter, advance, response } = fixture();
  adapter.prepare({ run_id: 'r', prompt }); adapter.submit('r');
  win.history.pushState({}, '', '/c/new-conversation');
  response();
  assert.equal(adapter.observe('r').phase, 'generating');
  advance(1600);
  assert.equal(adapter.observe('r').phase, 'image_ready');
  const user = win.document.querySelector('[data-message-author-role="user"]');
  user.replaceWith(user.cloneNode(true));
  assert.throws(() => adapter.observe('r'), /conversation_changed/);
});

test('an anonymous composer with a visible login control is unavailable and remains unchanged', () => {
  const { win, adapter } = fixture();
  win.document.body.insertAdjacentHTML('beforeend', '<button data-testid="login-button">Log in</button>');
  assert.deepEqual(adapter.probe(), { available: false, adapter: 'chatgpt-browser', reason: 'login_required' });
  assert.throws(() => adapter.prepare({ run_id: 'r', prompt }), /login_required/);
  assert.equal(win.document.querySelector('textarea').value, '');
});

test('login appearing after discovery prevents prompt preparation', () => {
  const { win, adapter } = fixture();
  assert.equal(adapter.probe().reason, 'composer_detected');
  win.document.body.insertAdjacentHTML('beforeend', '<button aria-label="登录"></button>');
  assert.throws(() => adapter.prepare({ run_id: 'r', prompt }), /login_required/);
  assert.equal(win.document.querySelector('textarea').value, '');
});

test('login expiry before submit blocks the click for known localized auth controls', () => {
  for (const html of [
    '<button data-testid="signup-button">Sign up for free</button>',
    '<button aria-label="Log in"></button>',
    '<a role="button" aria-label="注册"></a>',
    '<button>登录</button>',
    '<button><span>Sign up</span></button>',
    '<button aria-label="註冊"></button>',
  ]) {
    const { win, adapter } = fixture();
    let clicks = 0;
    win.document.querySelector('[data-testid="send-button"]').onclick = () => clicks++;
    adapter.prepare({ run_id: 'r', prompt });
    win.document.body.insertAdjacentHTML('beforeend', html);
    assert.throws(() => adapter.submit('r'), /login_required/, html);
    assert.equal(adapter.run.submitted, false);
    assert.equal(clicks, 0);
  }
});

test('hidden auth controls and unrelated labels do not establish an anonymous state', () => {
  const { win, adapter } = fixture();
  win.document.body.insertAdjacentHTML('beforeend', '<button hidden data-testid="login-button">Log in</button><div style="display:none"><button aria-label="Sign up">Sign up</button></div><div aria-hidden="true"><button>登录</button></div><button>How to log in</button>');
  assert.equal(adapter.probe().reason, 'composer_detected');
  adapter.prepare({ run_id: 'r', prompt });
  assert.deepEqual(adapter.submit('r'), { submitted: true });
});
