# Source ingestion

`ingest_project_paths(ProjectPaths, selected_paths, options)` copies source bytes to immutable SHA-256-addressed storage before parsing. Duplicates in one selection are skipped with a retained diagnostic. Copies deduplicate across imports through storage. `ingest_source` reparses a stored source by its explicit `SourceFile.kind`, since the hash filename has no extension. Desktop imports save the resulting version immediately.

## Provenance and preservation

The historical `char_start` / `char_end` field names mean **UTF-8 byte offsets**, end-exclusive. Text sources index the original UTF-8 file. `line` is one-based. `quote_hash` is SHA-256 of the exact selected bytes, including whitespace and syntax, never a normalized summary. Invalid UTF-8 or a text file exceeding the u32 anchor capacity returns an explicit error.

Binary sources index an exact, page-local extraction transcript serialized in `SourceFile.notes` as `ingestion_transcript:<JSON>`. Its JSON has `page`, `engine`, `text`, `sha256`, and `offset_unit`. A fresh reparse replaces prior ingestion transcript/diagnostic notes while preserving user notes, preventing old OCR text from ambiguously sharing the same page key. PDF page numbers come directly from PDFium page iteration and are one-based; images have no fabricated page number. Regions currently cover the full originating page/image, with top-left origin, `pdf_millipoints` (1/1000 point) or `pixels`. These are coarse page anchors, not claimed word bounding boxes. Transcripts, hashes, regions and diagnostics therefore survive project serialization and database recovery.

CommonMark parsing uses pulldown-cmark offset events. All six heading levels, including Setext headings, form parent relationships and `contains` edges. Supported formulas in heading text create separate exact formula nodes linked to their heading, including TeX delimiters whose backslashes CommonMark would ordinarily unescape. The displayed heading keeps the original mathematical notation. Ordinary source text is chunked on Unicode scalar boundaries until the entire range is consumed; there is no sentence-prefix truncation. Markdown syntax and code remain raw in bodies. Paragraph chunks reconstruct the original non-heading/non-formula ranges. Whitespace-only gaps are omitted from nodes but remain in the immutable source. Dollar inline/display math is a parser extension; TeX `\\[` / `\\(` delimiters are recognized outside code. Formula `latex` retains the exact interior; its anchor retains delimiters. Formula blocks are atomic even above the requested chunk size.

LaTeX is structurally scanned without macro expansion. Chapter/section levels, optional short titles, command spacing, and balanced nested environments are retained. Formulas in both short and full heading titles enter the review queue. An entire environment, including its nested environments, remains one atomic node, preserving the original source rather than partially parsing a macro language. Verbatim/lstlisting/minted bodies are opaque, including when nested inside an outer environment. Inline `\verb` is also opaque to formula scanning. All non-verbatim LaTeX environments containing retained LaTeX require formula review conservatively; unclosed environments emit a diagnostic while their original text remains in bodies. This is source capture, not a full TeX interpreter.

## PDF and OCR runtime

PDFium **151.0.7881.0**, bound by `pdfium-render =0.9.4` with `pdfium_7881` and `thread_safe`, is the only PDF engine. A process-wide instance is reused because pdfium-render owns process-wide bindings; switching library paths requires restarting. `IngestOptions.pdfium_dir` selects the library directory; `ENTWURF_PDFIUM_DIR` supports explicit test/development configuration. The desktop resolves packaged `resources/pdfium`. No system engine substitution or runtime download occurs.

Every PDF page is accessed by its explicit index and first attempts its text layer. A page-load failure records a page-specific error diagnostic and unresolved review item, while later readable pages retain their original page numbers; the dependency iterator is deliberately avoided because it stops silently on a load error. Pages without text render at 300 dpi through PDFium and feed the complete page to local Tesseract. Supported images are decoded from their actual bytes and normalized to temporary PNG for Tesseract, retaining their original stored bytes. OCR defaults to `chi_sim+eng`. `binary_dir` selects an explicit Tesseract directory; when set, a missing executable is reported instead of silently using PATH. Temporary rendered images are removed on all return paths.

OCR output remains unverified. Successful Tesseract exit status does not prove all requested language packs loaded: explicit language-load failures on stderr persist as warnings/review items while any partial text is retained. Recognized mathematical operators also create conservative formula candidates without inventing LaTeX. All parsed formulas/environments and every OCR node enter `KnowledgeIr.unverified`; missing/failed engines and malformed source diagnostics create review items. Diagnostics serialize in source notes as `ingestion_diagnostic:<JSON>`. Missing engines produce no fabricated text. `validate_ir` emits blocking findings for every unresolved review item, retaining its exact reason, node reference and text evidence. The normal compiler refuses to create export files until review explicitly clears the queue. A grounded source node's default `verified=false` flag alone does not block compilation.

## Build and licenses

Run `scripts/windows/prepare-pdfium.ps1` before a native build (the build script does this automatically). `-ArchivePath` supports a previously downloaded archive for offline builds. Both the archive and extracted DLL are SHA-256 checked. The ignored DLL is packaged by Tauri alongside tracked upstream notices; the executable needs no developer repository at runtime.

Pinned URLs and SHA-256 values are in `apps/desktop/src-tauri/resources/pdfium/manifest.json`. PDFium uses BSD-3-Clause; the binary distributor's packaging uses MIT. **Every** upstream bundled-library notice is retained in `resources/pdfium/licenses/`, including PDFium, ICU, FreeType, image codecs and other linked dependencies. `pdfium-render` is MIT OR Apache-2.0; pulldown-cmark is MIT. Tesseract remains a user-installed Apache-2.0 dependency, detected locally.

Primary references, fetched directly via HTTPS because the browser service returned HTTP 404:

- https://github.com/ajrcarey/pdfium-render (binding API, global initialization, supported runtime version)
- https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium%2F7881 (pinned archives and GitHub SHA-256 metadata)
- https://pdfium.googlesource.com/pdfium/+/refs/heads/chromium/7881 (PDFium source)
- https://docs.rs/pulldown-cmark/0.13.6/pulldown_cmark/ (offset events and math option)
- https://tesseract-ocr.github.io/tessdoc/Command-Line-Usage.html (local CLI and language selection)

## Regression execution

`cargo test -p entwurf-core` runs deterministic parsing, storage, and missing-engine coverage without optional native engines. Five named integration tests are explicitly ignored by default and require real PDFium/Tesseract:

```text
ENTWURF_PDFIUM_DIR=<directory containing libpdfium.so/pdfium.dll>
ENTWURF_TEST_OCR_DIR=<directory containing tesseract/tesseract.exe>
cargo test --locked -p entwurf-core --test ingestion -- --include-ignored
```

The public fixtures contain only synthetic data: a valid two-page text PDF, a deliberately broken middle-page PDF with readable pages on either side, an image-only PDF, and a bilingual image. `fixtures/public/ingestion/generate.py` regenerates PDFs without third-party Python packages. Private user samples and transcripts must not be added to this fixture directory.
