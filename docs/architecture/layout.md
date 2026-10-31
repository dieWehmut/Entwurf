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
