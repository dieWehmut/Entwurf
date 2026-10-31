import { nativeClient, type WorkspaceClient } from "./workspace";

export async function selectRuntime(): Promise<WorkspaceClient> {
  if (nativeClient.available) return nativeClient;
  // Static Pages is fully local to this browser and never probes a user's LAN.
  if (import.meta.env.BASE_URL === "/" && (location.hostname === "localhost" || location.hostname === "127.0.0.1" || location.hostname === "[::1]")) {
    const { createHttpClient, httpClientId } = await import("./http-client");
    const clientId = httpClientId();
    let response: Response | undefined;
    try {
      response = await fetch("/api/bootstrap", { headers: { "X-Entwurf-Client": clientId }, signal: AbortSignal.timeout(2500), credentials: "same-origin" });
    } catch { /* A local static preview has no HTTP workspace service. */ }
    if (response?.headers.get("Content-Type")?.includes("application/json")) {
      const capabilities = await response.json();
      // An identified service failure must not silently select another database.
      if (!response.ok) throw new Error(capabilities.error ?? "本机工作区服务暂时不可用，请重启服务后重试。");
      if (capabilities.runtime === "local-http" && typeof capabilities.session_token === "string") {
        return createHttpClient(capabilities, clientId);
      }
    }
  }
  const { createBrowserClient } = await import("./browser-client");
  return createBrowserClient();
}
