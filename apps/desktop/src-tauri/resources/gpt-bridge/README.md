# Windows bridge package resources

Run `scripts/windows/prepare-gpt-bridge.ps1` from a native Windows checkout before
building Tauri. It publishes the .NET 10 bridge and runtime here as self-contained
Windows x64 resources, so the installed app does not require a .NET installation.
Generated binaries are ignored. `scripts/windows/build.ps1` runs this preparation
before any Cargo/Tauri build; no startup download or runtime package installation
is performed. Missing resources safely leave manual handoff available.

The desktop uses `--automation-stdin` for the separate original-image workflow.
Its inherited framed stdin contains the local transport capability and owned
state directory, never account credentials. EOF or parent exit ends this child.
Submission requires an explicit new run and a verified interactive ChatGPT image
surface. Durable intent prevents repeated input or submission after reconnect;
original bytes remain staged until the destination project acknowledges them.

The older bounded, read-only discovery protocol remains available for structured
TaskEnvelope/ResultEnvelope planning. Its compatibility gates are independent of
the original-image workflow. Real-account acceptance is recorded in
`docs/development/automation-web-acceptance.md`.
