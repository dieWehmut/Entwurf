import type { Commands, WorkspaceClient, WorkspaceSnapshot } from "./workspace";
import { chooseFiles, chooseJson, download, sourceAccept } from "./web-files";

type Request = { type: "call"; command: string; args?: unknown; cleanupPaths?: string[] } | { type: "upload"; files: { path: string; bytes: Uint8Array }[] } | { type: "read"; path: string } | { type: "artifact"; args: unknown };
export function createBrowserClient(): WorkspaceClient {
  const worker = new Worker(new URL("./browser-worker.ts", import.meta.url), { type: "module" });
  let sequence = 0;
  const pending = new Map<number, { resolve(value: unknown): void; reject(error: Error): void }>();
  let workerFailure: Error | undefined;
  const selectedFiles = new Map<string, File>();
  worker.onmessage = event => {
    const { id, result, error } = event.data;
    const waiter = pending.get(id); if (!waiter) return; pending.delete(id);
    if (error !== undefined) waiter.reject(new Error(error)); else waiter.resolve(result);
  };
  worker.onerror = () => {
    workerFailure = new Error("浏览器计算服务启动失败。请确认页面与计算组件已完整加载，然后刷新重试。");
    for (const waiter of pending.values()) waiter.reject(workerFailure);
    pending.clear();
  };
  function request<T>(message: Request): Promise<T> {
    if (workerFailure) return Promise.reject(workerFailure);
    const id = ++sequence;
    return new Promise<T>((resolve, reject) => { pending.set(id, { resolve: value => resolve(value as T), reject }); worker.postMessage({ ...message, id }); });
  }
  const invoke = <T,>(command: string, args?: unknown) => request<T>({ type: "call", command, args });
  async function call(command: keyof Commands, args: unknown): Promise<unknown> {
    switch (command) {
      case "bootstrap": {
        await invoke("list_projects");
        return { version: "0.1.0", platform: "浏览器 / WebAssembly", runtime: "browser", storage: "indexeddb", tesseract_available: true, ocr_languages: ["chi_sim", "eng"], automation_enabled: false, bridge_status: "not_connected", bridge_detail: "项目与资料保存在此浏览器。PDF 解析及中英文 OCR 由页面随附组件在本机执行；ChatGPT 自动化尚未连接。" };
      }
      case "choose_directory": return `/projects/${crypto.randomUUID()}`;
      case "choose_sources": {
        const selected = await chooseFiles(sourceAccept, true);
        if (!selected.length) return [];
        if (selected.some(file => file.size > 32 * 1024 * 1024)) throw new Error("每个资料文件不得超过 32 MiB。");
        const token = crypto.randomUUID();
        const files = await Promise.all(selected.map(async (file, index) => ({ path: `/uploads/${token}/${index}/${file.name.replaceAll(/[\\/]/g, "_")}`, bytes: new Uint8Array(await file.arrayBuffer()) })));
        await request({ type: "upload", files });
        files.forEach((file, index) => selectedFiles.set(file.path, selected[index]));
        return files.map(file => file.path);
      }
      case "import_sources": {
        const { paths, taskId } = args as Commands["import_sources"][0];
        const snapshot = await invoke<WorkspaceSnapshot>("workspace_snapshot", { taskId });
        const { extractBinary } = await import("./browser-extraction");
        for (const path of paths) {
          const file = selectedFiles.get(path);
          if (!file) continue;
          const extraction = await extractBinary(file, snapshot.project.settings.ocr_languages);
          if (extraction) await invoke("register_extraction", { path, extraction });
        }
        const result = await invoke(command, args);
        paths.forEach(path => selectedFiles.delete(path));
        return result;
      }
      case "reimport_source": {
        const { taskId, sourceId } = args as Commands["reimport_source"][0];
        const snapshot = await invoke<WorkspaceSnapshot>("workspace_snapshot", { taskId });
        const source = snapshot.current_version?.ir.sources.find(item => item.id === sourceId);
        if (source) {
          const path = source.stored_path.startsWith("/") ? source.stored_path : `${snapshot.root}/${source.stored_path}`;
          const bytes = await request<Uint8Array>({ type: "read", path });
          const { extractBinary } = await import("./browser-extraction");
          const extraction = await extractBinary(new File([new Uint8Array(bytes).buffer], source.original_name), snapshot.project.settings.ocr_languages);
          if (extraction) {
            const extractedPath = `/uploads/${crypto.randomUUID()}/${source.original_name.replaceAll(/[\\/]/g, "_")}`;
            await request({ type: "upload", files: [{ path: extractedPath, bytes }] });
            await request({ type: "call", command: "register_extraction", args: { path: extractedPath, extraction }, cleanupPaths: [extractedPath] });
          }
        }
        return invoke(command, args);
      }
      case "import_style_images": {
        const result = await invoke(command, args);
        (args as Commands["import_style_images"][0]).paths.forEach(path => selectedFiles.delete(path));
        return result;
      }
      case "choose_ir_file": case "choose_result_file": return chooseJson();
      case "copy_prompt": {
        const snapshot = await invoke<WorkspaceSnapshot>("workspace_snapshot", args);
        if (!snapshot.handoff) throw new Error("请先生成 TaskEnvelope。");
        await navigator.clipboard.writeText(snapshot.handoff.markdown); return null;
      }
      case "open_chatgpt": window.open("https://chatgpt.com/", "_blank", "noopener,noreferrer"); return null;
      case "open_export": {
        const { path, bytes } = await request<{ path: string; bytes: Uint8Array }>({ type: "artifact", args });
        if (!path) throw new Error("此版本尚无对应导出文件，请先导出。");
        download(bytes, path.split("/").pop()!); return null;
      }
      case "export_diagnostics": {
        const diagnostics = await invoke<unknown>("diagnostics");
        download(JSON.stringify(diagnostics, null, 2), "entwurf-diagnostics.json", "application/json");
        return "entwurf-diagnostics.json";
      }
      default: return invoke(command, args);
    }
  }
  return { available: true, runtime: "browser", projects: () => invoke("list_projects"), call: call as WorkspaceClient["call"] };
}
