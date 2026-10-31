//! Deterministic source ingestion. Every node produced here points back to a
//! concrete source range, so later model output can be audited.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use sha2::{Digest, Sha256};

use crate::error::{EntwurfError, Result};
use crate::ir::{
    new_id, Importance, KnowledgeIr, KnowledgeNode, NodeKind, NodeOrigin, SourceAnchor, SourceFile,
    SourceKind, UnverifiedItem, UnverifiedKind,
};

#[derive(Debug, Clone, Default)]
pub struct IngestOptions {
    pub ocr_languages: Option<String>,
    pub max_chars_per_node: usize,
    pub binary_dir: Option<PathBuf>,
}

impl IngestOptions {
    pub fn standard() -> Self {
        Self {
            ocr_languages: Some("chi_sim+eng".to_string()),
            max_chars_per_node: 2200,
            binary_dir: None,
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
        self.sources.iter().map(|source| source.nodes.len()).sum()
    }

    pub fn into_ir(mut self, project_id: &str, task_id: &str, title: &str) -> KnowledgeIr {
        let mut ir = KnowledgeIr::empty(project_id, task_id, title);
        for source in self.sources.drain(..) {
            ir.sources.push(source.source);
            ir.nodes.extend(source.nodes);
            self.diagnostics.extend(source.diagnostics);
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
            code: code.to_string(),
            message: message.into(),
        });
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.level == DiagnosticLevel::Error)
    }
}

pub fn detect_kind(path: &Path) -> Result<SourceKind> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "pdf" => Ok(SourceKind::Pdf),
        "md" | "markdown" | "txt" => Ok(SourceKind::Markdown),
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "tif" | "tiff" => Ok(SourceKind::Image),
        "tex" | "latex" => Ok(SourceKind::Latex),
        other => Err(EntwurfError::UnsupportedSource(if other.is_empty() {
            path.display().to_string()
        } else {
            other.to_string()
        })),
    }
}

pub fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn ingest_path(path: &Path, options: &IngestOptions) -> Result<IngestedSource> {
    let bytes = fs::read(path).map_err(|error| EntwurfError::read(path, error))?;
    let kind = detect_kind(path)?;
    let hash = hash_bytes(&bytes);
    let id = new_id();
    let source = SourceFile {
        id: id.clone(),
        original_name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("source")
            .to_string(),
        kind: kind.clone(),
        content_hash: hash.clone(),
        stored_path: path.display().to_string(),
        byte_len: bytes.len() as u64,
        imported_at: chrono::Utc::now(),
        notes: Vec::new(),
    };
    let mut diagnostics = Vec::new();
    let nodes = match kind {
        SourceKind::Markdown => ingest_markdown(&source, &bytes, options)?,
        SourceKind::Latex => ingest_latex(&source, &bytes, options)?,
        SourceKind::Pdf => ingest_pdf(&source, path, options, &mut diagnostics)?,
        SourceKind::Image => ingest_image(&source, path, options, &mut diagnostics)?,
    };
    if nodes.is_empty() {
        diagnostics.push(Diagnostic {
            level: DiagnosticLevel::Warning,
            code: "empty_source".to_string(),
            message: format!("no readable text was found in {}", source.original_name),
        });
    }
    Ok(IngestedSource {
        source,
        nodes,
        diagnostics,
    })
}

pub fn ingest_paths(paths: &[PathBuf], options: &IngestOptions) -> Result<IngestReport> {
    let mut report = IngestReport::default();
    let mut hashes = std::collections::BTreeSet::new();
    for path in paths {
        let ingested = ingest_path(path, options)?;
        if !hashes.insert(ingested.source.content_hash.clone()) {
            report.push_diagnostic(
                DiagnosticLevel::Warning,
                "duplicate_source",
                format!(
                    "{} is identical to an earlier source and was skipped",
                    ingested.source.original_name
                ),
            );
            continue;
        }
        report
            .diagnostics
            .extend(ingested.diagnostics.iter().cloned());
        report.sources.push(ingested);
    }
    Ok(report)
}

fn ingest_markdown(
    source: &SourceFile,
    bytes: &[u8],
    options: &IngestOptions,
) -> Result<Vec<KnowledgeNode>> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|error| {
        EntwurfError::InvalidInput(format!("{} is not UTF-8: {error}", source.original_name))
    })?;
    Ok(markdown_nodes(source, &text, options))
}

fn markdown_nodes(source: &SourceFile, text: &str, options: &IngestOptions) -> Vec<KnowledgeNode> {
    let heading = Regex::new(r"^(#{1,6})\s+(.+?)\s*$").expect("valid heading regex");
    let formula = Regex::new(r"\$\$(.+?)\$\$|\\\[(.+?)\\\]").expect("valid formula regex");
    let mut nodes = Vec::new();
    let mut current_title = source.original_name.clone();
    let mut current_body = String::new();
    let mut current_start_line = 1usize;
    let mut current_start_byte = 0usize;
    let mut line_start = 0usize;

    for (line_index, raw_line) in text.split_inclusive('\n').enumerate() {
        let line_number = line_index as u32 + 1;
        let line_number_usize = line_index + 1;
        let line = raw_line.trim_end_matches(['\r', '\n']);
        if let Some(captures) = heading.captures(line) {
            flush_markdown_node(
                source,
                &mut nodes,
                &current_title,
                &current_body,
                current_start_line,
                current_start_byte,
                line_start,
                options,
            );
            let level = captures
                .get(1)
                .map(|value| value.as_str().len())
                .unwrap_or(1);
            current_title = captures
                .get(2)
                .map(|value| value.as_str().trim().to_string())
                .unwrap_or_else(|| source.original_name.clone());
            while current_title.starts_with('#') {
                current_title = current_title.trim_start_matches('#').trim().to_string();
            }
            current_body.clear();
            current_start_line = line_number_usize;
            current_start_byte = line_start;
            if level == 1 || level == 2 {
                nodes.push(KnowledgeNode {
                    id: new_id(),
                    kind: if level == 1 {
                        NodeKind::Chapter
                    } else {
                        NodeKind::Section
                    },
                    title: current_title.clone(),
                    body: String::new(),
                    latex: None,
                    anchors: vec![anchor_for(
                        source,
                        line_number,
                        current_start_byte,
                        raw_line,
                    )],
                    importance: if level == 1 {
                        Importance::Critical
                    } else {
                        Importance::High
                    },
                    origin: NodeOrigin::Source,
                    parent_id: None,
                    tags: vec![],
                    verified: false,
                    review_note: None,
                });
            }
        } else {
            let trimmed = line.trim();
            let formula_match = formula.captures(trimmed).and_then(|captures| {
                captures
                    .get(1)
                    .or_else(|| captures.get(2))
                    .map(|value| value.as_str().trim().to_string())
            });
            if let Some(latex) = formula_match {
                flush_markdown_node(
                    source,
                    &mut nodes,
                    &current_title,
                    &current_body,
                    current_start_line,
                    current_start_byte,
                    line_start,
                    options,
                );
                nodes.push(KnowledgeNode {
                    id: new_id(),
                    kind: NodeKind::Formula,
                    title: "Formula".to_string(),
                    body: String::new(),
                    latex: Some(latex),
                    anchors: vec![anchor_for(source, line_number, line_start, raw_line)],
                    importance: Importance::High,
                    origin: NodeOrigin::Source,
                    parent_id: None,
                    tags: vec![],
                    verified: false,
                    review_note: None,
                });
                current_body.clear();
                current_start_line = line_number_usize;
                current_start_byte = line_start;
            } else {
                if !trimmed.is_empty() {
                    current_body.push_str(trimmed);
                    current_body.push('\n');
                }
            }
        }
        line_start += raw_line.len();
    }
    flush_markdown_node(
        source,
        &mut nodes,
        &current_title,
        &current_body,
        current_start_line,
        current_start_byte,
        line_start,
        options,
    );
    nodes
}

fn flush_markdown_node(
    source: &SourceFile,
    nodes: &mut Vec<KnowledgeNode>,
    title: &str,
    body: &str,
    start_line: usize,
    start_byte: usize,
    end_byte: usize,
    options: &IngestOptions,
) {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return;
    }
    let mut anchor = anchor_for(source, start_line as u32, start_byte, trimmed);
    anchor.char_start = Some(start_byte as u32);
    anchor.char_end = Some(end_byte as u32);
    let kind = classify_text(title, trimmed);
    let (title, body) = split_node_text(title, trimmed, options.max_chars_per_node);
    nodes.push(KnowledgeNode {
        id: new_id(),
        kind,
        title,
        body,
        latex: None,
        anchors: vec![anchor],
        importance: Importance::Normal,
        origin: NodeOrigin::Source,
        parent_id: None,
        tags: vec![],
        verified: false,
        review_note: None,
    });
}

fn split_node_text(title: &str, body: &str, max_chars: usize) -> (String, String) {
    let max_chars = max_chars.max(80);
    if body.chars().count() <= max_chars {
        return (title.to_string(), body.to_string());
    }
    let mut out = String::new();
    let mut count = 0usize;
    for (index, character) in body.char_indices() {
        if count >= max_chars && matches!(character, '。' | '！' | '？' | '.' | '!' | '?' | '\n')
        {
            out = body[..index + character.len_utf8()].trim().to_string();
            break;
        }
        count += 1;
    }
    if out.is_empty() {
        out = body.chars().take(max_chars).collect();
    }
    (title.to_string(), out)
}

fn classify_text(title: &str, body: &str) -> NodeKind {
    let haystack = format!("{} {}", title, body).to_ascii_lowercase();
    for (needle, kind) in [
        ("定义", NodeKind::Definition),
        ("definition", NodeKind::Definition),
        ("定理", NodeKind::Theorem),
        ("theorem", NodeKind::Theorem),
        ("例题", NodeKind::Example),
        ("example", NodeKind::Example),
        ("注意", NodeKind::Warning),
        ("warning", NodeKind::Warning),
    ] {
        if haystack.to_ascii_lowercase().contains(needle) {
            return kind;
        }
    }
    if body.contains('$') || body.contains('\\') {
        NodeKind::Formula
    } else {
        NodeKind::Concept
    }
}

fn anchor_for(source: &SourceFile, line: u32, char_start: usize, text: &str) -> SourceAnchor {
    SourceAnchor {
        source_id: source.id.clone(),
        page: None,
        line: Some(line),
        char_start: Some(char_start as u32),
        char_end: Some((char_start + text.len()) as u32),
        quote_hash: Some(hash_bytes(text.as_bytes())),
        label: format!("{}:{}", source.original_name, line),
    }
}

fn ingest_latex(
    source: &SourceFile,
    bytes: &[u8],
    options: &IngestOptions,
) -> Result<Vec<KnowledgeNode>> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|error| {
        EntwurfError::InvalidInput(format!("{} is not UTF-8: {error}", source.original_name))
    })?;
    let mut nodes = Vec::new();
    let section =
        Regex::new(r"\\(chapter|section|subsection)\*?\{([^{}]+)\}").expect("valid section regex");
    let environment = Regex::new(r"\\begin\{(definition|theorem|example|proof|equation|align)\}")
        .expect("valid env regex");
    let mut current_title = source.original_name.clone();
    let mut body = String::new();
    let mut line = 1u32;
    for raw_line in text.lines() {
        let trimmed = raw_line.trim();
        if let Some(captures) = section.captures(trimmed) {
            flush_latex_node(source, &mut nodes, &current_title, &body, line, options);
            current_title = captures
                .get(2)
                .map(|value| value.as_str().trim().to_string())
                .unwrap_or_else(|| "Section".to_string());
            body.clear();
        } else if let Some(captures) = environment.captures(trimmed) {
            let kind = match captures.get(1).map(|value| value.as_str()).unwrap_or("") {
                "definition" => NodeKind::Definition,
                "theorem" => NodeKind::Theorem,
                "example" => NodeKind::Example,
                "equation" | "align" => NodeKind::Formula,
                _ => NodeKind::Note,
            };
            let latex = trimmed.to_string();
            nodes.push(KnowledgeNode {
                id: new_id(),
                kind,
                title: current_title.clone(),
                body: String::new(),
                latex: Some(latex),
                anchors: vec![anchor_for(source, line, 0, trimmed)],
                importance: Importance::High,
                origin: NodeOrigin::Source,
                parent_id: None,
                tags: vec![],
                verified: false,
                review_note: None,
            });
        } else if !trimmed.is_empty() && !trimmed.starts_with('%') {
            body.push_str(trimmed);
            body.push('\n');
        }
        line += 1;
    }
    flush_latex_node(source, &mut nodes, &current_title, &body, line, options);
    Ok(nodes)
}

fn flush_latex_node(
    source: &SourceFile,
    nodes: &mut Vec<KnowledgeNode>,
    title: &str,
    body: &str,
    line: u32,
    options: &IngestOptions,
) {
    let body = body.trim();
    if body.is_empty() {
        return;
    }
    let (title, body) = split_node_text(title, body, options.max_chars_per_node);
    nodes.push(KnowledgeNode {
        id: new_id(),
        kind: classify_text(&title, &body),
        title,
        body,
        latex: None,
        anchors: vec![anchor_for(source, line, 0, "")],
        importance: Importance::Normal,
        origin: NodeOrigin::Source,
        parent_id: None,
        tags: vec![],
        verified: false,
        review_note: None,
    });
}

fn ingest_pdf(
    source: &SourceFile,
    path: &Path,
    options: &IngestOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<KnowledgeNode>> {
    if let Ok(text) = extract_pdf_text(path, options.binary_dir.as_deref()) {
        if !text.trim().is_empty() {
            let mut nodes = markdown_nodes(source, &text, options);
            for (index, node) in nodes.iter_mut().enumerate() {
                node.anchors[0].page = Some(index as u32 / 8 + 1);
                node.anchors[0].label =
                    format!("{}:p{}", source.original_name, index as u32 / 8 + 1);
            }
            return Ok(nodes);
        }
    }
    diagnostics.push(Diagnostic {
        level: DiagnosticLevel::Warning,
        code: "pdf_text_layer_missing".to_string(),
        message: format!(
            "{} has no extractable text layer; install pdftotext or import an OCR transcript",
            source.original_name
        ),
    });
    Ok(vec![])
}

fn extract_pdf_text(path: &Path, binary_dir: Option<&Path>) -> Result<String> {
    let command = binary_dir
        .map(|dir| {
            dir.join(if cfg!(windows) {
                "pdftotext.exe"
            } else {
                "pdftotext"
            })
        })
        .filter(|candidate| candidate.exists())
        .unwrap_or_else(|| PathBuf::from("pdftotext"));
    let output = Command::new(&command)
        .arg("-layout")
        .arg(path)
        .arg("-")
        .output()
        .map_err(|error| EntwurfError::Unsupported(format!("pdftotext unavailable: {error}")))?;
    if !output.status.success() {
        return Err(EntwurfError::InvalidInput(format!(
            "pdftotext failed for {} with status {}",
            path.display(),
            output.status
        )));
    }
    String::from_utf8(output.stdout).map_err(|error| {
        EntwurfError::InvalidInput(format!("pdftotext returned non-UTF-8: {error}"))
    })
}

fn ingest_image(
    source: &SourceFile,
    path: &Path,
    options: &IngestOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<KnowledgeNode>> {
    let languages = options
        .ocr_languages
        .clone()
        .unwrap_or_else(|| "chi_sim+eng".to_string());
    let command = options
        .binary_dir
        .as_ref()
        .map(|dir| {
            dir.join(if cfg!(windows) {
                "tesseract.exe"
            } else {
                "tesseract"
            })
        })
        .filter(|candidate| candidate.exists())
        .unwrap_or_else(|| PathBuf::from("tesseract"));
    let output = Command::new(&command)
        .arg(path)
        .arg("stdout")
        .arg("-l")
        .arg(&languages)
        .output()
        .map_err(|error| {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Warning,
                code: "ocr_unavailable".to_string(),
                message: format!("Tesseract is not available: {error}"),
            });
            EntwurfError::OcrUnavailable
        });
    let output = match output {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            diagnostics.push(Diagnostic {
                level: DiagnosticLevel::Warning,
                code: "ocr_failed".to_string(),
                message: format!("Tesseract exited with status {}", output.status),
            });
            return Ok(Vec::new());
        }
        Err(_) => return Ok(Vec::new()),
    };
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let mut node = KnowledgeNode {
        id: new_id(),
        kind: NodeKind::Note,
        title: source.original_name.clone(),
        body: text.clone(),
        latex: None,
        anchors: vec![SourceAnchor {
            source_id: source.id.clone(),
            page: Some(1),
            line: None,
            char_start: Some(0),
            char_end: Some(text.chars().count() as u32),
            quote_hash: Some(hash_bytes(text.as_bytes())),
            label: format!("{}:ocr", source.original_name),
        }],
        importance: Importance::Normal,
        origin: NodeOrigin::Source,
        parent_id: None,
        tags: vec!["ocr".to_string()],
        verified: false,
        review_note: Some("OCR text needs confirmation".to_string()),
    };
    if let Some(latex) = extract_latex_like(&text) {
        node.latex = Some(latex);
    }
    Ok(vec![node])
}

fn extract_latex_like(text: &str) -> Option<String> {
    let start = text.find('$')?;
    let rest = &text[start + 1..];
    let end = rest.find('$')?;
    let candidate = rest[..end].trim();
    if candidate.is_empty() {
        None
    } else {
        Some(candidate.to_string())
    }
}

pub fn unverified_from_diagnostics(sources: &[IngestedSource]) -> Vec<UnverifiedItem> {
    let mut items = Vec::new();
    for source in sources {
        for node in &source.nodes {
            if node.tags.iter().any(|tag| tag == "ocr") {
                items.push(UnverifiedItem {
                    id: new_id(),
                    node_id: Some(node.id.clone()),
                    kind: UnverifiedKind::Ocr,
                    text: node.body.chars().take(160).collect(),
                    reason: "OCR-derived text must be confirmed against the image".to_string(),
                });
            }
        }
    }
    items
}

pub fn source_ids(ir: &KnowledgeIr) -> BTreeSet<&str> {
    ir.sources.iter().map(|source| source.id.as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_preserves_headings_and_formulas() {
        let source = SourceFile {
            id: "s".into(),
            original_name: "chapter.md".into(),
            kind: SourceKind::Markdown,
            content_hash: "h".into(),
            stored_path: "chapter.md".into(),
            byte_len: 0,
            imported_at: chrono::Utc::now(),
            notes: vec![],
        };
        let text = "# Probability\n\nA random variable maps outcomes.\n\n$$P(A \\mid B) = \\frac{P(A \\cap B)}{P(B)}$$\n\n## Bayes\n\nUse the prior and likelihood.\n";
        let nodes = markdown_nodes(&source, text, &IngestOptions::standard());
        assert!(nodes.iter().any(|node| node.kind == NodeKind::Chapter));
        assert!(nodes.iter().any(|node| node.kind == NodeKind::Formula));
        assert!(nodes.iter().all(|node| !node.anchors.is_empty()));
    }

    #[test]
    fn duplicate_sources_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("a.md");
        let second = dir.path().join("b.md");
        fs::write(&first, "# A\n\ntext").unwrap();
        fs::write(&second, "# A\n\ntext").unwrap();
        let report = ingest_paths(&[first, second], &IngestOptions::standard()).unwrap();
        assert_eq!(report.sources.len(), 1);
        assert!(report
            .diagnostics
            .iter()
            .any(|item| item.code == "duplicate_source"));
    }
}
