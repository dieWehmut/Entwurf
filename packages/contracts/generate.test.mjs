import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = dirname(fileURLToPath(import.meta.url));

test('schema changes generate runtime Rust fields and check rejects every output drift', () => {
  const temp = mkdtempSync(join(tmpdir(), 'entwurf-contracts-'));
  try {
    cpSync(root, temp, { recursive: true });
    const run = (...args) => execFileSync(process.execPath, [join(temp, 'generate.mjs'), ...args]);
    run();
    const manifest = JSON.parse(readFileSync(join(temp, 'generated/manifest.json')));
    assert.ok(manifest.rust_sha256, 'manifest must track generated Rust used by runtime');
    for (const name of ['types.ts', 'ir.rs', 'model.rs', 'project.rs', 'manifest.json']) {
      const path = join(temp, 'generated', name);
      const original = readFileSync(path, 'utf8');
      writeFileSync(path, original + '\n');
      assert.equal(spawnSync(process.execPath, [join(temp, 'generate.mjs'), '--check']).status, 1, name);
      rmSync(path);
      assert.equal(spawnSync(process.execPath, [join(temp, 'generate.mjs'), '--check']).status, 1, `missing ${name}`);
      writeFileSync(path, original);
    }
    const path = join(temp, 'schemas/knowledge-ir.schema.json');
    const schema = JSON.parse(readFileSync(path));
    schema.properties.generator_probe = { type: 'boolean' };
    schema.required.push('generator_probe');
    writeFileSync(path, JSON.stringify(schema));
    assert.equal(spawnSync(process.execPath, [join(temp, 'generate.mjs'), '--check']).status, 1);
    run();
    assert.match(readFileSync(join(temp, 'generated/ir.rs'), 'utf8'), /pub generator_probe: bool/);
    assert.match(readFileSync(join(temp, 'generated/types.ts'), 'utf8'), /"generator_probe": boolean/);
    run('--check');
    // Unsupported schema changes must fail explicitly, never become Value.
    schema.properties.generator_probe = { anyOf: [{ type: 'boolean' }, { type: 'string' }] };
    writeFileSync(path, JSON.stringify(schema));
    const unsupported = spawnSync(process.execPath, [join(temp, 'generate.mjs')], { encoding: 'utf8' });
    assert.notEqual(unsupported.status, 0);
    assert.match(unsupported.stderr, /only nullable anyOf/);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});
