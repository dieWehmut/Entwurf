# Distributable third-party attribution

The Windows installer includes `resources/licenses/THIRD-PARTY-NOTICES.txt` and
`resources/licenses/dependency-inventory.json`. Entwurf itself is Apache-2.0;
third-party components retain their own licenses.

From a prepared checkout, run:

```sh
node scripts/release/generate-notices.mjs
node scripts/release/generate-notices.mjs --check
node --test scripts/release/notices.test.mjs
```

Preparation means restored locked Cargo sources, `pnpm install --frozen-lockfile`
in `apps/desktop`, and the published self-contained Windows bridge. Generation
uses `cargo metadata --offline --locked --filter-platform
x86_64-pc-windows-msvc`; it makes no network requests. NuGet packages are read from
`NUGET_PACKAGES`, or the standard user NuGet cache. Node 22+ is sufficient. Cargo
metadata can require restoring platform-specific sources even though the current
host is not Windows. Missing sources/packages are a failed prerequisite, never a
reason to drop a dependency.

The generated inventory records the desktop Cargo dependency closure (normal and
build dependencies, excluding dev-only edges), installed production npm packages,
packages actually present in the published bridge's `.deps.json`, PDFium's pinned
binary notices, and bundled fonts. Build dependencies are deliberately included;
the list does not assert every package is linked into the executable. pnpm is
recorded as build tooling, not distributed software. Tesseract, ChatGPT and
WebView2 are externally installed/detected rather than copied into the installer.

The generator preserves complete upstream notice texts, deduplicates identical
texts by SHA-256, references each package's texts and sources, and records hashes
of lockfiles and the published sidecar dependency manifest (UTF-8 with CRLF
normalized to LF so Windows and Unix checkouts agree). The PDFium FreeType notice uses ISO-8859-1 and is losslessly transcoded
to UTF-8 in the consolidated text; its original bytes remain in PDFium resources.
Other invalid UTF-8 files are rejected rather than silently replacing characters. `--check` verifies byte-for-byte generated
content. Missing license declarations or notice texts are recorded explicitly as
`UNKNOWN`/`MISSING` and fail the packaging gate. Paths in records are package
relative or public upstream URLs, never build-machine paths.

Most notices come directly from Cargo/npm/NuGet packages. `upstream/index.json`
records supplemental text files, their public source URLs and content hashes.
These cover crates which publish a license declaration but omit the license file.
Recovered repository texts are pinned to the commit in `.cargo_vcs_info.json`.
For fxhash and glidesort, their original declarations accompany the official
Apache-2.0 text; selectors' original source header accompanies the official
MPL-2.0 text. These supplement the published alternatives without inventing
copyright holders or changing the declared expression.

SeaHash is the one provenance qualification: 4.1.0 declares MIT but omits its
license file, and its current GitLab host was unavailable. The preserved MIT
notice is from the original maintainer's `ticki/tfs` workspace, which contains
SeaHash, pinned to an immutable commit. This qualification is also in the shipped
inventory. A later source refresh should prefer the exact published revision if
that host becomes available.

Changes to dependencies require regenerating both resources. If a new package
omits notice text, capture its actual upstream text and provenance in
`upstream/index.json`; do not substitute a guessed notice. Unused override
entries may remain to explain prior package attribution. This is mechanical
attribution evidence, not a claim that inventory generation alone satisfies every
redistribution condition.
