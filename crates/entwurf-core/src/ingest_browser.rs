//! Binary extraction performed by browser PDF/OCR engines, rebound to retained bytes.
use super::*;
use crate::ir::{RegionUnit, SourceRegion};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserExtraction {
    pub schema_version: u32,
    pub source_sha256: String,
    pub source_kind: SourceKind,
    pub page_count: u32,
    pub pages: Vec<BrowserPage>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserPage {
    pub page: Option<u32>,
    pub text: String,
    pub engine: String,
    pub is_ocr: bool,
    pub region: SourceRegion,
}
fn invalid(message: &str) -> EntwurfError {
    EntwurfError::InvalidInput(message.into())
}
fn extraction_path(hash: &str) -> PathBuf {
    PathBuf::from("/extractions").join(format!("{hash}.json"))
}
fn check(extraction: &BrowserExtraction, bytes: &[u8], kind: &SourceKind) -> Result<()> {
    if extraction.schema_version != 1
        || &extraction.source_kind != kind
        || extraction.source_sha256 != hash_bytes(bytes)
    {
        return Err(invalid(
            "extraction schema, source type, or SHA-256 hash mismatch",
        ));
    }
    if extraction.page_count == 0
        || extraction.page_count > 10000
        || extraction.pages.len() != extraction.page_count as usize
    {
        return Err(invalid("extraction must account for every source page"));
    }
    let pdf = *kind == SourceKind::Pdf;
    if !pdf && *kind != SourceKind::Image {
        return Err(invalid("only PDF and image extraction is accepted"));
    }
    if pdf && !bytes.starts_with(b"%PDF-") {
        return Err(invalid("PDF signature is missing"));
    }
    if !pdf {
        if extraction.page_count != 1 {
            return Err(invalid("image extraction has exactly one page"));
        }
        let reader = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| invalid("invalid source image"))?;
        let (width, height) = reader
            .into_dimensions()
            .map_err(|_| invalid("invalid source image dimensions"))?;
        let region = &extraction.pages[0].region;
        if region.left != 0 || region.top != 0 || region.width != width || region.height != height {
            return Err(invalid(
                "image extraction region must cover the actual retained image",
            ));
        }
    }
    let mut pages = BTreeSet::new();
    for page in &extraction.pages {
        if !pages.insert(page.page)
            || (pdf
                && page
                    .page
                    .is_none_or(|n| n == 0 || n > extraction.page_count))
            || (!pdf && page.page.is_some())
        {
            return Err(invalid(
                "extraction page identity is missing, duplicated, or out of range",
            ));
        }
        if page.text.len() > 16 * 1024 * 1024
            || page.region.width == 0
            || page.region.height == 0
            || page.region.unit
                != if pdf {
                    RegionUnit::PdfMillipoints
                } else {
                    RegionUnit::Pixels
                }
            || (page.is_ocr && page.engine != "tesseract.js")
            || (!page.is_ocr && (page.engine != "pdfjs" || !pdf))
        {
            return Err(invalid(
                "invalid extraction engine, transcript size, or source region",
            ));
        }
    }
    Ok(())
}
/// Register only source-bound structured extraction. Anchors and transcript hashes
/// are computed here by the same ingestion primitives as native extraction.
pub fn register_extraction(path: &Path, extraction: BrowserExtraction) -> Result<()> {
    let bytes = fs::read(path).map_err(|e| EntwurfError::read(path, e))?;
    check(&extraction, &bytes, &detect_kind(path)?)?;
    let data = serde_json::to_vec(&extraction).map_err(|e| invalid(&e.to_string()))?;
    storage::write_atomic(&extraction_path(&extraction.source_sha256), &data)
}
fn extract(
    source: &mut SourceFile,
    bytes: &[u8],
    options: &IngestOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<KnowledgeNode>> {
    let path = extraction_path(&source.content_hash);
    let data = fs::read(&path).map_err(|_| {
        EntwurfError::Unsupported(
            "Run browser PDF/OCR extraction before importing this source".into(),
        )
    })?;
    let extraction: BrowserExtraction = serde_json::from_slice(&data)
        .map_err(|e| invalid(&format!("invalid saved extraction: {e}")))?;
    check(&extraction, bytes, &source.kind)?;
    diagnostics.extend(extraction.diagnostics);
    let mut nodes = Vec::new();
    for page in extraction.pages {
        // A successful empty transcript is a blank page. Only extraction-engine
        // diagnostics indicate failure; those remain ordinary source blockers.
        source.notes.push(format!("ingestion_transcript:{}",serde_json::json!({"page":page.page,"engine":page.engine,"text":page.text,"sha256":hash_bytes(page.text.as_bytes()),"offset_unit":"utf8_bytes"})));
        let start = nodes.len();
        chunks(
            source,
            &page.text,
            0..page.text.len(),
            &source.original_name,
            None,
            options,
            &mut nodes,
        );
        for node in &mut nodes[start..] {
            let anchor = &mut node.anchors[0];
            anchor.page = page.page;
            anchor.region = Some(page.region.clone());
            anchor.label = format!(
                "{}:{}:transcript-bytes:{}-{}",
                source.original_name,
                page.page
                    .map_or_else(|| "image".into(), |n| format!("p{n}")),
                anchor.char_start.unwrap(),
                anchor.char_end.unwrap()
            );
            node.tags.push("page_transcript".into());
            if node.body.contains(['=', '∑', '∫', '≤', '≥', '≠', '√']) {
                node.tags.push("formula_candidate".into());
            }
            if page.is_ocr {
                node.tags.push("ocr".into());
                node.review_note =
                    Some("OCR text and any formulas require visual source confirmation".into());
            }
        }
    }
    Ok(nodes)
}
pub(super) fn pdf(
    source: &mut SourceFile,
    bytes: &[u8],
    options: &IngestOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<KnowledgeNode>> {
    extract(source, bytes, options, diagnostics)
}
pub(super) fn image(
    source: &mut SourceFile,
    path: &Path,
    options: &IngestOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<KnowledgeNode>> {
    let bytes = fs::read(path).map_err(|e| EntwurfError::read(path, e))?;
    extract(source, &bytes, options, diagnostics)
}
