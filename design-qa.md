# Compact titlebar and task sidebar design QA

Final result: passed

The selected references guide the shared Entwurf shell. The implementation removes the breadcrumb, adds a compact working titlebar, makes the navigation icons smaller, places a theme switch above the settings gear, and adds a collapsible sidebar containing the current project's actual tasks. Existing project, source, review, image and diagnostic workflows remain available.

## Sources, normalization and evidence

- Breadcrumb removal reference: `/mnt/c/Users/han/AppData/Local/Temp/tmp819A.png`, 713 × 107 pixels.
- Titlebar reference: `/mnt/c/Users/han/AppData/Local/Temp/tmpD4B2.png`, 2529 × 74 pixels.
- Task sidebar reference: `/mnt/c/Users/han/AppData/Local/Temp/tmp1F7.png`, 401 × 1427 pixels.
- The references have different crops and sampling scales. The approved interpretation is a compact 36 CSS px titlebar, a 60 CSS px rail, 18 px navigation icons and a 256 px task sidebar. The settings category column remains 240 px. Reference physical pixels are not claimed to equal CSS pixels.
- Real browser implementation: `http://127.0.0.1:1436/Entwurf/`, production build, real Rust WASM and IndexedDB. Desktop captures use 1440 × 1000 CSS pixels at DPR 1; mobile captures use 390 × 844. Browser tooling was unavailable, so authorized Linux Playwright was used.
- Combined comparison inputs were created and opened at `/tmp/entwurf-titlebar-qa/titlebar-comparison.png` and `/tmp/entwurf-titlebar-qa/sidebar-comparison.png`. The first shows the titlebar source at approximately half its physical dimensions above the actual 36 px implementation. The second aligns the reference navigation with the actual task sidebar; different task content and document length are intentional.
- Final screenshots: `/tmp/entwurf-titlebar-qa/desktop-dark.png`, `menu-dark.png`, `settings-dark.png`, `settings-light.png`, `mobile-light.png`, and `mobile-tasks.png`. Captures disable CSS transitions. The root reviewer independently opened desktop, settings and mobile captures and accepted the layout.

These images show an actual public Markdown project with a retained task and saved edit. They contain no invented history, IPC mocks, or live ChatGPT output. The reference's account and conversation content is not copied into the application.

## Findings and fixes

1. **P2 — settings could gain an unnecessary fourth column; resolved.** Entering settings now hides the task sidebar without changing its persisted open preference. Leaving settings restores the workspace sidebar. The settings screenshot shows rail → categories → content directly.
2. **P2 — keyboard focus could leave the help dialog; resolved.** Help traps Tab/Shift+Tab, closes with Escape, and restores focus to its menu trigger. Modal overlays sit above the titlebar. Unit coverage verifies the focus cycle and restoration.
3. **P2 — native close protection needs the complete Tauri event contract; resolved and verified installed.** `onCloseRequested` prevents closing during an operation or unsaved node edit. Tauri's implementation calls `destroy()` after an allowed close, so both close and destroy capabilities are configured. Required permission names were checked against the generated desktop schema. Actual installed-window execution passed the final Windows cycles described below.

The final combined comparisons show no remaining actionable P0/P1/P2 visual mismatch in the approved scope. Browser window controls are omitted; native controls are conditional on the real Tauri runtime.

## Behavior and regression evidence

The following records the initial UI-branch verification. The final integrated
candidate `2f27ef9`, including automation recovery additions, passes 97 frontend
tests and the deployed Pages workflow's 12 browser cases. Root also reran the
separate real-browser shell flow successfully during integration.

- **73 frontend unit tests passed**, including six new shell cases: task sidebar/history, owned task selection with draft protection, real menu callbacks/theme, native API wiring and close prevention, settings column behavior, and help focus restoration. The first four shell cases were demonstrated failing before implementation; the settings/help cases also failed before their fixes.
- TypeScript and both ordinary and `/Entwurf/` production builds passed. No new frontend dependency was added.
- **All 23 distinct Playwright cases passed across the appropriate runtime configurations.** The initial dev run passed 21 cases. Its settings case expected the `/Entwurf/` deployment path and passed against the production Pages-base build. The multi-page fixture required an isolated Vite optimizer cache and the missing test-only Tauri event-unregister stub; all ten workspace cases then passed together. The new shell case additionally passed on the production Pages-base build.
- Real WASM cases cover Markdown edit/reload/export, separate active projects in two tabs, PDF extraction/image OCR, stale-tab edit protection, rotated JPEG anchors, settings/OCR save/reload and the new shell navigation flow. Automation tests use explicit local protocol fixtures, not a live ChatGPT session.
- The new browser shell test uses the real file chooser, creates an actual task, saves a node edit through the Edit menu, verifies draft survival across Back/Forward, reloads retained project/task/theme state, and checks mobile sidebar selection and horizontal overflow. Page errors are empty.
- Existing native-protocol presentation tests continue to cover five pages, both themes, 1280/1920 widths and 100/125/150% CSS zoom, review/export failures, IR restore and diagnostic cancellation. These are browser fixtures and do not claim Windows native execution.

## Final Windows acceptance coverage

`scripts/windows/accept-desktop.mjs` now includes actual installed-app assertions for:

1. The titlebar maximizes/restores and minimizes/restores the real window. State is read from Tauri; decorations are off and resizing remains enabled.
2. A real native `closeRequested` event preserves an unsaved node draft and its base version, then the explicit discard action restores the original field.
3. After all artifact checks and screenshots, the close button exits the owned desktop and its bridge. PID, executable path and creation time are captured before closing; matching surviving processes fail acceptance. The script does not kill a leaked bridge or weaken existing artifact checks.

Root executed the final candidate `2f27ef9` in actual uninstall/reinstall cycles 7
and 8 on 2026-11-01. Each passed all 15 installed checks, including the three real
window cases above, and preserved every project/local-WebUI file hash through
uninstall. Root opened the final installed task and settings captures in
`%LOCALAPPDATA%\EntwurfAcceptance\v0.1-20261031\cycle-8`.
They measure 1282 × 808 CSS pixels at DPR 2 and 2564 × 1616 PNG pixels. OS scaling
was not changed or independently verified; these results do not claim native
100/125/150% display-scaling coverage. Actual ChatGPT image generation remains a
separate, incomplete acceptance gate while Windows is locked.

## Assets and polish

The existing supplied PNG brand and Lucide icon library are reused. Colors retain the approved neutral gray surfaces and semantic status colors. File/Edit/View/Help entries invoke existing application functions; history controls use actual visited pages; the sidebar groups actual tasks by chapter. No decorative or nonfunctional menu entries were added. There are no outstanding visual polish blockers.
