// An isolated content world: no cookies, auth tokens, account text or conversation history is exported.
(async () => {
  const { ChatGptPage } = await import(chrome.runtime.getURL('dom-adapter.mjs'));
  const { captureOriginal } = await import(chrome.runtime.getURL('artifacts.mjs'));
  const page = new ChatGptPage(window);
  chrome.runtime.onMessage.addListener((message, sender, reply) => {
    if (sender.id !== chrome.runtime.id || message?.scope !== 'entwurf-chatgpt') return false;
    const operation = async () => {
      switch (message.type) {
        case 'probe': return page.probe();
        case 'prepare': return page.prepare(message.run);
        case 'submit': return page.submit(message.run_id);
        case 'observe': return page.observe(message.run_id);
        case 'download': return page.download(message.run_id);
        case 'cancel': return page.cancel(message.run_id);
        case 'capture':
          page.requireRun(message.run_id);
          if (!page.run.downloadClicked) throw new Error('download_not_started');
          return captureOriginal(message.url);
        default: throw new Error('unknown_operation');
      }
    };
    operation().then(result => reply({ ok: true, result }), error => reply({ ok: false,
      error: /^[a-z][a-z0-9_]{1,60}$/.test(error.message) ? error.message : 'adapter_failed' }));
    return true;
  });
})();
