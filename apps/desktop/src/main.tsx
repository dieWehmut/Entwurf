import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import "./styles/global.css";
import { selectRuntime } from "./lib/runtime";

const root = createRoot(document.getElementById("root")!);
function start() {
  root.render(<p role="status">正在打开工作区…</p>);
  void selectRuntime().then(client => root.render(
  <StrictMode>
    <App client={client} />
  </StrictMode>,
  )).catch(error => root.render(<main><p role="alert">{error instanceof Error ? error.message : String(error)}</p><button onClick={start}>重试连接</button></main>));
}
start();
