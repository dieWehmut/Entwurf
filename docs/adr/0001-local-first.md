# ADR 0001: local-first compiler, explicit model boundary

Status: accepted for v0.1.

## Context

Entwurf turns licensed textbook material into a visual knowledge sheet. Blind
model automation can corrupt formulas or silently invent unsupported facts.
Users also need the core workflow to work when offline.

## Decision

- Source ingestion, IR editing, deterministic layout, validation and all exports
  are local Rust operations with no model dependency.
- GPT is an optional planner. It receives a complete `TaskEnvelope` and returns a
  `ResultEnvelope` patch; the local compiler remains the authority for merging,
  validation and rendering.
- Corrections are recorded and rollbackable. User edits are never overwritten by
  a later model result.
- The Windows UI automation sidecar is disabled until it passes four observable
  compatibility gates. When it cannot observe controls, the product falls back
  to explicit copy/open/import handoff.

## Consequences

The first release ships a useful offline CLI/core even if Windows automation
cannot be validated. The desktop UI can be demoed without an account or network.
The trade-off is that fully automatic ChatGPT input is not part of the default
v0.1 path; manual handoff is the safe default.
