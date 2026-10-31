# Entwurf GPT Bridge

Windows .NET 10 sidecar with FlaUI 5/UIA3 **read-only ChatGPT discovery** and a
current-user named-pipe transport. Automation remains unavailable/default off:
there is no verified ChatGPT input, completion, or capture adapter in this build.
All protocol methods exist; unavailable operations return explicit errors.

Discovery checks the Windows package identity, not just `ChatGPT.exe`: Codex
also uses that executable name. Only `OpenAI.ChatGPT` / `OpenAI.ChatGPT-Desktop`
Store package identities with the expected publisher qualify. Non-Store builds
currently require manual handoff. Discovery inspects control types and pattern
availability without reading conversation text, account details, or edit values.

## Parent bootstrap (internal IPC, not a public CLI)

1. Parent creates an inheritable anonymous pipe and a cryptographically random
   32-byte token (64 hex characters); choose `entwurf-bridge-<32 lowercase hex>`.
2. Spawn `Entwurf.Bridge.exe --bootstrap-handle <inherited read handle>`.
   The Rust desktop parent instead uses `--bootstrap-stdin` with redirected piped
   stdin. This is the same single framed bootstrap, not an interactive/public CLI;
   interactive stdin is refused.
   Never place the token on the command line, in environment variables, or logs.
3. Write a length-prefixed UTF-8 JSON bootstrap object into the anonymous pipe:
   `{"pipe_name":"entwurf-bridge-...","token":"...","artifact_directories":[]}`.
   Directories must be explicitly authorized by the user, exist, and have no
   reparse-point ancestors. Maximum 16 roots. Only the parent supplies these roots.
4. Connect to `\\.\pipe\<pipe_name>` as the current user. Server uses
   `PipeOptions.CurrentUserOnly`, one instance, one client, and byte mode.
5. Call `attach` with `params.token`. The token is compared in constant time
   through SHA-256, consumed once, and its stored digest zeroed. A second attach
   returns `already_attached`. Authorization is scoped to this connection.
6. Closing the pipe cancels pending work; reconnect by starting a fresh sidecar
   with a new token. The sidecar never writes protocol traffic or secrets to stdout.

Bootstrap/connection timeout is 15 seconds; attach has a separate 15-second
absolute deadline. An authenticated idle/partial frame times out after 3 minutes.

## Wire protocol

Each frame is **4-byte unsigned little-endian UTF-8 byte length**, followed by
exactly that many bytes. Valid size is 1–1,048,576 bytes. Invalid length or truncated
frame terminates the session; a malformed JSON document returns a JSON-RPC error.
JSON depth is bounded at 64. At most 8 requests can be outstanding.

Requests are JSON-RPC 2.0 with a required string (max 128 characters) or signed
64-bit integer `id`, a `method`, and optional object `params`. Batches,
notifications, null/compound IDs, and duplicate object keys are unsupported.
Responses preserve valid IDs, contain either `result` or `error`, and use standard
JSON-RPC errors plus `-32000` application errors. `error.data.code` is a stable
machine-readable name. Error messages never include prompts or exception details.
Replies can arrive out of order; clients correlate them by ID.

```json
{"jsonrpc":"2.0","id":1,"method":"attach","params":{"token":"<bootstrap token>"}}
{"jsonrpc":"2.0","id":1,"result":{"attached":true}}
```

| Method | Parameters / behavior |
| --- | --- |
| `detect_app` | Public read-only capability report; never returns the token or authorizes the connection. Eight-second probe timeout. |
| `attach` | `token`; authenticate this connection exactly once. |
| `start_conversation` | `explicit_start:true`, `task_id`, positive `revision` (default 1). Fails `automation_unavailable` until all four gates pass. |
| `paste_prompt` | Nonempty `text`; only after an authorized task starts. |
| `submit` | Only after successful input; repeated submission is rejected. |
| `observe` | Reports idle/manual state without an active task; production completion detection is unverified. |
| `capture_text` | Only after an observed completion; production capture adapter is unavailable. |
| `watch_artifact` | `task_id`, `revision`, `timeout_ms` (1–120000, default 30000). Works in authenticated manual mode using bootstrap-authorized roots. |
| `pause` | Cancels pending work and enters paused state. |
| `cancel` | Cancels pending work and clears the local task; permits a fresh explicit start. |

Pause/cancel do **not** claim to stop ChatGPT's external generation:
`external_generation_stopped` is false. A paused task requires cancel then a new
explicit start. One automated task and one non-control bridge operation are
allowed at a time; pause/cancel remain accepted while an asynchronous file watch
or discovery operation is pending. Both public detection and start-time discovery
share one underlying probe and an eight-second cancellable wait outside the state
lock; retries reuse a probe that is still running after cancellation or timeout. Foreground/desktop-lock/target validity are
checked before and after adapter actions and every 100 ms for an active task.
The production adapter does not perform any mutating action.

## Artifact recovery

The watcher accepts only new, unique, stable files named exactly
`entwurf-task-<task_id>-v<revision>-image.(png|jpg|jpeg|webp)` in an authorized root.
Task IDs are 1–80 ASCII letters/digits/underscore/hyphen. Preexisting files,
unrelated downloads, subdirectories, and linked paths are not assigned to tasks.
Multiple candidates return `artifact_ambiguous`; arbitrary ChatGPT download names
require manual selection. Files must remain unchanged for 250 ms and open with
write sharing denied; the watcher returns the SHA-256 and byte length, capped at
32 MiB. This confirms file recovery, not image validity; the importer must decode
and validate it before use. It never moves, renames, or deletes downloads.

Watcher events trigger bounded rescans, including recovery from event overflow;
a 100-ms rescan also closes the setup/event race. All watchers allocated before a
later setup failure are disposed. Output stability is a file
integrity condition, **not** a claim that ChatGPT generation has completed.

## Release gates and limits

`automation_enabled` is true only when all four explicit gate records pass:
window discovery, reliable input, reliable completion, reliable capture. The
current production adapter always leaves the last three false. Neither RPC
parameters nor bootstrap can override this. The completion gate still requires
observable generation controls, stable valid task-matched ResultEnvelope JSON,
and confirmed artifact/save state in a real explicitly started foreground run.

No cookies, private endpoints, account bypass, clipboard reads, keyboard input,
blind coordinates, or background unattended ChatGPT operation are used. Native
host tests use a controlled in-memory adapter for state transitions; they are not
ChatGPT UIA input/completion acceptance tests. DPI/multimonitor interaction and
cross-account ACL rejection still require release acceptance testing.

## Build and test (native Windows)

```powershell
dotnet test sidecars/gpt-bridge/Entwurf.Bridge.Tests/Entwurf.Bridge.Tests.csproj
dotnet publish sidecars/gpt-bridge/Entwurf.Bridge/Entwurf.Bridge.csproj -c Release -r win-x64 --self-contained true
```

Tests include byte framing, malformed/oversized/truncated requests, error IDs,
one-time authentication, pause/cancel, task ordering, foreground/lock/structure
protection, file timeout/ambiguity/hash, and an actual inherited-bootstrap child
process + named-pipe exchange. The child-process test performs read-only package
and UIA discovery if ChatGPT is installed; it never sends or reads conversations.

## Desktop parent integration

The Tauri bootstrap launches the packaged executable from
`resources/gpt-bridge/Entwurf.Bridge.exe`, transfers fresh randomness over inherited
stdin, authenticates once over the current-user named pipe, reads `detect_app`,
and closes the connection. The client uses identification-level security QOS to
prevent server impersonation. One probe runs at a time; the parent deadline is
15 seconds plus bounded cleanup (two seconds grace, two seconds forced cleanup).
Dropping an in-flight parent future kills the owned child. Pipe responses must
have the exact request ID, valid framing no larger than 1 MiB, and all four gate
booleans consistent with the reported automation flag. Missing binaries, failed
authentication, timeouts, or protocol errors yield manual fallback with a fixed
non-sensitive error code. This parent exposes no mutating bridge RPC.

`prepare-gpt-bridge.ps1` publishes the self-contained runtime into ignored Tauri
resources; `build.ps1` calls it before the desktop build. A successful discovery
report is labelled `ready_manual`, never proof of an automated connection.

Native Rust tests in `apps/desktop/src-tauri/src/bridge.rs` include an actual
self-contained child handshake, token replay, empty stdout/stderr, bounded parent
timeout and process cleanup. Run with `ENTWURF_BRIDGE_TEST_EXE` set to the prepared
executable and `cargo test -p entwurf-desktop bridge::tests -- --include-ignored`.
