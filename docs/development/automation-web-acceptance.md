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

## Resumed live preparation — 2026-11-04

Fresh Win32 checks confirmed session 1 was Active and unlocked, with the Default
input desktop. The final installed desktop still owned the localhost listener;
its executable identity, installer SHA-256 and successful Pages deployment still
matched the recorded `2f27ef9` candidate. The local WebUI reopened the original
project/task/version and paused run, reported `ready`, and issued zero automation
writes during preparation; the original intent and observer state were unchanged.

One explicit same-run resume was then recorded. Preparation failed before Send
with `PropertyNotSupportedException`; the coordinator and native store retained
the original run, and the native store still explicitly recorded
`submit_intent: false`. The resume receipt is preserved and is not replayed.
Subsequent bounded diagnostics found one outer application Document and two
nested Documents, which the adapter's whole-window document-count guard at that
time rejected. These observations do not prove the earlier property exception's
cause.

The user clarified that new generations must use an independent client window
and new conversation, preserving the existing draft. Installed public application
resources define a feature-gated `File > New Window`. Expanding the live File
menu did not expose that command, so no new window was opened and the original
draft was not changed. The public source does not expose a user setting or a
supported launch argument that bypasses this gate. New-window binding remains
unverified. The independent-window implementation is committed at `3b9e227` on
its isolated branch. It scopes the outer application Document, verifies the
renderer menu's fixed chrome anchors, polls asynchronous menu content after one
expansion, and pins the newly created window and fresh conversation before any
input. Original drafts and embedded-document controls are excluded; Save As
ownership follows the target window's owner chain. The final Windows .NET suite
passed 173 tests with zero failures or skips. Independent review confirmed that
the tested source matches the reviewed commit and resolved the menu, source-cache
and initial-window-loading findings. Updated installer acceptance remains
pending. No real prompt was sent and no live
image-generation acceptance is claimed.

## Reference UI and renewed installer acceptance — 2026-11-05

The new homepage and settings references, including the explicit minimize-icon
request, are implemented and independently reviewed. The shared UI centers real
task creation, retains its task sidebar and project menu, and provides searchable
settings categories. A review found that the mobile sidebar covered the saved
project picker; the fix passed a regression that first failed before the change,
then directly selected the saved project at 390 × 844. The integrated source is
`bd83b071be16c2abf352cc167514e2fe8f5d89f1`. See [design QA](../../design-qa.md).

The earlier native tokio compilation failure did not reproduce in a clean
diagnostic release build at `b95682f`; its original cause remains unknown. The
final `bd83b07` NSIS build then passed formatting, contract checks, all **173
Windows .NET tests**, frontend/assets/notices preparation and native packaging.
Its frozen installer SHA-256 is
`f312cd3d99e410afed7590153868b7aa584295860f9e46203e78e7065c298abe`.

Actual uninstall/reinstall cycles **9 and 10 each passed all 15 checks**. Both
applied the new installer. Real minimize/restore, maximize/restore, draft-close
protection, clean app/bridge exit, offline import/edit/export, IR restoration,
layout undo and diagnostics passed. All five pages rendered in both themes
without page errors. Root inspected the installed home and settings screenshots.
Cycle 9 preserved every hash of **928 project files and 32 local-WebUI files**;
cycle 10 preserved **1027 and 32**, respectively, before and after uninstall.
Fixture image checks remain public test images, not account-generated output.

Both installed executables have SHA-256
`aedcf29fcd52420d91ef8256cd3701800fb151b6b2bb1f450fa3cb06e6caf8d6`.
The frozen post-build executable differs only in the three-byte Tauri bundle
marker: the bundler applies the NSIS marker during packaging and restores the
original afterward ([Tauri CLI 2.12.4 source](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.4/crates/tauri-bundler/src/bundle.rs#L212)). A complete binary comparison, matching upstream source,
generated NSIS input and separate installed-payload copy are preserved in the
private `artifacts/release-resumed-bd83b07` evidence. The original build receipt
and artifacts were retained unchanged.

[Pages run 41862954637](https://github.com/dieWehmut/Entwurf/actions/runs/41862954637)
successfully built and deployed the same source. Its fresh **99 frontend tests**,
**12 actual browser cases** and WASM smoke checks passed. The deployed JavaScript
exactly matches the reviewed production preview by SHA-256. The original Pages
browser profile retained its project, task and r1 knowledge version, and reached
the final installed local service without network failures or generation requests.

The local WebUI reopened and refreshed its original SQLite task and paused run,
retaining the same version, prompt hash and byte-identical intent/observer files.
Preparation issued zero automation writes. The desktop also reopened its original
public-source task. Reinstall fixtures had created newer global-style snapshots,
so its never-submitted preparation metadata was refreshed from r9 to r13 only
after proving identical semantic knowledge and project-style hashes, and absence
of any native submission intent or task in either run store. The prior metadata
was backed up; no existing run was rebound. Native preparation created no intent
or generation request.

## Windows tray follow-up — 2026-11-05

Windows now registers an Entwurf notification icon at startup. Minimize hides
the main window; clicking the icon restores it. The menu provides Show Window
and Exit, with Exit routed through the existing draft/busy close guard.

Installer source `f3f695e643526f6be6e25fd929d1d9f9e424b77b` passed **3 Rust
tray-action tests and 173 .NET tests**, then the full Windows/NSIS build.
Installer SHA-256 is
`d0931ec38ba6dc050a940b5c338517ee15be0f0a2f368937d8daff116a1ab0d8`.
The installed payload SHA-256 is
`63584a44aaaa0afdc4417bf47b53002eaea000cee0519e3c819ae82ff47b48d2`.
Its only difference from the frozen post-build executable is the same three-byte
Tauri NSIS marker described above; the complete comparison is retained privately.

Acceptance harness `83a8f7a51dfa7e56a26b3a465e627682487042b4` differs from the
installer source only in `scripts/windows/tray-acceptance.ps1`. Runtime source is
unchanged. The harness locates actual Shell controls, uses batched native pointer
input, and reads the owned Win32 popup menu when UIA exposes no menu items.
An installed diagnostic run verified actual tray restore and menu Exit, including
app/bridge shutdown. UIA Invoke alone was not accepted as proof of clicking the
notification icon.

The final install's cycle 11 passed **7 complete checks**, including normal and
maximized minimize/hide/real-tray restore, local service availability, offline
operation, restored edits and native import. Uninstall preserved all hashes of
**1130 project files and 34 local-WebUI files**. Windows disconnected and locked
during the subsequent draft/tray Exit check. That check and the rest of cycle 11,
cycle 12, and the normal tray handoff remain incomplete. Failed/interrupted
evidence is preserved; no complete reinstall acceptance is claimed for this build.

The automation subagent also reran **8 IndexedDB and duplicate-submission guard
tests**, all passing with zero live or generation requests. The original run still
has `submit_intent: false`; these tests do not establish real image generation.

## Locked-session background automation — 2026-11-10

The resumed objective is automation in a logged-on Windows session that remains
locked. The browser extension now prepares each explicit generation in a fresh,
unfocused window in the existing browser profile. It preserves the user's
conversation and draft. At this stage, the Windows image adapter still required
an unlocked foreground desktop; the later direct-native change below removes
that adapter guard.

Source `adaec41` makes browser automation the fresh-install default and migrates
idle v1 installations once. An unfinished native run keeps its provider, tokens,
identity and retained image so its original adapter can still recover and ACK it.
Explicit v2 provider choices remain durable. Restart and migration submit nothing.
All 31 real localhost coordinator tests and 17 HTTP/access tests pass on Linux.
The bundled-native fresh-install case also passed on native
`x86_64-pc-windows-msvc` with Rust 1.99.1, one exact test and cargo exit 0. It used
a clean independent Windows checkout at `b3a22d4`, whose relevant Rust source is
unchanged in the current integration. Root inspected its final receipt and test
log; no installation or real generation was performed.

Source `adb9dbe` holds a run-scoped Web Lock before preparing the task composer,
disables automatic task-tab discarding, sends independent WebSocket keepalives
every 20 seconds, and uses a one-minute reconnect alarm compatible with Chrome
116. A frozen original tab permits preparation in the new window; its live login
controls, empty composer and run binding must pass before the only Send click.
Read-only observation retries bounded page-response timeouts against the same
run. A durable ten-minute deadline stops an unresponsive run without replaying
Send or Download. ACK, cancellation and failed preparation release the matching
page lock; an orphaned lock expires after fifteen minutes.

The focused-window change alone would not meet this objective: Chromium marks
all Windows browser windows occluded when the session is locked, including their
active tabs. The Web Lock addresses Chromium's automatic freezing policy; page
timers may still be throttled. Sleep, logout, browser exit, expired login and
manually freezing the task page remain interruption conditions. These distinctions follow
the [Chromium Windows occlusion design](https://chromium.googlesource.com/chromium/src/+/main/docs/windows_native_window_occlusion_tracking.md),
[Web Lock freezing policy](https://chromium.googlesource.com/chromium/src/+/main/components/performance_manager/freezing/freezing_policy.cc)
and [Chrome 116 WebSocket worker guidance](https://developer.chrome.com/docs/extensions/how-to/web-platform/websockets).

All 49 extension regressions pass, including previously failing hung-probe,
transport-heartbeat, delayed-observation and worker-deadline cases. Source
`b3a22d4` adds Chinese connection instructions and status messages. All 99 frontend
tests, TypeScript checking and the production frontend build pass after restoring
the frozen dependency set. The distributed companion ZIP includes the new module
and alarm permission; its bundled source bytes were compared with the worktree.
The real HTTP/SQLite frontend scenario also passed automatic image import, durable
ACK, reload, identical original download, stale-version rejection and cancellation.
The three HTTP runtime scenarios pass after `62687ee` updates the test to open the
actual folded project-management form before creating a cloned-window project.

The reviewed runtime and UI source is integrated into `main`. The repository
default, the GitHub Pages environment's allowed deployment branch and the Pages
site's stored source branch are all `main`; its build type remains `workflow`.
[Pages run 42741441690](https://github.com/dieWehmut/Entwurf/actions/runs/42741441690)
passed the real WASM core, 99 frontend tests and browser gates, then failed at
publication: the backend reported `syncing_files` until deploy-pages reached its
ten-minute default and cancelled the deployment. No build or test failure was
reported. The already queued [97442ba run 42742926085](https://github.com/dieWehmut/Entwurf/actions/runs/42742926085)
is being observed through its original live job, without restarting it.

That second run also passed all build gates, then reached the same ten-minute
publication timeout while `syncing_files`. Its failed deployment receipt is
preserved. The existing [df9edbf run 42745114897](https://github.com/dieWehmut/Entwurf/actions/runs/42745114897)
contains the extended bounded wait and completed deployment successfully.

The workflow now allows thirty minutes for the deployment action within a
thirty-five-minute job. This changes only the bounded wait for the server result;
all build/test gates, error handling, permissions and environment rules remain.
The setting was checked against the official action's millisecond input and
independently parsed as YAML. Actual deployment success was subsequently verified
for run 42745114897 at `df9edbf`; the longer timeout alone was not counted as proof.
An isolated browser visited the public Pages site and passed Chinese rendering,
background/locked-session setting lookup, unpaired Start rejection, and project
and task persistence after reload. It reported no console/page errors or external
requests. The downloaded companion ZIP contained 16 shipped files, every one
matching the current canonical source bytes, including Web Lock and durable
generation-deadline code. Its SHA256 is
`30fcf9688a76e5912e024633a4405cae1895ee94e59ab71180ef9f7f4d8b052c`.
Root inspected the successful-run, browser and complete ZIP receipts. This is
public-site verification, not real-account generation acceptance.

A clean native Windows checkout at `97442ba` also built the NSIS candidate with
cargo exit 0 after 173 native adapter tests and 99 frontend tests passed. The
79,661,637-byte installer, post-build desktop executable, companion ZIP, canonical
Git source archive and packaged resources were frozen. Root independently
verified all 313 frozen file hashes, including 299 resources. Packaged companion
bytes match the clean native checkout; comparison with the Linux checkout
normalizes native CRLF line endings. The installer and desktop executable were
not launched against installed application data.

The same native source built the Windows web CLI with cargo exit 0. An explicit
fresh `artifacts` data directory and non-48123 port verified Windows/local-HTTP,
SQLite file bytes, served frontend and companion ZIP, browser as the default
provider despite a bundled native adapter, and another HTTP 200 after 35 seconds
in the locked session. No UIA adapter child or automation run was started. The
owned service subsequently stopped through its own console Ctrl+C handler with
exit 0 and its port released; existing application data was unchanged.

The final native locked-browser fixture passed on headed Chromium 156. It starts
after the original fixture tab is frozen, then submits once in its owned window.
The task's `chrome://discards` row reports `occluded`, `hidden` and `holding Web
Lock` as a CannotFreeze reason. Neither the task nor its worker has a debugger
attached, and no background-throttling bypass flag is used. The browser's actual
PID is bound to disconnected session 1; all 18 WTS snapshots report locked,
including 17 browser-bound samples taken about every five seconds. The input
desktop remains inaccessible with Win32 error 5 in those samples.

After **68,733 ms**, the extension delivered identical public-fixture original
bytes. The protocol fixture saved and independently reread them before ACK.
Three transport keepalives arrived, duplicate Start produced no second submit,
ACK removed the owned task tab and released its Web Lock, the original draft
remained exact, and the owned browser's exit was verified. Root inspected the
final `locked-browser-20261110-7/receipt.json` and its task-row diagnostic, and
compared every loaded runtime module with the current committed source hashes.
Earlier failed harness attempts remain preserved; they are not counted as passes.

The scripts use a dedicated new browser profile, an ephemeral localhost HTTPS/WS
fixture and a certificate exception scoped to the test certificate's public key.
Other hostnames are blocked; existing browser profiles, installed application
data and machine hosts/trust settings are unchanged. No actual ChatGPT request is
made. This proves the recorded locked-browser/localhost-protocol run, not live
ChatGPT selectors or all Edge sleep policies. Real Rust/frontend import evidence
and live-account acceptance remain separate gates.

The added optional real-backend fixture starts from the actual local WebUI and
detaches its debugger after coordinator acceptance. Its initial native run
reached download while locked but failed at extension IndexedDB initialization,
before an artifact reached Rust. A controlled same-browser/extension comparison
passed IndexedDB open/put/get/remove in a short profile and failed with
`UnknownError` in a long profile: the latter's SQLite database path would be 267
characters, beyond the traditional Windows path limit. The matching
[Chromium 156.0.8078.4 implementation](https://github.com/chromium/chromium/blob/156.0.8078.4/content/browser/indexed_db/file_path_util.cc#L119)
uses an origin directory and a 52-character database filename. The fixture now
checks journal-path room before launching Chromium; the failed run and both
diagnostic receipts are retained. This was a test-directory failure, not evidence
of a Rust import failure or a frontend freeze.

The new short-directory integration run passed **69,044 ms** of locked generation
using the real Windows backend and WebUI. The WebUI automatically imported and
ACKed the image. Before forwarding ACK to the extension, the fixture reopened
the SQLite project, verified original image bytes on disk and through the real
API, and checked that the knowledge version was unchanged. All 18 WTS samples
reported locked. Three keepalives arrived; duplicate Start produced one submit,
ACK released the task Web Lock and closed its tab, the original draft remained
exact, and the owned Chromium exit was verified. Root inspected the final
`artifacts/li65-2/receipt.json` and verified loaded companion module hashes against
the clean native source and canonical Git content. There were no fixture errors
or external requests.

The final six-minute integration fixture also passed, with **363,668 ms** of
locked generation and 18 keepalives. All 77 WTS samples reported locked with an
inaccessible input desktop. It used new short data/profile directories and the
updated path guard. The real WebUI imported and ACKed automatically; reopened
SQLite data and exact original disk bytes matched, while the knowledge version
remained unchanged. The owned task target disappeared and the page Web Lock was
released. Chromium exited with code 0, no signal and no forced termination; the
owned native web CLI then handled Ctrl+C, exited 0 and released its port. Root
inspected `artifacts/li360-3/receipt.json`, its loaded runtime and harness hashes,
original disk bytes and final service-stop receipt. WTS evidence consists of
repeated samples, not continuous observation. These public fixtures still do not
verify real ChatGPT login, live selectors or every Edge sleeping-tab policy;
live-account acceptance remains open.

## Direct native ChatGPT control — 2026-11-10

The image executor now uses UIA ValuePattern, InvokePattern and ExpandCollapse
directly. `WindowsImageSurface` no longer checks input-desktop unlock or requires
foreground focus, and it does not call SetFocus/SetForegroundWindow or inject
keyboard/mouse input. Every mutation remains bound to the trusted package, exact
HWND, PID and process start time, with identity checked before and after the
operation. Visible/enabled controls and supported patterns, the unique independent
New Window menu command, fresh conversation, prompt identity, submission intent
and the bound Save As owner chain remain required. The separate structured
`TaskEnvelope` entrypoint's `WindowsAutomation.CheckSafety()` retains its unlocked
input-desktop and foreground guard.

Public installed-source inspection of
`OpenAI.Codex_26.1103.4239.0_x64__2p2nqsd0c76g0` found the New Window callback
calling `createFreshWindow('/')`. File-menu insertion depends on `multiWindow`,
supplied by renderer feature gate `459748632`; production defaults it to false
until renderer feature state arrives. The shipped command is source evidence,
not proof that the current signed-in client's gate is enabled. An earlier live
menu inspection found the command absent; the current version needs its own
live menu and independent-window check. An earlier observation on 2026-11-10
found the session unlocked and cannot support a locked-session claim.

The later native launch check observed session 1 disconnected and locked in WTS
before and after the operation, with an inaccessible input desktop. One standard
main-AUMID launch started ChatGPT PID `51604` at
`2026-11-10T06:19:18.9728400Z`; five trusted ChatGPT processes belonged to that
same session. About 0.6 seconds after startup, the checked process had no window
handle. An isolated bridge WebSocket authentication/probe check used empty run
state and exited normally with code 0, but reported unavailable with
`chatgpt_not_found`. This establishes application-process startup and the bridge
control channel while locked, not an available target window, successful UIA
operation, independent-window creation or generation. A later bounded read-only
poll found HWND `77534922`. Further inspection identified it as a native `#32770`
startup dialog titled Codex. Raw/control/content trees had 11/8/8 nodes, no
Document or Chromium renderer host, and no recognized ChatGPT/Images/File control.
The production UIA3 probe with the final DLL
`01493f9da83bf9df5a0a0bd6a00e95001dcd0f5c1600a1aba69c7cb505e0643b`
reported `image_surface_unavailable` before and after a standard read-only MSAA
query, with normal bridge exits and empty owned run state. The eight-node tree
does not establish any renderer accessibility or hidden-page condition.

The startup dialog reported that organization settings could not be loaded
safely. The public bootstrap code uses this generic message for home creation,
preparation, policy reading, policy validation and node-permission failures.
Its mention of an IT administrator does not establish elevation: the app, native
probe and existing Explorer all had the same user, medium integrity and a filtered
token. A public unauthenticated HEAD returned 200 from chatgpt.com and reset at
auth.openai.com; system ProxyEnable was true. These observations do not identify
the failing startup phase or prove that the application helper used that proxy.

At `2026-11-10T06:51:40Z`, a unique visible enabled native Retry button was bound
to its HWND, parent dialog, trusted PID and process lifetime, then invoked once
through the standard MSAA default action. The pre-action receipt and completion
record show one completed call while session 1 was disconnected and locked. The
old dialog disappeared. A 104-second bounded read-only observation found the
same application alive but later displaying a new `#32770` dialog, HWND
`95819466`, with the same organization-settings error. There was no second Retry,
application relaunch, foreground operation or selection of Sign out or Quit.
This establishes one actual native application control operation while locked;
it does not establish ChatGPT page input, independent-window creation or image
generation. The vendor bootstrap may itself perform authentication recovery;
manual button selection does not establish its internal account state.

The exact fixed startup logger records for this app PID, at
`2026-11-10T06:20:22.295Z` and after Retry at `2026-11-10T06:52:12.531Z`, both
reported `phase=policy-read reason=process attempt=1 retrying=false`. Only the
four documented safe tags were extracted. This narrows the failure to the
startup-helper process path; it does not identify an OS error code, selected
executable, security-policy denial or proxy failure. Further diagnosis is needed.

The new self-contained `win-x64` bridge publish and production/full-test-project
cross-compilation passed with zero warnings or errors. A source-linked Linux
project passed 45 pure tests. The subsequent complete Windows runtime suite ran on an exact 40-file tracked
source snapshot of main `96e4530b7022f892563a8346cd778241d300ac49` using native
SDK 10.0.403. Offline restore, Release build and all 184 tests passed with zero
failed, skipped or aborted tests; the test process exited 0 and the TRX counters
were independently checked. These automated tests do not establish ChatGPT page
control, real image generation or original-image retrieval while locked.

The frontend keeps browser as the default provider and offers the native client
in connection settings. Its current guidance no longer requires unlocking or
bringing the client to the foreground. Legacy `windows_locked` and
`foreground_unavailable` reasons direct users to update the local companion and
reconnect. Existing paused runs and submission intents are retained; neither an
update nor reconnect authorizes replay.

## Unlocked native live preflight — 2026-11-12

The resumed local test observed Windows session 1 connected and unlocked at
`2026-11-12T05:35:03Z`: WTS flags were 1 and the default input desktop was
accessible. The installed provider package had updated to
`OpenAI.Codex_26.1103.7773.0_x64__2p2nqsd0c76g0`. Initially its processes had no
visible eligible window and Entwurf's service was stopped. Standard installed-app
and main-AUMID launches restored the actual localhost service and one trusted
source window, HWND `264044`, PID `22512`, started at
`2026-11-12T04:36:31.2500520Z`.

The source exposed 75 UIA nodes, global Images navigation and the four anchored
application menu controls. Its current home contained work/project/permission
controls and a coding-model picker; the fixed ChatGPT mode selector and ChatGPT
model picker were absent. The real installed adapter connected, but its
read-only status reported `chatgpt_mode_required`. The page title alone was not
accepted as proof of ChatGPT image mode.

A separate scoped UIA diagnostic, outside WindowsImageSurface preparation,
expanded the native File menu and exposed exactly one visible enabled
`新建窗口` command with InvokePattern. One scoped Invoke at
`2026-11-12T05:39:38Z` created HWND `6096260` in the same PID and process lifetime.
The diagnostic subsequently exited 1 because its menu Collapse cleanup found an
invalid menu state; it did not repeat Invoke. The subsequent window enumeration,
not the helper exit code, establishes that creation occurred. Production menu
cleanup already catches this provider failure.
The original source window remained present. This closes the earlier uncertainty
about the live New Window command on this coding/work home. The production
adapter still refuses this source before window creation because HasMode is
absent; neither ChatGPT source nor fresh-page acceptance passed. In the diagnostic,
the new window retained only 15–16 UIA nodes, an outer Document and empty root,
with no Edit, Images navigation or composer. A window-only diagnostic capture
also showed the blank page. The same bound HWND still had no Edit at
`2026-11-12T05:44:03Z`. One diagnostic SetForegroundWindow call returned false
and did not select the window; this is not background or foreground generation
acceptance.

Public installed-source review confirms that `createFreshWindow('/')` still
creates a real window and calls the renderer loader. The old mode-button string
also remains, with alternate component branches depending on home/work state;
its absence here does not establish a renamed or removed selector. Application
`Reload Window` is restricted to non-production builds; browser-page reload
targets an existing embedded page, and no usable page-reload target was validated
in this root-only window.
Static code and the root-only capture do not identify the renderer failure cause.

The installed local WebUI returned HTTP 200 and listed the existing public
acceptance project. Its actual task view displayed the original r1, the paused run
and no generated image. With the diagnostic window also open, it reported
`ambiguous_chatgpt_windows`, as required for multiple eligible sources. The same
coordinator/native run retained its run, task and
base-version IDs, `paused` phase and native `submit_intent: false`; its prior
resume request history was unchanged. Coordinator original-image and native
staged-image counts were both zero. No Start, resume, Send or prompt edit was
requested during this preflight. The new window remains a preparation diagnostic,
not a generated artifact. Private receipts and captures are retained under
`artifacts/live-native-20261112` in the task checkouts and excluded from Git.

The immediate live gates are a fully loaded independent ChatGPT/Images page and
validated current controls. Real prompt submission, response correlation,
original Save As, SQLite import/ACK, all three frontends and native locked-session
acceptance remain pending. Unlocking resolved the session condition; it did not
establish a usable image composer.

## Native composer compatibility follow-up — 2026-11-12

A scoped maximize/restore diagnostic and Images navigation eventually exposed a
loaded Images page in the already-owned HWND `6096260`. A template preview dialog
was dismissed on that window. These diagnostics do not establish a renderer
startup fix or locked-session acceptance.

UIA3 reads the empty ProseMirror composer as its accessible hint plus LF. A raw
five-node subtree independently exposes the genuinely empty paragraph's
`placeholder` class and `ProseMirror-trailingBreak`. The public renderer installs
that paragraph marker only for a single empty paragraph; its CSS generates the
hint. The adapter now recognizes this exact structure. Identical user text,
whitespace drafts, extra content, missing markers and ambiguous trees remain
unchanged. The native Windows suite passed **201/201** after this change.

A bounded UIA3 capture of the real Images page showed the model picker inside one
unnamed Group directly under the composer's row. The page matcher now accepts
that observed wrapper, with negative tests for unrelated/deeper/hidden wrappers
and multiple pickers. At `2026-11-12T07:36:29Z`, the updated reader returned an empty
composer and the actual page passed `NativeImagePage.IsHome`; the original source
input hash and foreground HWND were unchanged.

Preparation now accepts an already-verified Images source without the optional
ChatGPT mode button. A fresh same-process HWND must expose one actionable Images
button and a unique truly empty writable Edit before navigation; genuine Images
Home is still required before prompt input. With another app foreground, source
selection can use only one verified product surface. Multiple valid surfaces,
unreadable alternatives and changed cached identities do not authorize fallback.
The complete native Windows suite passed **232/232**, with zero skipped tests.

The direct no-submit input preflight at `2026-11-12T07:37:14Z` failed
`user_changed_composer`: after ValuePattern SetValue, the immediate read still
returned the empty hint. A later read at `07:39:46Z` also found that hint and no
preflight marker. The original source's raw input hash remained unchanged. The
foreground HWND changed during this diagnostic; its cause was not established.
Both top-level ChatGPT windows were later observed minimized, despite the
composer reporting visible. A diagnostic `SW_SHOWNOACTIVATE` restored only the
owned window at `07:47:33Z`; the foreground HWND was unchanged and the original
source remained minimized. At `07:49:31Z`, the original preflight marker appeared
with its exact SHA-256. This proves the earlier empty observations did not exclude
a queued write. Scoped cleanup of that exact marker positively verified the
empty-paragraph marker at `07:51:58Z`; the original source input hash remained
unchanged. A cleanup operation changed the foreground HWND, so no focus-preserving
input/cleanup acceptance is claimed.

The adapter now refuses pattern mutations on minimized windows, waits for exact
prompt acknowledgement and an enabled Send control, and verifies empty state
after clearing its exact owned prompt. A pre-submit failure already retains its
owned window/document binding and is not eligible for retry. The binding now
records whether draft writing or clearing remains unconfirmed, and the session
persists the final cleanup result. The cleanup cancellation budget permits the
full native acknowledgement polling interval. Minimization after a dispatched
request can still defer the external renderer; this remains an uncertain bound
operation rather than proof of a clean or retryable run.

A current read-only check at `2026-11-12T09:01:15Z` found the same bound Images
window restored, with an empty canonical composer and the original source's
unchanged input hash. One no-submit production write/clear preflight at
`09:02:57Z` acknowledged the exact new marker and then positively verified the
empty composer; the final exported binding recorded `draft_cleanup_state:
cleared`. The helper exited 0. Its foreground queries returned zero before and
after, so these samples do not establish focus-preserving execution.

Review found that an abrupt exit during prompt acknowledgement could occur
before failure handling saved the binding. The executor now saves the write
stage and prepared binding before dispatch. Historical interrupted write/submit
stages also refuse pre-submit retry even without a binding. Cleanup returns
without UI access if this conversation never dispatched its prompt write,
preserving a user's draft that happens to equal the planned prompt. The complete
Windows suite on the matching source snapshot passed **246/246**, with zero
failed or skipped tests and process exit 0; the new cases inspect an in-flight
disk snapshot before failure/cleanup and reopen that state for restart checks.

The current production Probe/Prepare preflight at `09:04:25Z` reported `ready`,
created a third same-process window, HWND `30736958`, and ended at `09:05:07Z`
with `image_surface_unavailable`. Subsequent read-only inspection found 15 nodes
and no writable composer in that new window. No prompt write or Submit was
called by preparation. The earlier successful input preflight on an already
loaded diagnostic window does not close this fresh-window preparation gate.

A separate public Probe/Prepare call reported `ready` but then failed
`independent_window_unavailable` before binding another window. Neither helper
called Submit, and the production run remains paused without submission intent.
These are preparation/input diagnostics, not generated-image acceptance.

Private source receipts, TRX reports and native preflight records are retained in
`artifacts/prosemirror-fix-20261112` and `artifacts/compatibility-fix-20261112` in
the Windows task checkout, with the later checks under
`artifacts/continuation-20261112`, and excluded from Git. Original generation/download,
SQLite import/ACK, all three frontends and locked-session acceptance remain pending.

## Native generated-response follow-up — 2026-11-12

The fresh blank-window layout change passed **271/271** native Windows tests and
was merged through PR #6 at `5a388b9`. It refreshes only the independently created
empty HWND once, preserves activation/Z order and restores the exact bounds.
Fresh production preparation remains a separate live gate.

At `09:23:27Z`, one dedicated native diagnostic submitted the sparse Bayes-diagram
prompt once. The owned window displayed the generated diagram, with a retained
private window capture. Its initial automatic observer failed `conversation_changed`
because it bound a temporary prompt Text control before the submitted turn settled.
This diagnostic is distinct from the original paused coordinator run and does not
establish local WebUI, Pages or desktop import/ACK acceptance.

The response matcher now excludes composer copies, binds the exact prompt hash
inside a new real h4 user heading and retains that heading while the prompt Text
ID changes. The generated numbered Button/Image pair is accepted only with exact
matching bounded labels and an immediate parent relationship inside its assistant
fragment. Generic `下载` / `Download` is accepted only inside the exact newly
opened, named image preview; it cannot become a transcript download action.
Recovery also refuses to replace an already saved image/action/assistant response.
These changes at `c1b8890` passed **335/335** native Windows tests, zero failed or
skipped, exit 0, on a source-hash-verified private snapshot.

At `12:42:41Z`, read-only inspection found that the same diagnostic window had
navigated to a home page with neither the submitted prompt nor generated-image
control. The exact previously captured title `Create Bayes Diagram` still existed
as one actionable chat entry. One no-submit recovery opened that entry at
`13:09:09Z`; the updated production response matcher positively verified the
existing submitted turn, assistant fragment and generated image. The adapter
opened its exact preview and invoked its scoped download. It then ended with
`download_dialog_unavailable` at `13:09:49Z`: no owned standard Save As dialog was
verified. The original source input hash was identical before and after. No new
prompt write or submission was performed by recovery. Later native enumeration
found the diagnostic HWND absent; its cause was not established. No original
image bytes, import, ACK or successful download are claimed.

A separate current production Probe/Prepare reported `ready` at `12:50:51Z`, then
failed with `PropertyNotSupportedException` before returning a prepared binding.
It performed no prompt write or Send. Private generation, recovery, source receipts
and TRX results are retained in `artifacts/continuation-20261112` and
`artifacts/response-fix-20261112` in the Windows task checkout, outside Git.
Original Save As, complete fresh-window preparation, coordinated original-byte
import/ACK in all three frontends and actual native locked/background execution
remain required. The diagnostic generation and fixture tests do not close them.

## Native runtime-identity and fresh-window verification — 2026-11-12

A production preparation attempt at `15:08:08Z` successfully selected the owned
Images source and created a new same-process window, but failed while reading a
new node's RuntimeId property. The private stack identified the property access
in frame capture. Treating absent identity as unavailable allowed a bounded wait
without omitting nodes or inventing IDs; that intermediate attempt still timed
out at `15:14:33Z`, preserving the original source input hash.

The reader now preserves an available RuntimeId property and otherwise uses the
UIA client's native `GetRuntimeId()` method. Neither path returning an identity
still stops capture. A read-only comparison at `15:37:47Z` inspected 24 nodes in
each of four existing windows: property and native IDs were identical on every
observed node, including the source's unchanged outer Document identity. This
comparison establishes compatibility of the two paths on those loaded nodes; it
does not prove every loading provider exposes the fallback.

The final `0d21e8d` source passed **342/342** native Windows tests, zero failed or
skipped, exit 0, and self-contained win-x64 publishing exited 0. Code/source hashes
match the retained private test receipt. The added checks preserve existing IDs,
require a real nonempty native fallback, refuse absent IDs and stop without
navigation replay if identity becomes unavailable after document binding.

At `15:47:46Z`, a no-submit helper reused the exact owned Images source selected
by the prior production probe. Its diagnostic setup seeded that already-verified
source identity; it does not establish cold-start source disambiguation among all
the now-open diagnostic windows. The updated public `PrepareAsync` created a new
same-process HWND `36308392`, verified its fresh empty page, navigated Images and
returned its unique truly empty composer at `15:48:21Z`. The binding remained
`draft_cleanup_state: not_started`, without prompt write or submit intent; the
original source input hash was unchanged. This proves the complete fresh-window
preparation sequence on that source and client version.

The prepared diagnostic window remains separate from both the original paused
coordinator run and the earlier real Bayes-diagram diagnostic. Original Save As
and original-byte retrieval, coordinated frontend import/ACK and actual native
locked/background generation remain unverified. Private receipt, comparison,
TRX, publish and prepared binding files remain under
`artifacts/runtime-id-fix-20261112` and `artifacts/runtime-id-native-fix-20261112`
in the Windows task checkout and are excluded from Git.

## Native Save As lifecycle follow-up — 2026-11-13

The save loop previously treated a still-visible dialog on the first observation
following Save as `download_failed`, before checking whether the owned file was
ready. UIA Invoke acknowledges dispatch rather than completion. The loop now pins
the first owned dialog identity, waits for its unique filename and enabled Save
controls, dispatches Save once and continues checking the new owned destination.
It refuses replacement dialogs before or after dispatch, ambiguous controls and
cancellation. An existing owned Save As is checked before opening the image and
again immediately before the scoped preview Download invocation. Filename and
Save controls are revalidated before changing the filename; the owner-chain,
ordinary-path, size and exclusive-file-access requirements remain intact.

The final source snapshot passed **352/352** native Windows tests, zero failed or
skipped, exit 0, and self-contained win-x64 publishing exited 0. All 53 retained
source/project hashes match the tested snapshot. Ten source-linked lifecycle tests
also passed on Linux, including a real exclusive writer that releases the owned
file later, delayed control readiness, replacement dialogs, timeout and
cancellation. These are implementation tests; no test asserts a successful live
ChatGPT original download. Private source receipts, TRX and publish files are
retained under `artifacts/save-dialog-final-20261113` in the Windows task checkout.

Read-only public-package inspection of
`OpenAI.Codex_26.1103.7773.0_x64__2p2nqsd0c76g0` found that the image save service
calls `showSaveDialog` with options alone, without a BrowserWindow argument.
The shipped main asset is `.vite/build/early-bootstrap-480-5def667ddfcd786d.js`,
SHA-256 `be8eba3b9283d6a9784fa069a019b83a89fd02277cc13e4f7d32d57faf0ada65`;
`selectSavePath` and this call occur at byte offsets 3115980 and 3116105. This is
source evidence for investigating dialog ownership, not proof of an observed
native owner chain or the cause of the earlier missing Save As. The adapter
continues to require an owner chain reaching the exact bound conversation HWND.

At `2026-11-12T17:29:00Z`, the live Windows session was connected and unlocked,
with the default input desktop accessible. At `17:35:47Z`, read-only UIA capture
confirmed that the prepared owned HWND `36308392` retained its exact Document
identity and truly empty Images composer. The known Bayes chat title was absent
from that document, including offscreen Button nodes. Its fixed Home button
exposed ExpandCollapse rather than Invoke; a prior attempted Invoke was refused
before dispatch because the pattern was absent. No prompt was written or sent.
These diagnostics do not supply original image bytes or frontend import/ACK.

The same owned window's Home and sidebar controls were expanded through their
advertised UIA patterns at `17:37:12Z` and `17:40:48Z`. The known chat title then
appeared offscreen; its exact ScrollItem action made it visible at `17:42:56Z`.
Opening that exact existing title performed no prompt write or send. At
`17:48:43Z`, a no-submit observer revalidated the original diagnostic prompt hash,
its level-four user heading and its generated image in the owned window. This
new diagnostic binding is not a coordinator rebind or frontend ACK. Opening its
exact preview then failed during whole-frame capture with
`UIA_E_ELEMENTNOTAVAILABLE` (`0x80040201`), before the scoped Download invocation
or a Save As observation. The receipt identifies the preview-wait capture call;
no partial UI frame was accepted. Original bytes remain undelivered.

## Native stale-frame capture follow-up — 2026-11-13

The proven preview-load failure called a property on an element that had been
removed (`UIA_E_ELEMENTNOTAVAILABLE`, `0x80040201`). Capture now retries only that
direct COM error. Every attempt starts from the bound HWND and rebuilds its entire
node/element tree. No incomplete frame is returned. A complete frame revalidates
the trusted process lifetime, exact bound Document and login state before any
subsequent action. Other HRESULTs, wrapped errors, missing runtime identity and
cancellation remain fatal. Invoke and SetValue stay outside the retry callback.

The helper allows at most four reads and waits at most 100 ms between reads,
within a three-second window for starting retries. The existing synchronous
provider/capture limits remain observable; a valid first read is not rejected
merely because it finished after the retry-start window. Generation and preview
pass their remaining phase time, and discarding a frame resets the two-second
image stability observation. Preview, Download and Save are never reinvoked by
capture retries.

Source `94609b3` passed **369/369** native Windows tests, zero failed or skipped,
exit 0, and self-contained win-x64 publishing exited 0. All 55 retained source
hashes match the tested snapshot. Seventeen source-linked tests also passed on
Linux, covering full reconstruction, fatal errors, attempt/deadline bounds,
cancellation, stability reset, changed bindings and no action replay. Private
receipts/TRX/publish files are retained under `artifacts/stale-frame-fix-20261113`
in the Windows task checkout.

The known owned preview remained visible in a read-only capture at
`2026-11-12T17:54:29Z`. Later recovery launches encountered WSL transport timeouts.
A native process inspection at `18:22:20Z` found no process for that specific
recovery job. The next launch ended at compilation, before creating its action
claim; a diagnostic Windows path literal was corrected. Its subsequent launch
and process-observation attempts again timed out, without a new receipt. Those
transport failures do not prove whether the corrected job started or became
terminal. No additional prompt was written or sent, and original-byte delivery
and frontend ACK remain unverified.

PR #9 was merged to main `a521f14`, and its exact Pages workflow
[43274450200](https://github.com/dieWehmut/Entwurf/actions/runs/43274450200)
completed successfully. That deployment check does not replace native-account
acceptance, and the subsequent stale-frame source still requires integration.

## Native viewer recovery and merged release — 2026-11-13

Source `7c318d9` passed **388/388** native Windows tests; source `1f4456d`
passed **430/430**, both with zero failures or skipped tests and test/publish
exit 0. The viewer-close repair recognizes `关闭查看器` / `Close viewer` only
inside the bound preview. After one close dispatch, bounded read-only polling
waits for that preview to disappear and the original prompt and user heading to
return. Replacement previews, changed evidence, cancellation and timeout never
replay the close action. These tests do not prove a live original download.

At `2026-11-12T21:16:44Z`, no-submit recovery had reverified the existing real
diagnostic response and invoked its scoped original Download. A standard Save As
dialog appeared in the trusted ChatGPT process with no native owner chain to the
bound conversation. The adapter returned `download_dialog_unavailable` without
writing a filename or invoking Save; no original bytes or frontend ACK were
obtained. The exact-window ownership requirement remains unsatisfied. The old
Entwurf then exited through its normal CloseRequested path. At `21:33:48Z`, a
native read-only check found no Entwurf application/adapter processes or local
service listener, with the workspace, paused coordinator and native journal
bytes unchanged.

PRs #12 and #13 merged the viewer repairs into main `81117d5`. Its native Windows
x64 NSIS build completed at `2026-11-12T22:01:13Z`, preserving tracked-source and
frozen-resource hashes before and after the build. The retained production code
matches the 430-test snapshot; the source difference is automation documentation.
The candidate installer is **79,708,837 bytes**, SHA-256
`7ea7687951608fef38700bd0838b1208b0d4c933453d73a9505d61f93a5df77b`.
The build reused the tested 275-file bridge publication. Its manifest explicitly
records `installed: false` and `locked_generation_validated: false`; installation
and real locked-session delivery require separate proof.

Linux inspection of that exact installer completed with extraction exit 0 and
without executing Windows code. All 306 packaged resources match the sealed
source bytes, including all 275 bridge files from the tested publication. The
desktop payload differs from the raw build only at the expected three-byte
`UNK` to `NSS` installer marker; its SHA-256 is
`a96a899275546b577b9be77f39a9e46bbe71f951eb5483da84a3e2bdecefd210`.
The solid-archive listing estimates the StartMenu plugin size; the extracted
plugin was checked against the exact native Unicode plugin bytes instead.
Ten metadata/refusal checks passed. This inspection proves package contents,
not installation, startup or original-image delivery.

## Native cold backup and deployed-site smoke — 2026-11-13

At `2026-11-12T22:51:21Z`, the native cold backup completed with exit 0.
It retained and reread all 308 installed files, 40 application-owned files and
1,135 project files, plus the independently pinned rollback installer: **608,311,240
bytes** total. The recovery ACL is protected and grants only the current user,
SYSTEM and Administrators access. Saved metadata survived the real PowerShell
5.1 CLIXML round trip, and installation, project, application, registry and
shortcut snapshots still matched their cold sources. The original coordinator
and native journal remained paused with explicit false submission intent. No
installer, desktop or bridge was started by this backup. The recovery path was
kept short enough for every destination file to remain below 260 characters.

PR #14 merged the package-verification record into main `f06807a`; its exact
[Pages workflow](https://github.com/dieWehmut/Entwurf/actions/runs/43323098460)
completed successfully. A fresh Playwright QA profile loaded the deployed
[application](https://diewehmut.github.io/Entwurf/), opened Settings and returned
to the project workbench. Desktop 1440×900 and mobile 390×844 captures were
reviewed; the mobile page had no horizontal overflow, all eleven requested
resources returned 200, and the console had zero errors or warnings. Browser
plugin was unavailable, so regular Playwright supplied this basic smoke check.
It does not establish an existing paired Pages profile, real image generation,
original-byte import/ACK, locked operation or native installation.

## Resumed native installation verification — 2026-11-13

The inherited `WSL_INTEROP` path no longer existed. Selecting the already
listening `/run/WSL/910_interop` endpoint restored native PowerShell execution
without restarting WSL or changing Windows processes. At `01:21:12Z`, the native
read-only audit found no Entwurf app, bridge, installer, uninstaller, delivery
worker or localhost listener, and no claim for the original installation job.
The same queued job `ac745ad4-53fd-427f-a1e3-5c0731c27931` then ran once.

Its pinned installer completed with exit 0 at `01:22:12Z`. The installation
helper verified the expected executable and all **306 resources**, unchanged
project/application bytes, shortcuts and autostart registration, plus the exact
original paused coordinator/native journal bytes with false submission intent.
The outer artifact wrapper subsequently returned exit 1 because StrictMode
rejected its unset `$LASTEXITCODE`; this occurred after the successful helper
return and did not indicate an installer failure. The consumed job was not
replayed. Running the same pinned helper directly in read-only `Verify` mode
completed with exit 0 at `01:23:34Z`.

Independent Linux reads compared the installed executable, uninstaller and
bridge executable/DLL byte for byte with the previously extracted NSIS payload;
all matched. The installed uninstaller SHA-256 is
`8fa5346b63b2a35818d81b52a65c355c6d815f18013229ac0ae973a53f70d564`.
Private installation, verification and comparison receipts are retained in
`artifacts/i15` in the Windows task checkout and
`artifacts/native-install-81117d5-20261113` in the Linux checkout, outside Git.
These checks do not establish startup, real original delivery, frontend ACK,
locked generation, final reinstall cycles or tray acceptance.

## Normal installed startup and existing preview checks — 2026-11-13

The normal installed application started once at `01:53:32Z`, without arguments
or WebView debugging overrides. Its exact executable/PID/lifetime and Windows
session were verified; it owned the `127.0.0.1:48123` listener and the pinned
production `--automation-stdin` bridge child. The initial postlaunch oracle
raised `RuntimeException` after checking bootstrap. A later restart reproduced
the failure and located a case-insensitive PowerShell variable collision in the
diagnostic helper: `$cold` replaced the retained `$Cold` coordinator snapshot.
The subsequent preservation check then failed with `PropertyNotFoundStrict`.
An independent read-only `VerifyOnly` run at `02:02:32Z` passed on the same app
PID and creation time, including bootstrap and authenticated status/original-run
reads. Adding failure-location reporting did not change those verification
conditions or restart the app. The native provider reported connected and
available; the original run remained paused with explicit false submission
intent, unchanged run/task/base/prompt/resume history and no image. Existing
pairing/access bindings and all **1,135 project files**, including both source
files, remained unchanged. Two independent Linux poststartup reads also matched
every project byte to the cold backup. The normal installed app was left running.
These checks issue no prompt, resume, generation, import or ACK.

At `01:47:37Z`, an independently reviewed, read-only diagnostic verified the
existing real image preview, its exact historical process/window/Document and
absence of conflicting available original user evidence. The preview contained one direct generated
image plus four nested button icons; only the unique accurately labelled direct
image was selected. The complete image/ancestor identity and current absence of
Save As/modal/menu windows were retained in a hashed inspection receipt. Thirty
pure guard checks passed without Windows calls or UI dispatches. A subsequent
single-use menu-diagnostic launch encountered a WSL transport timeout. Native
read-only enumeration at `02:05:43Z` found no process for it and no menu claim or
receipt. A fresh launch at `02:06:46Z` positively rechecked the bound image, then
refused before creating its action claim because the preview/image dimensions
differed from the reviewed inspection. It wrote a refusal receipt with dispatch
count zero. The existing receipt was retained; no menu dispatch, supported save
path, original bytes or frontend transaction is claimed.

Private startup/preservation and preview evidence is retained in
`artifacts/native-normal-start-20261113` and
`artifacts/native-preview-context-menu-20261113` in the Windows task checkout,
with Linux copies outside Git. PR #16 recorded installation; its exact
[Pages workflow](https://github.com/dieWehmut/Entwurf/actions/runs/43345773516)
completed successfully. Deployed-site smoke for subsequent source changes
remains a separate release check.

## Native restart and deployed Pages smoke — 2026-11-13

A native read-only snapshot at `02:48:42Z` found no Entwurf application, bridge or
listener. The earlier application and ChatGPT process had ended; their exit
causes were not established. The installed ChatGPT package was now
`26.1106.6628.0`, so earlier process/window/preview bindings cannot be reused.
The only visible ChatGPT window in that snapshot had native dialog class `#32770`.
No dialog was dismissed and no historical response was rebound.

Running native commands from the Windows task checkout with a live WSL interop
endpoint succeeded, although subsequent calls still sometimes timed out.
An already running Windows task worker started the unchanged, pinned normal
launch helper in a fresh restart directory. The old launch claim and identity
were preserved. The helper rechecked the sealed installer payload, complete
project, paused journals and absence of applications/installers/listeners before
creating its new single-use claim. The installed application started at
`02:57:09Z` without arguments or debugging flags and owned the pinned production
bridge and loopback listener.

The postlaunch oracle reproduced the helper variable collision described above.
A separate direct native `VerifyOnly` run completed at `02:59:23Z` and passed
on the same PID and creation time. All **306 packaged resources**, **1,135
project files** and both source files matched their preserved hashes. Existing
access, paused run identity, explicit false submission intent and empty image
state remained unchanged. The provider reported connected but unavailable.
The app was left running. The outer launch wrapper did not reliably obtain its
driver exit code, and its worker output drain has no terminal receipt; these
observer limitations do not authorize another launch. Private evidence is in
`artifacts/native-normal-restart-20261113` and
`artifacts/native-current-state-20261113`, outside Git.

The [Pages workflow for `c3c0c1f`](https://github.com/dieWehmut/Entwurf/actions/runs/43350992744)
completed successfully. A fresh dedicated browser profile exercised the deployed
public app at desktop `1440×900` and mobile `390×844`: workbench, Settings and
return to workbench, including closing the mobile sidebar. Visual review found
no clipping or overlap. All ten resources returned HTTP 200, with no console
warnings/errors, page errors or network failures. The smoke completed at
`02:36:01Z`; its receipts and reviewed screenshots are retained in
`artifacts/pages-c3c0c1f-20261113`, outside Git. It did not open existing paired
profiles or establish native generation, original delivery or ACK.

## Remaining release gates

Historical checks on November 4–5 found a locked or disconnected session, and
older local WebUI/Pages companions reported `windows_locked`. The latest native
snapshot was connected and unlocked; the newest restoration reported connected
but unavailable. Actual locked/background native image
generation and original retrieval still require live acceptance.

The last inspected original local coordinator run remained paused with
`submit_intent: false`, no image and its original submission intent preserved.
Reinstall startup changed its reason to `adapter_restarted` without submitting.
The existing explicit resume receipt remains preserved. Pages and desktop had no
submission intents. The real Bayes-diagram diagnostic is a separate run: its
single submission and generated response do not establish a frontend transaction
and cannot be rebound to the paused coordinator run.

The current client has passed complete no-submit production fresh-window
preparation on a previously verified source, with the original source input hash
unchanged. That proof used the retained source identity; cold-start selection
among the additional diagnostic windows is outside its scope. The correlated
real response and its preview are verified. Live Download exposed an ownerless
Save As dialog, so a supported exactly owned save path and original-byte
retrieval remain required.

One coordinated real native generation from each frontend is still required with
its explicitly selected, connected provider, preserving unrelated drafts. Each
must deliver original bytes into the application transaction, receive
`artifact_accepted`, retain the generated version after reload and download the
same committed image. Acceptance must include actual locked/background execution.
Implementation tests, browser fixtures and no-submit diagnostics cannot establish
these native-provider behaviors. The merged candidate has passed installation
byte and normal startup verification above. Two complete final
uninstall/reinstall cycles preserving projects and generated images, and tray
restoration/exit acceptance with a normal installed app left available remain
required. Release and Pages checks must be refreshed after the remaining changes,
including a deployed-site smoke test. Private source
images, account information and generated acceptance images remain outside Git.
