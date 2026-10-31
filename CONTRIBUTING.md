# Contributing to Entwurf

Read [the v0.1 plan](docs/plans/entwurf-v0.1.md) and the relevant architecture document before changing a contract or runtime path. This is an early prototype; describe implemented behavior separately from planned product capabilities.

Use English for code, identifiers and IR fields, and Simplified Chinese for product UI. Keep parsing, storage, compilation and validation in Rust; keep Windows accessibility experiments in the C# bridge. Update the JSON Schemas and compatibility tests together when changing serialized contracts.

For local verification, run `cargo test --locked -p entwurf-core -p entwurf-cli` and, from `apps/desktop`, `pnpm test` and `pnpm build`. Run `cargo fmt --all -- --check` before submitting Rust changes. Native shell and sidecar changes also require the commands in [the Windows guide](docs/development/windows.md). Include the commands, results and unverified platform gates in the change description; a browser preview is insufficient evidence for a Windows release.

Add focused regression tests for meaningful runtime behavior. Keep changes reviewable and preserve source anchors, original formula text, correction history and user edits. Automation must remain disabled until discovery, reliable input, completion detection and retrieval pass on the actual target application version.

Use public synthetic fixtures for committed tests. Put real textbooks, screenshots, exported personal knowledge and local diagnostics in ignored private paths. Do not include login state, API keys, cookies or user documents in patches or issue reports.

Contributions intentionally submitted for inclusion are licensed under Apache-2.0, unless explicitly marked otherwise. Preserve third-party notices and document the origin and license of added assets or dependencies. See [SECURITY.md](SECURITY.md) for vulnerability reports.
