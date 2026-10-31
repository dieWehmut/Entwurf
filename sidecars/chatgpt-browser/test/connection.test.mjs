import { test } from 'node:test';
import assert from 'node:assert/strict';
import { CompanionConnection, validateConnection } from '../connection.mjs';
const config = { websocket_url: 'ws://127.0.0.1:48123/api/automation/extension', extension_token: 'a'.repeat(64) };
class Socket {
  static OPEN = 1;
  constructor(url) { this.url = url; this.readyState = 1; this.sent = []; Socket.latest = this; }
  send(message) { this.sent.push(JSON.parse(message)); }
  close() { this.closed = true; }
}
test('authentication is the first frame and commands are ignored until authenticated', async () => {
  const commands = [];
  new CompanionConnection(config, { WebSocket: Socket, onCommand: message => commands.push(message) });
  const socket = Socket.latest;
  socket.onopen();
  assert.deepEqual(socket.sent, [{ type: 'authenticate', token: config.extension_token }]);
  assert.equal(socket.url.includes(config.extension_token), false);
  await socket.onmessage({ data: JSON.stringify({ type: 'start', run: {} }) });
  assert.equal(commands.length, 0);
  await socket.onmessage({ data: JSON.stringify({ type: 'authenticated' }) });
  await socket.onmessage({ data: JSON.stringify({ type: 'status', run_id: 'r' }) });
  assert.deepEqual(commands, [{ type: 'status', run_id: 'r' }]);
  socket.close();
});
test('configuration cannot send the pairing credential to a remote server', () => {
  for (const endpoint of ['wss://evil.test/api/automation/extension', 'ws://127.0.0.1:48123/wrong', 'ws://localhost:48123/api/automation/extension?token=secret']) {
    assert.throws(() => validateConnection({ ...config, websocket_url: endpoint }), /invalid_companion_address/);
  }
});
