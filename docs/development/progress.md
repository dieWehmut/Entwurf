# v0.1 implementation and acceptance status

The binding requirements are in [the supplied plan](../plans/entwurf-v0.1.md).
The user's later instruction authorizes staged commits and pushes to `origin`;
it overrides the plan's original no-push instruction. Work is on `feat/v0.1`.
Existing prototype code and green smoke tests do not establish release readiness.

| Stage | Required evidence | Status on 2026-10-31 |
| --- | --- | --- |
| 1. Foundation | Native Windows worktree; Rust/MSVC, .NET 10, WebView2, Node, OCR detection; root documents; runnable build scripts | Native checkout created; prerequisite checks, PowerShell tests and native Tauri check pass; full repository build pending |
| 2. Contracts and storage | Shared schema/types, transactional SQLite WAL migrations, immutable versions, hashed source copies, audited state transitions and database recovery | SQLite and schema-generated TS/Rust runtime types implemented and reviewed; legacy snapshots preserved; desktop event transitions and full integration still pending |
| 3. GPT bridge | Current-user named pipe, length-prefixed JSON-RPC, session token, observable UIA controls, tested pause/cancel and capability gates | Transport/discovery implemented and reviewed; 40 native tests pass; no verified ChatGPT window, automation remains unavailable; parent integration pending |
| 4. Ingestion | PDFium text/page extraction and scanned-page OCR, CommonMark, preserved LaTeX, image OCR, exact anchors and diagnostics | Implemented and reviewed; 17 actual-engine tests pass on Windows/Linux; installed resource resolution remains pending |
| 5. Compiler | Shared Typst text/math scene, deterministic A3 layout and orthogonal routes, at most three pages, SVG/vector PDF, 300 DPI PNG, cache | Pending; prototype export exists but Chinese/math output is not reliable |
| 6. Validation and repair | Schema/provenance/formula/coverage/layout/contrast/export gates, at most three content-preserving repair rounds, rollback | Pending; partial checks exist, but passing tests do not establish full content integrity |
| 7. Desktop UI | Five functional pages, specified themes, first-run detection, persisted edits/anchors/importance/order, logs, manual handoff, real version comparison | Pending; present UI is a visual demo |
| 8. End-to-end | Complete prompt/result round trip, one correction retry, durable revisions, local recompute, edited fields preserved, real offline import-to-export | Pending |
| 9. Release | Unsigned Windows x64 NSIS; clean-user install/uninstall/offline startup; Chinese/spaced paths; DPI/multi-monitor; dependency notices and redacted diagnostics | Pending |

## Release gates that require stronger evidence

- The licensed private probability chapter and handwritten style reference must
  stay under ignored `fixtures/private/`. The public synthetic sample is a CI
  fixture, not a substitute for private-chapter acceptance.
- A complete chapter must preserve all core content within three A3 landscape
  pages. Critical nodes and headings need source anchors; formulas must match
  their source or carry a complete reversible correction record.
- Export must reject unconfirmed formulas, overlapping or clipped content and
  text below 8 pt. Tests must inspect readable Chinese/math and artifact hashes,
  not just filenames, PDF headers or UI notifications.
- UI automation remains off until discovery, input, completion detection and
  capture all pass on the actual target ChatGPT version in a Windows foreground
  session. Manual mode must work independently of automation availability.
- Screenshot acceptance covers all five pages, both themes, 1280×800 and
  1920×1080, and 100%/125%/150% scale, including focus and contrast checks.
- The acceptance harness in `crates/entwurf-cli` is internal development code;
  no public CLI or network service is part of the product contract.

This file records unfinished requirements explicitly and must be updated with
test commands, artifacts and commit evidence as each stage is verified.

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
