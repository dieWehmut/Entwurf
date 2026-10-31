import { cp, mkdir } from "node:fs/promises";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";

const require = createRequire(import.meta.url);
const publicRoot = new URL("../public/vendor/", import.meta.url);
await mkdir(publicRoot, { recursive: true });
const pdf = dirname(require.resolve("pdfjs-dist/package.json"));
for (const name of ["cmaps", "standard_fonts", "wasm"]) await cp(join(pdf, name), new URL(`pdfjs/${name}/`, publicRoot), { recursive: true });
await cp(join(pdf, "build/pdf.worker.min.mjs"), new URL("pdfjs/pdf.worker.min.mjs", publicRoot));
const tesseract = dirname(require.resolve("tesseract.js/package.json"));
const core = dirname(createRequire(join(tesseract, "package.json")).resolve("tesseract.js-core/package.json"));
await cp(core, new URL("tesseract/core/", publicRoot), { recursive: true, filter: path => !path.endsWith("node_modules") });
await cp(join(tesseract, "dist/worker.min.js"), new URL("tesseract/worker.min.js", publicRoot));
await mkdir(new URL("tesseract/lang/", publicRoot), { recursive: true });
for (const language of ["eng", "chi_sim"]) {
  const data = require(`@tesseract.js-data/${language}`);
  await cp(join(data.langPath, `${language}.traineddata.gz`), new URL(`tesseract/lang/${language}.traineddata.gz`, publicRoot));
}
for (const [source, target] of [[join(pdf, "LICENSE"), "pdfjs/LICENSE"], [join(tesseract, "LICENSE.md"), "tesseract/LICENSE.md"]]) await cp(source, new URL(target, publicRoot));
console.log("Prepared same-origin PDF and OCR assets (English + Simplified Chinese).");
