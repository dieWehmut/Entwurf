import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { renderNotices, selectCargoTreePackages, validateInventory } from './notices-lib.mjs';

test('notices preserve original text and identify every dependency', () => {
  const packages = [{ ecosystem: 'cargo', name: 'fixture', version: '1.0', license: 'MIT', notices: ['abc'] }];
  const body = 'Copyright (c) Fixture\r\nVerbatim terms.\r\n';
  const result = renderNotices(packages, { abc: body });
  assert.ok(result.includes(body));
  assert.ok(result.includes('cargo:fixture@1.0'));
  assert.throws(() => renderNotices(packages, {}), /missing notice/);
});

test('missing expressions and missing texts fail the distributable gate', () => {
  assert.throws(() => validateInventory([{ name: 'missing', version: '1', license: null, notices: [] }]), /incomplete attribution/);
  assert.throws(() => validateInventory([{ name: 'missing', version: '1', license: 'MIT', notices: [] }]), /incomplete attribution/);
  assert.doesNotThrow(() => validateInventory([{ name: 'ok', version: '1', license: 'MIT', notices: ['abc'] }]));
});

test('browser closure follows the selected target tree, excluding unrelated workspace features', () => {
  const packages = ['entwurf-wasm', 'wasm-bindgen', 'js-sys', 'pdfium-render', 'rusqlite', 'test-only'].map(name => ({ id: name, name, version: '1.0.0' }));
  const tree = 'entwurf-wasm v1.0.0 (/fixture)\nwasm-bindgen v1.0.0\njs-sys v1.0.0\nwasm-bindgen v1.0.0\n';
  assert.deepEqual(selectCargoTreePackages({ packages }, tree).map(p => p.name), ['entwurf-wasm', 'wasm-bindgen', 'js-sys']);
  assert.throws(() => selectCargoTreePackages({ packages }, tree + 'missing v1.0.0\n'), /missing or ambiguous/);
  assert.throws(() => selectCargoTreePackages({ packages: [...packages, { ...packages[1], id: 'another-registry' }] }, tree), /missing or ambiguous/);
});

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const fontFiles = [
  'assets/fonts/OFL.txt',
  'assets/fonts/DejaVu-LICENSE.txt',
  'assets/fonts/Typst-NOTICE.txt',
  'apps/desktop/public/fonts/OFL.txt',
  'apps/desktop/public/fonts/DejaVu-LICENSE.txt',
];
for (const web of [false, true]) for (const changed of fontFiles) {
  test(`stale ${web ? 'web' : 'native'} checkout bytes in ${changed} fail before dependencies or resource writes`, t => {
    const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'entwurf-notice-checkout-'));
    t.after(() => fs.rmSync(fixture, { recursive: true, force: true }));
    for (const file of [...fontFiles, 'scripts/release/generate-notices.mjs', 'scripts/release/notices-lib.mjs']) {
      const destination = path.join(fixture, file);
      fs.mkdirSync(path.dirname(destination), { recursive: true });
      fs.copyFileSync(path.join(root, file), destination);
    }
    const stale = Buffer.from(fs.readFileSync(path.join(fixture, changed), 'utf8').replace(/\r?\n/g, '\r\n'));
    fs.writeFileSync(path.join(fixture, changed), stale);
    // A matching but stale public copy must not bypass the canonical source hash.
    const publicCopy = changed.replace('assets/fonts/', 'apps/desktop/public/fonts/');
    if (changed.startsWith('assets/') && fontFiles.includes(publicCopy)) fs.writeFileSync(path.join(fixture, publicCopy), stale);
    const output = path.join(fixture, web ? 'apps/desktop/public/licenses' : 'apps/desktop/src-tauri/resources/licenses');
    fs.mkdirSync(output, { recursive: true });
    for (const file of ['dependency-inventory.json', 'THIRD-PARTY-NOTICES.txt']) fs.writeFileSync(path.join(output, file), 'untouched sentinel\r\n');
    // No Cargo/npm/NuGet fixtures: checkout validation must run before dependency discovery.
    const result = spawnSync(process.execPath, [path.join(fixture, 'scripts/release/generate-notices.mjs'), ...(web ? ['--web'] : [])], { encoding: 'utf8' });
    assert.ifError(result.error);
    assert.notEqual(result.status, 0);
    assert.ok(result.stderr.includes(`font notice byte mismatch: ${changed}`), result.stderr);
    assert.match(result.stderr, /refresh.*checkout/i);
    assert.deepEqual(fs.readFileSync(path.join(fixture, changed)), stale);
    for (const file of ['dependency-inventory.json', 'THIRD-PARTY-NOTICES.txt']) assert.equal(fs.readFileSync(path.join(output, file), 'utf8'), 'untouched sentinel\r\n');
  });
}
