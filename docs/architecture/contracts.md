# Entwurf v0.1 contracts

The canonical machine-readable contracts live in `packages/contracts/schemas`.
The application embeds these schemas at compile time and registers every local
schema resource for offline reference resolution. Installed binaries do not read
the source checkout and validation never fetches remote references.

`node packages/contracts/generate.mjs` deterministically generates TypeScript
and Rust wire declarations directly from these schemas, with no npm dependencies.
`generated/ir.rs`, `generated/model.rs`, and `generated/project.rs` are included by
the corresponding core modules: these are the types used by ingestion, storage,
prompting, layout, validation and export. Their constructors, state transitions
and other behavioral implementations stay in the Rust modules. TypeScript
consumers import `@entwurf/contracts` or `generated/types.ts`.

Run `node packages/contracts/generate.mjs --check` to reject drift in every output.
The generated manifest hashes all schema sources, the generator itself, TS and
each Rust file; the core contract tests verify those embedded hashes without
requiring Node or a source checkout at runtime. `node --test
packages/contracts/generate.test.mjs` additionally exercises schema mutation and
checks rejection of edited/missing generated artifacts in a temporary directory.

Schema annotations retain existing native APIs without copying field declarations:

| Annotation | Meaning |
| --- | --- |
| `x-rust-name`, `x-rust-module` | Name and original core module for a generated struct, enum or alias. Repeated enum declarations must generate identically or generation fails. |
| `x-rust-type` | Checked native scalar override: string `Id`/`String`, integer `u16`/`u32`/`u64`, or number `f32`/`f64`. Default integer and number mappings are `u32` and `f32`. |
| `x-rust-derives` | Additional existing traits (`Copy`, equality/order traits or `Default`); all structs/enums derive Debug, Clone and serde traits. |
| `x-rust-field` | Native field spelling, preserving the JSON key through serde rename (for example `type` → `edge_type`). |
| `x-rust-default` | Existing serde default: `true` or a named local function. |
| `x-rust-skip-none` | Omit an absent optional field on serialization; used only for the new anchor region so old immutable IR bytes remain stable. |

Date-time strings become `chrono::DateTime<chrono::Utc>`, arrays become `Vec`,
typed object maps become `BTreeMap`, and nullable or optional fields become
`Option`. Enum variants use explicit serde names, including uppercase task states.
Closed objects reject unknown fields during serde deserialization as well as
schema validation. Unsupported unions or native types fail generation instead
of degrading to untyped JSON.

JSON Schema remains the import-validation authority for string patterns, ranges,
required nullable fields and version constants. Serde retains the previous IR
version default and absent-option behavior; raw JSON validation runs first so
those defaults cannot hide an invalid imported document. Native integer widths
also reject overflow. Existing JSON field names, enum values, IDs, timestamps,
numeric precision and null serialization are preserved. The additive optional
anchor `region` is absent in older files and deserializes as `None`; new serialized
anchors omit an absent region, preserving pre-region immutable snapshot bytes.

| Schema | Purpose | Stability |
| --- | --- | --- |
| `knowledge-ir.schema.json` | Traceable source-grounded knowledge graph | Stable v1 |
| `task-envelope.schema.json` | Complete GPT request with baseline IR | Stable v1 |
| `result-envelope.schema.json` | Patch, visual plan and correction proposals | Stable v1 |
| `style.schema.json` | A3 paper, type and palette contract | Stable v1 |
| `project.schema.json` | Portable project, task/version/audit, layout and report snapshot | Stable v1 |

## Knowledge IR invariants

- IDs are opaque stable strings. A node is never identified by its array index.
- Source regions use a top-left origin, with `left`, `top`, `width`, and `height`
  stored as unsigned integers. `pdf_millipoints` means thousandths of a PDF point;
  `pixels` means source-image pixels. Anchors may omit `region`.
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

The Windows .NET sidecar uses a current-user-only named pipe with JSON-RPC 2.0.
Each UTF-8 JSON frame has a four-byte unsigned little-endian length prefix.
The parent passes the random one-time session token through an inherited anonymous
bootstrap pipe; it never appears in command-line arguments, environment variables
or logs. `attach` consumes the token once and authorizes only that connection.
Public `detect_app` returns a read-only capability report and never returns a token.

The methods are `detect_app`, `attach`, `start_conversation`, `paste_prompt`,
`submit`, `observe`, `capture_text`, `watch_artifact`, `pause` and `cancel`.
Current production discovery verifies ChatGPT package identity and inspects UIA
control capabilities. Real input, completion detection and text capture are
unverified and remain disabled; simulated state-machine tests do not enable them.
All four release gates must pass before automation can start. Authenticated manual
artifact watching uses only parent-authorized directories and exact task/version
filenames. See [the bridge contract](../../sidecars/gpt-bridge/README.md) for framing,
limits, errors, bootstrap and recovery details.

The bridge never reads cookies, calls private APIs, bypasses login/quota/checks,
or clicks by screen coordinates. Unavailable automation presents manual handoff.

## Export contract

The authoritative page is SVG. PNG is rasterized from that same SVG at 300 DPI;
the PDF is generated from the same placed geometry. `manifest.json` records the
IR hash, page count, dimensions, per-file SHA-256 and block IDs. `knowledge.ir.json`
is re-importable.
