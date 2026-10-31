import { BrowserStore, type ActiveProject, type StoredWorkspace } from "./browser-store";

interface WasmCore {
  default(options: { module_or_path: URL }): Promise<unknown>;
  browser_reset(): void;
  browser_put_file(path: string, bytes: Uint8Array): void;
  browser_delete_file(path: string): void;
  browser_get_file(path: string): Uint8Array;
  browser_list_files(): string;
  browser_call(command: string, args: string): string;
}
interface Request { id: number; type: "call" | "upload" | "read" | "artifact" | "generated_image"; command?: string; args?: unknown; cleanupPaths?: string[]; path?: string; files?: { path: string; bytes: Uint8Array }[] }
const context = self as unknown as { onmessage: (event: MessageEvent<Request>) => void; postMessage(value: unknown): void; location: Location };
const database = new BrowserStore();
let core: WasmCore;
let active: ActiveProject | null = null;
let loadedRevision = -1;
let queue = Promise.resolve();
const initialized = (async () => {
  // Public assets stay outside the JS bundle; BASE_URL also works on project Pages.
  const base = new URL(import.meta.env.BASE_URL, context.location.origin);
  const moduleUrl = new URL("wasm/entwurf_wasm.js", base);
  core = await import(/* @vite-ignore */ moduleUrl.href) as WasmCore;
  await core.default({ module_or_path: new URL("wasm/entwurf_wasm_bg.wasm", base) });
})();

function restore(stored: StoredWorkspace) {
  core.browser_reset();
  for (const [path, bytes] of stored.files) core.browser_put_file(path, bytes);
  // Each tab keeps its own project. The persisted selection is only a startup
  // default, never authority to redirect another tab's create_task command.
  active = loadedRevision < 0 ? stored.active : active;
  if (active) {
    core.browser_call("load_existing_project", JSON.stringify({ root: active.root }));
    if (active.taskId) core.browser_call("workspace_snapshot", JSON.stringify({ taskId: active.taskId }));
  }
  loadedRevision = stored.revision;
}
function capture(): Map<string, Uint8Array> {
  const paths = JSON.parse(core.browser_list_files()) as string[];
  return new Map(paths.map(path => [path, core.browser_get_file(path)]));
}
function equalFiles(a: Map<string, Uint8Array>, b: Map<string, Uint8Array>) {
  if (a.size !== b.size) return false;
  for (const [path, bytes] of a) { const old = b.get(path); if (!old || old.length !== bytes.length || bytes.some((byte, i) => byte !== old[i])) return false; }
  return true;
}
async function execute(request: Request) {
  await initialized;
  const before = await database.read();
  if (loadedRevision !== before.revision) restore(before);
  const previousActive = active;
  if (request.type === "read") return core.browser_get_file(request.path!);
  if (request.type === "generated_image") {
    const path = JSON.parse(core.browser_call("generated_image_path", JSON.stringify(request.args))) as string;
    return core.browser_get_file(path);
  }
  if (request.type === "artifact") {
    const path = JSON.parse(core.browser_call("artifact_path", JSON.stringify(request.args))) as string;
    return { path, bytes: core.browser_get_file(path) };
  }
  let result: unknown;
  let commandError: unknown;
  try {
    if (request.type === "upload") {
      for (const file of request.files ?? []) core.browser_put_file(file.path, file.bytes);
      result = null;
    } else {
      result = JSON.parse(core.browser_call(request.command!, JSON.stringify(request.args ?? {})));
      for (const path of request.cleanupPaths ?? []) if (path.startsWith("/uploads/")) core.browser_delete_file(path);
      const snapshot = result as { root?: string; selected_task_id?: string | null } | null;
      if (snapshot?.root) active = { root: snapshot.root, ...(snapshot.selected_task_id ? { taskId: snapshot.selected_task_id } : {}) };
      if (request.command === "import_sources" || request.command === "import_style_images") {
        for (const path of (request.args as { paths: string[] }).paths) if (path.startsWith("/uploads/")) core.browser_delete_file(path);
      }
      if (request.command === "import_generated_image") {
        const path = (request.args as { path: string }).path;
        if (path.startsWith("/uploads/")) core.browser_delete_file(path);
      }
    }
  } catch (error) { commandError = error; }
  // Rejected ResultEnvelopes can persist correction counters and audit records.
  const after = capture();
  if (!equalFiles(before.files, after) || JSON.stringify(before.active) !== JSON.stringify(active)) {
    try { await database.commit(before.revision, after, active); loadedRevision = before.revision + 1; }
    catch (error) { active = previousActive; restore(await database.read()); throw error; }
  }
  if (commandError !== undefined) throw commandError;
  return result;
}
context.onmessage = event => {
  const request = event.data;
  queue = queue.then(async () => {
    try { context.postMessage({ id: request.id, result: await execute(request) }); }
    catch (error) { context.postMessage({ id: request.id, error: error instanceof Error ? error.message : String(error) }); }
  });
};
