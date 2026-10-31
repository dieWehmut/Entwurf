import { validateConnection } from './connection.mjs';
const status = document.querySelector('#status');
const labels = { connected: '已连接本机服务', connecting: '正在连接…', disconnected: '本机服务未连接，将自动重连', unpaired: '尚未配对', error: '连接失败，请检查本机服务和配对信息' };
async function refresh() {
  const { connection_status } = await chrome.storage.local.get('connection_status');
  status.textContent = labels[connection_status?.phase] ?? '尚未配对';
}
document.querySelector('#pair').addEventListener('submit', async event => {
  event.preventDefault();
  const input = document.querySelector('#settings');
  try {
    const config = validateConnection(JSON.parse(input.value));
    const reply = await chrome.runtime.sendMessage({ type: 'configure', config });
    if (!reply?.ok) throw new Error('pairing_failed');
    input.value = ''; await refresh();
  } catch { status.textContent = '配对信息无效，请重新复制本机服务提供的 JSON。'; }
});
chrome.storage.onChanged.addListener(changes => { if (changes.connection_status) void refresh(); });
void refresh();
