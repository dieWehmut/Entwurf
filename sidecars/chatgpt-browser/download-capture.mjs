import { allowedImageUrl } from './artifacts.mjs';
export async function captureDownload(downloads, conversationUrl, click, read, { settleMs = 750, timeoutMs = 60000, expectedUrl = null } = {}) {
  const conversation = new URL(conversationUrl);
  if (conversation.origin !== 'https://chatgpt.com') throw new Error('wrong_origin');
  let settle, deadline, listener;
  const pending = new Map();
  try {
    return await new Promise((resolve, reject) => {
      deadline = setTimeout(() => reject(new Error('download_timeout')), timeoutMs);
      listener = item => {
        let referrer;
        try { referrer = new URL(item.referrer); } catch { /* Blob downloads may omit referrer. */ }
        const exactLink = expectedUrl && item.url === expectedUrl;
        const sameConversation = referrer?.origin === conversation.origin && referrer?.pathname === conversation.pathname;
        if ((!exactLink && !sameConversation) || pending.has(item.id)) return;
        const url = item.finalUrl || item.url;
        if (!allowedImageUrl(url)) { reject(new Error('image_origin_rejected')); return; }
        // Start consuming a short-lived browser blob immediately. No local download paths are read.
        const bytes = Promise.resolve().then(() => read(url));
        const outcome = bytes.then(value => ({ value }), error => ({ error }));
        pending.set(item.id, outcome);
        if (pending.size > 1) { reject(new Error('download_ambiguous')); return; }
        settle = setTimeout(async () => {
          const result = await outcome;
          if (result.error) reject(result.error); else resolve(result.value);
        }, settleMs);
      };
      downloads.onCreated.addListener(listener);
      Promise.resolve().then(click).catch(reject);
    });
  } finally {
    clearTimeout(settle); clearTimeout(deadline);
    if (listener) downloads.onCreated.removeListener(listener);
  }
}
