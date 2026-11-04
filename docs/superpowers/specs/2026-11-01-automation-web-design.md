# Entwurf: ChatGPT automation and shared web runtimes

## Requested outcome

Implement an independent ChatGPT workflow that submits a prompt, observes completion,
retrieves the generated original image and persists/displays it in Entwurf. Provide
GitHub Pages with automatic Actions deployment and browser database persistence,
and a local WebUI with the same pages and local database persistence. The existing
desktop application remains supported. All three entry points must be verified
against the user's locally signed-in ChatGPT account. Develop on multiple branches
with multiple `gpt-6-astra` subagents, committing and pushing completed stages.

The reference is `fwerkor/local-shell-mcp` revision
`3e651c27967b09e5ef6270474f9e48ee7e10110e`. It exposes local tools to ChatGPT as an MCP
client; it does not implement the requested ChatGPT conversation/image adapter.
Only architecture was studied; no source is copied or added as a dependency.

## Runtime design

One React App consumes `WorkspaceClient` through three implementations:

- Tauri calls the native Rust Workspace, retaining SQLite and local files.
- Local HTTP calls a loopback Rust service and retains SQLite/local files beneath
  its configured data directory. Default port is 48123.
- Pages runs the same Rust Workspace/compiler in a dedicated WASM worker and
  persists virtual files, immutable versions, source bytes and artifacts in IndexedDB.

Native filesystem/SQLite/PDFium/process code is feature-gated behind a shared
filesystem and storage seam. Browser ingestion uses genuine PDF/OCR extraction and
registers retained transcripts; the Rust provenance and review gates remain active.
The browser app must perform real parsing, editing, validation and export independently
of the local companion. The companion is needed only for local ChatGPT automation.

## Frozen transport contracts

The WASM worker exports `browser_reset()`, `browser_put_file(path, Uint8Array)`,
`browser_get_file(path)`, `browser_list_files()` (JSON string array) and
`browser_call(command, args_json)` (JSON result, throwing on errors). Optional
`browser_delete_file(path)` supports rollback. Paths are absolute virtual paths:
`/projects/<id>` and `/uploads/<token>/<name>`. Browser commits are serialized,
atomic, and fail without leaving the worker ahead of persisted state. Tab-local
selection and stale-write checks prevent cross-tab project confusion.

The local server exposes:

| Request | Result |
| --- | --- |
| `GET /api/bootstrap` | capabilities, `runtime: local-http`, `storage: sqlite`, session token |
| `GET /api/projects` | `[{root,name,id}]` with opaque `/projects/<id>` roots |
| `POST /api/command` | `{command,args}` using current camelCase command arguments; direct JSON result |
| `POST /api/uploads` | multipart `files`; `[{path,name}]` with client-owned handles |
| `GET /api/artifacts` | verified bytes selected by task ID, version ID, kind and index |

Errors use non-success HTTP status plus `{error: string}`. API clients send
`X-Entwurf-Session` and a tab-local `X-Entwurf-Client` UUID. The server validates
Host/Origin, binds loopback by default, scopes current project and uploads to the
client, and resolves opaque roots strictly beneath the data directory. Renderers
own file pickers, clipboard and downloads. The shared project picker accepts
`choose_directory({purpose: create|open, name?})`; native callers remain compatible.

## Automation and image ownership

A narrow local run coordinator serves all three entry points. Each run records
`run_id`, `task_id`, `base_version_id` and prompt SHA-256 before submission.
The prompt travels in the active start command and is not persisted in run logs.
It retains durable phase and result state; reconnection never replays a submission.
Phases are queued, preparing, submitting, generating, downloading, awaiting_import,
completed, paused, failed and cancelled.

An independently implemented MV3 extension can control an existing signed-in
`chatgpt.com` browser session without copying profiles, cookies or tokens. It may
open a new conversation/tab in that same profile, preserving the user's drafts.
The local Windows client exposes the ChatGPT product mode and Images page; a
native UI Automation adapter is therefore required for the user's existing account.
Each new Windows image run must open an independent client window and a fresh
conversation using that same signed-in session. The original window, conversation
and unsent draft must be preserved. Verify the new window's HWND, process/start
identity, product mode and empty composer before image preparation; never reuse
the current conversation or choose an arbitrary existing window. A submitted run
continues observation only in its originally bound window. Native Save As dialogs
must belong to that window, not merely share its process.
It uses the packaged bridge with an inherited stdin bootstrap, separate native
credential and parent/EOF lifetime. Only the selected provider may authenticate.
Durable submit intent prevents retyping or resending after reconnect/restart.
Recovery may observe a submitted response only when its saved UI identity still
matches. Live original-image controls remain subject to real-account acceptance.

The extension connects to `/api/automation/extension` by WebSocket. Its first frame
authenticates with a locally paired credential. Only start, status and cancel are
accepted. Run identity accompanies every update and artifact. Original image bytes
are bounded (32 MiB, 64 million pixels), decoded and hashed before acceptance.
Generation and download must be tied to the response caused by that run; a screenshot,
old response, arbitrary returned path or merely absent Stop button is insufficient.

Pages pairs explicitly with the local coordinator and receives an origin-bound,
automation-only capability. It cannot access arbitrary local files or shell commands.
Images are imported into the destination runtime's own database/files with immutable
run/task/version provenance. Only after that commit does the client acknowledge
`artifact_accepted`; stale task versions keep the result available for review without
overwriting edits. The UI shows original generated images and supports their download.
Structured ResultEnvelope planning remains a separate operation.

## Branch ownership

| Branch | Owner | Scope |
| --- | --- | --- |
| `feat/automation-web` | root | integration, local HTTP/coordinator, generated-image domain/contracts, final tests |
| `feat/browser-core` | core agent | portable filesystem/storage, WASM bridge, extraction contract, native regressions |
| `feat/shared-web` | frontend agent | shared runtime clients/UI, worker/IndexedDB, PDF/OCR integration, Pages workflow |
| `feat/chatgpt-automation` | automation agent | existing-session adapter/extension, durable submission and capture, fixtures |
| `feat/chatgpt-windows` | native agent | existing Windows ChatGPT Images surface, durable observation and original download |

Each branch uses its own managed worktree. Agents may commit their tested chunks;
root reviews, pushes branches and integrates. Preserve the existing Windows checkout,
whose current tracked differences must not be overwritten. Native builds use a
separate checkout. Private samples, account data, live transcripts and generated
acceptance images stay outside Git. The user's icon remains in the shipped assets.

## Acceptance and limited live traffic

Use deterministic local fixtures for delayed output, cancellation, ambiguous clicks,
restarts, duplicate requests, stale versions, image download failures and UI changes.
No CI job logs into ChatGPT. Initially budget one real image generation per entry
point (Pages, local WebUI, installed desktop), performed sequentially with time between
runs. Additional real submissions require a concrete defect that local fixtures
cannot settle; never repeat requests simply to obtain more screenshots. Pause on
login, quota, captcha, unsupported controls or navigation ambiguity.

Completion requires: real browser persistence after reload; local SQLite persistence
after server restart; existing native behavior preserved; original generated images
automatically imported and visible from all three entry points using the existing
account; a successful deployed Pages Actions run and live site smoke test; and two
actual final desktop uninstall/reinstall cycles preserving user projects. Build/mock
success alone cannot satisfy any real-account or installed-app requirement.

## Home and settings references — 2026-11-04

The supplied homepage and settings screenshots define the next shared-shell
refinement. Use their compact titlebar, narrow icon navigation, secondary sidebar,
subdued surfaces, centered content and spacing with Entwurf's own content and
existing controls. The homepage's primary input must create a real task; creating
and opening projects, entering an optional chapter and selecting saved tasks must
remain available. Settings must provide a searchable category sidebar and grouped
rows for the existing theme, OCR, ChatGPT, storage and diagnostic controls.

Native minimize, maximize/restore and close icons must be visible at the top right
and invoke the actual Tauri window. Keep the existing unsaved-draft close protection.
The browser uses its host's window controls. Verify both themes, narrow viewports,
keyboard operation and real workspace persistence, and compare rendered screenshots
with the user's references before including the UI in the next Windows installer.
