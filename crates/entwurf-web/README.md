# Local WebUI

The local server uses the same Rust `Workspace`, SQLite persistence, source
retention and verified exports as the desktop. Each browser document has its own
project selection. Project and upload handles are opaque; browser clients cannot
choose arbitrary native paths.

Build the shared frontend with base `/`, then start:

```sh
cargo run --locked -p entwurf-web -- \
  --frontend apps/desktop/dist-local \
  --data-dir /path/to/entwurf-data \
  --port 48123
```

Open `http://127.0.0.1:48123`. Optional `--pdfium-dir` and `--tesseract-dir`
select installed native extraction tools. The server binds loopback only. It
checks exact Host and Origin, then requires the bootstrap session token and
document client ID on workspace APIs. Restarting changes the session token;
the UI reloads its saved project selection through a fresh client.

Uploads allow up to 64 files, each at most 32 MiB. Failed batches are removed;
successful imports remove staging only after the core has retained the source
and committed the database. Unused staging older than 24 hours is removed on
startup. Explicitly opening a project reloads SQLite after external edits.

`cargo test --locked -p entwurf-web` checks access boundaries, batch rollback,
upload ownership, actual SQLite restart and external-edit recovery. Browser
acceptance exercises the rendered shared UI against the real HTTP service.
