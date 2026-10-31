# Entwurf

Entwurf 是面向 Windows 10/11 x64 的本地优先知识图工具。目标流程是将教材解析为带来源锚点的 Knowledge IR，借助 GPT 规划，再由本机编译、校验和导出可编辑的知识图。界面使用简体中文，代码与数据字段使用英文。

当前仓库是 **v0.1 开发中的原型**。已有 Rust 核心、契约草案、React 五页工作台和 Tauri 命令壳；这些文件与单元测试不代表完整产品已通过验收。PDF/OCR、公式排版、导出保真度、持久化与界面的端到端连接仍需按计划验证。ChatGPT 桌面自动化保持关闭；C# bridge 独立构建，尚未作为 Tauri sidecar 接入或随安装包交付。浏览器开发预览中的样例数据不等同于真实项目存储。

完整范围与发布门槛见 [v0.1 开发计划](docs/plans/entwurf-v0.1.md)。目标包含来源追溯、可撤销纠错、SVG/300 DPI PNG/A3 PDF 与离线工作；这些是验收要求，并非当前完成声明。

## 开发

Windows 原生开发需要 Rust MSVC、Visual Studio C++ Build Tools 与 Windows SDK、Node.js 22.12+、pnpm、.NET 10 SDK、WebView2 Runtime。Tesseract 与 `chi_sim`/`eng` 数据为可选 OCR 能力。

在原生 Windows PowerShell 5.1 或 PowerShell 7 中，从本地 Windows 工作树运行：

```powershell
.\scripts\windows\check-prerequisites.ps1
.\scripts\windows\test.ps1
.\scripts\windows\build.ps1
# 仅生成待验收的未签名 NSIS 包：
.\scripts\windows\build.ps1 -Bundle
```

脚本只检测和使用工具，不自动安装。完整安装说明、路径与输出位置见 [Windows 开发指南](docs/development/windows.md)。依赖下载与首次工具安装需要联网；离线产品能力需另行验收。

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
| `sidecars/gpt-bridge` | Windows C# bridge 技术验证 |
| `scripts/windows` | 原生 Windows 检测、构建与测试 |
| `fixtures` | 可公开合成样本；私有教材放 `fixtures/private/` |
| `docs` | 计划、架构与验收说明 |

## 许可证与字体

Entwurf 自有代码以 [Apache-2.0](LICENSE) 发布。第三方字体保留各自许可证，Apache-2.0 不覆盖它们：

- `assets/fonts/LXGWWenKai-Regular.ttf` 与 `apps/desktop/public/fonts/LXGWWenKai-Regular.ttf`：LXGW WenKai，© 2021–2026 LXGW、© 2020 The Klee Project Authors，SIL OFL 1.1。完整版权、保留名称与许可见 [OFL.txt](assets/fonts/OFL.txt)，前端分发目录附同一副本。
- `apps/desktop/public/fonts/DejaVuSansMono.ttf`：DejaVu Sans Mono，Bitstream Vera 基础字体 © 2003 Bitstream, Inc.，DejaVu 修改为公有领域；上游许可证亦保留 Arev 字形 © 2006 Tavmjong Bah 的许可。见 [DejaVu-LICENSE.txt](assets/fonts/DejaVu-LICENSE.txt)，前端分发目录附同一副本。

许可证文本分别来自 [Apache Software Foundation](https://www.apache.org/licenses/LICENSE-2.0.txt)、[LXGW WenKai](https://github.com/lxgw/LxgwWenKai) 和 [DejaVu Fonts](https://dejavu-fonts.github.io/License.html)。发布前仍须生成覆盖所有实际分发依赖的第三方许可证清单。
