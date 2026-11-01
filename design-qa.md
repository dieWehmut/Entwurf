# Shared workspace and settings design QA

Final result: passed

The selected references guide Entwurf's navigation, spacing and settings hierarchy. The approved scope retains Entwurf's existing project, source, task, review, image and diagnostic workflows. Account/chat lists and unrelated settings from the reference are intentionally absent.

## Visual sources and density

- Home source: `/mnt/c/Users/han/AppData/Local/Temp/tmpCA47.png`, 2175 × 1442 pixels.
- Settings source: `/mnt/c/Users/han/AppData/Local/Temp/tmpA745.png`, 2088 × 1436 pixels.
- Read-only Windows evidence: `C:\Users\han\project\Entwurf-automation-20261101\artifacts\live-account-20261101\window-density-readonly.json`. `GetDpiForWindow` returned 192 (200%); the normal window placement measured 2184 × 1442 pixels. The inspected window was not restored or otherwise manipulated.
- The top 90 physical pixels contain native window/menu chrome. Comparisons exclude that strip; no native chrome is reproduced in the web app.
- Home implementation: 1088 × 676 CSS pixels at DPR 2, yielding 2176 × 1352 pixels. The comparison crops one right-edge pixel to align with the 2175 × 1352 source app region; this is rounding, not image scaling.
- Settings implementation: 1044 × 673 CSS pixels at DPR 2, yielding 2088 × 1346 pixels, matching the source app region.
- Mobile implementation: 390 × 844 CSS pixels. Both DPR 1 Linux and DPR 2 native Windows captures were inspected; neither claims physical OS display-scale coverage.

## Browser-rendered evidence

Preview: `http://127.0.0.1:1438/Entwurf/` from the isolated `feat/shared-web` worktree. Browser plugin unavailable; Playwright was used. Final density comparisons use **native Windows Chromium**, including the Windows system fonts, with real WASM and IndexedDB. There is no IPC mock or automation-service fixture in these screenshots.

Private evidence directory (ignored, no screenshots committed):
`C:\Users\han\project\Entwurf-automation-20261101\artifacts\design-ui-20261101\`

| Evidence | File |
| --- | --- |
| Empty project/home, dark | `entwurf-ui-reference-home-v4.png` |
| Existing public probability project, dark | `entwurf-ui-reference-home-project-v4.png` |
| General settings, dark | `entwurf-ui-reference-settings-v4.png` |
| General settings, light | `entwurf-ui-settings-light-v4.png` |
| Mobile settings / storage / project | `entwurf-ui-settings-mobile-v4.png`, `entwurf-ui-storage-mobile-v4.png`, `entwurf-ui-home-mobile-v4.png` |
| Full source/implementation comparisons | `comparison-home-normalized-v4.png`, `comparison-settings-normalized-v4.png` |
| Focused settings typography, navigation and grouped rows | `comparison-settings-focused-v4.png` |
| Reproducible private real-browser capture/check script | `check.mjs` |

The final screenshots fast-forward CSS transitions with Playwright's `animations: "disabled"`. A previously visible gray project button was the 150 ms outgoing selection transition: the settled DOM has only Settings marked `aria-current="page"`; the Project background is transparent and the Settings background is `rgb(48, 48, 48)`.

## Findings and comparison history

1. **P2, initial capture density — resolved.** V1/V2 used source pixel widths as DPR 1 CSS widths, making the implementation too small in the comparison. Those images are not fidelity evidence. The measured 192 DPI and app-region crops above replace that assumption; V4 uses native Windows Chromium at DPR 2.
2. **P2, mobile category label — resolved.** The wrapping label included option text in its accessible name. The select now explicitly exposes `aria-label="设置分类"`; the real mobile category interaction passes.
3. **P2, Chinese fallback — resolved.** Generic system fonts before the packaged fallback could leave Chinese glyphs unavailable during Linux rendering. Named Windows fonts precede the packaged `Entwurf UI` fallback, and generic families come last. Final Windows captures show readable system typography; Linux captures retain complete Chinese coverage.
4. **P2, navigation scroll position — resolved.** At the normalized viewport, switching from a long project page retained its scroll offset and clipped the settings title. Page and category changes now reset the document scroll position. V4 shows the headings, and the real browser test checks navigation from a scrolled settings page back to the visible project heading.
5. **P3, selection transition in screenshot — evidence corrected.** The outgoing project highlight was captured before its transition ended. The stable capture verifies a single active item; no product change was needed.

The final full and focused comparisons were opened together and reviewed after these fixes. No actionable P0/P1/P2 findings remain.

## Required fidelity surfaces

| Surface | Result |
| --- | --- |
| Typography | Windows uses the existing Segoe UI / Microsoft YaHei stack; title/body hierarchy and readable Chinese are verified at normalized density. The bundled fallback supports other hosts. |
| Spacing and layout | Approved 72 px main rail, settings at its bottom, centered project content and 240 px settings category column. Mobile uses a compact top navigation and category select. Reference account navigation is intentionally excluded. |
| Colors and tokens | Flat gray/black surfaces, restrained borders and neutral selected states replace the workspace grid and purple glow. Light theme remains functional. Semantic review/error colors remain intact. |
| Images and icons | The supplied PNG brand is reused unchanged through `BASE_URL`; existing Lucide icons are reused. No new raster, placeholder artwork or copied account imagery. |
| Product content | All fields/actions correspond to existing Entwurf behavior. No invented model, account or network settings. Image connection status still belongs to the task flow. |

## Interaction and regression evidence

- 67 frontend unit tests passed, including retained OCR drafts across categories, theme persistence, native capability gates and diagnostic cancellation/error behavior.
- TypeScript build and production Vite build with `/Entwurf/` base passed.
- Two real WASM/IndexedDB Playwright cases passed: Markdown import/edit/reload/SVG/PNG/PDF/diagnostic flow; and grouped settings/OCR save/reload/theme/mobile navigation/scroll restoration.
- Three related existing protocol-fixture UI tests passed: five pages with both themes at two desktop sizes and CSS zooms, native discovery gates, and diagnostic export/cancellation. These fixtures verify presentation only and are not described as installed-app evidence.
- Native Windows Chromium additionally completed real public Markdown import, OCR draft retention, saved OCR reload, mobile category navigation, task navigation and a real diagnostic download. Page errors: `[]`. No ChatGPT generation, login, account inspection or production automation request was performed.
- The Windows installed-app acceptance script now selects `存储与诊断` before export and `常规` before theme changes. Its business, ownership and artifact checks are unchanged. The final installer cycles remain a separate root-owned acceptance step.

## Follow-up / limits

The reference's unrelated account/chat controls are deliberately not reproduced. The new shared UI has not yet been exercised inside the final rebuilt Windows installer; its existing release driver has been adapted for that run. Physical multi-monitor behavior and additional Windows display scales are not claimed.

Final result: passed
