# Windows ChatGPT image automation

The image executor uses the user's existing Windows ChatGPT application through UI Automation controls. It connects to the same companion coordinator used by the browser extension. It never reads a browser profile, credentials, cookies, application databases, or private ChatGPT endpoints.

## Parent process contract

Start `Entwurf.Bridge.exe --automation-stdin` with redirected stdin, stdout, and stderr. Write one existing `FrameCodec` frame: four-byte little-endian JSON length followed by UTF-8 JSON.

```json
{
  "websocket_url": "ws://127.0.0.1:48123/api/automation/extension",
  "token": "<companion extension capability>",
  "state_directory": "C:\\...\\Entwurf\\automation\\windows",
  "parent_pid": 1234
}
```

Keep stdin open for the child's lifetime. EOF or additional input closes the executor. The child also observes the original parent process and its start time. The capability travels only in the inherited bootstrap frame and the first WebSocket authentication frame; it is not placed in arguments, URLs, logs, or executor state.

The WebSocket contract is identical to the browser adapter's contract in `sidecars/chatgpt-browser/README.md`: authenticate first, then `probe`, `start`, `status`, `cancel`, `artifact`, and `artifact_accepted`. Probe identifies `adapter: "chatgpt-windows"`. A `start` contains `run_id`, `task_id`, `base_version_id`, `prompt`, and `prompt_sha256`.

The state directory belongs exclusively to one executor. `runs.json` stores run identities, prompt hashes, phases, and artifact metadata; it does not store prompts. `adapter.lock` prevents two processes operating the same state. Original downloads and staged image files are confined to that directory. Run identifiers cannot contain paths.

## Execution and recovery

`WindowsImageSurface` does not require an unlocked input desktop or foreground focus. It calls UIA ValuePattern, InvokePattern and ExpandCollapse directly, without calling SetFocus/SetForegroundWindow or injecting keyboard/mouse input. Each mutation verifies the trusted package, exact HWND, PID and process start time before and after the call. Controls must still expose the required pattern and be reported visible and enabled; inaccessible, offscreen or unsupported controls stop the operation. Removing the desktop/focus guard does not establish successful locked-session generation on the installed ChatGPT version.

1. Verify the OpenAI Store publisher/package and ChatGPT product mode. Use the validated original source window for later runs in the same adapter lifetime. After restart, an already-foreground eligible window can disambiguate multiple sources; otherwise refuse an ambiguous selection. Enumeration order never chooses a source, and the adapter does not bring a source to the foreground.
2. Snapshot eligible window identities, expand the source's application File menu through UIA, and invoke its unique visible enabled New Window command once. If the command is absent or unsupported, stop with `independent_window_unavailable`. Accept exactly one new HWND from the same process and process start time. Bind that window and its document, verify an empty fresh ChatGPT page without transcript or generation, then open Images in that new window. Wait for the actual image-page heading, model picker, and a unique empty writable composer. Never navigate, fill, or clear the original conversation/draft; keep both windows open.
3. Set the prompt through UIA ValuePattern, verify the exact value and enabled Send control, then durably record submission intent before the sole Invoke. Cancel and submit transitions have one ordering.
4. Bind the response to a newly observed user turn matching this prompt and its actual level-four speaker heading. Follow the document's accessibility order from that user heading into the assistant response and stop at the next user heading. Accept a newly observed image/action only within its assistant fragment; reject user attachments, old images, ambiguous images, changed conversations, and active generation. Require stable image/control identity before downloading.
5. Invoke that response's original-image download, opening its preview if required. Use the standard Save As dialog whose native owner chain reaches the exact bound window, and a new file under the run's owned directory. A shared process ID alone does not establish dialog ownership. Never scan the user's Downloads directory. Screenshots cannot become artifacts.
6. Read bounded original bytes, check the image signature and SHA-256, and stage them atomically. Send `artifact` and remain `awaiting_import`. Only `artifact_accepted`, after the destination application's validated database commit, persists `completed` and removes staged bytes.

Disconnected/restarted sessions never resubmit. Reconnection reports durable state and retransmits an existing staged artifact. Interrupted runs without a completed artifact pause. A `status` request can resume observation/download for an already submitted run whose minimal UI binding was persisted. This recovery path never prepares a new chat, fills the prompt, or submits. It revalidates the original process/start time, document, and new user turn against the stored prompt hash. A bound image preview can be closed to revalidate its source; an unrelated conversation is refused. The destination image import retains responsibility for full decoding, dimensions, and its application transaction.

## Observable requirements and validation status

On the inspected installed build, the Store package is named `OpenAI.Codex` while the application exposes a genuine ChatGPT product selector and Images page. The executor therefore requires both the known OpenAI publisher/package identity and the actual ChatGPT controls. A package or process name by itself never authorizes typing into a coding/work task.

Earlier local inspection, without sending, found the global ChatGPT mode selector, global Images navigation, image-page heading, a writable ValuePattern composer, ChatGPT model picker, disabled Send for an empty composer, Back navigation, and the fixed `You said:` / `ChatGPT said:` headings (Chinese equivalents included) as UIA Text + HeadingLevel.Level4 + ARIA heading. The existing window was minimized, and Windows was locked. Those observations established selector candidates, not successful mutations or generation while locked. A connected adapter refreshes its read-only capability probe every five seconds so changes in the application's accessible controls update availability without restarting it.

The observed renderer `文件` / File menu supports UIA ExpandCollapse. Its ARIA menubar sits two groups below the outer application Document, with the four `application-menu-trigger-{file,edit,view,help}-menu` controls. The expanded File menu uses `application-menu-content`. These anchors are required; matching labels inside transcripts or embedded documents cannot authorize window creation. The adapter polls the asynchronously refreshed menu after one expansion, for at most three seconds, before refusing an absent command. Public-source inspection on 2026-11-10 of Store package `OpenAI.Codex_26.1103.4239.0_x64__2p2nqsd0c76g0` confirms `createFreshWindow('/')` and File-menu insertion guarded by `multiWindow`, supplied by renderer feature gate `459748632`. This proves the shipped implementation, not that the current signed-in application's gate is enabled. An earlier live menu inspection did not expose `新建窗口` / New Window and created no independent window; the current version requires a fresh live check. Independent-window creation, fresh-page acceptance, generated-response and original Save As selectors still require coordinated live validation, including an actually locked session. Automated fixtures validate correlation and refusal behavior; they do not claim that all provider UI variants are validated. An unsupported layout pauses with a reason code and never silently switches to screenshots or repeats a send.

Frame capture requires one outermost Document and uses only nodes directly owned by it; nested document subtrees cannot supply duplicate controls or composer evidence. Independent outer documents, missing parents, duplicate identities and cycles are refused. The exact outer Document runtime identity remains bound throughout preparation and submitted-run recovery.

Probe reasons include `ready`, `chatgpt_not_found`, `ambiguous_chatgpt_windows`, `chatgpt_mode_required`, and `image_surface_unavailable`. Run reasons also include `independent_window_unavailable`, `chatgpt_login_required`, `user_changed_composer`, `conversation_changed`, `ambiguous_images`, `generation_timeout`, `download_unavailable`, `download_dialog_unavailable`, `download_failed`, `artifact_invalid`, `submission_uncertain`, `adapter_restarted`, `adapter_busy`, and `cancelled`. Older companions or retained runs may still report `windows_locked` or `foreground_unavailable`; update the local companion and reconnect instead of treating those legacy reasons as current image-adapter requirements. Reconnection never replays a paused run.

Run the native .NET tests with `dotnet test sidecars/gpt-bridge/Entwurf.Bridge.Tests/Entwurf.Bridge.Tests.csproj`. New tests use fake UI providers, real temporary files, and a localhost WebSocket. The process bootstrap test deliberately withholds authentication success, so it does not inspect or operate ChatGPT. On 2026-11-04, the native Windows suite passed all 173 tests in a private scratch checkout, including document ownership, window identity, menu readiness, source selection, draft preservation, cancellation and resume refusal cases. This is automated fixture evidence; it does not replace the pending independent-window live acceptance.

For the 2026-11-10 direct-pattern change, the self-contained `win-x64` publish and production/full-test-project cross-compilation passed with zero warnings or errors. A source-linked Linux test project passed 45 pure tests. The subsequent complete native Windows suite on an exact main `96e4530` source snapshot passed 184 tests with SDK 10.0.403. Those automated checks do not replace real locked-session image acceptance. One real MSAA Retry action on the client’s startup dialog completed while locked, but the application returned to its organization-settings startup error without exposing the ChatGPT page. Independent-window preparation, input and generation remain unverified.

The structured `TaskEnvelope` bridge remains a separate named-pipe entrypoint. Its `WindowsAutomation.CheckSafety()` still checks an unlocked input desktop and foreground target. Those existing capability gates and protocol are independent of the direct-pattern image executor and must not be presented as image-mode restrictions.


### Explicit recovery before submission

`resume_pre_submit` accepts `{request_id, run}` with the original full run identity and prompt hash. It is a separate, explicitly requested operation; ordinary `start`, `status`, and reconnect never retry input. The adapter requires a paused run with an explicitly stored `submit_intent: false`, no UI binding or retained/downloaded original, and no active worker or other nonterminal run. Missing historical intent remains unknown and cannot qualify.

The canonical UUID request ID is retained before UI work, with a maximum of 64 IDs per run and no eviction. Duplicate IDs only report status; IDs previously used by another run are rejected. A rejected request emits `resume_rejected` followed by the current status. Status adds `pre_submit_retryable` and the most recently accepted `resume_request_id`; eligibility is refreshed after the prior worker actually finishes.

Failures retain only the fixed `stage` (`prepare`, `write`, `submit`, `wait`, `download`, `report`), exception type names (at most three), and a fixed `failure_method`. Messages, stacks, prompts, and UI content are not persisted. A reporting failure preserves verified staged original bytes and their awaiting-import state.
