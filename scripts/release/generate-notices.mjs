/** Offline after cargo fetch, pnpm install --frozen-lockfile and bridge publish. */
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { renderNotices, validateInventory } from './notices-lib.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const read = p => fs.readFileSync(p, 'utf8');
const json = p => JSON.parse(read(p).replace(/^\uFEFF/, ''));
const digest = data => createHash('sha256').update(data).digest('hex');
const posix = p => p.split(path.sep).join('/');
const args = process.argv.slice(2);
const check = args.includes('--check');
const output = path.join(root, 'apps/desktop/src-tauri/resources/licenses');
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
const metadata = JSON.parse(execFileSync('cargo', ['metadata', '--offline', '--locked', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc'], { cwd: root, maxBuffer: 64 * 1024 * 1024, encoding: 'utf8' }));
const nodes = new Map(metadata.resolve.nodes.map(n => [n.id, n]));
const start = metadata.packages.find(p => p.name === 'entwurf-desktop');
if (!start) throw new Error('desktop package missing');
const reachable = new Set();
function visit(id) {
  if (reachable.has(id)) return;
  reachable.add(id);
  for (const dep of nodes.get(id).deps) if (dep.dep_kinds.some(k => k.kind !== 'dev')) visit(dep.pkg);
}
visit(start.id);
for (const p of metadata.packages.filter(p => p.source && reachable.has(p.id))) {
  const dir = path.dirname(p.manifest_path);
  const files = licenseFiles(dir);
  if (p.license_file && fs.existsSync(path.resolve(dir, p.license_file))) files.push(path.resolve(dir, p.license_file));
  const source = `https://crates.io/crates/${p.name}/${p.version}`;
  const provenance = files.map(f => `${source}#${posix(path.relative(dir, f))}`);
  for (const o of overrides[`${p.name}@${p.version}`] ?? []) {
    const file = path.join(upstream, o.file);
    if (digest(fs.readFileSync(file)) !== o.sha256) throw new Error(`upstream notice hash mismatch: ${o.file}`);
    files.push(file); provenance.push(o.source);
  }
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
  add({ ecosystem: 'npm', name: p.name, version: p.version, license: typeof p.license === 'string' ? p.license : p.license?.type ?? 'UNKNOWN', source }, files, files.map(f => `${source}#${posix(path.relative(dir, f))}`));
  for (const name of Object.keys(p.dependencies ?? {})) npm(resolvePackage(dir, name));
}
const desktop = path.join(root, 'apps/desktop');
for (const name of Object.keys(json(path.join(desktop, 'package.json')).dependencies)) npm(resolvePackage(desktop, name));
const nuget = process.env.NUGET_PACKAGES || path.join(os.homedir(), '.nuget/packages');
const bridge = path.join(root, 'apps/desktop/src-tauri/resources/gpt-bridge/Entwurf.Bridge.deps.json');
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
add({ ecosystem: 'font', name: 'LXGW WenKai', version: 'bundled', license: 'OFL-1.1', source: 'https://github.com/lxgw/LxgwWenKai' }, [path.join(root, 'assets/fonts/OFL.txt')], ['assets/fonts/OFL.txt']);
const typstAssets = metadata.packages.find(p => p.name === 'typst-assets');
if (typstAssets && reachable.has(typstAssets.id)) add({ ecosystem: 'font', name: 'Typst bundled fonts', version: typstAssets.version, license: 'SEE-NOTICE', source: `https://crates.io/crates/typst-assets/${typstAssets.version}` }, [path.join(root, 'assets/fonts/Typst-NOTICE.txt')], ['assets/fonts/Typst-NOTICE.txt']);
add({ ecosystem: 'font', name: 'DejaVu', version: 'bundled', license: 'Bitstream-Vera AND public-domain', source: 'https://dejavu-fonts.github.io/' }, [path.join(root, 'assets/fonts/DejaVu-LICENSE.txt')], ['assets/fonts/DejaVu-LICENSE.txt']);
packages.sort((a, b) => `${a.ecosystem}:${a.name}@${a.version}`.localeCompare(`${b.ecosystem}:${b.name}@${b.version}`, 'en'));
const inventory = {
  schema_version: 1, target: 'x86_64-pc-windows-msvc',
  scope: 'desktop normal/build Cargo closure; pnpm production closure; published self-contained sidecar; fonts; PDFium',
  tools: { package_manager: json(path.join(desktop, 'package.json')).packageManager, distributed: false },
  attribution_notes: [
    'License declarations are preserved as published; alternative license expressions are not rewritten.',
    'FlaUI and UIAutomation interop use their bundled LICENSE.txt; SEE-LICENSE-FILE is a file reference, not an unknown license.',
    'SeaHash 4.1.0 declares MIT but omits the text. The original maintainer ticki/tfs workspace contains SeaHash and its MIT notice; that notice is pinned by commit. The current GitLab source host was unavailable during capture.',
    'For fxhash/glidesort/selectors, original source license declarations accompany the official Apache-2.0/MPL-2.0 texts.',
    'pnpm is build tooling and is not distributed. Only the production npm dependency closure is included.',
    'PDFium freetype.txt uses ISO-8859-1. Its copyright character is losslessly transcoded to UTF-8 in the consolidated text; the hash references original bytes, also shipped verbatim in PDFium resources.',
    'Tesseract, ChatGPT and WebView2 are detected external applications/runtimes and are not included in this inventory.',
  ],
  input_hash_normalization: 'UTF-8, CRLF normalized to LF for identical Windows and Unix checkouts',
  inputs: Object.fromEntries(['Cargo.lock', 'apps/desktop/pnpm-lock.yaml', 'apps/desktop/src-tauri/resources/gpt-bridge/Entwurf.Bridge.deps.json'].map(p => [p, digest(read(path.join(root, p)).replace(/\r\n/g, '\n'))])),
  package_count: packages.length,
  unresolved: packages.filter(p => p.license === 'UNKNOWN' || !p.notices.length).map(p => `${p.ecosystem}:${p.name}@${p.version}`),
  packages,
};
fs.mkdirSync(output, { recursive: true });
for (const [file, data] of [['dependency-inventory.json', JSON.stringify(inventory, null, 2) + '\n'], ['THIRD-PARTY-NOTICES.txt', renderNotices(packages, texts)]]) {
  const destination = path.join(output, file);
  if (check) { if (!fs.existsSync(destination) || read(destination) !== data) throw new Error(`stale generated resource: ${file}`); }
  else fs.writeFileSync(destination, data);
}
// Emit incomplete inventories for inspection, but never approve packaging them.
validateInventory(packages);
console.log(`Verified ${packages.length} dependency records and ${Object.keys(texts).length} verbatim notice texts (${check ? 'checked' : 'generated'}, offline).`);
