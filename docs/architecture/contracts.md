# Entwurf v0.1 contracts

The canonical machine-readable contracts live in `packages/contracts/schemas`.
The application embeds these schemas at compile time and registers every local
schema resource for offline reference resolution. Installed binaries do not read
the source checkout and validation never fetches remote references.

`node packages/contracts/generate.mjs` deterministically generates
`packages/contracts/generated/types.ts` directly from these schemas. Run
`node packages/contracts/generate.mjs --check` to reject drift. A Rust contract
test also checks hashes of the embedded schema sources and generated TS against
the generated manifest, so changing a schema without regeneration or editing
only generated types fails core tests. The generator has no npm dependencies.

Rust types remain hand-authored serde models because they contain behavioral
implementations and existing compiler/state-machine contracts. Their serialized
values are checked against the canonical schemas in contract and persistence
tests; these are sampled compatibility checks rather than exhaustive Rust parity.
Generated Rust from the plan remains pending. Moving to generated Rust models would
require a separate model/behavior split. Typescript consumers can import the
schema-derived types from `@entwurf/contracts` or the generated file.

| Schema | Purpose | Stability |
| --- | --- | --- |
| `knowledge-ir.schema.json` | Traceable source-grounded knowledge graph | Stable v1 |
| `task-envelope.schema.json` | Complete GPT request with baseline IR | Stable v1 |
| `result-envelope.schema.json` | Patch, visual plan and correction proposals | Stable v1 |
| `style.schema.json` | A3 paper, type and palette contract | Stable v1 |
| `project.schema.json` | Portable project, task/version/audit, layout and report snapshot | Stable v1 |

## Knowledge IR invariants

- IDs are opaque stable strings. A node is never identified by its array index.
- Every `critical` node and every chapter/section node must have an anchor with a
  source, page/line/character range or quote hash.
- Formula changes are only legal through an auditable `Correction`. The original
  value, replacement, reason, anchors, timestamp and rollback state are kept.
- `GptCandidate` and `Source` nodes may be patched by a later GPT result.
  `UserEdit` nodes are never overwritten by model output.
- The project controls the exact source of truth; GPT output is a candidate
  patch, not a replacement for the IR.

## Task envelope

`TaskEnvelope` carries `task_version`, `task_id`, `revision`, locale, a source
excerpt, the complete baseline IR, style/canvas constraints, the requested
artifact, an embedded response schema and rules. `result_envelope` must echo the
same task ID and revision. Invalid identity is rejected before any merge.

`ResultEnvelope` is a patch, not a full replacement. A malformed result may be
retried once by the caller; a second failure becomes a manual handoff. The
result is never applied to a user-edited field.

## Validation precedence

1. Schema and identity. Raw imported result/project/IR JSON is schema-checked
   before deserialization so defaults cannot hide a missing version or unknown
   fields. Result patch, visual group and correction shapes are validated.
2. Source anchors and correction audit.
3. Formula integrity and unverified-item policy.
4. Layout bounds, overlap, clipping and minimum 8 pt print size.
5. Visual/export checks.

A high visual score cannot offset a blocking content finding. `compiler::compile`
refuses to emit artifacts whenever any blocking finding exists.

## Task state machine

```
DRAFT -> INGESTING -> IR_READY -> PROMPT_READY -> SENDING -> WAITING
      -> IMPORTING -> COMPILING -> VERIFYING -> NEEDS_REVIEW | APPROVED
      -> EXPORTED

Any active state -> PAUSED | FAILED | CANCELLED
```

`PAUSED` is resumable. Versions are append-only; rollback creates a new snapshot
instead of deleting later revisions. The same task cannot run two automation
jobs concurrently.

## Bridge protocol

The Windows sidecar speaks one JSON object per line over stdin/stdout. It first
returns a one-time session token from `detect_app`; every other call must attach
with that token. It exposes `detect_app`, `attach`, `start_conversation`,
`paste_prompt`, `submit`, `observe`, `capture_text`, `watch_artifact`, `pause`
and `cancel`.

The bridge never reads cookies, calls private APIs, bypasses login/quota/checks,
or clicks by screen coordinates. It may only drive observable UI controls. A
failed discovery, reliable-input, completion-detection or capture gate keeps
automation disabled and presents manual handoff.

## Export contract

The authoritative page is SVG. PNG is rasterized from that same SVG at 300 DPI;
the PDF is generated from the same placed geometry. `manifest.json` records the
IR hash, page count, dimensions, per-file SHA-256 and block IDs. `knowledge.ir.json`
is re-importable.
