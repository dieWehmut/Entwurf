# Windows development

Use a native **Windows 10/11 x64** checkout on a local drive, for example `C:\src\Entwurf`. WSL can run frontend and Rust core checks, but cannot establish the Windows shell, installer or foreground automation acceptance. Build from a local Windows path rather than a WSL UNC path: Node package-manager wrappers may use `cmd.exe`, which does not support a UNC working directory.

## Prerequisites

The scripts support Windows PowerShell 5.1 and PowerShell 7. Install tools explicitly, then reopen the terminal so PATH changes take effect. The detector does not install tools or change persistent settings. Scripts append user PATH entries to the current process to find newly installed tools, use `pnpm.cmd` to avoid PowerShell shim policy failures, and disable .NET CLI telemetry for that process.

| Tool | Requirement | Installation/check |
| --- | --- | --- |
| Rust | stable x64 MSVC host and target, Cargo | [rustup](https://www.rust-lang.org/tools/install); `rustup default stable-x86_64-pc-windows-msvc`; `rustup target add x86_64-pc-windows-msvc`; `rustup component add rustfmt` |
| C++ tools | Visual Studio 2022 Build Tools, MSVC x64 compiler/linker | Install **Desktop development with C++** and a Windows 10/11 SDK with headers and x64 UCRT libraries |
| Node.js | 22.12+; x64 LTS recommended | [Node.js](https://nodejs.org/en/download); `node --version` |
| pnpm | pnpm 10 required; pinned 10.34.9 | `npm install --global pnpm@10.34.9`; `pnpm --version` |
| .NET | x64 **10 SDK**, not only runtime | [.NET 10 SDK](https://dotnet.microsoft.com/en-us/download/dotnet/10.0); `dotnet --list-sdks` |
| WebView2 | installed Evergreen Runtime | [Microsoft WebView2](https://developer.microsoft.com/en-us/microsoft-edge/webview2/); detected through EdgeUpdate registry entries |
| Tesseract | optional executable on PATH plus `chi_sim` and `eng` data | `tesseract --version`; `tesseract --list-langs` |

Tauri's [Windows prerequisites](https://v2.tauri.app/start/prerequisites/) explain MSVC and WebView2 requirements. If Visual Studio is installed in a custom layout without `vswhere.exe`, the detector will report missing tools; verify or repair the Visual Studio Installer installation. OCR absence is reported as optional and does not block builds. Actual OCR imports must report unavailable capability rather than fabricate text.

## Detect, test and build

From the repository root in native PowerShell:

```powershell
.\scripts\windows\check-prerequisites.ps1
# JSON includes detected versions; Strict exits nonzero for missing required tools.
.\scripts\windows\check-prerequisites.ps1 -Json -Strict
# Standalone detector/failure-propagation regression checks, without extra packages:
.\scripts\windows\scripts.tests.ps1
.\scripts\windows\test.ps1
.\scripts\windows\build.ps1
# Produce an unsigned NSIS installer candidate:
.\scripts\windows\build.ps1 -Bundle
```

If local execution policy blocks scripts, run a reviewed script with `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows\test.ps1`. This applies only to that process; do not change the machine policy for this project.

`test.ps1` checks Rust formatting, runs the full Rust workspace for `x86_64-pc-windows-msvc`, runs the explicit C# test project, and installs/tests/builds the frontend with its frozen lockfile. Add `-E2E` to run the existing Playwright suite after installing its browser with `pnpm --dir apps/desktop exec playwright install chromium`. Playwright browser tests exercise the web frontend, not native IPC or ChatGPT controls. Test results are written under ignored `artifacts/test-results/`.

`build.ps1` builds the frontend, the release Rust workspace, the self-contained `win-x64` bridge and the Tauri application. It stops at the first failed command. Cargo build/test and Tauri's Cargo runner use `--locked`, so stale or missing Cargo.lock fails instead of resolving a new graph. The frontend pins pnpm 10.34.9 in `package.json`; preflight requires a stable pnpm 10 version and rejects incompatible majors. The local `@tauri-apps/cli` dependency backs `pnpm tauri`; no global Tauri CLI is required. The normal build uses `--no-bundle`; `-Bundle` requests NSIS.

Outputs:

- Frontend: `apps/desktop/dist/`.
- Native release executable: `target/x86_64-pc-windows-msvc/release/`.
- NSIS candidate: `target/x86_64-pc-windows-msvc/release/bundle/nsis/`.
- Standalone bridge publish: `artifacts/gpt-bridge/`.

The bridge is currently published separately. Tauri launch/IPC integration and installation of that sidecar are still required before claiming automated operation or a complete installer. Build success alone does not satisfy these gates.

For manual frontend/native commands:

```powershell
cargo test --locked -p entwurf-core -p entwurf-cli
# Native shell tests need the full Windows toolchain:
cargo test --locked --workspace --target x86_64-pc-windows-msvc
pnpm --dir apps/desktop install --frozen-lockfile
pnpm --dir apps/desktop test
pnpm --dir apps/desktop build
pnpm --dir apps/desktop tauri dev -- --locked
pnpm --dir apps/desktop tauri build --target x86_64-pc-windows-msvc --bundles nsis -- --locked
# Explicit project paths are necessary: there is no root .NET solution.
dotnet build sidecars/gpt-bridge/Entwurf.Bridge/Entwurf.Bridge.csproj -c Release
dotnet test sidecars/gpt-bridge/Entwurf.Bridge.Tests/Entwurf.Bridge.Tests.csproj -c Release
```

## Release gate

The current prototype must still pass install/uninstall in a clean user profile; Chinese and space-containing paths; offline startup and real project operations; 100%/125%/150% DPI and multiple monitors; content/formula/source verification; faithful SVG/PNG/PDF/manifest export; font and dependency notice inclusion; and absence of telemetry. Keep automation disabled unless window discovery, reliable input, completion detection and reliable retrieval all pass in a real foreground session on the target ChatGPT version. Preserve drafts and pause safely when the user changes windows, locks the screen, or the application structure changes.
