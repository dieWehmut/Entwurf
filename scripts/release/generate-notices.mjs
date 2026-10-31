/** Offline after cargo fetch, pnpm install --frozen-lockfile and bridge publish. */
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { renderNotices, selectCargoTreePackages, validateInventory } from './notices-lib.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const read = p => fs.readFileSync(p, 'utf8');
const json = p => JSON.parse(read(p).replace(/^\uFEFF/, ''));
const digest = data => createHash('sha256').update(data).digest('hex');
const posix = p => p.split(path.sep).join('/');
// Git attributes do not refresh bytes left behind by an older Windows checkout.
// Pin the reviewed source bytes; never normalize third-party notice text here.
const fontNotices = [
  { file: 'assets/fonts/OFL.txt', sha256: '1a25e35da1031c6c3436fde545bb9cb5aca954e9873afe510c834b8b79bd21a0', publicCopy: 'apps/desktop/public/fonts/OFL.txt' },
  { file: 'assets/fonts/DejaVu-LICENSE.txt', sha256: '7a083b136e64d064794c3419751e5c7dd10d2f64c108fe5ba161eae5e5958a93', publicCopy: 'apps/desktop/public/fonts/DejaVu-LICENSE.txt' },
  { file: 'assets/fonts/Typst-NOTICE.txt', sha256: '80ac46fb7d70f1c30bf0c14d3408e6a6f0f9cc9997a00b592aa0d2c8a5243a9d' },
];
function fontByteMismatch(file, expected) {
  throw new Error(`font notice byte mismatch: ${file}; expected ${expected}. Back up intentional edits, then refresh the affected checkout files from canonical Git bytes with the current .gitattributes. Do not normalize notice text.`);
}
for (const notice of fontNotices) {
  const bytes = fs.readFileSync(path.join(root, notice.file));
  if (digest(bytes) !== notice.sha256) fontByteMismatch(notice.file, `SHA256 ${notice.sha256}`);
  if (notice.publicCopy && !fs.readFileSync(path.join(root, notice.publicCopy)).equals(bytes)) {
    fontByteMismatch(notice.publicCopy, `bytes identical to ${notice.file}`);
  }
}
const args = process.argv.slice(2);
const check = args.includes('--check');
const web = args.includes('--web');
const target = web ? 'wasm32-unknown-unknown' : 'x86_64-pc-windows-msvc';
const output = path.join(root, web ? 'apps/desktop/public/licenses' : 'apps/desktop/src-tauri/resources/licenses');
const texts = {};
const packages = [];
const upstream = path.join(root, 'docs/licenses/upstream');
const overrides = json(path.join(upstream, 'index.json'));
function textFile(file) {
  const bytes = fs.readFileSync(file);
  const legacyLatin1 = path.resolve(file) === path.join(root, 'apps/desktop/src-tauri/resources/pdfium/licenses/freetype.txt');
  if (!legacyLatin1 && !Buffer.from(bytes.toString('utf8'), 'utf8').equals(bytes)) throw new Error(`notice is not valid UTF-8: ${path.basename(file)}`);
  const hash = digest(bytes);
  texts[hash] = bytes.toString(legacyLatin1 ? 'latin1' : 'utf8');
  return hash;
}
function licenseFiles(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, entry.name);
    if (entry.isDirectory() && !['node_modules', '.git', 'target'].includes(entry.name)) out.push(...licenseFiles(p));
    else if (entry.isFile() && /^(license|licence|copying|notice|copyright|third.party.notices)([.\-_]|$)/i.test(entry.name)
      && !/\.(rs|py|c|h|json|html|spdx)$/i.test(entry.name)) out.push(p);
  }
  return out.sort();
}
function add(p, files, provenance = []) {
  p.notices = [...new Set(files.map(textFile))].sort();
  p.notice_sources = [...new Set(provenance)].sort();
  packages.push(p);
}
function addUpstreamNotices(key, files, provenance) {
  for (const o of overrides[key] ?? []) {
    const file = path.join(upstream, o.file);
    if (digest(fs.readFileSync(file)) !== o.sha256) throw new Error(`upstream notice hash mismatch: ${o.file}`);
    files.push(file); provenance.push(o.source);
  }
}
const cargo = command => execFileSync('cargo', command, { cwd: root, maxBuffer: 64 * 1024 * 1024, encoding: 'utf8' });
// Full package metadata supplies host build-dependency details too. The selected
// tree, not workspace-wide feature unification, defines the browser inventory.
const metadata = JSON.parse(cargo(['metadata', '--offline', '--locked', '--format-version', '1', ...(web ? [] : ['--filter-platform', target])]));
const nodes = new Map(metadata.resolve.nodes.map(n => [n.id, n]));
const start = metadata.packages.find(p => p.name === (web ? 'entwurf-wasm' : 'entwurf-desktop'));
if (!start) throw new Error('entry package missing');
const reachable = new Set();
function visit(id) {
  if (reachable.has(id)) return;
  reachable.add(id);
  for (const dep of nodes.get(id).deps) if (dep.dep_kinds.some(k => k.kind !== 'dev')) visit(dep.pkg);
}
if (web) {
  for (const p of selectCargoTreePackages(metadata, cargo(['tree', '--offline', '--locked', '-p', 'entwurf-wasm', '--target', target, '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}', '--no-dedupe']))) reachable.add(p.id);
} else visit(start.id);
for (const p of metadata.packages.filter(p => p.source && reachable.has(p.id))) {
  const dir = path.dirname(p.manifest_path);
  const files = licenseFiles(dir);
  if (p.license_file && fs.existsSync(path.resolve(dir, p.license_file))) files.push(path.resolve(dir, p.license_file));
  const source = `https://crates.io/crates/${p.name}/${p.version}`;
  const provenance = files.map(f => `${source}#${posix(path.relative(dir, f))}`);
  // Retain the original Cargo keys; other ecosystems use an explicit namespace.
  addUpstreamNotices(`${p.name}@${p.version}`, files, provenance);
  add({ ecosystem: 'cargo', name: p.name, version: p.version, license: p.license ?? 'UNKNOWN', source }, files, provenance);
}
// Resolve only installed production dependencies, following pnpm's real package symlinks.
const npmSeen = new Set();
function resolvePackage(from, name) {
  for (let d = from; ; d = path.dirname(d)) {
    const p = path.join(d, 'node_modules', name, 'package.json');
    if (fs.existsSync(p)) return path.dirname(fs.realpathSync(p));
    if (d === path.dirname(d)) throw new Error(`production package missing: ${name}`);
  }
}
function npm(dir) {
  const p = json(path.join(dir, 'package.json'));
  const id = `${p.name}@${p.version}`;
  if (npmSeen.has(id)) return;
  npmSeen.add(id);
  const files = licenseFiles(dir);
  const source = `https://www.npmjs.com/package/${p.name}/v/${p.version}`;
  const provenance = files.map(f => `${source}#${posix(path.relative(dir, f))}`);
  addUpstreamNotices(`npm:${id}`, files, provenance);
  add({ ecosystem: 'npm', name: p.name, version: p.version, license: typeof p.license === 'string' ? p.license : p.license?.type ?? 'UNKNOWN', source }, files, provenance);
  for (const name of Object.keys(p.dependencies ?? {})) npm(resolvePackage(dir, name));
}
const desktop = path.join(root, 'apps/desktop');
for (const name of Object.keys(json(path.join(desktop, 'package.json')).dependencies)) npm(resolvePackage(desktop, name));
if (!web) {
  const bridge = path.join(root, 'apps/desktop/src-tauri/resources/gpt-bridge/Entwurf.Bridge.deps.json');
  const nuget = process.env.NUGET_PACKAGES || path.join(os.homedir(), '.nuget/packages');
  for (const id of Object.keys(json(bridge).libraries).filter(id => !id.startsWith('Entwurf.'))) {
    const [name, version] = id.replace(/^runtimepack\./, '').split('/');
    const dir = path.join(nuget, name.toLowerCase(), version);
    const spec = read(path.join(dir, fs.readdirSync(dir).find(f => f.endsWith('.nuspec'))));
    const license = spec.match(/<license type="expression">([^<]+)<\/license>/)?.[1]
      ?? (spec.includes('<license type="file">') ? 'SEE-LICENSE-FILE' : 'UNKNOWN');
    const source = `https://www.nuget.org/packages/${name}/${version}`;
    const files = licenseFiles(dir);
    add({ ecosystem: 'nuget', name, version, license, source }, files, files.map(f => `${source}#${posix(path.relative(dir, f))}`));
  }
  const pdfium = path.join(root, 'apps/desktop/src-tauri/resources/pdfium');
  const pdf = json(path.join(pdfium, 'manifest.json'));
  const pdfFiles = [path.join(pdfium, 'LICENSE'), ...fs.readdirSync(path.join(pdfium, 'licenses')).map(f => path.join(pdfium, 'licenses', f))];
  add({ ecosystem: 'binary', name: 'PDFium', version: pdf.version, license: pdf.license, source: pdf.distributor }, pdfFiles, [pdf.distributor]);
}
add({ ecosystem: 'font', name: 'LXGW WenKai', version: 'bundled', license: 'OFL-1.1', source: 'https://github.com/lxgw/LxgwWenKai' }, [path.join(root, 'assets/fonts/OFL.txt')], ['assets/fonts/OFL.txt']);
const typstAssets = metadata.packages.find(p => p.name === 'typst-assets');
if (typstAssets && reachable.has(typstAssets.id)) add({ ecosystem: 'font', name: 'Typst bundled fonts', version: typstAssets.version, license: 'SEE-NOTICE', source: `https://crates.io/crates/typst-assets/${typstAssets.version}` }, [path.join(root, 'assets/fonts/Typst-NOTICE.txt')], ['assets/fonts/Typst-NOTICE.txt']);
add({ ecosystem: 'font', name: 'DejaVu', version: 'bundled', license: 'Bitstream-Vera AND public-domain', source: 'https://dejavu-fonts.github.io/' }, [path.join(root, 'assets/fonts/DejaVu-LICENSE.txt')], ['assets/fonts/DejaVu-LICENSE.txt']);
packages.sort((a, b) => `${a.ecosystem}:${a.name}@${a.version}`.localeCompare(`${b.ecosystem}:${b.name}@${b.version}`, 'en'));
const inventory = {
  schema_version: 1, target,
  scope: web ? 'entwurf-wasm wasm32 normal/build Cargo closure; pnpm production closure; browser PDF/OCR assets; bundled fonts' : 'desktop normal/build Cargo closure; pnpm production closure; published self-contained sidecar; fonts; PDFium',
  tools: { package_manager: json(path.join(desktop, 'package.json')).packageManager, distributed: false },
  attribution_notes: [
    'License declarations are preserved as published; alternative license expressions are not rewritten.',
    'FlaUI and UIAutomation interop use their bundled LICENSE.txt; SEE-LICENSE-FILE is a file reference, not an unknown license.',
    'SeaHash 4.1.0 declares MIT but omits the text. The original maintainer ticki/tfs workspace contains SeaHash and its MIT notice; that notice is pinned by commit. The current GitLab source host was unavailable during capture.',
    'For fxhash/glidesort/selectors, original source license declarations accompany the official Apache-2.0/MPL-2.0 texts.',
    'pnpm is build tooling and is not distributed. Only the production npm dependency closure is included.',
    'tr46 0.0.3 omits the MIT text. Its published source revision is the parent of the pinned, license-only commit by the original maintainer that supplies the verbatim MIT notice.',
    '@tesseract.js-data/eng and @tesseract.js-data/chi_sim 1.0.0 publish MIT package declarations but omit full MIT notices. Those declarations are preserved separately from their 4.0.0 traineddata, which matches tesseract-ocr/tessdata 4.0.0 byte-for-byte after decompression and is covered by its pinned COPYING and Apache-2.0 text. The accompanying Entwurf provenance note is not an upstream license or an invented copyright notice.',
    'PDFium freetype.txt uses ISO-8859-1. Its copyright character is losslessly transcoded to UTF-8 in the consolidated text; the hash references original bytes, also shipped verbatim in PDFium resources.',
    'Native Tesseract, ChatGPT and WebView2 are detected external applications/runtimes and are not included in this inventory. Bundled tesseract.js, its core and traineddata packages are included in the production npm closure.',
  ].filter(note => !web || (!note.startsWith('FlaUI ') && !note.startsWith('PDFium '))),
  input_hash_normalization: 'UTF-8, CRLF normalized to LF for identical Windows and Unix checkouts',
  inputs: Object.fromEntries(['Cargo.lock', 'apps/desktop/pnpm-lock.yaml', ...(web ? ['apps/desktop/package.json', 'crates/entwurf-wasm/Cargo.toml', 'crates/entwurf-core/Cargo.toml', 'docs/licenses/upstream/index.json'] : ['apps/desktop/src-tauri/resources/gpt-bridge/Entwurf.Bridge.deps.json'])].map(p => [p, digest(read(path.join(root, p)).replace(/\r\n/g, '\n'))])),
  ...(web ? { font_files: Object.fromEntries(['assets/fonts/LXGWWenKai-Regular.ttf', 'apps/desktop/public/fonts/LXGWWenKai-Regular.ttf', 'apps/desktop/public/fonts/DejaVuSansMono.ttf'].map(p => [p, digest(fs.readFileSync(path.join(root, p)))])) } : {}),
  package_count: packages.length,
  unresolved: packages.filter(p => p.license === 'UNKNOWN' || !p.notices.length).map(p => `${p.ecosystem}:${p.name}@${p.version}`),
  packages,
};
fs.mkdirSync(output, { recursive: true });
for (const [file, data] of [['dependency-inventory.json', JSON.stringify(inventory, null, 2) + '\n'], ['THIRD-PARTY-NOTICES.txt', renderNotices(packages, texts, { web })]]) {
  const destination = path.join(output, file);
  if (check) { if (!fs.existsSync(destination) || read(destination) !== data) throw new Error(`stale generated resource: ${file}`); }
  else fs.writeFileSync(destination, data);
}
// Emit incomplete inventories for inspection, but never approve packaging them.
validateInventory(packages);
console.log(`Verified ${packages.length} dependency records and ${Object.keys(texts).length} verbatim notice texts (${check ? 'checked' : 'generated'}, offline).`);
