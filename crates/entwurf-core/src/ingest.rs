//! Lossless source ingestion. `char_start/end` are UTF-8 byte offsets, never
//! Unicode scalar indices. For binary sources they index the exact page
//! transcript persisted as JSON in SourceFile.notes, not the binary file.
use crate::files as fs;

use crate::error::{EntwurfError, Result};
use crate::ir::{
    new_id, EdgeKind, GraphEdge, Importance, KnowledgeIr, KnowledgeNode, NodeKind, NodeOrigin,
    SourceAnchor, SourceFile, SourceKind, UnverifiedItem, UnverifiedKind,
};
use crate::storage::{self, ProjectPaths};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
#[cfg(not(feature = "browser"))]
#[path = "ingest_binary.rs"]
mod binary;
#[cfg(feature = "browser")]
#[path = "ingest_browser.rs"]
mod binary;
#[cfg(feature = "browser")]
pub use binary::{register_extraction, BrowserExtraction, BrowserPage};
#[path = "ingest_text.rs"]
mod text;

#[derive(Debug, Clone)]
pub struct IngestOptions {
    pub ocr_languages: Option<String>,
    pub max_chars_per_node: usize,
    /// Explicit Tesseract directory; no PATH fallback when configured.
    pub binary_dir: Option<PathBuf>,
    /// Directory containing the pinned PDFium shared library. Packaged callers
    /// resolve their resource directory; no downloads occur during ingestion.
    pub pdfium_dir: Option<PathBuf>,
}
impl Default for IngestOptions {
    fn default() -> Self {
        Self::standard()
    }
}
impl IngestOptions {
    pub fn standard() -> Self {
        Self {
            ocr_languages: Some("chi_sim+eng".into()),
            max_chars_per_node: 2200,
            binary_dir: None,
            pdfium_dir: None,
        }
    }
}
#[derive(Debug, Clone)]
pub struct IngestedSource {
    pub source: SourceFile,
    pub nodes: Vec<KnowledgeNode>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub code: String,
    pub message: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticLevel {
    Info,
    Warning,
    Error,
}
#[derive(Debug, Clone, Default)]
pub struct IngestReport {
    pub sources: Vec<IngestedSource>,
    pub diagnostics: Vec<Diagnostic>,
}
impl IngestReport {
    pub fn node_count(&self) -> usize {
        self.sources.iter().map(|s| s.nodes.len()).sum()
    }
    pub fn into_ir(self, project_id: &str, task_id: &str, title: &str) -> KnowledgeIr {
        let mut ir = KnowledgeIr::empty(project_id, task_id, title);
        ir.unverified = unverified_from_diagnostics(&self.sources);
        for mut source in self.sources {
            for d in &source.diagnostics {
                source.source.notes.push(diagnostic_note(d));
            }
            for node in &source.nodes {
                if let Some(parent) = &node.parent_id {
                    ir.edges.push(GraphEdge {
                        id: new_id(),
                        from: parent.clone(),
                        to: node.id.clone(),
                        edge_type: EdgeKind::Contains,
                        label: None,
                        anchors: node.anchors.clone(),
                    });
                }
            }
            ir.sources.push(source.source);
            ir.nodes.extend(source.nodes);
        }
        // Report-wide notices (for example duplicate selections) also survive serialization.
        if let Some(source) = ir.sources.first_mut() {
            for d in &self.diagnostics {
                let note = diagnostic_note(d);
                if !source.notes.contains(&note) {
                    source.notes.push(note);
                }
            }
        }
        ir.touch();
        ir
    }
    pub fn push_diagnostic(
        &mut self,
        level: DiagnosticLevel,
        code: &str,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            level,
            code: code.into(),
            message: message.into(),
        });
    }
    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .chain(self.sources.iter().flat_map(|s| &s.diagnostics))
            .any(|d| d.level == DiagnosticLevel::Error)
    }
}
fn diagnostic_note(d: &Diagnostic) -> String {
    format!(
        "ingestion_diagnostic:{}",
        serde_json::to_string(d).expect("diagnostic serialization")
    )
}
pub fn detect_kind(path: &Path) -> Result<SourceKind> {
    match path
        .extension()
        .and_then(|x| x.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "pdf" => Ok(SourceKind::Pdf),
        "md" | "markdown" | "txt" => Ok(SourceKind::Markdown),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" => Ok(SourceKind::Image),
        "tex" | "latex" => Ok(SourceKind::Latex),
        ext => Err(EntwurfError::UnsupportedSource(ext.into())),
    }
}
pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn source_for(path: &Path, bytes: &[u8]) -> Result<SourceFile> {
    Ok(SourceFile {
        id: new_id(),
        original_name: path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("source")
            .into(),
        kind: detect_kind(path)?,
        content_hash: hash_bytes(bytes),
        stored_path: path.display().to_string(),
        byte_len: bytes.len() as u64,
        imported_at: chrono::Utc::now(),
        notes: vec![],
    })
}
pub fn ingest_path(path: &Path, options: &IngestOptions) -> Result<IngestedSource> {
    let bytes = fs::read(path).map_err(|e| EntwurfError::read(path, e))?;
    ingest_source(source_for(path, &bytes)?, &bytes, path, options)
}
/// Re-import a stored source using its explicit format, even though its hashed
/// filename has no extension. Source identity is retained.
pub fn ingest_source(
    mut source: SourceFile,
    bytes: &[u8],
    path: &Path,
    options: &IngestOptions,
) -> Result<IngestedSource> {
    if source.content_hash != hash_bytes(bytes) {
        return Err(EntwurfError::InvalidInput("source SHA-256 mismatch".into()));
    }
    // A reparse establishes a new baseline. Do not leave page-local offsets
    // ambiguous by retaining an older transcript produced with different OCR.
    source.notes.retain(|note| {
        !note.starts_with("ingestion_transcript:") && !note.starts_with("ingestion_diagnostic:")
    });
    let mut diagnostics = vec![];
    let nodes = match source.kind {
        SourceKind::Markdown | SourceKind::Latex => {
            let input = std::str::from_utf8(bytes).map_err(|e| {
                EntwurfError::InvalidInput(format!("{} is not UTF-8: {e}", source.original_name))
            })?;
            if input.len() > u32::MAX as usize {
                return Err(EntwurfError::InvalidInput(
                    "source exceeds UTF-8 anchor capacity".into(),
                ));
            }
            if source.kind == SourceKind::Markdown {
                text::markdown(&source, input, options)
            } else {
                text::latex(&source, input, options, &mut diagnostics)
            }
        }
        SourceKind::Pdf => binary::pdf(&mut source, bytes, options, &mut diagnostics)?,
        SourceKind::Image => binary::image(&mut source, path, options, &mut diagnostics)?,
    };
    if nodes.is_empty() {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            code: "empty_source".into(),
            message: format!("No readable text found in {}", source.original_name),
        });
    }
    Ok(IngestedSource {
        source,
        nodes,
        diagnostics,
    })
}
pub fn ingest_paths(paths: &[PathBuf], options: &IngestOptions) -> Result<IngestReport> {
    import_paths(None, paths, options)
}
/// Copy selected files into content-addressed project storage before parsing.
/// Callers attach the resulting IR to their project task/version and save it.
pub fn ingest_project_paths(
    project: &ProjectPaths,
    paths: &[PathBuf],
    options: &IngestOptions,
) -> Result<IngestReport> {
    import_paths(Some(project), paths, options)
}
fn import_paths(
    project: Option<&ProjectPaths>,
    paths: &[PathBuf],
    options: &IngestOptions,
) -> Result<IngestReport> {
    let mut report = IngestReport::default();
    let mut hashes = BTreeSet::new();
    for path in paths {
        let bytes = fs::read(path).map_err(|e| EntwurfError::read(path, e))?;
        let mut source = source_for(path, &bytes)?;
        if !hashes.insert(source.content_hash.clone()) {
            report.push_diagnostic(
                DiagnosticLevel::Info,
                "duplicate_source",
                format!(
                    "{} has identical content and was skipped",
                    source.original_name
                ),
            );
            continue;
        }
        let stored = if let Some(project) = project {
            storage::copy_source(project, path, &source.content_hash, &source.original_name)?
        } else {
            path.clone()
        };
        source.stored_path = stored.display().to_string();
        let ingested = ingest_source(source, &bytes, &stored, options)?;
        report.diagnostics.extend(ingested.diagnostics.clone());
        report.sources.push(ingested);
    }
    Ok(report)
}
fn node(
    source: &SourceFile,
    input: &str,
    range: std::ops::Range<usize>,
    kind: NodeKind,
    title: &str,
    parent: Option<String>,
    latex: Option<String>,
) -> KnowledgeNode {
    let quote = &input[range.clone()];
    KnowledgeNode {
        id: new_id(),
        kind,
        title: title.into(),
        body: quote.into(),
        latex,
        anchors: vec![SourceAnchor {
            source_id: source.id.clone(),
            page: None,
            line: Some(input[..range.start].bytes().filter(|b| *b == b'\n').count() as u32 + 1),
            char_start: Some(range.start as u32),
            char_end: Some(range.end as u32),
            quote_hash: Some(hash_bytes(quote.as_bytes())),
            label: format!(
                "{}:bytes:{}-{}",
                source.original_name, range.start, range.end
            ),
            region: None,
        }],
        importance: Importance::Normal,
        origin: NodeOrigin::Source,
        parent_id: parent,
        tags: vec![],
        verified: false,
        review_note: None,
    }
}
fn chunks(
    source: &SourceFile,
    input: &str,
    range: std::ops::Range<usize>,
    title: &str,
    parent: Option<String>,
    options: &IngestOptions,
    nodes: &mut Vec<KnowledgeNode>,
) {
    if input[range.clone()].trim().is_empty() {
        return;
    }
    let mut start = range.start;
    while start < range.end {
        let end = input[start..range.end]
            .char_indices()
            .nth(options.max_chars_per_node.max(1))
            .map_or(range.end, |(i, _)| start + i);
        nodes.push(node(
            source,
            input,
            start..end,
            NodeKind::Concept,
            title,
            parent.clone(),
            None,
        ));
        start = end;
    }
}
pub fn unverified_from_diagnostics(sources: &[IngestedSource]) -> Vec<UnverifiedItem> {
    let mut items = vec![];
    for s in sources {
        for n in &s.nodes {
            for (required, kind, reason) in [
                (
                    n.kind == NodeKind::Formula
                        || n.latex.is_some()
                        || n.tags.iter().any(|t| t == "formula_candidate"),
                    UnverifiedKind::Formula,
                    "Confirm exact formula against its source",
                ),
                (
                    n.tags.iter().any(|t| t == "ocr"),
                    UnverifiedKind::Ocr,
                    "Confirm OCR transcription against the original image",
                ),
            ] {
                if required {
                    items.push(UnverifiedItem {
                        id: new_id(),
                        node_id: Some(n.id.clone()),
                        kind,
                        text: n.latex.clone().unwrap_or_else(|| n.body.clone()),
                        reason: reason.into(),
                    });
                }
            }
        }
        for d in &s.diagnostics {
            if d.level != DiagnosticLevel::Info && d.code != "empty_source" {
                items.push(UnverifiedItem {
                    id: new_id(),
                    node_id: None,
                    kind: if d.code.starts_with("ocr") {
                        UnverifiedKind::Ocr
                    } else {
                        UnverifiedKind::MissingAnchor
                    },
                    text: s.source.original_name.clone(),
                    reason: format!("{}: {}", d.code, d.message),
                });
            }
        }
    }
    items
}
pub fn source_ids(ir: &KnowledgeIr) -> BTreeSet<&str> {
    ir.sources.iter().map(|s| s.id.as_str()).collect()
}
