import { test } from 'node:test';
import assert from 'node:assert/strict';
import { renderNotices, validateInventory } from './notices-lib.mjs';

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
