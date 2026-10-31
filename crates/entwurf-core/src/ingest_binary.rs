use super::*;
use crate::ir::{RegionUnit, SourceRegion};
use pdfium_render::prelude::*;
use std::process::Command;

fn diagnostic(ds: &mut Vec<Diagnostic>, code: &str, message: impl Into<String>) {
    ds.push(Diagnostic {
        level: DiagnosticLevel::Warning,
        code: code.into(),
        message: message.into(),
    });
}
fn pdfium(options: &IngestOptions) -> std::result::Result<&'static Pdfium, String> {
    let dir = options
        .pdfium_dir
        .clone()
        .or_else(|| std::env::var_os("ENTWURF_PDFIUM_DIR").map(PathBuf::from))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.join("resources/pdfium")))
        })
        .unwrap_or_else(|| PathBuf::from("resources/pdfium"));
    // Explicitly bind PDFium; do not substitute another engine or silently
    // consult an unrelated system library after a configured path fails.
    static ENGINE: std::sync::OnceLock<(PathBuf, Pdfium)> = std::sync::OnceLock::new();
    static INIT: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let library = Pdfium::pdfium_platform_library_name_at_path(&dir)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    let _guard = INIT.lock().map_err(|e| e.to_string())?;
    if let Some((loaded, engine)) = ENGINE.get() {
        if loaded != &library {
            return Err(
                "PDFium was already initialized from a different path; restart to change engines"
                    .into(),
            );
        }
        return Ok(engine);
    }
    let bindings = Pdfium::bind_to_library(&library).map_err(|e| e.to_string())?;
    let _ = ENGINE.set((library, Pdfium::new(bindings)));
    Ok(&ENGINE.get().expect("PDFium initialized").1)
}
pub(super) fn pdf(
    source: &mut SourceFile,
    bytes: &[u8],
    options: &IngestOptions,
    ds: &mut Vec<Diagnostic>,
) -> Result<Vec<KnowledgeNode>> {
    let engine = match pdfium(options) {
        Ok(e) => e,
        Err(e) => {
            diagnostic(
                ds,
                "pdfium_unavailable",
                format!("Packaged PDFium 7881 is unavailable: {e}"),
            );
            return Ok(vec![]);
        }
    };
    let document = match engine.load_pdf_from_byte_slice(bytes, None) {
        Ok(d) => d,
        Err(e) => {
            diagnostic(ds, "pdf_open_failed", e.to_string());
            return Ok(vec![]);
        }
    };
    let mut nodes = vec![];
    // The dependency iterator converts page-load errors into end-of-iteration.
    // Indexed access preserves later readable pages and reports every failed page.
    for index in 0..document.pages().len() {
        let number = index as u32 + 1;
        let page = match document.pages().get(index) {
            Ok(page) => page,
            Err(error) => {
                ds.push(Diagnostic {
                    level: DiagnosticLevel::Error,
                    code: "pdf_page_load_failed".into(),
                    message: format!("Page {number}: {error}; page content could not be read"),
                });
                continue;
            }
        };
        let region = SourceRegion {
            left: 0,
            top: 0,
            width: (page.width().value * 1000.0).ceil() as u32,
            height: (page.height().value * 1000.0).ceil() as u32,
            unit: RegionUnit::PdfMillipoints,
        };
        let extracted = match page.text() {
            Ok(t) => t.all(),
            Err(e) => {
                diagnostic(ds, "pdf_text_failed", format!("Page {number}: {e}"));
                String::new()
            }
        };
        if !extracted.trim().is_empty() {
            nodes.extend(transcript(
                source,
                &extracted,
                Some(number),
                region,
                false,
                options,
            ));
        } else {
            // Full page at 300 dpi, preserving aspect ratio and all page content.
            // A failed render is reported per page and other pages continue.
            let bitmap = match page.render_with_config(
                &PdfRenderConfig::new()
                    .set_target_width((page.width().value * 300.0 / 72.0).ceil() as i32),
            ) {
                Ok(b) => b,
                Err(e) => {
                    diagnostic(ds, "pdf_render_failed", format!("Page {number}: {e}"));
                    continue;
                }
            };
            let image = match bitmap.as_image() {
                Ok(i) => i,
                Err(e) => {
                    diagnostic(ds, "pdf_render_failed", format!("Page {number}: {e}"));
                    continue;
                }
            };
            let temp = tempfile::Builder::new()
                .suffix(".png")
                .tempfile()
                .map_err(|e| EntwurfError::InvalidInput(e.to_string()))?;
            image
                .save_with_format(temp.path(), ::image::ImageFormat::Png)
                .map_err(|e| EntwurfError::InvalidInput(e.to_string()))?;
            if let Some(text) = ocr(temp.path(), options, ds) {
                nodes.extend(transcript(
                    source,
                    &text,
                    Some(number),
                    region,
                    true,
                    options,
                ));
            }
        }
    }
    Ok(nodes)
}
pub(super) fn image(
    source: &mut SourceFile,
    path: &Path,
    options: &IngestOptions,
    ds: &mut Vec<Diagnostic>,
) -> Result<Vec<KnowledgeNode>> {
    // Content-addressed files have no suffix; inspect the actual bytes.
    let img = ::image::ImageReader::open(path)
        .map_err(|e| EntwurfError::read(path, e))?
        .with_guessed_format()
        .map_err(|e| EntwurfError::read(path, e))?
        .decode()
        .map_err(|e| EntwurfError::InvalidInput(format!("Image decoding failed: {e}")))?;
    let region = SourceRegion {
        left: 0,
        top: 0,
        width: img.width(),
        height: img.height(),
        unit: RegionUnit::Pixels,
    };
    // Normalize supported images for Tesseract (including WebP), without changing
    // the original stored source. TempFile cleanup runs on every return path.
    let temp = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .map_err(|e| EntwurfError::InvalidInput(e.to_string()))?;
    img.save_with_format(temp.path(), ::image::ImageFormat::Png)
        .map_err(|e| EntwurfError::InvalidInput(e.to_string()))?;
    Ok(ocr(temp.path(), options, ds)
        .map(|t| transcript(source, &t, None, region, true, options))
        .unwrap_or_default())
}
fn ocr(path: &Path, options: &IngestOptions, ds: &mut Vec<Diagnostic>) -> Option<String> {
    let executable = if cfg!(windows) {
        "tesseract.exe"
    } else {
        "tesseract"
    };
    let command = options
        .binary_dir
        .as_ref()
        .map(|p| p.join(executable))
        .unwrap_or_else(|| executable.into());
    let output = match Command::new(command)
        .arg(path)
        .arg("stdout")
        .arg("-l")
        .arg(options.ocr_languages.as_deref().unwrap_or("chi_sim+eng"))
        .output()
    {
        Ok(o) => o,
        Err(e) => {
            diagnostic(
                ds,
                "ocr_unavailable",
                format!("Local Tesseract with chi_sim+eng is unavailable: {e}"),
            );
            return None;
        }
    };
    if !output.status.success() {
        diagnostic(
            ds,
            "ocr_failed",
            format!(
                "Tesseract {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        );
        return None;
    }
    // Tesseract may exit successfully with only some requested languages loaded.
    // Keep its explicit language failure even when useful partial text exists.
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lowered = stderr.to_ascii_lowercase();
    if lowered.contains("failed loading language") || lowered.contains("error opening data file") {
        diagnostic(
            ds,
            "ocr_language_unavailable",
            format!(
                "Requested OCR languages {} were not all available: {}",
                options.ocr_languages.as_deref().unwrap_or("chi_sim+eng"),
                stderr.trim()
            ),
        );
    }
    match String::from_utf8(output.stdout) {
        Ok(t) => Some(t),
        Err(e) => {
            diagnostic(ds, "ocr_invalid_utf8", e.to_string());
            None
        }
    }
}
fn transcript(
    source: &mut SourceFile,
    input: &str,
    page: Option<u32>,
    region: SourceRegion,
    is_ocr: bool,
    options: &IngestOptions,
) -> Vec<KnowledgeNode> {
    source.notes.push(format!("ingestion_transcript:{}",serde_json::json!({"page":page,"engine":if is_ocr{"tesseract"}else{"pdfium-7881"},"text":input,"sha256":hash_bytes(input.as_bytes()),"offset_unit":"utf8_bytes"})));
    // PDF text and OCR are plain transcripts, never interpreted as Markdown.
    let mut nodes = vec![];
    chunks(
        source,
        input,
        0..input.len(),
        &source.original_name,
        None,
        options,
        &mut nodes,
    );
    for n in &mut nodes {
        let a = &mut n.anchors[0];
        a.page = page;
        a.region = Some(region.clone());
        a.label = format!(
            "{}:{}:transcript-bytes:{}-{}",
            source.original_name,
            page.map_or_else(|| "image".into(), |p| format!("p{p}")),
            a.char_start.unwrap(),
            a.char_end.unwrap()
        );
        n.tags.push("page_transcript".into());
        if n.body.contains(['=', '∑', '∫', '≤', '≥', '≠', '√']) {
            // Flag potential mathematics without inventing LaTeX from glyphs.
            n.tags.push("formula_candidate".into());
        }
        if is_ocr {
            n.tags.push("ocr".into());
            n.review_note =
                Some("OCR text and any formulas require visual source confirmation".into());
        }
    }
    nodes
}
