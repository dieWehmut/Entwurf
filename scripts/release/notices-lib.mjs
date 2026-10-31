export function validateInventory(packages) {
  const missing = packages.filter(p => !p.license || p.license === 'UNKNOWN' || !p.notices.length);
  if (missing.length) throw new Error(`incomplete attribution: ${missing.map(p => `${p.name}@${p.version}`).join(', ')}`);
}

export function selectCargoTreePackages(metadata, tree) {
  const selected = new Map();
  for (const line of tree.split(/\r?\n/).filter(Boolean)) {
    const match = line.match(/^(\S+) v(\S+)(?: |$)/);
    if (!match) throw new Error(`unrecognized cargo tree package: ${line}`);
    const [, name, version] = match;
    const candidates = metadata.packages.filter(p => p.name === name && p.version === version);
    if (candidates.length !== 1) throw new Error(`missing or ambiguous cargo metadata: ${name}@${version}`);
    selected.set(candidates[0].id, candidates[0]);
  }
  if (![...selected.values()].some(p => p.name === 'entwurf-wasm')) throw new Error('browser package missing from cargo tree');
  return [...selected.values()];
}

export function renderNotices(packages, texts, { web = false } = {}) {
  let result = `Entwurf third-party notices\nGenerated from locked ${web ? 'wasm32 browser dependencies and distributed web resources' : 'Windows dependencies and distributed resources'}.\n`;
  result += 'The inventory includes build dependencies; this does not assert that each is linked into the executable.\n';
  for (const p of packages) result += `\n${p.ecosystem}:${p.name}@${p.version}\nLicense: ${p.license ?? 'UNKNOWN'}\nNotices: ${p.notices.join(', ') || 'MISSING'}\n`;
  for (const hash of [...new Set(packages.flatMap(p => p.notices))].sort()) {
    if (typeof texts[hash] !== 'string') throw new Error(`missing notice ${hash}`);
    result += `\n${'='.repeat(80)}\nNOTICE SHA256 ${hash}\n${'='.repeat(80)}\n`;
    result += texts[hash]; // Preserve upstream text, including CRLF and BOM.
    result += '\n';
  }
  return result;
}
