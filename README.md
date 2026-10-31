# Entwurf

Entwurf 是本地优先的知识图工具，提供 Windows 10/11 x64 桌面应用、本机 WebUI 和 GitHub Pages 网页版。三个入口使用同一界面和 Rust 知识核心：桌面与本机 WebUI 保存到本地文件和 SQLite，网页版保存到当前浏览器的 IndexedDB。界面使用简体中文，代码与数据字段使用英文。

当前提供 **v0.1 候选版本**：五页工作台支持 PDF/Markdown/LaTeX/图片导入、来源审阅、节点编辑、版本比较和矢量导出。自包含 Windows 适配器与 PDFium 随安装包交付。ChatGPT 图片生成作为独立任务结果保存；结构化知识规划继续使用 TaskEnvelope / ResultEnvelope 人工交接。当前自动化与三个入口的实际验收进度见 [共享网页与自动化验收](docs/development/automation-web-acceptance.md)。

来源、公式和布局校验通过后，可导出 SVG、300 DPI PNG、A3 矢量 PDF、文件哈希清单及可重导入的 Knowledge IR。完整范围见 [v0.1 开发计划](docs/plans/entwurf-v0.1.md)，实际测试证据与仍未验证的环境见 [验收状态](docs/development/progress.md)。公开合成样本的通过结果不代表真实教材章节验收。

## 使用桌面应用

安装 Windows x64 NSIS 包后启动 Entwurf，在「项目」页选择本地目录创建项目和任务。「资料与风格」分别导入知识来源与风格参考图；参考图不会自动成为知识来源。PDF 文字层可直接解析，扫描页和图片 OCR 需要本机 Tesseract 的 `chi_sim` 与 `eng` 语言包。

在「结果」页核对待确认项、编辑节点并编译预览，校验通过后导出。再次生成会保留手工修改；纠错、版本和版式修复均有撤销入口。导出的 `knowledge.ir.json` 可在同一项目的新空任务中重导入；它依赖项目保留的原始资料，单独复制 JSON 不会携带源文件。

人工 GPT 流程只复制提示文本。如需参考已导入的风格图，请在 ChatGPT 对话中手动附加对应图片；提示中的本机路径不会上传图片。返回的图片引用会保留为未应用状态，本地编译仅采用兼容的分组与分页建议。

项目目录保存原始资料副本、SQLite、恢复镜像、不可变版本和导出文件，备份时请复制整个目录。应用本地运行且无遥测；「设置」中的脱敏诊断只在主动导出时写入所选本地文件。

## 网页与图片生成

安装版启动后，本机 WebUI 位于 `http://127.0.0.1:48123/`。GitHub Pages 地址为 [dieWehmut.github.io/Entwurf](https://diewehmut.github.io/Entwurf/)；浏览器数据库按网站和浏览器隔离，不会自动与桌面项目同步。网页构建、存储和部署说明见 [共享网页工作区](docs/development/web.md)。

导入知识资料后，在任务页打开「ChatGPT 连接设置」，选择 Windows 客户端或浏览器扩展。Windows 客户端模式使用本机已经登录的 ChatGPT，操作时保持 Windows 已解锁；浏览器模式使用已登录浏览器中的配套扩展。网页版通过本机配对窗口连接同一个服务，本机 WebUI 与桌面自动连接。配对和刷新不会发送提示。

填写「图片生成提示」，点击「启动一次图片生成」。匹配当前任务和知识版本的原图自动保存到该入口的项目中，并显示来源与 SHA-256；可在任务页、结果页查看并下载原始图片。连接中断后继续观察原运行，不自动重发。「停止观察」不会撤回已经提交给 ChatGPT 的请求。

## 开发

Windows 原生开发需要 Rust MSVC、Visual Studio C++ Build Tools 与 Windows SDK、Node.js 22.12+、pnpm、.NET 10 SDK、WebView2 Runtime。Tesseract 与 `chi_sim`/`eng` 数据为可选 OCR 能力。

在原生 Windows PowerShell 5.1 或 PowerShell 7 中，从本地 Windows 工作树运行：

```powershell
.\scripts\windows\check-prerequisites.ps1
.\scripts\windows\test.ps1
.\scripts\windows\build.ps1
# 生成未签名 NSIS 安装包：
.\scripts\windows\build.ps1 -Bundle
```

脚本只检测和使用工具，不自动安装。完整构建说明、路径与输出位置见 [Windows 开发指南](docs/development/windows.md)。依赖下载与首次工具安装需要联网；本地导入、编辑和导出不需要 API Key。

仅预览前端，可在 Windows、Linux 或 WSL 运行：

```sh
cd apps/desktop
pnpm install --frozen-lockfile
pnpm dev
pnpm test
pnpm build
```

Rust 核心测试可独立运行 `cargo test --locked -p entwurf-core -p entwurf-cli`。仓库中的 Rust CLI 是开发验收工具，首版产品不承诺公共 CLI 接口。原生 Windows 安装包、WebView2 与 ChatGPT 前台会话的验收不能由 WSL 前端预览替代。

## 目录

| 路径 | 职责 |
| --- | --- |
| `apps/desktop` | React/Vite 前端与 Tauri 壳 |
| `crates` | Rust 解析、IR、存储、编译和验证；内部开发工具 |
| `packages/contracts` | JSON Schema 与共享契约 |
| `sidecars/gpt-bridge` | 随包交付的 Windows UI Automation 适配器与独立结构化规划检测 |
| `sidecars/chatgpt-browser` | 复用浏览器登录会话的图片自动化扩展 |
| `scripts/windows` | 原生 Windows 检测、构建与测试 |
| `fixtures` | 可公开合成样本；私有教材放 `fixtures/private/` |
| `docs` | 计划、架构与验收说明 |

## 许可证与字体

Entwurf 自有代码以 [Apache-2.0](LICENSE) 发布。第三方字体保留各自许可证，Apache-2.0 不覆盖它们：

- `assets/fonts/LXGWWenKai-Regular.ttf` 与 `apps/desktop/public/fonts/LXGWWenKai-Regular.ttf`：LXGW WenKai，© 2021–2026 LXGW、© 2020 The Klee Project Authors，SIL OFL 1.1。完整版权、保留名称与许可见 [OFL.txt](assets/fonts/OFL.txt)，前端分发目录附同一副本。
- `apps/desktop/public/fonts/DejaVuSansMono.ttf`：DejaVu Sans Mono，Bitstream Vera 基础字体 © 2003 Bitstream, Inc.，DejaVu 修改为公有领域；上游许可证亦保留 Arev 字形 © 2006 Tavmjong Bah 的许可。见 [DejaVu-LICENSE.txt](assets/fonts/DejaVu-LICENSE.txt)，前端分发目录附同一副本。

许可证文本分别来自 [Apache Software Foundation](https://www.apache.org/licenses/LICENSE-2.0.txt)、[LXGW WenKai](https://github.com/lxgw/LxgwWenKai) 和 [DejaVu Fonts](https://dejavu-fonts.github.io/License.html)。构建会生成并随包分发 [第三方许可证清单](apps/desktop/src-tauri/resources/licenses/THIRD-PARTY-NOTICES.txt) 与 [依赖目录](apps/desktop/src-tauri/resources/licenses/dependency-inventory.json)。
