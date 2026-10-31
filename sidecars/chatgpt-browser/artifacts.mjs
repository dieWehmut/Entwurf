import { MAX_BYTES, sha256 } from './executor.mjs';
export function allowedImageUrl(value) {
  try {
    const url = new URL(value);
    if (url.username || url.password || url.port) return false;
    if (url.protocol === 'blob:') return url.origin === 'https://chatgpt.com';
    return url.protocol === 'https:' && (url.hostname === 'chatgpt.com' || url.hostname === 'oaiusercontent.com' || url.hostname.endsWith('.oaiusercontent.com'));
  } catch { return false; }
}
function imageMime(bytes) {
  if ([137,80,78,71,13,10,26,10].every((b, i) => bytes[i] === b)) return 'image/png';
  if (bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255) return 'image/jpeg';
  if (new TextDecoder().decode(bytes.slice(0,4)) === 'RIFF' && new TextDecoder().decode(bytes.slice(8,12)) === 'WEBP') return 'image/webp';
  throw new Error('image_format_rejected');
}
export async function captureOriginal(url, { fetch: fetchImage = globalThis.fetch, decode = blob => createImageBitmap(blob) } = {}) {
  if (!allowedImageUrl(url)) throw new Error('image_origin_rejected');
  const response = await fetchImage(url, { credentials: 'include', signal: AbortSignal.timeout(60000), redirect: 'error' });
  if (!response.ok || !response.body) throw new Error('image_download_failed');
  if (Number(response.headers.get('content-length')) > MAX_BYTES) throw new Error('image_too_large');
  const chunks = []; let total = 0;
  const reader = response.body.getReader();
  try {
    while (true) {
      const { done, value } = await reader.read(); if (done) break;
      total += value.byteLength;
      if (total > MAX_BYTES) throw new Error('image_too_large');
      chunks.push(value);
    }
  } catch (error) { await reader.cancel(); throw error; }
  finally { reader.releaseLock(); }
  const bytes = new Uint8Array(total); let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  const mime_type = imageMime(bytes);
  let bitmap;
  try { bitmap = await decode(new Blob([bytes], { type: mime_type })); }
  catch { throw new Error('image_decode_failed'); }
  const { width, height } = bitmap; bitmap.close();
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1 || width * height > 64_000_000) throw new Error('image_dimensions_rejected');
  let binary = '';
  for (let i = 0; i < bytes.length; i += 32768) binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
  return { mime_type, bytes_base64: btoa(binary), sha256: await sha256(bytes) };
}
