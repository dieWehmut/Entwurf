# Browser core

One WASM module instance runs the canonical Rust `Workspace` in one dedicated
worker. The host persists virtual files to IndexedDB atomically after every
command, including rejected result imports which can update persisted attempts.

`browser_reset()` clears all files and the active workspace. `browser_put_file`
and `browser_get_file` exchange `Uint8Array` bytes under absolute POSIX paths.
`browser_list_files()` returns a JSON string array; `browser_delete_file()` removes
one file. Persist bytes, not object URLs. On failed IndexedDB commit the host must
reset, restore the prior files, reopen the project, and restore task selection.

`browser_call(command, args_json)` returns JSON and throws an error string.
It uses the existing frontend command names/argument casing for all Workspace
operations. Additional commands are `list_projects` (returns root/name/id),
`artifact_path` (validates the complete current export manifest before returning
its path), `diagnostics` (uses the native allowlisted redaction builder), and `register_extraction`. Dialogs, clipboard, runtime capabilities,
and downloads belong to the host. The host should persist active project root
and selected task separately from the canonical project files.

Binary PDF/OCR extraction is performed in JavaScript and registered before
ordinary import/reimport. `register_extraction` takes `{path, extraction}` with:

```json
{
  "schema_version": 1,
  "source_sha256": "<SHA-256 of retained original bytes>",
  "source_kind": "pdf",
  "page_count": 1,
  "pages": [{
    "page": 1,
    "text": "Exact extracted text",
    "engine": "pdfjs",
    "is_ocr": false,
    "region": {"left": 0, "top": 0, "width": 595000, "height": 842000, "unit": "pdf_millipoints"}
  }],
  "diagnostics": []
}
```

For images, page is null, count is one, engine is `tesseract.js`, is_ocr is true,
and region covers actual image dimensions with `pixels`. PDF pages must account
for every page 1 through page_count, including failed/empty pages with a diagnostic.
Diagnostics carry `level` (`info`, `warning`, `error`), `code`, and `message`.
The core computes transcript/quote hashes and UTF-8 byte anchors, verifies original
source bytes, and requires explicit OCR/formula review. Registration is retained
under `/extractions/<hash>.json`; include those files in persistence/backups.

Build with an installed `wasm32-unknown-unknown` target and wasm-bindgen CLI matching
Cargo.lock's version: `scripts/build-wasm.sh --test`. The test executes actual WASM
Markdown/TeX import, review, SVG/PNG/PDF export, reopen, integrity rejection and
binary-extraction binding. Web output goes to `apps/desktop/public/wasm/`.

Generated images are separate project assets, never fake knowledge versions.
`import_generated_image` accepts `{taskId, baseVersionId, runId, promptSha256,
path, expectedSha256, mimeType}`; `path` refers to registered VFS bytes, not JSON
encoded bytes. `generated_image_path({taskId, imageId})` verifies owned retained
bytes before returning the download path. Project `generated_images` is optional
for legacy snapshots and append-only once populated. Exact delivery retries are
idempotent even after a newer knowledge version exists; changed run/hash metadata
is rejected. New deliveries require the current base version. Accepted images are
fully decoded PNG/JPEG/WebP, at most 32 MiB and 64 megapixels, stored beneath the
project's `generated/<sha256>` directory with `origin: "chatgpt_ui"`.
