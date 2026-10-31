# Automation and shared web acceptance in progress

The implementation uses independent branches for the portable core, shared
frontend, browser adapter and integration. The reference repository was inspected
at `3e651c27967b09e5ef6270474f9e48ee7e10110e`; no implementation was copied.

## Verified locally

- The real Rust/WASM pipeline imports, reviews, compiles and exports actual
  SVG/PNG/PDF, and survives an IndexedDB reset/reopen. Original hashes and
  immutable version guards remain active. Native regression: 145 tests passed.
- Shared browser UI checks cover actual PDF and OCR, oriented JPEGs, retained
  source reimport, persistence, multiple tabs and `/Entwurf/` asset paths.
- Local HTTP tests use actual SQLite. Sixteen checks cover access boundaries,
  upload ownership and rollback, external-edit reopening, image retention and
  repeated delivery, pairing scope, provider selection and embedded frontend assets.
- The coordinator has 20 real localhost HTTP/WebSocket tests. They cover durable
  submit intent, no replay after restart, exact owner/origin scope, import ACK,
  cancellation while disconnected, recovery from failed image persistence,
  persisted provider migration, native/browser token isolation and bounded
  shutdown without changing durable runs or images.
- The independent browser adapter has 34 fixture tests plus an isolated Chromium
  HTTPS round trip. It submits once, binds the newly submitted user turn, captures
  original image bytes and keeps them until the application confirms persistence.
  The fixture maps its ChatGPT hostname to localhost and prevents external requests.
- Shared UI recovery checks use real WASM/IndexedDB. They cover automatic import,
  no resend after uncertain submission, refusing an ACK for corrupted retained
  bytes, stale-version cleanup and cancellation of paused or pending runs.
- The actual local HTTP UI, coordinator and a fixture extension completed image
  import, ACK, reload and original-byte download together.
- Native Windows `cargo check --locked -p entwurf-desktop --target
  x86_64-pc-windows-msvc` passes with the new desktop companion and image commands.
  This used a separate checkout; the previous installation and 420 existing
  acceptance-project files remained unchanged.
- The reinstall driver also hashes the application's local WebUI workspace and
  waits for owned bridge children to exit before uninstalling. PowerShell fixture
  tests verify changed data, redirected paths, child timeout and PID reuse.

## Remaining release gates

The installed unified ChatGPT client exposes the ChatGPT product mode and its
Images page with an editable composer. Its Windows package identity is Codex;
the product-mode controls, rather than the executable name alone, distinguish
the supported surface. The normal Edge ChatGPT page is anonymous. Windows is
currently locked, so live input and original-image capture remain unverified.
The native UI adapter is being integrated against this existing client.

No real ChatGPT prompt has been sent in this acceptance run. Local fixtures
establish protocol behavior, not live generation or account acceptance.

Live Pages deployment, one sparse real-account generation from each of the three
frontends, final Windows packaging and two complete reinstall cycles are still
required. Previous desktop reinstall results apply to the previous release only.
Private source images, account information and generated acceptance images remain
outside Git.
