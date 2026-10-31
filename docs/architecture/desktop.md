# Desktop shell

Tauri 2 is the intended production shell. The React workspace in
`apps/desktop` is already buildable and has browser tests. The Rust command
layer in `apps/desktop/src-tauri` exposes project creation, loading, source
ingestion, compilation and default style. It deliberately does not expose an
HTTP API or a public CLI contract beyond the local acceptance harness.

## UI behavior

- Five pages: project, sources/style, tasks, result, settings.
- Always-visible theme switch; dark is pure-black-led, light is white-led, both
  use the same purple semantic accent and the same information hierarchy.
- 230 px navigation, 1120 px content column, 28 px grid, restrained glass and
  thin borders.
- Result page presents content validation before visual polish; export is
  blocked if any content or layout finding is blocking.
- Settings shows a capability report and defaults automation off. The manual
  handoff card is visible and actionable even when automation is unavailable.

## Current verification boundary

The React build, Vitest interactions and Playwright browser checks run in this
workspace. Native Windows Tauri `cargo check` and the 35 core plus 3 internal
acceptance tests also pass. These checks do not establish that the native UI is
wired to project state or that exports are readable. The full Tauri build, NSIS
installation and foreground automation remain separate acceptance gates. Bridge
protocol evidence is recorded with its own implementation and review.
