import { nativeClient, type WorkspaceClient } from "./workspace";

export async function selectRuntime(): Promise<WorkspaceClient> {
  if (nativeClient.available) return nativeClient;
  // Static Pages is fully local to this browser and never probes a user's LAN.
  if (import.meta.env.BASE_URL === "/" && (location.hostname === "localhost" || location.hostname === "127.0.0.1" || location.hostname === "[::1]")) {
    const { createHttpClient, httpClientId, restoreHttpSelection } = await import("./http-client");
    const clientId = httpClientId();
    let response: Response | undefined;
    try {
      response = await fetch("/api/bootstrap", { headers: { "X-Entwurf-Client": clientId }, signal: AbortSignal.timeout(10_000), credentials: "same-origin" });
    } catch { throw new Error("连接本机工作区超时或中断。请检查本机服务后重试；项目存储未切换。"); }
    if (response?.headers.get("Content-Type")?.includes("application/json")) {
      const capabilities = await response.json();
      // An identified service failure must not silently select another database.
      if (!response.ok) throw new Error(capabilities.error ?? "本机工作区服务暂时不可用，请重启服务后重试。");
      if (capabilities.runtime === "local-http" && typeof capabilities.session_token === "string") {
        const client = createHttpClient(capabilities, clientId);
        await restoreHttpSelection(client);
        return client;
      }
      throw new Error("本机服务没有返回有效的工作区信息，请检查服务后重试。");
    }
    if (response.status !== 404 && !(response.ok && response.headers.get("Content-Type")?.includes("text/html"))) throw new Error("本机服务暂时无法连接，请检查服务后重试；项目存储未切换。");
  }
  const { createBrowserClient } = await import("./browser-client");
  return createBrowserClient();
}
