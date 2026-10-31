import type { Capabilities, Commands, WorkspaceClient, WorkspaceSnapshot } from "./workspace";
import { chooseFiles, chooseJson, download, sourceAccept } from "./web-files";

// sessionStorage is copied by window.open/duplicate-tab. Only selection can be
// restored from it; the server session identity must belong to this document.
const documentClientId = crypto.randomUUID();
const selectionKey = "entwurf.http-selection.v1";
export function httpClientId(): string { return documentClientId; }
export async function restoreHttpSelection(client: WorkspaceClient): Promise<void> {
  let selection: { root?: string; taskId?: string } | null;
  try { selection = JSON.parse(sessionStorage.getItem(selectionKey) ?? "null"); }
  catch { sessionStorage.removeItem(selectionKey); return; }
  if (typeof selection?.root !== "string") return;
  await client.call("load_existing_project", { root: selection.root });
  if (typeof selection.taskId === "string") await client.call("workspace_snapshot", { taskId: selection.taskId });
}
export function createHttpClient(initial: Capabilities & { session_token: string }, clientId = httpClientId()): WorkspaceClient {
  const headers = { "X-Entwurf-Session": initial.session_token, "X-Entwurf-Client": clientId };
  async function request(path: string, init: RequestInit = {}): Promise<unknown> {
    const response = await fetch(path, { ...init, headers: { ...headers, ...init.headers }, credentials: "same-origin" });
    const body = await response.json();
    if (!response.ok) throw new Error(body.error ?? `本地服务返回 ${response.status}`);
    return body;
  }
  const command = async (name: string, args?: unknown) => {
    const result = await request("/api/command", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ command: name, args: args ?? {} }) });
    const snapshot = result as WorkspaceSnapshot | null;
    if (snapshot?.project && typeof snapshot.root === "string") sessionStorage.setItem(selectionKey, JSON.stringify({ root: snapshot.root, taskId: snapshot.selected_task_id ?? undefined }));
    return result;
  };
  async function call(name: keyof Commands, args: unknown): Promise<unknown> {
    switch (name) {
      case "open_automation_setup": window.open("/pair.html", "_blank", "noopener,noreferrer"); return null;
      case "import_generated_image": {
        const { bytes, ...metadata } = args as Commands["import_generated_image"][0];
        const body = new FormData();
        body.append("files", new Blob([new Uint8Array(bytes)], { type: metadata.mimeType }), "generated-image");
        const uploaded = await request("/api/uploads", { method: "POST", body }) as { path: string }[];
        if (!uploaded[0]?.path) throw new Error("生成图片暂存失败，请重试导入。");
        let result: unknown;
        let importError: unknown;
        try { result = await command(name, { ...metadata, path: uploaded[0].path }); }
        catch (error) { importError = error; }
        try { await command("discard_uploads", { paths: uploaded.map(file => file.path) }); }
        catch (cleanupError) { throw new Error(`${importError instanceof Error ? `${importError.message}；` : ""}清理暂存图片失败：${cleanupError instanceof Error ? cleanupError.message : String(cleanupError)}`); }
        if (importError !== undefined) throw importError;
        return result;
      }
      case "bootstrap": return request("/api/bootstrap");
      case "choose_directory": return `/projects/${crypto.randomUUID()}`;
      case "choose_ir_file": case "choose_result_file": return chooseJson();
      case "choose_sources": {
        const files = await chooseFiles(sourceAccept, true);
        if (!files.length) return [];
        if (files.some(file => file.size > 32 * 1024 * 1024)) throw new Error("每个资料文件不得超过 32 MiB。");
        const body = new FormData(); for (const file of files) body.append("files", file);
        const result = await request("/api/uploads", { method: "POST", body }) as { path: string }[];
        return result.map(file => file.path);
      }
      case "copy_prompt": {
        const snapshot = await command("workspace_snapshot", args) as WorkspaceSnapshot;
        if (!snapshot.handoff) throw new Error("请先生成 TaskEnvelope。");
        await navigator.clipboard.writeText(snapshot.handoff.markdown); return null;
      }
      case "open_chatgpt": window.open("https://chatgpt.com/", "_blank", "noopener,noreferrer"); return null;
      case "open_export": {
        const { taskId, kind, index = 0 } = args as Commands["open_export"][0];
        const snapshot = await command("workspace_snapshot", { taskId }) as WorkspaceSnapshot;
        if (!snapshot.current_version) throw new Error("当前任务没有可导出的版本。");
        const query = new URLSearchParams({ task_id: taskId, version_id: snapshot.current_version.id, kind, index: String(index) });
        const response = await fetch(`/api/artifacts?${query}`, { headers, credentials: "same-origin" });
        if (!response.ok) throw new Error((await response.json()).error ?? "下载导出文件失败。");
        const filename = kind === "ir" ? "knowledge.ir.json" : kind === "manifest" ? "manifest.json" : `page-${index + 1}.${kind}`;
        download(new Uint8Array(await response.arrayBuffer()), filename, response.headers.get("Content-Type") ?? undefined); return null;
      }
      case "export_diagnostics": {
        const result = await command(name);
        download(JSON.stringify(result, null, 2), "entwurf-diagnostics.json", "application/json");
        return "entwurf-diagnostics.json";
      }
      default: return command(name, args);
    }
  }
  return { available: true, runtime: "local-http", projects: () => request("/api/projects") as Promise<{ root: string; name: string; id: string }[]>, call: call as WorkspaceClient["call"] };
}
