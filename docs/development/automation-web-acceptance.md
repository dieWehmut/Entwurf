# Automation and shared web acceptance in progress

The implementation uses independent branches for the portable core, shared
frontend, browser adapter and integration. The reference repository was inspected
at `3e651c27967b09e5ef6270474f9e48ee7e10110e`; no implementation was copied.

## Verified implementation and releases

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
  The selected native host and eager desktop startup also pass Windows checking.
  This used a separate checkout; the previous installation and 420 existing
  acceptance-project files remained unchanged.
- The reinstall driver also hashes the application's local WebUI workspace and
  waits for owned bridge children to exit before uninstalling. PowerShell fixture
  tests verify changed data, redirected paths, child timeout and PID reuse.

- The combined native Windows adapter passes 95 .NET tests, including real child-process
  bootstrap/EOF, single submission, cancellation, persisted binding, original-byte
  ACK/replay, lock-screen probe recovery and transcript speaker boundaries. The
  final fixes were independently reviewed against the exact Windows test inputs.
  Missing UIA visibility metadata is treated conservatively. A real method-stack
  capture identified a cleanup lock wait; four blocked-child exit cases failed
  before the fix and passed after bounded cleanup was added. Retained run and
  original-image bytes remain unchanged, and restart does not resubmit.
- Browser dependency distribution now includes 393 records and 254 original
  notice texts. The native inventory remains 614 records and 409 texts, with no
  unresolved notices. The supplied desktop icon is also the browser favicon.
- The first actual Pages run passed core/frontend checks and ten of eleven
  browser scenarios, then correctly stopped before deployment. A persistence test
  refreshed before the asynchronous save completed; it now waits for the explicit
  post-commit UI acknowledgment. Three affected scenarios each passed three runs.
  The second deployment run is `41253803685` at `cd17d82`.
- Pages deployment `41255363101` at `b8d2fcc` passed 66 frontend tests,
  11 actual browser tests and the WASM smoke checks. The deployed page was opened
  again with the earlier persistent browser profile: its project survived the
  update, the supplied icon and Chinese text rendered correctly, and no browser
  errors occurred. This verifies that UI revision, before the later requested
  homepage/settings layout changes.
- The requested navigation rail and categorized settings are implemented in the
  shared UI. Native Windows Chromium screenshots were compared with the supplied
  references using the observed 192 DPI. See [design QA](../../design-qa.md).
  Candidate `6f7bbb7` was deployed by [Pages run 41258979120](https://github.com/dieWehmut/Entwurf/actions/runs/41258979120),
  with 67 frontend and 12 real browser tests passing. Reusing the earlier browser
  profile on the public site preserved its project, task and edited node. The
  updated navigation, four settings categories, themes, mobile layout, icon and
  dependency notices were verified without browser errors.
- The Windows NSIS candidate was built and installed from `6f7bbb7`.
  Installer SHA-256: `f147c0d749b9bbe2346870c4d5c33905daa9e35ca26fbbeb03b25d32550acbb6`.
  Two actual uninstall/reinstall cycles used that same installer; neither reused
  the previous installation. Each passed all 12 native UI checks, including real
  SVG/PNG/PDF export, IR restoration, reversible layout repair, diagnostics save
  and cancellation, and five pages in both themes.
- Reinstall cycle 3 preserved all 420 native-project and 19 local-WebUI files;
  cycle 4 preserved all 494 native-project and 20 local-WebUI files. Every file's
  SHA-256 was compared before and after uninstall. Both installed processes owned
  the eager localhost service. Independent HTTP clients reopened the existing
  public-fixture project and retrieved its identical 7,433-byte image after each
  reinstall. These images are explicitly public fixtures, not ChatGPT output.

## Resumed candidate checks — 2026-11-01

The requested compact titlebar and real task sidebar are integrated at `e13e8af`;
the candidate at `64dd4e8` also contains reviewed native-test repairs. Root reran
82 frontend tests, all 27 coordinator tests, the real WASM smoke suite, 12 Pages-path
browser cases and the separate real-browser shell flow. All passed. The shell flow
covers task selection, keyboard menus, node drafts through Back/Forward, theme and
project persistence, settings layout and the mobile task sidebar. Root compared
the newly captured desktop/settings/mobile images with the supplied references.

The first fresh native test run passed 118 of 121 and stopped packaging. One
assertion read an artifact before its asynchronous delivery; two teardown deletes
encountered Windows sharing violations. The reviewed test-only repair waits for
the matching artifact and briefly waits for a conflicting directory handle while
retaining explicit watcher disposal, process exit and exclusive state-lock checks.
Actual held-handle tests and a missing-disposal mutation verify those checks. The
clean final native suite passes 124 of 124, with no skipped tests. The original
intermittent directory-handle owner was not identified; no production lifecycle
repair is claimed.

Candidate `64dd4e8` built a new Windows NSIS installer with SHA-256
`743729c98d86d462445f1a9a9ef505a67e2738df922fb401ca10a927a9207094`.
Actual reinstall cycles 5 and 6 each passed all 15 native UI checks. The titlebar
maximized/restored and minimized/restored the real window, an unsaved node draft
blocked close, and the final close action exited both the app and its owned bridge.
The offline import/edit/export, IR restoration, layout repair, diagnostics and
five-page/two-theme checks also passed. Cycle 5 preserved all 572 native-project
files and 25 local-WebUI files; cycle 6 preserved all 655 and 25 respectively, by
comparing every file hash before and after uninstall. Root inspected the new
installed task and settings screenshots. These remain intermediate-candidate
results; affected artifacts must be rebuilt after the final review fix.

[Pages run 41271982734](https://github.com/dieWehmut/Entwurf/actions/runs/41271982734)
successfully built, tested and deployed `64dd4e8`, including the compact titlebar.
The public site reopened its original dedicated browser profile with the exact
same project, task and r1 version. Its existing origin-bound pairing reached the
installed candidate's local service without network failures. The status correctly
reported the locked Windows session. Preparation issued zero generation requests,
left the original submission-intent state unchanged, and produced a new screenshot
that root inspected.

The final implementation review found a required workflow gap: an original image
staged against an older task version could not be previewed or downloaded, and
stopping observation also blocked its retrieval. The scoped fix is integrated at
`8286a11` and independently approved. It provides authenticated, hash/MIME-bound
original preview/download and keeps captured bytes readable after cancellation,
while retaining the core's stale-import rejection and cancelled-run ACK/update
guards. Viewing or downloading changes neither knowledge nor delivery state.

The same review caught and resolved a recovery regression in the initial fix:
images already saved before an interrupted ACK still need explicit confirmation
after a later knowledge edit. The final action requires matching run/task/base,
prompt, image hash and MIME, then independently rereads destination bytes before
ACK. The reviewer exercised the actual new control against real WASM state and
verified one ACK, no new import/download and unchanged knowledge.

All 97 frontend tests pass on the integrated tree. The fix also passed all 28
coordinator tests, six real-WASM browser automation scenarios and the isolated
real HTTP/SQLite browser scenario, with the final confirmation-label adjustment
checked in the affected corruption scenario. Tests cover exact retained downloads
after edit/reload/stop/reload, owner/origin boundaries, corrupt/oversized bytes,
late-result rejection and no extra submission or ACK during review. The HTTP
scenario preserves direct stale-import rejection and explicit upload-cleanup
checks, uses its own data directory and refuses the installed service's port.
Candidate `2f27ef9` contains the reviewed fix. Its new Windows installer has SHA-256
`4c46e622a33c29fc24788147baad02480c898bbaa884f041ff9387125c7fcc45`.
The native build passed formatting, contract checks, TypeScript and all 124 .NET
tests. Final reinstall cycles 7 and 8 each passed all 15 installed-app checks,
including actual titlebar actions, node-draft close protection, offline editing
and export, and clean app/bridge exit. Cycle 7 preserved every hash of 742 project
files and 25 local-WebUI files; cycle 8 preserved all 833 and 25 respectively.
Root inspected the final installed task and settings screenshots. Generated images
in these repeated fixture checks remain public test images, not account output.

[Pages run 41274652492](https://github.com/dieWehmut/Entwurf/actions/runs/41274652492)
successfully built and deployed the same `2f27ef9` source. Its real WASM smoke,
97 frontend tests and 12 browser tests passed. The public site's original browser
profile retained its exact project/task/r1 identity and paired with the final
installed service without network failures or a generation request.

All three entry preparations were checked against the final artifacts. The local
WebUI reopened and refreshed its original SQLite project and paused run with the
same task, base version and prompt hash; it issued zero automation write requests
and left the original intent/state files byte-identical. The desktop reopened its
preserved public-source task. Before that preparation, its never-submitted test
identity was refreshed after the reinstall fixtures' global style snapshots;
the prior setup was retained and knowledge/style hashes were proven unchanged,
with no native intent or corresponding run in either store. No existing submitted
run was rebound. Pages and desktop preparations created no generation intent.

## Remaining release gates

The installed unified ChatGPT client exposes the ChatGPT product mode and its
Images page with an editable composer. Its Windows package identity is Codex;
the product-mode controls, rather than the executable name alone, distinguish
the supported surface. The normal Edge ChatGPT page is anonymous. A temporary
disconnection of the Windows interactive session was confirmed using read-only
Win32 metadata; after the user unlocked Windows, that same session returned to
Active with the Default input desktop. No session switching or credentials were
needed. A subsequent read-only probe exposed an unsupported UIA visibility
property on the client's top-level window; conservative handling was verified by
a successful live read-only readiness probe. The interactive session subsequently
disconnected again. After reconnection, WTS extended session flags and an
uncloaked, full-screen LockApp window confirmed an actual locked desktop.
Live generation remains pending an unlocked, continuously connected desktop.
During the resumed acceptance, both authoritative stores retained one local run
paused before submission with an explicit native `submit_intent: false` and no
image or conversation binding. Fresh checks briefly confirmed an unlocked session;
the session then disconnected and locked before the explicit resume click. The
existing run and its original intent are preserved for continuation.
After the final reinstall, the local run still has explicit native
`submit_intent: false`, no binding or image, and zero resume receipts. The latest
entry checks continue to report `windows_locked`.

No real ChatGPT prompt has been sent in this acceptance run. Local fixtures
establish protocol behavior, not live generation or account acceptance.

One sparse real-account generation from each frontend is still required. The
final reviewed result will then be integrated into `main`; repository default
and Pages deployment policy will be switched to `main` as requested.
Private source images, account information and generated acceptance images remain
outside Git.
