const prefix = 'entwurf-run-';
export function runStore(storage) {
  return {
    async get(id) { return (await storage.get(prefix + id))[prefix + id]; },
    async put(record) { await storage.set({ [prefix + record.run_id]: record }); },
    async list() { const all = await storage.get(null); return Object.entries(all).filter(([key]) => key.startsWith(prefix)).map(([, value]) => value); }
  };
}
let opened;
function database() {
  opened ??= new Promise((resolve, reject) => {
    const request = indexedDB.open('entwurf-chatgpt-artifacts', 1);
    request.onupgradeneeded = () => request.result.createObjectStore('images', { keyPath: 'run_id' });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new Error('artifact_storage_failed'));
  });
  return opened;
}
async function access(mode, operation) {
  const db = await database();
  return new Promise((resolve, reject) => {
    const tx = db.transaction('images', mode); const request = operation(tx.objectStore('images'));
    tx.oncomplete = () => resolve(request.result);
    tx.onerror = tx.onabort = () => reject(new Error('artifact_storage_failed'));
  });
}
export const artifactStore = {
  put: record => access('readwrite', store => store.put(record)),
  get: id => access('readonly', store => store.get(id)),
  remove: id => access('readwrite', store => store.delete(id))
};
