# Deterministic layout and export

Rust packs every IR node into stable chapter/section groups on A3 landscape
pages, with three columns by default. Card size comes from Typst's shaped text
and math frame at the exact available width. The renderer places that same
frame in the final Typst page; PDF and SVG export the same `PagedDocument`.
PNG only rasterizes that SVG. At 300 DPI an A3 landscape page is 4961 × 3508 px.
SVG outlines its glyphs, so neither a viewer's fonts nor a font database in the
PNG rasterizer changes the result. PDF retains embedded fonts and text.

The compiler embeds the existing LXGW WenKai font and fonts from the pinned
`typst-assets` crate, including New Computer Modern Math. It never probes system
fonts or downloads dependencies at runtime. `assets/fonts/Typst-NOTICE.txt`
contains their licenses. A sealed Typst World serves only the generated main
source and bundled fonts; arbitrary files, packages, imports and clock access
are unavailable. User text becomes a quoted Typst string, never source markup.

LaTeX stays byte-for-byte unchanged in IR and exported knowledge.ir.json. A
bounded, documented command/environment allowlist is converted using pinned
MiTeX without macros. Unsupported commands, code escapes, comments, missing
glyphs, conversion errors and Typst warnings are explicit failures. Formulas
use a larger parent size to keep scripts legible. Every actual glyph, including
scripts, must meet the 8pt floor. Overwide or overheight formulas/cards fail
rather than shrinking below the floor or dropping content.

Cards are packed by shortest available column with stable tie breaking. Group
geometry belongs to each page/column fragment. Edges follow orthogonal lanes
outside card columns. Every node is placed once, or compilation reports that
the requested page budget cannot preserve the content. The compiler does not
rewrite knowledge, split a formula arbitrarily, or discard a node to fit.

`layout.json` records exact placed cards, glyph runs, minimum measured glyph
sizes and glyph ink bounds for downstream clipping checks. Original text is
retained separately because shaped runs may contain ligatures and math Unicode.
Legacy `wrap_text` is only a compatibility helper; production layout never uses
its character-count estimate.

An in-process bounded card cache keys the complete generated source (text,
formula, available width, typography, colors) and pinned renderer version.
Cached and uncached frames produce identical output. The bundle manifest hashes
every SVG, PNG, PDF, IR and layout artifact. Its `generated_at` is the IR's stable
revision timestamp, so the same inputs produce byte-identical exports. All
validation and rendering finishes before any output file is published.

The full source consistency/contrast/repair/rollback gates belong to phase 6.
Measured bounds make those gates possible but do not claim that phase complete.

## Sources and versions

- [Typst World and compiler, 0.15.1](https://docs.rs/typst/0.15.1/typst/trait.World.html)
- [Typst page document](https://docs.rs/typst-layout/0.15.1/typst_layout/struct.PagedDocument.html)
- [SVG exporter](https://docs.rs/typst-svg/0.15.1/typst_svg/fn.svg.html)
- [PDF exporter](https://docs.rs/typst-pdf/0.15.1/typst_pdf/fn.pdf.html)
- [MiTeX 0.2.4](https://github.com/mitex-rs/mitex)
- [Bundled fonts and licenses](https://github.com/typst/typst-assets)

Dependencies and their asset bytes are pinned by Cargo.lock. The existing LXGW
font and its OFL license remain checked into the repository. No external Typst
CLI, font install or network connection is required to compile an export.

## Bounded repair and undo

`prepare` (and therefore both desktop preview and export) validates the painted
text pairs (`text` against card `surface`, and against page `background`) before
rendering. Low contrast or malformed colors block output. Repair never changes
colors or knowledge. Routes must be orthogonal, stay on-page, meet card borders,
exist for same-page graph edges, and clear card interiors including the connector
stroke. The deterministic obstacle grid can reroute through clear gutters.

When geometry fails, preparation tries at most three recorded adjustments. Fixed
policies compact padding/gaps, reduce body fonts to the 8pt floor, widen columns
for oversized cards, reduce oversized headings (24/16/8pt caps), or reroute
connectors. The page budget is unchanged. Failure after the budget stays blocking;
content is never discarded to manufacture a passing result.

Generated `VisualRepairRecord` values store deterministic identity, round,
reason, exact IR/style hashes, before/after `LayoutOptions`, geometry hashes and
outcome. `TaskVersion` optionally retains effective options, history and whether
automatic repair is enabled. `repair_rounds_used` prevents repeated preview/export
calls from silently extending the three-round budget of the same version.

`visual_repair::rollback` verifies the current IR/style and option chain, then
returns the selected record's preceding options and history with that adjustment
and dependent successors marked reverted. The service saves a new version with
the identical IR and disables automatic repair. An undo may restore a blocked
layout; preparation preserves that result instead of redoing the repair. Existing
version knowledge and previous repair records remain immutable.

## Advisory GPT visual plans

`CompileOptions.visual_plan` passes the accepted proposal into the same measured
preparation used by preview, validation and export. Plans must cover every node
exactly once, with unique nonempty group IDs and legal page references within the
current page budget. Invalid proposals use canonical packing and return explicit
warnings. They never weaken content/source validation or clear review items.

Compatible contiguous group boundaries change column packing; compatible next-page
hints create page breaks. Canonical IR node order always wins, including after a
manual reorder. Noncontiguous/reversed groups and out-of-order group lists receive
specific warnings for their ignored ordering hints. Hierarchy fallback groups are
also contiguous runs, so a later sibling cannot move across an intervening node.

Measured placement remains authoritative. A group that spills or cannot reach its
requested page reports the actual pages. Unused page-count requests are reported;
blank pages are not inserted. If hinted packing fails, the engine retries without
proposal hints and records that fallback. Both routes retain every node and use
the same readability/page/repair gates. Group titles and notes remain metadata;
proposal text is never painted as additional knowledge or interpreted as math.

`PreparedCompilation` and `ArtifactSet` expose `visual_plan_status` (`not_evaluated`,
`applied`, `partially_applied`, `ignored`) and `visual_plan_warnings`. Warnings also
appear in validation and exported `layout.json` diagnostics. Optional version
`gpt_import` metadata preserves the proposal, append-only warnings, and truthful
image-reference flags. Exact visual undo and version rollback retain the proposal
and reevaluate its status. No image path supplied by a GPT result is opened by the
layout engine.
