import { test } from 'node:test';
import assert from 'node:assert/strict';
import { captureOriginal, allowedImageUrl } from '../artifacts.mjs';
const png = Uint8Array.from([137,80,78,71,13,10,26,10,1,2,3]);

test('original bytes are preserved while misleading MIME types are ignored', async () => {
  const captured = await captureOriginal('blob:https://chatgpt.com/test', {
    fetch: async () => new Response(png, { headers: { 'content-type': 'text/plain' } }),
    decode: async () => ({ width: 1024, height: 1024, close() {} })
  });
  assert.equal(captured.mime_type, 'image/png');
  assert.deepEqual(Buffer.from(captured.bytes_base64, 'base64'), Buffer.from(png));
  assert.equal(captured.sha256, '7f47b756761a46e6d4a4d96f0d8a4448f8449235009d1f3ad1493f5c773c19e8');
});

test('does not fetch arbitrary model-supplied or cross-origin URLs', async () => {
  for (const url of ['file:///etc/passwd', 'https://evil.test/image.png', 'https://chatgpt.com.evil.test/a', 'blob:https://evil.test/a']) {
    assert.equal(allowedImageUrl(url), false);
    await assert.rejects(captureOriginal(url), /image_origin_rejected/);
  }
});

test('refuses decompression-sized images and nonimage response bytes', async () => {
  await assert.rejects(captureOriginal('https://chatgpt.com/rendered-image', {
    fetch: async () => new Response(png), decode: async () => ({ width: 10000, height: 10000, close() {} })
  }), /image_dimensions_rejected/);
  await assert.rejects(captureOriginal('https://chatgpt.com/rendered-image', {
    fetch: async () => new Response('<html>Login</html>')
  }), /image_format_rejected/);
});
