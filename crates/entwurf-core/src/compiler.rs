//! Deterministic artifact compilation. SVG is the authoritative vector page;
//! PNG is rasterized from the same scene, and the PDF-writer-compatible output
//! is emitted directly from the placed geometry.

use std::fs;
use std::path::{Path, PathBuf};

use resvg::tiny_skia;
use resvg::usvg;
use serde::{Deserialize, Serialize};

use crate::error::{EntwurfError, Result};
use crate::ir::{KnowledgeIr, StyleSpec};
use crate::layout::{layout, LayoutOptions};
use crate::model::{mm_to_pt, LayoutDocument, PlacedNode, TextLine};
use crate::validate::{validate_ir, validate_layout, ValidationOptions};

#[derive(Debug, Clone)]
pub struct CompileOptions {
    pub output_dir: PathBuf,
    pub raster_dpi: f32,
    pub layout: LayoutOptions,
    pub validation: ValidationOptions,
    pub write_manifest: bool,
}

impl CompileOptions {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            raster_dpi: 300.0,
            layout: LayoutOptions::default(),
            validation: ValidationOptions::default(),
            write_manifest: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSet {
    pub svg_paths: Vec<PathBuf>,
    pub png_paths: Vec<PathBuf>,
    pub pdf_path: PathBuf,
    pub manifest_path: Option<PathBuf>,
    pub ir_path: PathBuf,
    pub page_count: u16,
    pub validation_passed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactManifest {
    pub schema_version: u32,
    pub project_id: String,
    pub task_id: String,
    pub ir_id: String,
    pub ir_hash: String,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub page_count: u16,
    pub width_mm: f32,
    pub height_mm: f32,
    pub files: Vec<ManifestFile>,
    pub block_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestFile {
    pub role: String,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

pub fn compile(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    options: &CompileOptions,
) -> Result<ArtifactSet> {
    let mut validation = validate_ir(ir, &options.validation)?;
    let layout_options = LayoutOptions {
        max_pages: options.validation.max_pages,
        ..options.layout.clone()
    };
    let document = layout(ir, style, &layout_options)?;
    validate_layout(ir, &document, &mut validation, &options.validation)?;
    if !validation.is_passing() {
        return Err(EntwurfError::ValidationBlocked {
            blocking: validation.blocking_count(),
        });
    }
    fs::create_dir_all(&options.output_dir)
        .map_err(|error| EntwurfError::write(&options.output_dir, error))?;
    let svg_paths = write_svgs(&document, ir, style, &options.output_dir)?;
    let png_paths = write_pngs(&svg_paths, &options.output_dir, options.raster_dpi)?;
    let pdf_path = options.output_dir.join("entwurf.pdf");
    write_pdf(&document, ir, style, &pdf_path)?;
    let ir_path = options.output_dir.join("knowledge.ir.json");
    let ir_json =
        serde_json::to_vec_pretty(ir).map_err(|error| EntwurfError::Export(error.to_string()))?;
    fs::write(&ir_path, &ir_json).map_err(|error| EntwurfError::write(&ir_path, error))?;
    let manifest_path = if options.write_manifest {
        let manifest_path = options.output_dir.join("manifest.json");
        let mut files = Vec::new();
        for (index, path) in svg_paths.iter().enumerate() {
            files.push(manifest_file("svg", &[path.clone()], index + 1)?);
        }
        for (index, path) in png_paths.iter().enumerate() {
            files.push(manifest_file("png", &[path.clone()], index + 1)?);
        }
        files.push(manifest_file("pdf", &[pdf_path.clone()], 1)?);
        files.push(manifest_file("ir", &[ir_path.clone()], 1)?);
        let manifest = ArtifactManifest {
            schema_version: 1,
            project_id: ir.project_id.clone(),
            task_id: ir.task_id.clone(),
            ir_id: ir.id.clone(),
            ir_hash: crate::ingest::hash_bytes(&ir_json),
            generated_at: chrono::Utc::now(),
            page_count: document.pages.len() as u16,
            width_mm: style.paper.width_mm,
            height_mm: style.paper.height_mm,
            files,
            block_ids: document
                .all_nodes()
                .map(|node| node.node_id.clone())
                .collect(),
        };
        let bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| EntwurfError::Export(error.to_string()))?;
        fs::write(&manifest_path, bytes)
            .map_err(|error| EntwurfError::write(&manifest_path, error))?;
        Some(manifest_path)
    } else {
        None
    };
    Ok(ArtifactSet {
        svg_paths,
        png_paths,
        pdf_path,
        manifest_path,
        ir_path,
        page_count: document.pages.len() as u16,
        validation_passed: validation.is_passing(),
    })
}

fn manifest_file(role: &str, paths: &[PathBuf], page: usize) -> Result<ManifestFile> {
    let path = &paths[0];
    let bytes = fs::read(path).map_err(|error| EntwurfError::read(path, error))?;
    Ok(ManifestFile {
        role: format!("{role}:page-{page}"),
        path: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        bytes: bytes.len() as u64,
        sha256: crate::ingest::hash_bytes(&bytes),
    })
}

pub fn render_svg(document: &LayoutDocument, ir: &KnowledgeIr, style: &StyleSpec) -> String {
    let mut svg = String::new();
    for (index, page) in document.pages.iter().enumerate() {
        if index > 0 {
            svg.push('\n');
        }
        svg.push_str(&render_page_svg(page, ir, style));
    }
    svg
}

fn render_page_svg(page: &crate::model::LayoutPage, ir: &KnowledgeIr, style: &StyleSpec) -> String {
    let colors = &style.colors;
    let mut out = String::new();
    out.push_str(&format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.2}mm\" height=\"{:.2}mm\" viewBox=\"0 0 {:.2} {:.2}\" role=\"img\" aria-label=\"{} page {}\">\n",
        style.paper.width_mm,
        style.paper.height_mm,
        page.width_pt,
        page.height_pt,
        escape_xml(&ir.title),
        page.index
    ));
    out.push_str(
        "<defs><pattern id=\"paper\" width=\"24\" height=\"24\" patternUnits=\"userSpaceOnUse\">\
<path d=\"M2 5h8M14 2v6M5 19h11M18 13v5\" stroke=\"#8b8170\" stroke-width=\"0.35\" opacity=\"0.16\"/>\
<circle cx=\"20\" cy=\"8\" r=\"0.45\" fill=\"#8b8170\" opacity=\"0.14\"/>\
</pattern></defs>\n",
    );
    out.push_str(&format!(
        "<rect width=\"100%\" height=\"100%\" fill=\"{}\"/>\n<rect width=\"100%\" height=\"100%\" fill=\"url(#paper)\"/>\n",
        colors.background
    ));
    out.push_str(&format!(
        "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"28\" font-weight=\"700\" fill=\"{}\">{}</text>\n",
        page.margin_pt,
        page.margin_pt + 30.0,
        escape_xml(&style.typography.family),
        colors.text,
        escape_xml(&ir.title)
    ));
    out.push_str(&format!(
        "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"9\" fill=\"{}\">A3 · {} / {}</text>\n",
        page.margin_pt,
        page.margin_pt + 46.0,
        escape_xml(&style.typography.family),
        colors.muted,
        escape_xml(&ir.subject.clone().unwrap_or_else(|| "Knowledge sheet".to_string())),
        page.index
    ));
    out.push_str(&format!(
        "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"{}\" stroke-width=\"0.8\"/>\n",
        page.margin_pt,
        page.margin_pt + 54.0,
        page.width_pt - page.margin_pt,
        page.margin_pt + 54.0,
        colors.border
    ));
    for group in &page.groups {
        out.push_str(&format!(
            "<g id=\"group-{}\"><rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"12\" fill=\"{}\" stroke=\"{}\" stroke-width=\"0.7\"/>\n",
            escape_xml(&group.id),
            group.rect.x,
            group.rect.y,
            group.rect.width,
            group.rect.height,
            colors.surface,
            colors.border
        ));
        out.push_str(&format!(
            "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"11\" font-weight=\"700\" fill=\"{}\">{}</text></g>\n",
            group.rect.x + 12.0,
            group.rect.y + 17.0,
            escape_xml(&style.typography.family),
            colors.text,
            escape_xml(&group.title)
        ));
    }
    for node in &page.nodes {
        out.push_str(&render_node_svg(node, style));
    }
    for edge in &page.edges {
        let points = edge
            .points
            .iter()
            .map(|point| format!("{:.2},{:.2}", point.x, point.y))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!(
            "<polyline points=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"1\" stroke-dasharray=\"4 3\" opacity=\"0.65\"/>\n",
            points, colors.accent
        ));
        if let Some(label) = &edge.label {
            if let Some(point) = edge.points.get(1) {
                out.push_str(&format!(
                    "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"7.5\" fill=\"{}\">{}</text>\n",
                    point.x + 2.0,
                    point.y - 3.0,
                    escape_xml(&style.typography.family),
                    colors.muted,
                    escape_xml(label)
                ));
            }
        }
    }
    out.push_str(&format!(
        "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"7.5\" fill=\"{}\">Entwurf · local deterministic compile · page {}/{}</text>\n",
        page.margin_pt,
        page.height_pt - page.margin_pt,
        escape_xml(&style.typography.family),
        colors.muted,
        page.index,
        document_page_hint(page)
    ));
    out.push_str("</svg>\n");
    out
}

fn document_page_hint(page: &crate::model::LayoutPage) -> u16 {
    page.index
}

fn render_node_svg(node: &PlacedNode, style: &StyleSpec) -> String {
    let colors = &style.colors;
    let mut out = String::new();
    let fill = match node.kind {
        crate::ir::NodeKind::Formula | crate::ir::NodeKind::Definition => colors.surface.clone(),
        crate::ir::NodeKind::Warning => "#fff3df".to_string(),
        _ => "#fffefb".to_string(),
    };
    out.push_str(&format!(
        "<g id=\"node-{}\"><rect x=\"{:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" rx=\"8\" fill=\"{}\" stroke=\"{}\" stroke-width=\"0.6\"/>\n",
        escape_xml(&node.node_id),
        node.rect.x,
        node.rect.y,
        node.rect.width,
        node.rect.height,
        fill,
        colors.border
    ));
    out.push_str(&format!(
        "<rect x=\"{:.2}\" y=\"{:.2}\" width=\"3\" height=\"{:.2}\" rx=\"1.5\" fill=\"{}\"/>\n",
        node.rect.x,
        node.rect.y + 7.0,
        (node.rect.height - 14.0).max(4.0),
        colors.accent
    ));
    for line in &node.title_lines {
        out.push_str(&render_text_line(
            line,
            &colors.text,
            &style.typography.family,
        ));
    }
    for line in &node.body_lines {
        out.push_str(&render_text_line(
            line,
            &colors.text,
            &style.typography.family,
        ));
    }
    for line in &node.latex_lines {
        out.push_str(&format!(
            "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"DejaVu Serif\" font-size=\"{:.2}\" font-style=\"italic\" fill=\"{}\">{}</text>\n",
            line.x + 6.0,
            line.baseline,
            line.font_size,
            colors.text,
            escape_xml(&line.text)
        ));
    }
    out.push_str("</g>\n");
    out
}

fn render_text_line(line: &TextLine, color: &str, family: &str) -> String {
    format!(
        "<text x=\"{:.2}\" y=\"{:.2}\" font-family=\"{}\" font-size=\"{:.2}\" font-weight=\"{}\" fill=\"{}\">{}</text>\n",
        line.x,
        line.baseline,
        escape_xml(family),
        line.font_size,
        if line.bold { "700" } else { "400" },
        color,
        escape_xml(&line.text)
    )
}

fn write_svgs(
    document: &LayoutDocument,
    ir: &KnowledgeIr,
    style: &StyleSpec,
    output: &Path,
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for page in &document.pages {
        let path = output.join(format!("page-{:02}.svg", page.index));
        fs::write(&path, render_page_svg(page, ir, style))
            .map_err(|error| EntwurfError::write(&path, error))?;
        paths.push(path);
    }
    Ok(paths)
}

fn write_pngs(svg_paths: &[PathBuf], output: &Path, dpi: f32) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for (index, svg_path) in svg_paths.iter().enumerate() {
        let data = fs::read(svg_path).map_err(|error| EntwurfError::read(svg_path, error))?;
        let options = usvg::Options::default();
        let tree = usvg::Tree::from_data(&data, &options)
            .map_err(|error| EntwurfError::Render(error.to_string()))?;
        let size = tree.size();
        let scale = (dpi / 72.0).max(0.1);
        let width = (size.width() * scale).ceil() as u32;
        let height = (size.height() * scale).ceil() as u32;
        let mut pixmap = tiny_skia::Pixmap::new(width.max(1), height.max(1))
            .ok_or_else(|| EntwurfError::Render("cannot allocate PNG canvas".to_string()))?;
        let transform = tiny_skia::Transform::from_scale(scale, scale);
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        let path = output.join(format!("preview-{:02}.png", index + 1));
        pixmap
            .save_png(&path)
            .map_err(|error| EntwurfError::Export(format!("{}: {error}", path.display())))?;
        paths.push(path);
    }
    Ok(paths)
}

fn write_pdf(
    document: &LayoutDocument,
    ir: &KnowledgeIr,
    _style: &StyleSpec,
    path: &Path,
) -> Result<()> {
    let mut objects: Vec<Vec<u8>> = Vec::new();
    let mut page_object_ids = Vec::new();
    let font_object_id = 3 + document.pages.len() * 2;
    for page in &document.pages {
        let mut content = String::new();
        content.push_str(&format!(
            "q\n1 1 1 rg\n0 0 {:.2} {:.2} re f\nQ\n",
            page.width_pt, page.height_pt
        ));
        content.push_str(&format!(
            "BT /F1 24 Tf 0.12 0.12 0.13 rg {:.2} {:.2} Td ({}) Tj ET\n",
            page.margin_pt,
            page.height_pt - page.margin_pt - 24.0,
            escape_pdf_text(&ir.title)
        ));
        for node in &page.nodes {
            content.push_str(&format!(
                "q 1 0.99 0.96 rg {:.2} {:.2} {:.2} {:.2} re f Q\n",
                node.rect.x,
                page.height_pt - node.rect.bottom(),
                node.rect.width,
                node.rect.height
            ));
            for line in node.title_lines.iter().chain(node.body_lines.iter()) {
                content.push_str(&format!(
                    "BT /F1 {:.2} Tf 0.12 0.12 0.13 rg {:.2} {:.2} Td ({}) Tj ET\n",
                    line.font_size,
                    line.x,
                    page.height_pt - line.baseline,
                    escape_pdf_text(&line.text)
                ));
            }
        }
        content.push_str(&format!(
            "q 0.42 0.24 0.72 RG 0.8 w {:.2} {:.2} m {:.2} {:.2} l S Q\n",
            page.margin_pt,
            page.height_pt - page.margin_pt - 54.0,
            page.width_pt - page.margin_pt,
            page.height_pt - page.margin_pt - 54.0
        ));
        let content_id = objects.len() + 1;
        objects.push(format_pdf_stream(content.as_bytes()));
        let page_id = objects.len() + 1;
        page_object_ids.push(page_id);
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {:.2} {:.2}] /Contents {} 0 R /Resources << /Font << /F1 {} 0 R >> >> >>",
            page.width_pt, page.height_pt, content_id, font_object_id
        ).into_bytes());
    }
    let pages_id = 2u32;
    let catalog_id = 1u32;
    let page_ids = page_object_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    let mut ordered: Vec<Vec<u8>> = Vec::new();
    ordered.push(format!("<< /Type /Catalog /Pages {pages_id} 0 R >>").into_bytes());
    ordered.push(
        format!(
            "<< /Type /Pages /Kids [{}] /Count {} >>",
            page_ids,
            page_object_ids.len()
        )
        .into_bytes(),
    );
    ordered.extend(objects);
    let _font_id = ordered.len() + 1;
    ordered.push(format!("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>").into_bytes());
    let mut pdf = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = vec![0usize];
    for (index, object) in ordered.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_offset = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", ordered.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root {} 0 R >>\nstartxref\n{}\n%%EOF\n",
            ordered.len() + 1,
            catalog_id,
            xref_offset
        )
        .as_bytes(),
    );
    fs::write(path, pdf).map_err(|error| EntwurfError::write(path, error))
}

fn format_pdf_stream(bytes: &[u8]) -> Vec<u8> {
    let mut out = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
    out.extend_from_slice(bytes);
    out.extend_from_slice(b"\nendstream");
    out
}

fn escape_pdf_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .chars()
        .map(|character| if character.is_ascii() { character } else { '?' })
        .collect()
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn export_svg_bundle(
    document: &LayoutDocument,
    ir: &KnowledgeIr,
    style: &StyleSpec,
    out: &Path,
) -> Result<()> {
    fs::write(out, render_svg(document, ir, style)).map_err(|error| EntwurfError::write(out, error))
}

pub fn a3_dimensions(style: &StyleSpec) -> (f32, f32) {
    (
        mm_to_pt(style.paper.width_mm),
        mm_to_pt(style.paper.height_mm),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Importance, KnowledgeNode, NodeKind, NodeOrigin, SourceAnchor, SourceFile, SourceKind,
    };

    fn sample_ir() -> KnowledgeIr {
        let mut ir = KnowledgeIr::empty("p", "t", "概率论");
        ir.sources.push(SourceFile {
            id: "s".into(),
            original_name: "s.md".into(),
            kind: SourceKind::Markdown,
            content_hash: "h".into(),
            stored_path: "s.md".into(),
            byte_len: 1,
            imported_at: chrono::Utc::now(),
            notes: vec![],
        });
        ir.nodes.push(KnowledgeNode {
            id: "n".into(),
            kind: NodeKind::Concept,
            title: "Conditional probability".into(),
            body: "Probability of an event given another event.".into(),
            latex: Some("P(A \\mid B) = \\frac{P(A \\cap B)}{P(B)}".into()),
            anchors: vec![SourceAnchor {
                region: None,
                source_id: "s".into(),
                page: Some(1),
                line: Some(1),
                char_start: Some(0),
                char_end: Some(10),
                quote_hash: None,
                label: "s:1".into(),
            }],
            importance: Importance::Critical,
            origin: NodeOrigin::Source,
            parent_id: None,
            tags: vec![],
            verified: false,
            review_note: None,
        });
        ir
    }

    #[test]
    fn exports_svg_png_pdf_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let ir = sample_ir();
        let style = StyleSpec::default();
        let mut options = CompileOptions::new(dir.path());
        options.raster_dpi = 72.0;
        let artifacts = compile(&ir, &style, &options).unwrap();
        assert_eq!(artifacts.page_count, 1);
        assert!(artifacts.svg_paths[0].exists());
        assert!(artifacts.png_paths[0].exists());
        assert!(artifacts.pdf_path.exists());
        assert!(artifacts.manifest_path.unwrap().exists());
        let pdf = fs::read(&artifacts.pdf_path).unwrap();
        assert!(pdf.starts_with(b"%PDF-1.7"));
        assert!(pdf.ends_with(b"%%EOF\n"));
    }

    #[test]
    fn paper_texture_uses_bounded_pattern_instead_of_page_filter() {
        let ir = sample_ir();
        let style = StyleSpec::default();
        let document = layout(&ir, &style, &LayoutOptions::from_style(&style)).unwrap();
        let svg = render_svg(&document, &ir, &style);
        assert!(svg.contains("<pattern id=\"paper\" width=\"24\" height=\"24\""));
        assert!(svg.contains("fill=\"url(#paper)\""));
        assert!(!svg.contains("<filter"));
        assert!(!svg.contains("feTurbulence"));
    }

    #[test]
    fn refuses_to_export_when_validation_blocks() {
        let dir = tempfile::tempdir().unwrap();
        let mut ir = sample_ir();
        ir.nodes[0].anchors.clear();
        let style = StyleSpec::default();
        let error = compile(&ir, &style, &CompileOptions::new(dir.path())).unwrap_err();
        assert!(matches!(error, EntwurfError::ValidationBlocked { .. }));
    }
}
