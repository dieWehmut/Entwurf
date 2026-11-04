# Home and settings reference refinement — Task 8

final result: passed

The shared production UI passes the selected visual-direction gate. The homepage now centers a real task composer; project management is collapsible and the actual task list stays in the sidebar. Settings has a searchable category column and centered groups of setting rows. Native minimize remains a real Tauri action; browser windows do not show simulated OS controls. Final installed Windows and deployment verification is recorded below.

## Source and capture normalization

- Selected home: `/mnt/c/Users/han/AppData/Local/Temp/tmpE5C8.png`, measured **2559 × 1494 physical pixels**.
- Selected settings: `/mnt/c/Users/han/AppData/Local/Temp/tmp1AF6.png`, measured **2559 × 1492 physical pixels**.
- Image-tool display initially showed these near 2048 px wide. For explicit comparison the sources were resampled using Lanczos to **2048 × 1196** (home) and **2048 × 1194** (settings), approximately 0.8003×. Source OS scaling/DPR is unknown; this normalization does not assert it was captured at 125% scaling.
- The implementation was captured at exactly those CSS viewports, DPR 1, against `http://127.0.0.1:1448/Entwurf/`, using the production build and an isolated Playwright Chromium context. No signed-in browser was controlled.
- A temporary `FONTCONFIG_FILE=/tmp/entwurf-task8-fonts.conf` points the QA process at the machine's existing Windows fonts. This lets the existing Segoe UI / Microsoft YaHei CSS stack resolve while comparing with the Windows reference. No font files or system font settings were changed or copied into the repository. Separate default Linux fallback screenshots are included below.
- The approved compact titlebar remains 36 CSS px; the reference's normalized titlebar is about 49 px. Rail + task/category sidebar is 316 px versus approximately 326 px in the normalized references. These small chrome differences preserve the existing compact shell. The composer is 720 px wide; the settings cards are 816 px wide, aligned at approximately x=774 at the matched viewport, versus about x=769 / 815 px in the reference.

## Combined comparisons inspected

All files are local, outside git, under `/tmp/entwurf-task8-qa/`:

- `home-comparison.png`: normalized reference left, real WASM home right, equal viewport dimensions.
- `settings-comparison.png`: normalized reference left, real WASM general settings right, equal viewport dimensions.
- `home-detail-comparison.png`, `settings-detail-comparison.png`: readable focused crops with source above implementation, without scaling the individual crops.
- `window-controls-comparison.png`: source and native-controls browser fixture crop, at equal pixel scale. The smaller compact titlebar is intentional.
- `desktop-dark.png`, `desktop-light.png`, `settings-dark.png`, `settings-light.png`: actual retained browser project after task creation, Markdown import and node edit.
- `first-use-dark.png`, `menu-dark.png`: first-use project form and working File menu.
- `mobile-dark.png`, `mobile-light.png`, `mobile-tasks.png`, `mobile-settings-dark.png`, `mobile-settings-light.png`: 390 × 844 CSS px, including active search and keyboard focus.
- `native-home-dark.png`, `native-settings-dark.png`: Tauri IPC **browser fixture**, same full comparison viewports; not installed Windows screenshots.
- `native-home-1280.png`, `native-settings-1280.png`: 1280 × 808 CSS px native-controls fixture.
- `browser-fallback-mobile.png`, `browser-fallback-settings-mobile.png`: actual default Linux font fallback at 390 × 844, without the temporary font configuration.

## Findings, corrections, and final fidelity pass

1. **P2, home toolbar — fixed.** The current-task label wrapped onto two lines at desktop width during initial visual inspection. The compact selector now keeps its label on one line; final home captures show the correction.
2. **P2, home placement — fixed.** The first composer sat approximately 50 px below the intended composition. Increased lower breathing room raises the composer while keeping the heading and controls together; the final equal-viewport comparison shows the intended centered 720 px input. Narrow screens use their own padding.
3. **P2, project form reopening — fixed.** A first-use `<details>` forced open by a derived prop could become out of sync after a manual collapse. Its actual open state is now controlled consistently. The final real browser flow collapses it, reopens and focuses it from both the sidebar and File menu, creates a project, and later reopens a saved project from the sidebar picker.
4. **Typography environment difference — normalized, documented.** Initial Linux captures used the bundled WenKai fallback because Windows UI fonts were unavailable to fontconfig. Scoped fontconfig now tests the real existing Windows CSS choices; separate unmodified fallback evidence records the Linux appearance. Native Windows rendering remains an independent installed check.

Required fidelity surfaces:

- **Fonts/typography:** the existing Windows UI font stack produces neutral sans serif UI in the normalized captures. Large home/general headings, 13–14 px rows and secondary descriptions maintain hierarchy. Long project/task names truncate in chrome, while body text wraps. Linux fallback differs in character style but remains legible without overflow.
- **Spacing/layout:** narrow icon rail, separate real task sidebar, quiet centered composer, and settings category→content arrangement follow the source. Cards use grouped rows and separators with headings outside the cards. First-use, mobile and 1280 px views remain coherent. There is no extra task column on settings.
- **Colors/tokens:** retained neutral #181818 canvas, #222222 grouped surfaces and subtle gray borders follow the source's dark gray direction. Existing light tokens provide corresponding light surfaces. Focus rings and real action enabled/disabled states remain visible.
- **Assets/icons:** supplied Entwurf PNG branding is retained. Existing Lucide icons provide consistent line weights, including a clearer 18 px minimize glyph. No decorative imagery was needed or generated. No source account or conversation text was copied.
- **Copy/content:** references guide composition, while genuine Entwurf task, chapter, OCR, ChatGPT, storage and diagnostic controls supply content. The reference's long account-settings list and full chat history are intentionally absent: Entwurf only displays actual categories and saved tasks. The optional chapter line, project selector, app footer, and real create-task arrow are intentional production differences; no fake voice/chat controls were added.

No remaining actionable P0/P1/P2 visual issues were found in the scoped shared-UI comparison. Pixel-identical reproduction of another product's account content, OS frame, or display scaling is not claimed.

## Verification

- Full frontend suite: 99 tests passed. Focused shell/app coverage also passed after the final project-form correction, including native minimize wiring, no drag-region ancestor, close protection, sidebar preferences and draft retention.
- Ordinary production build, TypeScript, and `/Entwurf/` production build passed using pnpm 10.34.9.
- All six existing real-WASM browser cases passed: Markdown/edit/reload/export, two-tab project isolation, PDF/OCR ingestion, stale draft protection, EXIF source dimensions, and settings/OCR persistence. The project-creation helper now opens the actual File menu before filling a potentially collapsed form.
- Extended shell browser flow passed with real WASM/IndexedDB: sidebar/File form focus, Enter to create a task, Markdown import, edit/save, Back/Forward draft safety, saved-project picker focus/reopen, search by setting content, no results/Escape, theme and OCR reload, mobile task selection, and no horizontal overflow. Page errors are empty.
- Separate native-controls browser fixture passed keyboard minimize IPC dispatch and drag-region exclusion and produced desktop/narrow captures. Unit tests retain real close-request prevention coverage. Actual Windows minimize/restore/close and native dialog execution remain the root's installed-acceptance step.
- The Windows acceptance driver's cycle-zero project entry now uses File → Open/New Project; all existing assertions and task/chapter selectors are preserved. `node --check` passed.

The isolated implementation preview used port 1448. That implementation handoff did not deploy or push; the final integration and deployment are recorded below.

## Review follow-up: mobile project picker

The Important finding in `task8-review.md` is resolved. At widths ≤760 px, opening the saved-project picker now closes the task sidebar before focusing the picker. Desktop sidebar behavior is unchanged.

The regression was added to the existing real-WASM shell flow, and demonstrated failing on the original implementation (the sidebar remained present). After the fix it passes at **390 × 844**: explicitly open sidebar → click **切换项目** → verify sidebar gone and picker focused → click the saved project without forced input → observe the actual open completion and retained task → verify the project form closes. The flow then reopens the sidebar and continues existing task/settings checks.

`/tmp/entwurf-task8-qa/mobile-project-picker.png` is the post-fix screenshot, opened and inspected: the saved-project button is visible and unobstructed. The original desktop/reference comparisons remain applicable; this follow-up changes only narrow-screen sidebar dismissal.

Targeted follow-up verification: `pnpm exec vitest run src/Shell.test.tsx` (**8 passed**), `pnpm exec tsc -b`, `pnpm exec vite build --base=/Entwurf/`, and the existing real-WASM shell browser case (**1 passed**). The 99-test suite and six broader browser cases above are the preceding `be3a20b` verification and were not repeated for this one-line fix. The separate final follow-up commit hash is recorded in `/home/han/project/Entwurf/.superpowers/sdd/2026-11-01-automation-web/task8-report.md`. No installed/account/backend/protocol state changed.

## Final installed and deployed verification — 2026-11-05

The shared-UI follow-up review approved the final implementation. It was integrated and pushed as `bd83b071be16c2abf352cc167514e2fe8f5d89f1`; the integrated application and scripts exactly match the reviewed implementation.

The actual Windows NSIS installer passed two complete uninstall/reinstall cycles, **9 and 10**, with **15 of 15 checks passing in each**. The actual window minimized/restored and maximized/restored, an unsaved draft prevented close, and the final close button exited the app and its owned bridge. Five installed pages rendered in both themes without page errors. All 173 native .NET tests passed before bundling.

Installed screenshots are under `/mnt/c/Users/han/AppData/Local/EntwurfAcceptance/v0.1-20261031/cycle-9/` and `cycle-10/`, including `installed-project-dark.png`, `installed-project-light.png`, `installed-settings-dark.png`, and `installed-settings-light.png`. Root opened the cycle-9 home and dark/light settings captures and confirmed the centered composer, grouped searchable settings, visible minimize icon and readable Windows fonts. Cycle 10 uses the same installed executable hash and independently records both themes. The reference supplies visual direction; identical source display scaling is not asserted.

Each installed capture measures **1282 × 808 CSS px**, DPR **2**, PNG **2564 × 1616 px**. The WebView reports a 1280 × 800 CSS-pixel screen. No OS scaling or user font setting was changed; this does not claim testing other display scales.

[Pages run 41862954637](https://github.com/dieWehmut/Entwurf/actions/runs/41862954637) successfully built and deployed the same source. Its fresh **99 frontend tests**, **12 real browser cases**, and WASM smoke checks passed. The live page's `index-DI5QmPdj.js` has SHA-256 `a54e43eaf60f16003bc56f26e9cec7cb32cfb64f13e6e3a4ac604d742b9c8d33`, exactly matching the reviewed production preview. The original dedicated Pages profile reopened the same project, task and r1 knowledge version, retained its local-service connection and reported no network failures or generation requests.

The local WebUI and installed desktop also reopened their preserved public-source tasks. Real ChatGPT generation remains a separate incomplete acceptance gate: the application's Windows session is disconnected and locked. These visual, persistence and installation checks do not claim a live generated image.
