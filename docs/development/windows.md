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

The ordinary Cargo command skips tests marked `ignored` that require real PDFium,
OCR or a published bridge. To include the actual parsing and rendering engines
after building/preparing the resources, run:

```powershell
$env:ENTWURF_PDFIUM_DIR = (Resolve-Path 'apps/desktop/src-tauri/resources/pdfium').Path
# Set this to the directory containing your installed tesseract.exe, or use PATH.
$env:ENTWURF_TEST_OCR_DIR = (Split-Path (Get-Command tesseract.exe).Source)
cargo test --locked -p entwurf-core -p entwurf-cli --target x86_64-pc-windows-msvc -- --include-ignored --test-threads=1
# Exercise the Rust parent against the real published sidecar too:
$env:ENTWURF_BRIDGE_TEST_EXE = (Resolve-Path 'apps/desktop/src-tauri/resources/gpt-bridge/Entwurf.Bridge.exe').Path
cargo test --locked -p entwurf-desktop --target x86_64-pc-windows-msvc bridge::tests -- --include-ignored --test-threads=1
```

`build.ps1` builds the frontend and self-contained `win-x64` bridge, fetches the locked Rust dependencies, generates release notices, and lets Tauri compile the shipped application once. It stops at the first failed command. Cargo build/test and Tauri's Cargo runner use `--locked`, so stale or missing Cargo.lock fails instead of resolving a new graph. The frontend pins pnpm 10.34.9 in `package.json`; preflight requires a stable pnpm 10 version and rejects incompatible majors. The local `@tauri-apps/cli` dependency backs `pnpm tauri`; no global Tauri CLI is required. The normal build uses `--no-bundle`; `-Bundle` requests NSIS.

Outputs:

- Frontend: `apps/desktop/dist/`.
- Native release executable: `target/x86_64-pc-windows-msvc/release/`.
- NSIS candidate: `target/x86_64-pc-windows-msvc/release/bundle/nsis/`.
- Self-contained bridge publish/resources: `apps/desktop/src-tauri/resources/gpt-bridge/`.

If `CARGO_TARGET_DIR` is set, native executable and installer paths use that directory instead of `target/`. The self-contained bridge, pinned PDFium DLL and release notices are copied into Tauri resources and installed with the application. The Rust parent probes the bridge through the authenticated pipe; this does not enable ChatGPT automation or imply its compatibility gates passed.

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

Actual results and remaining environment limits are in [acceptance status](progress.md).
Keep automation disabled unless window discovery, reliable input, completion
detection and reliable retrieval all pass in a real foreground session on the
target ChatGPT version. Unit tests and CSS zoom screenshots do not establish
native OS scaling or physical multi-monitor behavior.

The installed-app harness uses an owned test installation under
`%LOCALAPPDATA%\Programs\Entwurf` and a separate project/evidence directory. It
refuses to replace an existing installation without its ownership record. From a
native checkout with frontend development dependencies installed:

```powershell
.\scripts\windows\reinstall-acceptance.ps1 `
  -Installer 'C:\path\to\Entwurf_0.1.0_x64-setup.exe' `
  -EvidenceRoot "$env:LOCALAPPDATA\EntwurfAcceptance\new-run"
```

Cycle 0 installs and exercises the app; cycles 1 and 2 actually uninstall,
verify every project file hash, reinstall, and repeat native import/edit/export
checks. The driver uses a localhost WebView2 debugging endpoint only for the
test-launched process and native file-dialog controls scoped to that process.
It disables WebView networking; it does not disconnect host network adapters.
Evidence remains outside Git. A unique WebView profile is not by itself a fresh
native application configuration; test those separately.
