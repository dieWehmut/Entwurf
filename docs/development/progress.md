# v0.1 implementation and acceptance status

This report records the original offline desktop baseline. The later shared web
app, automated image workflow, revised UI and newer installer results are tracked
in [automation and shared web acceptance](automation-web-acceptance.md); that
report takes precedence for the current candidate.

The binding requirements are in [the supplied plan](../plans/entwurf-v0.1.md).
The user's later instruction authorizes staged commits and pushes to `origin`;
it overrides the plan's original no-push instruction. Work is on `feat/v0.1`.
The current Windows desktop candidate has actual installed-app evidence below.
See [native acceptance](native-acceptance.md) for the tested installer and limits.

| Stage | Required evidence | Status on 2026-11-01 |
| --- | --- | --- |
| 1. Foundation | Native Windows worktree; Rust/MSVC, .NET 10, WebView2, Node, OCR detection; root documents; runnable build scripts | Native tools verified; Release NSIS built at 536a23a and installed; build compiles the shipped desktop once |
| 2. Contracts and storage | Shared schema/types, transactional SQLite WAL migrations, immutable versions, hashed source copies, audited state transitions and database recovery | Implemented and reviewed, including desktop event transitions, immutable edits, recovery and legacy snapshot compatibility |
| 3. GPT bridge | Current-user named pipe, length-prefixed JSON-RPC, session token, observable UIA controls, tested pause/cancel and capability gates | Native parent and self-contained resources integrated; 41 C# and 8 actual parent/child tests pass; no verified ChatGPT window and all four gates remain false |
| 4. Ingestion | PDFium text/page extraction and scanned-page OCR, CommonMark, preserved LaTeX, image OCR, exact anchors and diagnostics | Actual-engine tests pass; installed two-page PDF uses the pinned bundled DLL; two private images retain exact originals and pending OCR/formula reviews |
| 5. Compiler | Shared Typst text/math scene, deterministic A3 layout and orthogonal routes, at most three pages, SVG/vector PDF, 300 DPI PNG, cache | Shared measured scene and bounded cache implemented; Chinese/math vector PDF independently verified; installed one-page baseline and applied two-page proposal export pass |
| 6. Validation and repair | Schema/provenance/formula/coverage/layout/contrast/export gates, at most three content-preserving repair rounds, rollback | Reviewed and tested; installed UI confirms real heading repair, exact-IR undo, preserved history, disabled replay and invalidated exports |
| 7. Desktop UI | Five functional pages, specified themes, first-run detection, persisted edits/anchors/importance/order, logs, manual handoff, real version comparison | 33 React and 10 browser tests pass; installed five-page/two-theme flows, native dialogs, IR restoration and diagnostics save/cancel pass |
| 8. End-to-end | Complete prompt/result round trip, one correction retry, durable revisions, local recompute, edited fields preserved, real offline import-to-export | Root actual-engine run: 145 passed, zero ignored; installed offline reload/import/edit/export, applied grouping, manual-edit preservation and IR reimport pass |
| 9. Release | Unsigned Windows x64 NSIS; clean-user install/uninstall/offline startup; Chinese/spaced paths; DPI/multi-monitor; dependency notices and redacted diagnostics | Actual repeated uninstall/reinstall passes with project hashes intact; separate fresh App configuration and database reconstruction pass; native OS scaling and physical multi-monitor remain unverified |

## Remaining environment limits

- The user confirmed no original textbook chapter is available. The two private
  images were tested for style storage/OCR, with reviews left pending. The public
  synthetic chapter does not establish real-textbook or handwriting accuracy.
- UI automation remains off until discovery, input, completion detection and
  capture all pass on the actual target ChatGPT version in a Windows foreground
  session. Manual mode must work independently of automation availability.
- Browser screenshots cover five pages, both themes, two viewports and three
  CSS zoom levels. Native screenshots record actual viewport/DPR. This machine
  exposes one monitor; physical multi-monitor and OS 100/125/150% remain untested.
- Offline tests disable WebView networking and reload the UI. Host adapters and
  native process networking remain unchanged; no whole-machine disconnection or
  system-wide packet-capture result is claimed.
- The acceptance harness in `crates/entwurf-cli` is internal development code;
  no public CLI or network service is part of the product contract.

The checkpoints below are historical; their then-pending items are superseded by
the current table and linked native acceptance report.

## Verified development baseline

- `c6a3ddf`: foundation documentation, supplied plan, ignore rules and Windows
  scripts; pushed to `origin/feat/v0.1` after review and native PowerShell tests.
- `3517957`: native desktop scaffold and SQLite/contracts baseline; `9c3f42a`
  adds `/sample/` to ignore private local acceptance images. Both pushed.
- Native Windows `cargo test --locked -p entwurf-core -p entwurf-cli --target
  x86_64-pc-windows-msvc`: 35 core and 3 acceptance tests pass after storage review
  fixes, including recovery from an empty migrated database and immutable history.
- Native Windows `cargo check --locked -p entwurf-desktop --target
  x86_64-pc-windows-msvc`: passes after installer-mode, icon and borrow fixes.
- Frontend `pnpm test` (5 tests) and `pnpm build` pass for the existing demo;
  neither proves real IPC or export behavior.
- `node packages/contracts/generate.mjs --check` and workspace Rust formatting
  checks pass. Schemas also validate from a relocated core executable without
  opening schema files from the development checkout.
- Native checkout `pnpm tauri build --debug --no-bundle --target
  x86_64-pc-windows-msvc -- --locked` builds the desktop executable with embedded
  frontend assets. This is a development build, not an installer acceptance.
- Bridge native tests: 40 passed, zero failed or skipped, after review fixes for
  cancellable shared discovery and watcher setup cleanup. Self-contained Windows
  x64 Release publishing passes; actual ChatGPT input/capture remains unavailable.
- Source/contract milestone: all 59 tests pass with real PDFium/OCR paths enabled
  (37 core, 17 ingestion, 5 internal acceptance). Review regressions cover broken
  middle PDF pages, heading formulas, missing OCR languages and blocked exports.
  Public-chapter export tests explicitly verify and persist formula review before
  compilation; they do not silently approve the raw source in the application.
- Shared Rust declarations now come from the same schemas as TypeScript. Tests
  preserve legacy immutable snapshot bytes, native field types and serialization
  order; canonical contract files use LF in Windows and WSL for stable hashes.

The supplied private sample directory currently contains two PNG references.
The user confirmed no original probability chapter PDF/Markdown/LaTeX is
available. Use those images for style/OCR checks; real-chapter acceptance remains
unverified and must not be inferred from synthetic or generated content.

## Compiler and desktop integration checkpoint

- Root full actual-engine run: **105 passed, zero failed or ignored** using
  `cargo test --locked -p entwurf-core -p entwurf-cli -- --include-ignored
  --test-threads=1`, with pinned PDFium and real Tesseract enabled. This includes
  measured export tests, inline formula/evidence regressions, export-directory
  identity and tamper checks, selection persistence and immutable manual edits.
- The public chapter retains all 18 nodes on one A3 landscape page. PNG is
  4961 × 3508 with 300 DPI metadata. PDFium reads Chinese/math from the vector
  PDF; the smallest card glyph is 10.5 pt and the footer is 8 pt.
- Native Windows `cargo check --locked -p entwurf-desktop --target
  x86_64-pc-windows-msvc` passes after the final provenance, artifact, remembered
  workspace and parent-bridge integrations.
- Frontend tests use a clearly isolated mock IPC boundary. The 60 screenshots
  cover five pages, two themes, two viewport sizes and three CSS zoom levels;
  these do not establish native OS DPI or installed-app behavior.
- User-supplied `sample/icon.png` was converted with the local Tauri icon tool
  into the bundled multi-resolution ICO and PNG. Commit `6d93982` is pushed;
  the original sample directory remains ignored.
- Native environment currently exposes one 1280 × 800 monitor. Physical
  multi-monitor acceptance remains unavailable. No uninstall/reinstall cycle
  has yet been completed; at least two are required before delivery.

## Visual repair and release hardening checkpoint

- Root reran all core/internal acceptance tests with PDFium and actual Tesseract:
  **123 passed, zero failures or ignored tests**. This adds visual repair,
  immutable history/undo, whole-version restoration and diagnostic redaction.
- Independent review probes confirm that repeated failed exports cannot reset
  the three-round budget, selected undo preserves exact IR bytes and disables
  replay, changed styles invalidate only incompatible visual proof, and legacy
  versions without new optional fields reopen and compile.
- Latest native desktop check passes with repair and diagnostics commands.
  Frontend verification is 27 React tests and 8 browser tests, including repair
  undo and successful/cancelled diagnostic export.
- Offline notice generation records 554 dependencies and 351 notice texts with
  zero unresolved records. Native Windows generation in a Chinese/spaced path
  matches the WSL output byte for byte. Original notice bytes have explicit Git
  checkout attributes to avoid CRLF conversion and false native dirty status.
- Diagnostics export is local and opt-in, containing fixed version/platform
  strings, capability booleans and aggregate counts. It excludes source content,
  filenames/paths, prompts, tokens, identifiers and arbitrary error text.
- Physical multi-monitor, installed UI operations, actual reinstall cycles and
  private textbook coverage remain separate acceptance gates.

## Remaining desktop flow gaps closed

- Native Knowledge IR import accepts an empty destination task and binds sources
  to verified originals already retained by the project. Incoming file paths and
  extraction transcripts are never trusted. Unknown originals require explicit
  source import first; existing tasks and manual work are preserved.
- GPT visual proposals now affect compatible group boundaries/page breaks while
  preserving canonical node order and every knowledge node. Incompatible advice
  has visible fallback warnings; unretrieved image references remain unapplied.
- Mandatory formula review retains its proposal and warnings. IR reimport chooses
  matching trusted extraction history, including a later successful PDF parse
  after an earlier failed attempt. Both regressions were independently reproduced
  and revalidated after correction.
- Root's complete actual-engine test run passes **145 tests, zero ignored**;
  generator tests pass twice, and native desktop checks pass after the fixes.

## Actual first installation

The unsigned Windows x64 NSIS bundle at `988fc22` built successfully (74.13 MiB).
It was installed under the current user's `Programs/Entwurf`, separately from
build tools and test projects. Its extracted executable icon matches the supplied
image. Initial installed acceptance passes startup, real capability detection,
native folder/file dialogs, a Chinese/spaced Markdown path, four explicit formula
reviews, retained manual edits through a synthetic result import, actual SVG/PNG/
PDF plus manifest hashes, and all five pages in both themes.

WebView2 network access was disabled through its test-only localhost debugging
connection for this flow; host network adapters were not changed. This is actual
native IPC and installed code, with no mocked commands. The synthetic result
does not claim real ChatGPT generation. Two uninstall/reinstall cycles and final
installed checks for subsequent additions remain outstanding.

## Final installed candidate checkpoint — 2026-11-01

The `536a23a` bundle passes two complete runs of the reviewed native reinstall
driver, with 11 workflow checks per cycle and 285/350 existing project files
unchanged after the respective uninstalls. The same installer/executable hashes
were verified in both runs. Native IR reimport, actual two-page GPT grouping,
repair undo and diagnostic Save As/cancel all pass. Separate fresh App configuration
checks pass 17/17, and installed database recovery checks pass 22/22. The final
application is installed; test processes are closed. See [the complete native
acceptance record](native-acceptance.md) for hashes, commands, scope and limitations.

The final rebuild corrects stale font-notice line endings retained by the earlier
Windows checkout. Canonical Git bytes are now verified before notice generation,
with seven regression tests passing on Windows and WSL. Installed notices match
the checked-in bytes; the earlier native functional checks remain scoped to their
recorded builds because this fix changes only notice bytes and build validation.
