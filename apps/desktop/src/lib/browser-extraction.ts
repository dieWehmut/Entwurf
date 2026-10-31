interface ExtractionPage {
  page: number | null; text: string; engine: "pdfjs" | "tesseract.js"; is_ocr: boolean;
  region: { left: number; top: number; width: number; height: number; unit: "pdf_millipoints" | "pixels" };
}
export interface Extraction {
  schema_version: 1; source_sha256: string; source_kind: "pdf" | "image"; page_count: number;
  pages: ExtractionPage[]; diagnostics: { level: "info" | "warning" | "error"; code: string; message: string }[];
}
const asset = (path: string) => new URL(`${import.meta.env.BASE_URL}vendor/${path}`, location.origin).href;
async function recognize(canvas: HTMLCanvasElement, languages: string) {
  const { createWorker } = await import("tesseract.js");
  const worker = await createWorker(languages.split("+").filter(Boolean), 1, {
    workerPath: asset("tesseract/worker.min.js"), corePath: asset("tesseract/core"), langPath: asset("tesseract/lang"),
    workerBlobURL: false,
  });
  try { return (await worker.recognize(canvas)).data.text; }
  finally { await worker.terminate(); }
}
export async function extractBinary(file: File, languages = "chi_sim+eng"): Promise<Extraction | null> {
  const isPdf = /\.pdf$/i.test(file.name);
  if (!isPdf && !/\.(png|jpe?g|webp|bmp|tiff?)$/i.test(file.name)) return null;
  if (!languages.split("+").every(language => language === "eng" || language === "chi_sim")) throw new Error("浏览器已内置 eng 与 chi_sim OCR 语言，请在设置中选择这两种语言。");
  const bytes = new Uint8Array(await file.arrayBuffer());
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  const extraction: Extraction = { schema_version: 1, source_sha256: Array.from(digest, value => value.toString(16).padStart(2, "0")).join(""), source_kind: isPdf ? "pdf" : "image", page_count: 1, pages: [], diagnostics: [] };
  if (!isPdf) {
    const canvas = document.createElement("canvas");
    if (/\.tiff?$/i.test(file.name)) {
      const tiff = await import("utif");
      const frames = tiff.decode(bytes.buffer);
      if (!frames[0]) throw new Error("TIFF 文件没有可解码图像。");
      if (frames.length > 1) extraction.diagnostics.push({ level: "warning", code: "multipage_tiff", message: "此 TIFF 含多个图像，本次仅提取第一幅；请将其余图像分别导入并核对。" });
      tiff.decodeImage(bytes.buffer, frames[0]);
      canvas.width = frames[0].width; canvas.height = frames[0].height;
      canvas.getContext("2d")!.putImageData(new ImageData(new Uint8ClampedArray(tiff.toRGBA8(frames[0])), canvas.width, canvas.height), 0, 0);
    } else {
      const bitmap = await createImageBitmap(file);
      try { canvas.width = bitmap.width; canvas.height = bitmap.height; canvas.getContext("2d")!.drawImage(bitmap, 0, 0); }
      finally { bitmap.close(); }
    }
    const text = await recognize(canvas, languages);
    extraction.pages.push({ page: null, text, engine: "tesseract.js", is_ocr: true, region: { left: 0, top: 0, width: canvas.width, height: canvas.height, unit: "pixels" } });
    if (!text.trim()) extraction.diagnostics.push({ level: "warning", code: "empty_image", message: "图像未识别出文字，请核对原件。" });
    return extraction;
  }
  const pdfjs = await import("pdfjs-dist");
  pdfjs.GlobalWorkerOptions.workerSrc = asset("pdfjs/pdf.worker.min.mjs");
  const pdf = await pdfjs.getDocument({ data: bytes, cMapUrl: asset("pdfjs/cmaps/"), cMapPacked: true, standardFontDataUrl: asset("pdfjs/standard_fonts/"), wasmUrl: asset("pdfjs/wasm/") }).promise;
  extraction.page_count = pdf.numPages;
  try {
    // Keep the engine's initial reported range stable throughout extraction.
    for (let number = 1; number <= extraction.page_count; number++) {
      let page: Awaited<ReturnType<typeof pdf.getPage>> | undefined;
      let region = { left: 0, top: 0, width: 1, height: 1, unit: "pdf_millipoints" as const };
      try {
        page = await pdf.getPage(number);
        const viewport = page.getViewport({ scale: 1 });
        region = { ...region, width: Math.round(viewport.width * 1000), height: Math.round(viewport.height * 1000) };
        const content = await page.getTextContent();
        let text = content.items.map(item => "str" in item ? `${item.str}${item.hasEOL ? "\n" : " "}` : "").join("").trim();
        let isOcr = false;
        if (!text) {
          const canvas = document.createElement("canvas"); const renderViewport = page.getViewport({ scale: 2 });
          canvas.width = Math.ceil(renderViewport.width); canvas.height = Math.ceil(renderViewport.height);
          await page.render({ canvas, canvasContext: canvas.getContext("2d")!, viewport: renderViewport }).promise;
          text = await recognize(canvas, languages); isOcr = true;
        }
        extraction.pages.push({ page: number, text, engine: isOcr ? "tesseract.js" : "pdfjs", is_ocr: isOcr, region });
        if (!text.trim()) extraction.diagnostics.push({ level: "warning", code: "empty_page", message: `第 ${number} 页未提取到文字，需要人工核对。` });
      } catch (error) {
        extraction.pages.push({ page: number, text: "", engine: "pdfjs", is_ocr: false, region });
        extraction.diagnostics.push({ level: "error", code: "page_extraction_failed", message: `第 ${number} 页解析失败：${String(error)}` });
      } finally { page?.cleanup(); }
    }
  } finally { await pdf.destroy(); }
  return extraction;
}
