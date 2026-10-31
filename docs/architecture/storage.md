# Project storage

Each project directory contains:

```text
project.sqlite3                          # authoritative project/task/version/audit database
project.sqlite3-wal / project.sqlite3-shm # SQLite WAL side files while connections are active
project.json                             # portable validated recovery mirror
.project.lock                            # process lock across database commit + mirror writes
sources/<sha256>                         # copied, verified content-addressed source bytes
versions/<task-id>/<version-id>/knowledge.ir.json  # immutable saved version IR
versions/<task-id>/<ir-sha256>/knowledge.ir.json   # standalone save_ir snapshots
exports/                                 # SVG, PNG, PDF, manifest and re-importable IR
logs/                                    # local diagnostics
```

SQLite is compiled into the Rust application through `rusqlite`'s `bundled`
feature. No external SQLite installation is needed. Connections use WAL,
`foreign_keys=ON`, `synchronous=FULL` and a five-second SQLite busy timeout.
The v1 migration creates project, tasks, versions and audit tables in an immediate
transaction and records `PRAGMA user_version=1`. Unsupported future database
versions are rejected without migration.

A save validates the complete project schema and cross-project/task identities.
It compares persisted history before inserting/updating project, task and version
records and appending audit rows in a single immediate transaction. The complete
serialized project is retained alongside those records; this preserves all IR
fields, IDs, UTC timestamps, ordering, layout, reports, settings and provenance.
Previously saved version identity, revision, creation time, reason, IR and prompt
hash cannot change. Layout, validation and artifact references may be attached to
an existing version. Removing tasks or historical versions, or changing/removing
an audit prefix, is rejected. User edits and rollback create new versions.

IR files are installed without replacement and existing bytes must match exactly.
Source import verifies the supplied SHA-256 against copied bytes and stores one
file per hash regardless of original name or extension. Callers record the
returned path in `SourceFile.stored_path`; ingestion must preserve the source kind
rather than infer it from the extension of the copied file.

Mutable files use a unique temporary file in the destination directory, flush the
file, then replace the destination. `tempfile` implements Windows replacement via
`MoveFileExW(MOVEFILE_REPLACE_EXISTING)`. On Unix, directory metadata is also
flushed. Project operations hold an OS file lock through database commit and JSON
mirror replacement to prevent concurrent readers/savers publishing a stale
mirror. SQLite and filesystem writes are not one atomic transaction: interruption
can leave unreferenced IR files or an old mirror, while committed database state
remains authoritative.

`load_project` validates the mirror if present and loads the committed database.
It repairs a missing/stale mirror and missing immutable IR files. Differing
immutable files, malformed snapshots and unsupported schemas produce an explicit
corruption error. They are never silently coerced or migrated. If the database is
missing, `load_project` and `rebuild_database` reconstruct it from the validated
mirror and preserve the exact serialized IR. Saving after database loss, or after
an interrupted initialization leaves migrated tables with no authoritative
project row, first restores validated mirror history before accepting a new save.
An existing corrupt database must be quarantined with its WAL/SHM files before calling rebuild; the
rebuild API never overwrites an existing database.

Recovery requires `project.json` to contain the latest committed mirror. If both
the database and the latest mirror are lost, only surviving standalone immutable
IR snapshots remain. Windows filesystem durability, locking and installation
behavior still require native release acceptance; Linux tests cannot establish
power-loss durability on NTFS.
