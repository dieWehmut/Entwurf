# Windows ChatGPT image automation

The image executor uses the user's existing Windows ChatGPT application and its visible UI. It connects to the same companion coordinator used by the browser extension. It never reads a browser profile, credentials, cookies, application databases, or private ChatGPT endpoints.

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

1. Verify the OpenAI Store publisher/package, a unique application window, the unlocked input desktop, the visible ChatGPT product mode, and the global Images navigation control.
2. Restore/focus that existing window and open its Images page. Wait for the actual image-page heading, model picker, and a unique empty writable composer. Preserve existing drafts by refusing to replace nonempty text.
3. Set the prompt through UIA ValuePattern, verify the exact value and enabled Send control, then durably record submission intent before the sole Invoke. Cancel and submit transitions have one ordering.
4. Bind the response to a newly observed user turn matching this prompt and its actual level-four speaker heading. Follow the document's accessibility order from that user heading into the assistant response and stop at the next user heading. Accept a newly observed image/action only within its assistant fragment; reject user attachments, old images, ambiguous images, changed conversations, and active generation. Require stable image/control identity before downloading.
5. Invoke that response's original-image download, opening its preview if required. Use the owned standard Save As dialog and a new file under the run's owned directory. Never scan the user's Downloads directory. Screenshots cannot become artifacts.
6. Read bounded original bytes, check the image signature and SHA-256, and stage them atomically. Send `artifact` and remain `awaiting_import`. Only `artifact_accepted`, after the destination application's validated database commit, persists `completed` and removes staged bytes.

Disconnected/restarted sessions never resubmit. Reconnection reports durable state and retransmits an existing staged artifact. Interrupted runs without a completed artifact pause. A `status` request can resume observation/download for an already submitted run whose minimal UI binding was persisted. This recovery path never prepares a new chat, fills the prompt, or submits. It revalidates the original process/start time, document, and new user turn against the stored prompt hash. A bound image preview can be closed to revalidate its source; an unrelated conversation is refused. The destination image import retains responsibility for full decoding, dimensions, and its application transaction.

## Observable requirements and validation status

On the inspected installed build, the Store package is named `OpenAI.Codex` while the application exposes a genuine ChatGPT product selector and Images page. The executor therefore requires both the known OpenAI publisher/package identity and the actual ChatGPT controls. A package or process name by itself never authorizes typing into a coding/work task.

Confirmed locally without sending: global ChatGPT mode selector, global Images navigation, image-page heading, a writable ValuePattern composer, ChatGPT model picker, disabled Send for an empty composer, Back navigation, and the fixed `You said:` / `ChatGPT said:` headings (Chinese equivalents included) as UIA Text + HeadingLevel.Level4 + ARIA heading. The existing window was minimized, and Windows was locked; stale UIA visibility could therefore not substitute for foreground/desktop checks. A connected adapter refreshes its read-only capability probe every five seconds so unlocking or opening ChatGPT updates availability without restarting it.

Real generated-response and original Save As selectors still require the first coordinated live generation. Automated fixtures validate correlation and refusal behavior; they do not claim that all provider UI variants are validated. An unsupported layout pauses with a reason code and never silently switches to screenshots or repeats a send.

Probe reasons include `ready`, `windows_locked`, `chatgpt_not_found`, `ambiguous_chatgpt_windows`, `chatgpt_mode_required`, `image_surface_unavailable`, and `foreground_unavailable`. Run reasons also include `chatgpt_login_required`, `user_changed_composer`, `conversation_changed`, `ambiguous_images`, `generation_timeout`, `download_unavailable`, `download_dialog_unavailable`, `download_failed`, `artifact_invalid`, `submission_uncertain`, `adapter_restarted`, `adapter_busy`, and `cancelled`.

Run the native .NET tests with `dotnet test sidecars/gpt-bridge/Entwurf.Bridge.Tests/Entwurf.Bridge.Tests.csproj`. New tests use fake UI providers, real temporary files, and a localhost WebSocket. The process bootstrap test deliberately withholds authentication success, so it does not inspect or operate ChatGPT.

The structured `TaskEnvelope` bridge remains a separate named-pipe entrypoint; its existing capability gates and protocol are independent of this image executor.


### Explicit recovery before submission

`resume_pre_submit` accepts `{request_id, run}` with the original full run identity and prompt hash. It is a separate, explicitly requested operation; ordinary `start`, `status`, and reconnect never retry input. The adapter requires a paused run with an explicitly stored `submit_intent: false`, no UI binding or retained/downloaded original, and no active worker or other nonterminal run. Missing historical intent remains unknown and cannot qualify.

The canonical UUID request ID is retained before UI work, with a maximum of 64 IDs per run and no eviction. Duplicate IDs only report status; IDs previously used by another run are rejected. A rejected request emits `resume_rejected` followed by the current status. Status adds `pre_submit_retryable` and the most recently accepted `resume_request_id`; eligibility is refreshed after the prior worker actually finishes.

Failures retain only the fixed `stage` (`prepare`, `write`, `submit`, `wait`, `download`, `report`), exception type names (at most three), and a fixed `failure_method`. Messages, stacks, prompts, and UI content are not persisted. A reporting failure preserves verified staged original bytes and their awaiting-import state.
