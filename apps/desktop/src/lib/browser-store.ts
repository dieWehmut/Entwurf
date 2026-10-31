export interface ActiveProject { root: string; taskId?: string }
export interface StoredWorkspace { revision: number; files: Map<string, Uint8Array>; active: ActiveProject | null }
export class StaleWriteError extends Error {
  constructor() { super("另一个窗口已保存项目。请刷新工作区后重试；当前草稿仍保留。"); this.name = "StaleWriteError"; }
}

/** One transaction owns the revision, bytes, deletions and selection. */
export class BrowserStore {
  private database: Promise<IDBDatabase> | undefined;
  constructor(private name = "entwurf.workspace.v1") {}
  private open(): Promise<IDBDatabase> {
    return this.database ??= new Promise((resolve, reject) => {
      const request = indexedDB.open(this.name, 1);
      request.onupgradeneeded = () => {
        request.result.createObjectStore("files");
        request.result.createObjectStore("meta");
      };
      request.onerror = () => reject(request.error);
      request.onblocked = () => reject(new Error("数据库正在其他窗口升级，请关闭其他 Entwurf 窗口后重试。"));
      request.onsuccess = () => {
        const db = request.result;
        db.onversionchange = () => { db.close(); this.database = undefined; };
        resolve(db);
      };
    });
  }
  async read(): Promise<StoredWorkspace> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const transaction = db.transaction(["files", "meta"], "readonly");
      const files = new Map<string, Uint8Array>();
      let revision = 0;
      let active: ActiveProject | null = null;
      const cursor = transaction.objectStore("files").openCursor();
      cursor.onsuccess = () => { const entry = cursor.result; if (entry) { files.set(String(entry.key), new Uint8Array(entry.value)); entry.continue(); } };
      const meta = transaction.objectStore("meta").get("state");
      meta.onsuccess = () => { revision = meta.result?.revision ?? 0; active = meta.result?.active ?? null; };
      transaction.oncomplete = () => resolve({ revision, active, files });
      transaction.onabort = () => reject(transaction.error ?? new Error("读取浏览器数据库失败。"));
    });
  }
  async commit(expectedRevision: number, files: Map<string, Uint8Array>, active: ActiveProject | null): Promise<void> {
    const db = await this.open();
    return new Promise((resolve, reject) => {
      const transaction = db.transaction(["files", "meta"], "readwrite");
      let failure: Error | null = null;
      transaction.oncomplete = () => resolve();
      transaction.onabort = () => reject(failure ?? transaction.error ?? new Error("浏览器存储写入失败；未保存本次修改。"));
      const meta = transaction.objectStore("meta");
      const read = meta.get("state");
      read.onsuccess = () => {
        if ((read.result?.revision ?? 0) !== expectedRevision) { failure = new StaleWriteError(); transaction.abort(); return; }
        const store = transaction.objectStore("files");
        store.clear();
        for (const [path, bytes] of files) store.put(bytes, path);
        meta.put({ revision: expectedRevision + 1, active }, "state");
      };
    });
  }
}
