# Entwurf ChatGPT browser adapter

Independent implementation; `fwerkor/local-shell-mcp` revision
`3e651c27967b09e5ef6270474f9e48ee7e10110e` was read for architectural reference only.
No source was copied. The reference exposes tools to ChatGPT as an MCP client;
this extension instead executes explicitly requested image tasks in the user's
existing signed-in ChatGPT browser session.

## Installation and connection

Load this directory as an unpacked Chrome/Edge extension in the browser profile
where ChatGPT is already signed in. Keep an existing `https://chatgpt.com/` tab
open; reload that tab once after installation so the content adapter can attach.
This does not create a new browser profile, copy cookies, or automate login.

Start the Entwurf local companion on port 48123. Its authenticated setup endpoint
`GET /api/automation/pairing` supplies an object containing `extension_token` and
`websocket_url`. Paste that object into the extension popup. The address must be
`ws://127.0.0.1:<port>/api/automation/extension` (or `localhost`); remote hosts,
query credentials, embedded usernames/passwords and other paths are rejected.
The token goes only in the first WebSocket frame and local extension settings.

The three Entwurf frontends use the same local companion. On an explicit start,
the extension verifies an existing ChatGPT tab exposes a composer, then opens a
new ChatGPT tab in **that same browser profile and window**. This preserves the
user's existing conversation and draft. No prompt is submitted during discovery,
pairing, reconnection or installation.

## Companion protocol

Client first frame: `{"type":"authenticate","token":"..."}`.
Server acknowledgement: `{"type":"authenticated"}`. No task messages execute
before that acknowledgement. The server must authenticate the extension and
authorize each task before sending it.

Server messages:

```json
{"type":"start","run":{"run_id":"run-1","task_id":"task-1","base_version_id":"version-1","prompt":"Generate an image...","prompt_sha256":"sha256-of-exact-UTF8-prompt"}}
{"type":"status","run_id":"run-1"}
{"type":"cancel","run_id":"run-1"}
{"type":"artifact_accepted","run_id":"run-1"}
{"type":"probe"}
```

Extension messages:

```json
{"type":"probe","available":true,"adapter":"chatgpt-browser","origin":"https://chatgpt.com","reason":"composer_detected"}
{"type":"status","run_id":"run-1","phase":"generating"}
{"type":"artifact","run_id":"run-1","task_id":"task-1","base_version_id":"version-1","mime_type":"image/png","bytes_base64":"...","sha256":"..."}
```

Wire phases are `preparing`, `submitting`, `generating`, `downloading`,
`awaiting_import`, `completed`, `paused`, `failed`, `cancelled`. Local records
retain `submit_intent`, `submitted` and `submission_uncertain`; these map to
`submitting`, `generating` and `paused` (detail `submission_uncertain`). A run ID
is permanently bound to its task, base version and prompt hash. Submit intent is
persisted in `chrome.storage.local` before the only click. Recovery never repeats
an ambiguous submission. Prompts are not persisted in the run records.
Cancellation received while intent is being persisted prevents the click.
Late acknowledgement or cancellation for an earlier run cannot release a newer run.

One run may be active. Cancellation stops local observation and import; it
reports `external_generation_stopped:false`. A failed/paused result never means
that ChatGPT generation was stopped. Image bytes remain in extension IndexedDB
until `artifact_accepted` confirms the destination database committed them.
Reconnection can replay that artifact; the server must deduplicate by run and
hash. It must not respond with acceptance merely because bytes reached staging.

## Completion and original image capture

The content adapter rejects a nonempty composer and changes made before submit.
Visible known login/signup controls produce `login_required` during discovery,
preparation and immediately before submission, including login expiry after
preparation. Detection uses fixed control IDs and exact English/Chinese control
labels, without collecting account text. `composer_detected` reports control
readiness only; absence of these login controls does not verify an account or
authenticated session. Live account acceptance remains pending.
It verifies the new user turn matches the submitted prompt and binds only the
assistant response following that turn; replacing the bound turn is rejected.
This check reads only the new turn, without collecting prior conversation text. Completion
requires a loaded image, a scoped download action, no visible generation control,
and stable image identity/dimensions for 1.5 seconds. A missing Stop button or an
old image does not satisfy completion. Navigation, multiple images/downloads,
unsupported controls and a ten-minute deadline yield bounded failure reasons.

The extension registers a browser download listener before clicking that
response's Download action. The download must name the exact conversation in its
referrer when no anchor URL is available, or match the exact URL of the response's Download anchor (Chrome omits
referrers for some blob downloads). It fetches the URL emitted by that browser download (including a
same-origin blob), not a model-returned URL or filesystem path. The fetched image
must decode as PNG/JPEG/WebP, remain within 32 MiB and 64 million pixels, and have
its exact bytes SHA-256 hashed. No screenshot fallback is used. Short-lived blobs,
missing referrers, changed controls or blocked download URLs fail explicitly.

Only browser-observed ChatGPT and `*.oaiusercontent.com` image URLs are accepted.
Normal browser credentials accompany the resource request; no cookie API,
credential storage access, private conversation API or account text scraping is
implemented. There is no arbitrary script, shell, file or navigation RPC.

## Verification status

`npm ci --ignore-scripts && npm test` exercises deterministic local DOM, transport,
authentication, duplicate submission, stale responses, downloads and import-ack
fixtures. These selectors are **fixture-verified, not yet accepted against the
user's live ChatGPT account**. The exact live composer, generated-image controls,
download referrer and blob lifetime must be checked before declaring automation
working. A real run must demonstrate original image bytes reaching each frontend's
database, reload persistence and no resend after interruption.

The Windows native UIA bridge remains separate. This extension does not establish
compatibility with the installed Windows application's ChatGPT/image surface.
