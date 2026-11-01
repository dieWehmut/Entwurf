# 共享网页工作区

三个入口使用 `apps/desktop` 的同一 React 应用：Tauri 调用原生 IPC；本机 HTTP 服务使用本机 SQLite；静态网页使用 Web Worker 中的真实 Rust/WASM 核心，并把项目、原件、提取记录、版本与导出文件保存到 IndexedDB。

浏览器项目按网站来源隔离，不会自动同步到另一浏览器或设备。刷新后恢复最近项目。不同标签页保留各自当前项目；共享文件系统使用事务与修订号防止较旧写入覆盖较新内容。数据库提交失败时不显示保存成功，工作线程恢复已提交内容。ResultEnvelope 被拒绝时，核心产生的审计记录与纠正次数仍提交。

本机 HTTP 入口为每个文档生成独立客户端标识，即使通过 `window.open` 或复制标签页打开也不共享服务端当前项目。刷新只恢复会话中保存的项目和任务选择，先显式打开该项目，再显示工作台。连接超时、中断或服务错误显示可重试状态，不切换到浏览器数据库；只有明确静态页面响应才选择浏览器运行时。

PDF.js 解析 PDF 文字层，扫描页和图片由 Tesseract.js OCR。PDF worker、CMaps、标准字体、解码 WASM、OCR 核心以及 `eng`、`chi_sim` 数据包随构建部署，运行时从同一网站加载。OCR 来源保留页码、区域和待审阅状态；空页或提取失败不会被当成完整来源。

## 构建

先安装 Node.js 22.12+、pnpm 10、Rust 的 `wasm32-unknown-unknown` target，以及与 `Cargo.lock` 相同版本的 `wasm-bindgen-cli`。

```sh
pnpm --dir apps/desktop install --frozen-lockfile
cargo fetch --locked
bash scripts/build-wasm.sh --test
pnpm --dir apps/desktop web:assets
cd apps/desktop
pnpm exec tsc -b
pnpm exec vite build --base=/Entwurf/
pnpm exec vite preview --host 127.0.0.1 --port 1435 --base=/Entwurf/
```

打开 `http://127.0.0.1:1435/Entwurf/`。本机 HTTP 服务使用默认 `/` 的构建；不要给本机服务使用 Pages 子路径构建。

`public/wasm` 和 `public/vendor` 是可重建产物，不提交到 Git。`web:assets` 准备锁定的依赖，不在运行时依赖 CDN。普通 `pnpm build` 保留桌面构建行为；网页验收前需要准备上述 WASM 和浏览器提取组件。

`web:assets` 同时离线生成 `public/licenses/THIRD-PARTY-NOTICES.txt` 与 `dependency-inventory.json`，可用 `node scripts/release/generate-notices.mjs --web --check` 验证。清单依据 `entwurf-wasm` 的 wasm32 normal/build 依赖树、生产 npm 依赖和实际字体，沿用固定上游声明与 SHA 校验，不要求 NuGet、原生 PDFium 或 Windows 资源。首次准备前运行 `cargo fetch --locked`，供 Cargo 元数据读取使用；这些缓存中的非网页依赖不会因此加入清单。生成产物随网页一起发布，浏览器设置页的“开源许可”链接支持 `/Entwurf/` 子路径。

## 验收

```sh
pnpm test
pnpm exec playwright install --with-deps chromium
pnpm exec playwright test --config playwright.web.config.ts --workers 1
```

网页测试使用已构建的 `/Entwurf/` 页面、真实 WASM 核心及 IndexedDB，不注入 Tauri IPC。覆盖项目创建、Markdown 原件导入、节点编辑、刷新恢复、真实预览和 SVG/PNG/PDF 下载、脱敏诊断、PDF 文字层与图片/扫描 PDF OCR，以及同一数据库两个标签页的项目隔离和过期草稿保护。浏览器插件不可用时使用仓库 Playwright。

损坏的 PDF 页树可能使 PDF.js 报告的页数少于原始声明；解析错误保留为阻断审阅项，不当作完整提取。多幅 TIFF 当前提取首幅并产生明确待核对记录，其余图像需分别导入。中英文 OCR 仍需人工审阅识别内容。

原生 UI 的既有 IPC 测试继续独立存在；它们证明交互契约，不能替代网页持久化和原生后端验收。ChatGPT 登录、发送消息与真实自动化不得由 CI 触发。

真实本机 HTTP 服务启动后，可运行 `ENTWURF_HTTP_TEST_URL=http://127.0.0.1:1437 pnpm exec playwright test --config playwright.http.config.ts --workers 1`，验证复制标签页的项目隔离、刷新恢复、三秒延迟连接及中断后重试。此测试连接真实服务，不模拟项目后端。

## GitHub Pages

`.github/workflows/pages.yml` 在 `main` 更新或手动触发时构建、测试并上传 `apps/desktop/dist`。仓库默认分支为 `main`，Pages 的发布来源设为 GitHub Actions。部署任务使用 `github-pages` 环境和 `pages: write`、`id-token: write` 权限；环境规则须允许 `main` 发布。项目路径为 `/Entwurf/`，不需要 `gh-pages` 分支。

本工作流只发布静态应用与引擎资源，不运行用户任务、不上传本地资料，也不连接 ChatGPT 账号。

## 图片自动化

任务页保留人工 TaskEnvelope/ResultEnvelope 流程，同时提供独立的图片生成提示和单次启动按钮。网页通过本机配对窗口取得范围受限的访问凭据；仅接受指定本机来源与当前配对窗口的消息。桌面与本机 HTTP 页面通过各自工作区命令取得访问凭据。

本机连接页提供「Windows 客户端」和「浏览器扩展」两种连接方式。安装版会提供原生适配器路径；已有选择会保留，服务重启后恢复所选连接。连接和配对不会发送提示。活跃任务结束或停止前不能切换连接方式。三种入口都可以通过同一个本机服务使用 Windows 上已登录的 ChatGPT；原生输入与下载需要 Windows 已解锁且客户端可交互。

本机 HTTP 页面的自动化连接和扩展设置使用服务实际监听端口，支持 CLI `--port`。访问地址仅允许 HTTP 回环主机，不接受凭据、额外路径、查询或片段；静态网页配对与桌面内置服务继续使用默认 48123。

启动请求携带唯一运行 ID、任务 ID 和当前知识版本 ID。状态轮询和页面刷新只读取已有运行，不重复提交。停止观察不会撤回已经发送的 ChatGPT 请求。图片就绪且仍匹配当前任务与知识版本时自动导入一次：校验下载哈希、提交项目存储、再次读取并校验已持久化的图片字节，再确认自动化服务。失败后保留手动重试，不随轮询无限重试；版本已变化时不自动入库。若确认请求中断，重试仍须通过已保存字节的核心校验，损坏或缺失时不释放服务端副本。失败的图片导入会清理临时上传。暂停和等待导入的运行均可停止，明确的提交拒绝不会留下虚假的排队运行。

生成图片显示在任务页与结果页，标明 ChatGPT 来源、运行、基础知识版本和文件哈希，可下载原始字节。生成图片不修改知识节点、来源锚点或知识版本。仓库自动化 UI 测试使用本地响应夹具配合真实 WASM/IndexedDB，验证单次提交、刷新续接、保存后确认及原图下载，不调用真实 ChatGPT。

隔离的本机服务运行在 48123 时，可用 `ENTWURF_HTTP_TEST_URL=http://127.0.0.1:48123 ENTWURF_HTTP_TEST_DATA_DIR=<隔离数据目录> pnpm exec playwright test --config playwright.http.config.ts -g 'real HTTP coordinator' --workers 1` 验证真实 HTTP、SQLite 和自动化协调器。测试连接一个本地 WebSocket 扩展夹具，交付已有 PNG 字节，验证自动入库后确认、原图下载、过期版本拒绝和上传文件清理；不操作 ChatGPT。
