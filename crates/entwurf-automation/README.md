# Shared automation coordinator

`entwurf-automation` provides the same task coordinator to the local web host and
Tauri. It does not depend on Workspace and exposes no file, command or general
browser RPC. Construct one state per data directory and clone it within the
process; the host owns the single loopback listener.

```rust,ignore
let automation = entwurf_automation::AutomationState::new(data_dir, 48123)?;
let app = existing_router.merge(automation.router());
// Call only from a host endpoint already protected by the local application session.
let access = automation.issue_access("local", "http://127.0.0.1:48123")?;
let pairing = automation.pairing_info(); // Browser extension credential.
// A protected host setting may select the bundled Windows adapter.
automation.set_provider("windows")?;
let native_pairing = automation.native_pairing_info();
```

`issue_access(owner, exact_origin)` returns and persistently reuses a capability
for that owner/origin pair. The host decides which owners and origins may pair;
its dedicated Pages pairing flow should permit only the intended Pages origin
(by default `https://diewehmut.github.io`). Tauri registers its actual WebView
origin, such as `http://tauri.localhost`. Capabilities authorize automation only.
No capability-issuing or extension-pairing HTTP endpoint is exposed by this crate.

The router uses complete `/api/automation/*` paths. All HTTP routes require a
strict loopback Host with the configured port, `Authorization: Bearer ...`, and
the exact paired Origin. When a same-origin request omits Origin, only
`http://<validated-loopback-Host>` is inferred; requests declaring
`Sec-Fetch-Site: cross-site` cannot use that inference. Pages and native WebView
capabilities still require their own exact Origin. Known-origin preflights return explicit CORS headers
and permit Private Network Access; wildcard origins and unsupported request
headers are rejected. The extension WebSocket permits no Origin or an exact
`chrome-extension://<32-letter-id>` origin, and authenticates in the first frame
with the credential for the selected provider. No HTTP session or query token is accepted for the socket.

| Endpoint | Response |
| --- | --- |
| `GET /status` | `{connected, available, reason, provider}` |
| `POST /runs` | `{run_id, task_id, base_version_id, prompt}` → RunView, 202 new / 200 duplicate |
| `GET /runs?task_id=...` | Array of the caller owner's RunViews |
| `GET /runs/{id}` | RunView |
| `POST /runs/{id}/cancel` | RunView; external generation is not claimed stopped |
| `GET /runs/{id}/artifact` | Original bytes, Content-Type, X-Entwurf-Sha256 |
| `POST /runs/{id}/ack` | `{sha256, image_id}` → RunView |
| `GET /extension` | Frozen extension WebSocket protocol |

`provider()` returns the persistent selection, either `browser` or `windows`.
`pairing_info()` always returns the browser credential; `native_pairing_info()`
returns an independent native credential in the same `extension_token` field and
uses the same WebSocket URL. Pass the native credential to the adapter through
its protected stdin configuration, never through a URL. Only the selected
provider's token can authenticate, so a reconnecting browser extension cannot
occupy the native adapter's connection.

`set_provider(provider)` is a host API, not a public automation HTTP operation.
Selecting the same value is idempotent even while a run is active. A different
value is rejected with `run_active` until every run is terminal, including paused
runs and results awaiting import. Successful changes persist first, then disconnect
the old peer and report `available: false`, `reason: provider_changed`. Failed
persistence leaves the prior selection and connection intact. Connection changes
never send a new `start`. Old databases default to `browser` and receive a stable,
persisted native credential when opened; an existing selection survives restart.
The host may choose `windows` for a fresh installation with a native bundle but
must preserve the selection when reopening an existing database.

RunView contains `run_id`, `task_id`, `base_version_id`, `prompt_sha256`, `phase`,
`reason`, `sha256`, and `mime_type`. Optional metadata is JSON null until present.
HTTP errors use `{error: "bounded_reason_code"}`. Ownership and credentials are
never included in a run response. Unknown and foreign-owner run IDs both return
404. Only one nonterminal run exists globally; a paused run must be cancelled
before starting another.

A new run is atomically persisted before one in-memory `start` message is queued.
Prompts are never written to disk or logs. Duplicate HTTP requests return existing
state; reconnects send `status` only, and restarted unfinished runs become paused.
When the companion reports a locally unfinished run after reconnecting, durable
server cancellation/failure is reconciled with `cancel`, and durable completion
with `artifact_accepted`. Terminal confirmations do not echo additional commands;
reconciliation does not affect a newer run. There is no automatic start retry.
Staged artifacts remain available across
restart and frontend refresh.

Authenticated artifact messages must match the run, task, base version, MIME
allowlist, SHA-256, and 32 MiB limit. Bytes are stored unchanged. The consuming
application must decode the image, enforce its pixel limit, verify the destination
task version, commit the asset to its own database, and only then send `/ack` with
the actual image ID. This crate cannot independently prove a caller's database
commit. It never treats receipt in staging or an extension `completed` status as
an application import. Only the matching acknowledgement persists `completed`
and sends `artifact_accepted` to the extension. Repeated same-hash artifacts after
completion receive the acknowledgement again, allowing recovery from a lost ACK.

A failed durable write while processing an extension message sends the bounded
error and closes that WebSocket. The companion reconnects and restores its retained
artifact bytes; no prompt is replayed, and only the eventual owner database ACK
completes the run.

State updates publish to memory only after atomic persistence succeeds. State and
image files use private creation permissions where supported; state credentials
remain local to this application directory. No real ChatGPT requests occur in
the integration suite:

```sh
cargo test -p entwurf-automation
```

Tests cover localhost WebSockets, authentication, Host/Origin boundaries, CORS,
durable capabilities, no replay, owner isolation, byte/hash preservation, restart
recovery, rejected artifacts, import acknowledgements, provider/token isolation,
provider changes, legacy state migration and failed durable writes.
