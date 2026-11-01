# ChatGPT Automation and Shared Web Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically generate and retain ChatGPT images in desktop, local WebUI and Pages, with one UI and the required local/browser persistence.

**Architecture:** Preserve the Rust domain/compiler through a portable filesystem seam. Add browser worker/IndexedDB and loopback HTTP adapters beneath the shared UI. An independently implemented existing-session ChatGPT adapter communicates with a durable local coordinator.

**Tech Stack:** Rust, WASM/wasm-bindgen, React/TypeScript, IndexedDB, loopback HTTP/WebSocket, MV3, existing Tauri and SQLite.

**Spec:** `docs/superpowers/specs/2026-11-01-automation-web-design.md`

## Global constraints

- Use multiple branches/worktrees and `gpt-6-astra` subagents; root owns integration and staged pushes.
- Reuse the locally signed-in ChatGPT account for all three real acceptance flows.
- Do not copy reference implementation, credentials or browser profiles.
- Real generations are sparse and sequential; fixtures cover repeated failure/race tests.
- Existing source provenance, immutable versions, review gates, manual edits and user icon remain valid.
- Pages uses a real browser database; local WebUI uses local SQLite/files. Both render the same React pages.

## Task 1: Portable core and actual WASM compilation

Files: `crates/entwurf-core/src/{lib,error,storage,workspace,ingest,compiler,provenance,ir_import}.rs`, new filesystem/storage modules, `crates/entwurf-wasm/**`, `scripts/build-wasm.sh`, Cargo manifests/lock.

- [ ] Capture a failing executable WASM smoke test importing Markdown/TeX, resolving literal source reviews, compiling and retrieving actual SVG/PNG/PDF bytes.
- [x] Introduce default native and browser features; share domain invariants and add a virtual filesystem/storage implementation.
- [x] Export the frozen WASM methods and browser transcript registration with verified original/transcript hashes and pending OCR/formula review.
- [x] Run the real WASM smoke including reopen/tamper detection, then native regression tests for the altered seam.
- [x] Commit the tested portable-core branch and provide exact build/worker artifacts to the frontend owner.

## Task 2: Local WebUI service

Files: new `crates/entwurf-web/**`, Cargo workspace entry, local launcher scripts and service tests.

- [x] Add HTTP boundary tests for Host/Origin/session rejection, opaque path confinement, two clients opening different projects, upload ownership and artifact identity.
- [x] Implement the frozen bootstrap/projects/command/uploads/artifacts endpoints, shared Workspace dispatch and static frontend serving on loopback port 48123.
- [x] Verify a real public-source import/edit/export and server restart with SQLite files retained beneath the configured data directory.
- [x] Commit the local service, preserving browser-only storage independence.

## Task 3: Shared browser/HTTP frontend and persistence

Files: `apps/desktop/src/lib/**`, runtime worker, shared project picker, existing App/components, package/Vite configuration and browser tests.

- [x] Export typed commands, add runtime clients and remove the native-only browser lockout.
- [x] Implement native/HTTP/browser file selection and artifact download through backend-owned handles.
- [x] Commit WASM virtual files and metadata atomically to IndexedDB, retaining immutable bytes and guarding quota/stale-tab failures.
- [x] Integrate real PDF/OCR browser extraction and the core transcript hook.
- [x] Run genuine browser-mode project/import/review/compile/export/reload tests without injected native IPC mocks, then local HTTP UI checks against the same pages.
- [x] Commit the shared frontend branch.

## Task 4: Existing-session ChatGPT adapter

Files: `sidecars/chatgpt-browser/**`; native adapter files only if required by the identified signed-in entry.

- [ ] Discover the actual local signed-in surface read-only; identify composer, new response, generation completion and original download controls.
- [x] Implement the narrow authenticated transport, durable submit intent and no-replay recovery against deterministic DOM/download fixtures.
- [x] Implement exact response/run correlation, original image capture, bounded bytes and cancellation/navigation guards.
- [x] Keep the artifact pending until the destination app commits it and sends `artifact_accepted`.
- [x] Commit tested adapter chunks; defer all real sends to the coordinated acceptance stage.

The installed ChatGPT product mode, Images page and composer have been identified
read-only. Current original-output/download controls remain subject to the live
acceptance in Task 7; fixture evidence does not complete that discovery step.

## Task 5: Durable run coordinator and generated-image domain

Files: local service automation modules, core/project/contracts image metadata, Tauri integration, shared UI automation controls/image result display.

- [x] Add version-bound generated asset records and tested trusted byte import; reject tampering and stale automatic replacement.
- [x] Persist run state and staging in the local coordinator; authenticate paired extension and origin-bound Pages capability.
- [x] Connect start/status/cancel/download/ack from all three Workspace clients, including refresh recovery and exact-once database import.
- [x] Verify complete flows with deterministic adapter fixtures before touching the real account.
- [x] Commit and integrate each reviewed branch, resolving shared schema/Cargo changes explicitly.
- [x] Complete the final-review fix for preview/download of staged stale originals, including after stopping observation, while preserving import version guards and ACK boundaries.

## Task 6: Pages build and deployment

Files: `.github/workflows/**`, WASM/frontend build scripts and deployment acceptance.

- [x] Build WASM and the actual browser app at `/Entwurf/`, testing fonts, worker and assets under the subpath.
- [x] Add Pages artifact upload/deploy jobs with minimal explicit token permissions; trigger automatically on final `main`, with manual dispatch for staged branches.
- [x] Enable Pages Actions publishing when the deployable artifact is concrete; push and inspect the actual run/deployment status.
- [x] Open the live site and verify real project persistence and local companion pairing; record deployment URL/commit.

## Task 7: Coordinated real acceptance and final delivery

- [ ] Use one bounded public image-generation prompt from each of Pages, local WebUI and the installed desktop, on the same existing account; verify returned original images appear and persist in the correct database.
- [ ] Record run IDs/hashes/timing privately, with no account identifiers or private images committed.
- [x] Rebuild the Windows installer and perform two actual uninstall/reinstall cycles, preserving project and generated-image data.
- [ ] Audit every explicit requirement against real evidence; unresolved account/selector/deployment checks remain incomplete.
- [ ] Publish concise usage/setup/test evidence, push final integration and only then mark the goal complete.

Final reviewed candidate `2f27ef9` passed native reinstall cycles 7 and 8 and Pages
deployment `41274652492`, followed by preserved-project/connection preparation in
all three entry points. The real-account generation and final main/default-branch
gates remain open while the Windows session is locked. See
`docs/development/automation-web-acceptance.md` for detailed evidence.
