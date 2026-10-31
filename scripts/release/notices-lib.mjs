export function validateInventory(packages) {
  const missing = packages.filter(p => !p.license || p.license === 'UNKNOWN' || !p.notices.length);
  if (missing.length) throw new Error(`incomplete attribution: ${missing.map(p => `${p.name}@${p.version}`).join(', ')}`);
}

export function renderNotices(packages, texts) {
  let result = 'Entwurf third-party notices\nGenerated from locked Windows dependencies and distributed resources.\n';
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
