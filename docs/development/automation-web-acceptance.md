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
- Local HTTP tests use actual SQLite. Fourteen checks cover access boundaries,
  upload ownership and rollback, external-edit reopening, image retention and
  repeated delivery, pairing scope and embedded frontend assets.
- The coordinator has 11 real localhost HTTP/WebSocket tests. They cover durable
  submit intent, no replay after restart, exact owner/origin scope, import ACK,
  cancellation while disconnected and recovery from failed image persistence.
- The independent browser adapter has 28 fixture tests plus an isolated Chromium
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

## Remaining release gates

The real signed-in ChatGPT entry has not yet been confirmed. No real ChatGPT
prompt has been sent in this acceptance run. Local fixtures establish protocol
behavior, not compatibility with the current live ChatGPT controls or account.

Live Pages deployment, one sparse real-account generation from each of the three
frontends, final Windows packaging and two complete reinstall cycles are still
required. Previous desktop reinstall results apply to the previous release only.
Private source images, account information and generated acceptance images remain
outside Git.
