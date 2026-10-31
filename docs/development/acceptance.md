# Acceptance checklist

## Automated in this repository

- `cargo test --workspace`: IR, contract, ingestion, layout, validation,
  correction rollback, project versions, storage and SVG/PNG/PDF output.
- `cargo test -p entwurf-cli`: public Chinese probability sample, Chinese path
  with spaces, CLI compile, IR re-import and project persistence.
- `pnpm test`: five-page navigation, command palette, automation default and
  manual handoff.
- `pnpm build`: TypeScript and production bundle.
- `pnpm exec playwright test`: rendered dark/light/mobile screenshots and export
  interaction.

## Manual / Windows-only

- Tauri NSIS installation, uninstall and clean-profile startup.
- WebView2 accessibility discovery against the current ChatGPT desktop version.
- DPI 100/125/150%, multiple monitors and window switching.
- Tesseract OCR with `chi_sim+eng` installed by the user.
- A licensed private probability chapter in `fixtures/private/` (never commit).

Private fixtures must not be copied into Git. The public synthetic chapter is
the CI gate; it covers definitions, formulas, theorems, warnings and source
anchors without redistributing textbook material.
