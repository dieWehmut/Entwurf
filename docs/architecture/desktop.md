# Desktop workspace service

`entwurf_core::workspace::Workspace` owns an open project and its storage paths. Tauri commands delegate to it on a blocking worker, under one application mutex. Business behavior is tested without a GUI. Mutations validate a cloned project, save it through SQLite/immutable version storage, and then replace the in-memory project.

## IPC snapshot

Project commands return `WorkspaceSnapshot` with `root`, canonical `project`, `selected_task_id`, nullable canonical `current_version`, `preview_pages` (`page`, `svg`), and nullable `handoff`. Canonical data fields use snake_case; Tauri argument names use camelCase. Draft tasks have no current version. A missing validation report means validation has not run. Pages contain actual compiler SVG and never placeholder exports.

Commands:

- Project: `open_workspace(root?, name)`, `create_project(root, name)`, `load_existing_project(root)`, `workspace_snapshot(taskId?)`, `create_task(title, chapter?)`.
- Inputs: `import_sources(taskId, paths)`, `import_ir(taskId, baseVersionId?, json)`, `reimport_source(taskId, baseVersionId, sourceId)`, `import_style_images(paths)`, `save_style(style)`, `save_settings(ocrLanguages)`.
- Editing: `update_nodes(taskId, baseVersionId, edits, nodeOrder?)`, `rollback_version(taskId, versionId)`, `revert_correction(taskId, baseVersionId, correctionId)`, `resolve_review(taskId, baseVersionId, itemId, note)`.
- GPT handoff: `prepare_handoff(taskId)`, `import_result(taskId, json)`, `copy_prompt(taskId)`, `open_chatgpt()`.
- Outputs: `validate_current(taskId)`, `preview_current(taskId)`, `export_current(taskId, outputDir?)`, `open_export(taskId, kind, index?)`, `undo_visual_repair(taskId, baseVersionId, repairId)`.
- Diagnostics: `export_diagnostics()` opens a native JSON save dialog and returns the saved path, or null on cancellation; works without an open project.
- Native pickers: `choose_directory()` returns a path or null; `choose_sources()` returns paths; `choose_result_file()` returns ResultEnvelope JSON **text**, or null; `choose_ir_file()` separately returns KnowledgeIR JSON text, bounded to 16 MiB, or null.

`NodeEdit` contains `id`, `title`, `body`, nullable `latex`, canonical `anchors`, and `importance`. `nodeOrder`, when supplied, must contain every current node ID exactly once. A stale `baseVersionId` is rejected without rewriting knowledge. Text, formula, anchor, importance and order edits create new immutable versions. Explicit desktop actions use one audited state helper; they do not fabricate automated sending/waiting events.

## Fidelity and review

Copied originals remain inside project storage. Reference images are validated and copied into storage without entering the knowledge IR or OCR workflow. OCR language settings persist and apply to subsequent imports/reimports. Reimport parses a retained original and replaces that source's extraction; it refuses to erase manual nodes.

Formula changes keep before/after/reason/source anchors in correction records. GPT cannot delete source baseline knowledge or overwrite manual nodes, including through corrections. Newly added GPT formulas enter the review queue. Correction undo rejects a superseded value and creates a new version. Reviewing a node requires a note and valid source/formula evidence. A node-less extraction failure cannot be approved with a note: its retained diagnostic blocks export until successful reimport.

Provenance validation checks retained source hashes and lengths, UTF-8 byte ranges, quote hashes, line/page evidence and retained binary transcript hashes. A formula must match an exact source formula or the final value of a complete anchored correction chain. Persistent extraction diagnostics cannot be bypassed by clearing the review queue.

KnowledgeIR import requires an empty task and an exact current-version guard; it never replaces a populated task. The service validates the raw schema and reference identity before source access. Incoming `stored_path` and transcript/diagnostic notes carry no authority. Each source binds by SHA-256, kind and length to an original already retained anywhere in the project; the canonical file must remain in the project's content-addressed sources directory and its actual bytes must match. Missing originals require explicit document import first. Bound source metadata comes from the retained original under the incoming source ID, preserving all anchor and correction payloads. Nodes, formulas, manual origins, corrections and review items remain intact; destination project/task/snapshot identity changes, with the original identity and JSON hash audited. The new version stores its real content report without inherited layout, approval, artifacts or visual proof.

## Handoff and output lifetime

Handoffs live under `handoffs/`, keyed by task and immutable baseline version. Reopening or preparing the same version preserves the correction retry budget. The first rejected result produces a correction prompt; the second changes status to `manual_required`. Further successful import requires an explicit manual action. Task/revision and baseline identity are checked before applying any result.

Accepted canonical result JSON, including visual plan and notes, is retained under `imports/entwurf-task-<task>-v<baseline revision>-plan.json`; its hash is audited. The new version records its source prompt hash and optional canonical `gpt_import` metadata: the original plan, import warnings, image-reference presence/applied flags and evaluated plan status (`not_evaluated`, `applied`, `partially_applied`, or `ignored`). Protected manual updates/deletions/corrections produce visible warnings. Image references remain unapplied data and are never silently opened or attached.

Preview/export pass the saved visual proposal into the local compiler as advisory grouping and page hints. Canonical node order and knowledge remain unchanged. The compiler returns actual application status and explicit invalid/unfit/fallback warnings; the service persists them without discarding prior import warnings. The original proposal/image flags are immutable and warnings append-only on a saved version. Version rollback and visual undo carry the proposal but reset its status for fresh evaluation. Review-only confirmation also carries the proposal, warnings and source prompt hash, while discarding derived layout/repair proof because the reviewed IR has a new identity. Substantive content/order/anchor or style edits create a new version without the prior proposal and record an explicit invalidation audit; the original remains visible in history.

Validation and preview attach the exact compiler report/layout. Blocked or failed export preserves its report and has no current artifact paths. Successful export attaches the compiler's actual SVG/PNG/PDF/IR/manifest paths in task/version subdirectories, including inside a chosen output directory. Opening an artifact checks manifest project/task/IR identity and every bundled file's hash/length, rejecting missing, replaced or externally modified output. Any content/style change creates a version without these outputs, requiring fresh compilation. Historical versions remain intact.

## Visual repair lifetime

Each current version persists the compiler's effective `layout_options`, append-only `visual_repairs`, and optional `visual_repair_enabled` flag (absent means enabled). Preview and export retain previous records when compilation needs no further adjustment. At most three active repair rounds are allowed across repeated previews and reopening; content and source failures remain blockers.

Undo checks the current baseline and repair identity, then creates a new version with byte-identical knowledge IR and the same prompt hash. It restores the selected repair's prior options, marks that repair and its dependents reverted, and disables automatic repair. Immediate validation returns the real state, including blocked output when undo removes a necessary adjustment. Previous versions and their repair records remain unchanged. Version rollback also preserves the selected repair/undo state; if the current project style differs from its visual proof, only derived visual options/history are reset with an explicit audit entry, and fresh validation is required.

## Native boundary

The shell remembers the last opened project under application data; initial workspace inspection restores it without creating a replacement when the remembered directory is missing. Project content remains in its chosen directory. Native dialogs, clipboard and artifact opening go through scoped Entwurf commands, rather than broad frontend file-system permissions. The setup report probes actual local Tesseract availability/languages. Manual ChatGPT opening requests launch of a uniquely discovered installed Start-menu application. The packaged parent bridge also performs bounded read-only capability discovery. Neither operation claims automation compatibility. Automation remains disabled until separately verified on the real desktop application.

Diagnostic export uses the core redaction builder and cached native capability booleans. The JSON contains fixed status labels, booleans and aggregate counts; it omits project/source text, paths, IDs, prompts and arbitrary error messages. The chosen destination path is returned separately to display the save result.


## Knowledge IR and imported visual proposals

Sources can restore an exported Knowledge IR into an empty task through native
file selection. Existing versions disable the action and can be kept while the
user creates a fresh task. Referenced originals must already be verified within
the project; source matching and schema/provenance checks remain in the core.
The UI sends the complete selected JSON without reconstructing fields.

Tasks shows the current version's persisted GPT visual proposal, warnings,
image-reference status and compiler-reported adoption status. Proposed pages
and groups do not claim to be actual output. Unretrieved image references are
labelled unapplied and never opened by the frontend.
