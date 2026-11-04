# Home and settings reference refinement — Task 8

final result: passed

The shared production UI passes the selected visual-direction gate. The homepage now centers a real task composer; project management is collapsible and the actual task list stays in the sidebar. Settings has a searchable category column and centered groups of setting rows. Native minimize remains a real Tauri action; browser windows do not show simulated OS controls. Installed Windows acceptance belongs to the root agent and is not claimed here.

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

The local QA server remains available on port 1448. No deployment or push was performed.
