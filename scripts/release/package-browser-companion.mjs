// Small deterministic ZIP (stored entries) built solely from the shipped extension.
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const source = join(root, "sidecars/chatgpt-browser");
const files = (await readdir(source)).filter(name => /\.(m?js|html|css)$/.test(name) || ["manifest.json", "README.md"].includes(name)).sort();
const entries = await Promise.all(files.map(async name => [name, await readFile(join(source, name))]));
entries.push(["LICENSE", await readFile(join(root, "LICENSE"))]);

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
}
const chunks = [], directory = [];
let offset = 0, centralSize = 0;
for (const [name, bytes] of entries) {
  const filename = Buffer.from(`entwurf-chatgpt-browser/${name}`);
  const checksum = crc32(bytes);
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0); local.writeUInt16LE(20, 4);
  local.writeUInt16LE(0x0800, 6); local.writeUInt16LE(0x0021, 12);
  local.writeUInt32LE(checksum, 14); local.writeUInt32LE(bytes.length, 18);
  local.writeUInt32LE(bytes.length, 22); local.writeUInt16LE(filename.length, 26);
  chunks.push(local, filename, bytes);
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0); central.writeUInt16LE(20, 4); central.writeUInt16LE(20, 6);
  central.writeUInt16LE(0x0800, 8); central.writeUInt16LE(0x0021, 14);
  central.writeUInt32LE(checksum, 16); central.writeUInt32LE(bytes.length, 20);
  central.writeUInt32LE(bytes.length, 24); central.writeUInt16LE(filename.length, 28);
  central.writeUInt32LE(offset, 42); directory.push(central, filename);
  offset += local.length + filename.length + bytes.length;
  centralSize += central.length + filename.length;
}
const end = Buffer.alloc(22);
end.writeUInt32LE(0x06054b50, 0); end.writeUInt16LE(entries.length, 8); end.writeUInt16LE(entries.length, 10);
end.writeUInt32LE(centralSize, 12); end.writeUInt32LE(offset, 16);
const destination = join(root, "apps/desktop/public/companion");
await mkdir(destination, { recursive: true });
await writeFile(join(destination, "entwurf-chatgpt-browser.zip"), Buffer.concat([...chunks, ...directory, end]));
console.log(`Packaged ${entries.length} browser companion files.`);
