# Windows desktop candidate acceptance

Recorded on **2026-11-01 (Asia/Shanghai)**. The tested application is the unsigned
Windows x64 Entwurf 0.1.0 NSIS bundle built from **`536a23a`**. Later documentation
and acceptance-driver changes do not change that application binary.

| Artifact | Identity |
| --- | --- |
| Frozen installer | `artifacts/installers/Entwurf_0.1.0_536a23a_x64-setup.exe` in the native Windows checkout |
| Installer size | 77,931,550 bytes |
| Installer SHA-256 | `b8c433727f3c9b6d8a9509c9a9e6c2d41a1c2a51d912c199ae51c91fc06130ff` |
| Installed executable SHA-256 | `5e4dd07320544f4baee40fd742f49c497854ff405fa181155b5ee6eca5058ad4` |
| Test installation | `%LOCALAPPDATA%\Programs\Entwurf` |
| Reinstall evidence | `%LOCALAPPDATA%\EntwurfAcceptance\v0.1-20261031\cycle-1` and `cycle-2` |

The user-provided `sample/icon.png` was converted into the bundled PNG and
multi-resolution ICO. The icon extracted from the installed baseline executable
matched that source; the icon assets are unchanged in the final bundle. Private
sample originals and local evidence remain outside Git.

## Installed application results

Two complete uninstall/reinstall cycles pass with the same frozen installer.
Each run executes the installed WebView2 application and actual Tauri commands,
with no IPC mocks or production test hooks. Each uninstall removes the application
executable and leaves every existing project file byte-identical. Each reinstall
restores the project and earlier manual edits. Installed dependency inventory and
notice hashes match the checked-in release resources.

Both cycles pass all **11 workflow checks**:

1. Installed startup and actual native capability detection, including Tesseract
   with `chi_sim` and `eng`.
2. WebView networking disabled, UI reloaded offline, and native commands available.
3. Existing project and manual edits restored after reinstall.
4. Public Markdown imported through the native chooser from a Chinese/spaced path;
   the retained original remains usable after removing the test input copy.
5. Four formula reviews acknowledged only after literal source and hash checks.
6. A synthetic ResultEnvelope cannot overwrite a saved manual title.
7. Compatible two-group visual advice produces two real pages, with complete SVG,
   300 DPI PNG, vector PDF, IR and layout manifest coverage and file hashes.
8. Exported Knowledge IR restored into a new empty task through the native chooser,
   preserving knowledge, reviews and provenance, then compiled and exported again.
9. A valid 56pt long heading triggers a real 24pt repair. Undo preserves exact IR
   and historical versions, disables replay and invalidates the repaired export.
10. Diagnostics saved through native Save As contain only the fixed allowlist;
    cancelling a second export produces no file changes or false success message.
11. All five installed pages render in both themes without page errors.

| Final reviewed-driver run (UTC) | Checks | Existing project files unchanged after uninstall |
| --- | ---: | ---: |
| Cycle 1, 2026-10-31 16:44:12–16:45:29 | 11/11 | 285 |
| Cycle 2, 2026-10-31 16:45:29–16:46:50 | 11/11 | 350 |

Both installation records explicitly confirm the requested installer was applied
in that cycle and that no existing executable was reused.

The acceptance driver was independently reviewed. Three native preflight checks
also prove it rejects nonempty unowned evidence, overlapping installation/evidence
paths, and an existing unowned installation without changing the sentinel data.
Earlier attempts that exposed invalid test inputs or overly strict driver
assertions were retained separately; they are not counted as passing cycles.

The final rebuild resolves stale CRLF font-notice copies in the earlier Windows
checkout. The affected files were compared with canonical Git blobs before being
refreshed; original bytes were retained as local evidence. A new build gate pins
three font-notice hashes and checks both frontend copies before dependency reads
or output writes. Seven regression tests and complete notice-generation checks
pass on Windows and WSL, with byte-identical generated resources. Existing Git
attributes already preserved notice bytes; the stale checkout required refreshing.

## Separate native checks

The following functional checks used earlier installed builds as identified
below. The final `536a23a` change adds the notice build gate and restores canonical
notice bytes; application source code is unchanged from `f05e572`.

- **Fresh application configuration: 17/17 passed.** Only Entwurf's remembered
  workspace file was temporarily isolated, with a new WebView profile. Empty
  startup, capabilities, offline UI reload, native project creation and public PDF
  import passed. The exact original configuration was restored, a new process
  automatically reopened the original project, and all 55 original project files
  remained unchanged. This used `f05e572` on the same Windows account, not a newly
  created OS user.
- **Database recovery: 22/22 passed.** In a separate owned public project, the
  SQLite/WAL/SHM files were quarantined while closed. Normal native project opening
  rebuilt the database. Two tasks, seven versions, 22 audit events, seven immutable
  IR files, two source copies, five artifact files and the manual edit survived.
  All 17 non-database files remained byte-identical; SQLite integrity, relational
  records and WAL checks passed on `f05e572`. The main project was restored unchanged.
- **Packaged PDFium:** a two-page PDF retains anchors for pages 1 and 2; Windows
  process-module inspection on `f05e572` confirms the DLL is loaded from installed resources.
  Its SHA-256 is `79d4676b656cfb1abcea88f9ade3b4b0826c5200382db5f4ec72a636c598c118`.
- **Private references: 9/9 checks passed on baseline `988fc22`.** Two images were
  stored as style references with no knowledge sources; separate OCR imports kept
  original/transcript/anchor hashes and pending OCR/formula reviews. No private
  knowledge was approved or exported. This check is explicitly scoped to the
  baseline; final-bundle PDF and recovery checks are recorded separately above.

## Supporting verification and limits

The root actual-engine suite passes **145 Rust core/internal acceptance tests**,
with PDFium and real Tesseract enabled and zero ignored tests. Frontend checks pass
**33 React tests and 10 Playwright tests**; these use an isolated mock IPC boundary.
Native bridge checks pass **41 C# tests and 8 Rust parent/sidecar tests**. Generator,
formatting and native desktop build checks pass. Full commands are in the
[Windows guide](windows.md). Release notices cover 554 dependencies and 351 texts,
with zero unresolved records and byte-identical Windows/WSL generation.

The host exposes one logical 1280×800 monitor. Installed captures measure a
1282×780 CSS viewport and devicePixelRatio 2; these are measurements, not a claim
that OS scaling was changed. The 60 browser screenshots cover two viewport sizes,
two themes, five pages and three CSS zoom levels. Physical multi-monitor and OS
100%/125%/150% acceptance remain unverified.

Offline acceptance disables WebView networking through a test-only localhost CDP
connection, including reload; it does not disconnect host adapters or block native
process networking. No system-wide packet-capture result is claimed. Test processes
are closed after each run; normal application launches receive no debugging flags.

No original textbook chapter was supplied, so real-chapter completeness and OCR
accuracy remain unverified. GPT results used in these tests are synthetic local
fixtures. All four real ChatGPT automation gates remain false and automation stays
disabled. Manual mode copies text only: reference images must be attached manually
in ChatGPT, and returned image references remain unapplied.
