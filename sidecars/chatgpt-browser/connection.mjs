import { safeCode } from './executor.mjs';
export function validateConnection(config) {
  let url;
  try { url = new URL(config?.websocket_url); } catch { throw new Error('invalid_companion_address'); }
  if (url.protocol !== 'ws:' || !['127.0.0.1', 'localhost'].includes(url.hostname) || !url.port || url.pathname !== '/api/automation/extension' || url.search || url.hash || url.username || url.password) throw new Error('invalid_companion_address');
  if (typeof config.extension_token !== 'string' || !/^[A-Za-z0-9_-]{32,256}$/.test(config.extension_token)) throw new Error('invalid_pairing_token');
  return { websocket_url: url.href, extension_token: config.extension_token };
}
export class CompanionConnection {
  constructor(config, { WebSocket: Socket = globalThis.WebSocket, onCommand, onAuthenticated = () => {}, onDisconnected = () => {}, onError = () => {} }) {
    const settings = validateConnection(config);
    this.authenticated = false;
    this.socket = new Socket(settings.websocket_url);
    const deadline = setTimeout(() => this.socket.close(), 10000);
    this.socket.onopen = () => this.socket.send(JSON.stringify({ type: 'authenticate', token: settings.extension_token }));
    this.socket.onmessage = async event => {
      try {
        if (typeof event.data !== 'string' || event.data.length > 1024 * 1024) throw new Error('invalid_companion_message');
        const message = JSON.parse(event.data);
        if (!this.authenticated) {
          if (message.type === 'authenticated') {
            clearTimeout(deadline); this.authenticated = true; await onAuthenticated();
          } else if (message.type === 'error') { onError('authentication_failed'); this.socket.close(); }
          return;
        }
        if (!['start', 'status', 'cancel', 'artifact_accepted', 'probe'].includes(message.type)) return;
        await onCommand(message);
      } catch (error) { onError(safeCode(error)); }
    };
    this.socket.onclose = () => { clearTimeout(deadline); this.authenticated = false; onDisconnected(); };
    this.socket.onerror = () => onError('companion_connection_failed');
  }
  send(message) {
    if (!this.authenticated || this.socket.readyState !== 1) return false;
    this.socket.send(JSON.stringify(message)); return true;
  }
  keepAlive() {
    try { return this.send({ type: 'keepalive' }); }
    catch { this.socket.close(); return false; }
  }
  close() { this.socket.close(); }
}
