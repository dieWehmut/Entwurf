import type { Capabilities, Commands, WorkspaceClient, WorkspaceSnapshot } from "./workspace";
import { chooseFiles, chooseJson, download, sourceAccept } from "./web-files";

export function httpClientId(): string {
  let id = sessionStorage.getItem("entwurf.http-client.v1");
  if (!id) { id = crypto.randomUUID(); sessionStorage.setItem("entwurf.http-client.v1", id); }
  return id;
}
export function createHttpClient(initial: Capabilities & { session_token: string }, clientId = httpClientId()): WorkspaceClient {
  const headers = { "X-Entwurf-Session": initial.session_token, "X-Entwurf-Client": clientId };
  async function request(path: string, init: RequestInit = {}): Promise<unknown> {
    const response = await fetch(path, { ...init, headers: { ...headers, ...init.headers }, credentials: "same-origin" });
    const body = await response.json();
    if (!response.ok) throw new Error(body.error ?? `本地服务返回 ${response.status}`);
    return body;
  }
  const command = (name: string, args?: unknown) => request("/api/command", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify({ command: name, args: args ?? {} }) });
  async function call(name: keyof Commands, args: unknown): Promise<unknown> {
    switch (name) {
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
