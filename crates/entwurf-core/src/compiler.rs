//! Deterministic artifact compilation. SVG is the authoritative vector page;
//! PNG rasterizes the authoritative SVG; PDF and SVG export one Typst scene.

use std::fs;
use std::path::{Path, PathBuf};

use resvg::tiny_skia;
use resvg::usvg;
use serde::{Deserialize, Serialize};

use crate::error::{EntwurfError, Result};
use crate::ir::{KnowledgeIr, StyleSpec};
use crate::layout::{layout, LayoutOptions};
use crate::model::{mm_to_pt, LayoutDocument};
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
    pub validation_report: crate::ir::ValidationReport,
    pub layout_document: LayoutDocument,
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

#[derive(Debug, Clone)]
pub struct PreparedCompilation {
    pub document: Option<LayoutDocument>,
    pub validation: crate::ir::ValidationReport,
    pub svg_pages: Vec<String>,
    scene: Option<typst_layout::PagedDocument>,
}

/// Produce the exact export scene for desktop previews. Preserve blocking
/// diagnostics instead of losing them in a generic export exception.
pub fn prepare(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    options: &CompileOptions,
) -> Result<PreparedCompilation> {
    let validation = validate_ir(ir, &options.validation)?;
    let mut prepared = PreparedCompilation {
        document: None,
        validation,
        svg_pages: Vec::new(),
        scene: None,
    };
    if !prepared.validation.is_passing() {
        return Ok(prepared);
    }
    let layout_options = LayoutOptions {
        max_pages: options
            .layout
            .max_pages
            .min(options.validation.max_pages)
            .min(style.paper.max_pages),
        ..options.layout.clone()
    };
    let document = match layout(ir, style, &layout_options) {
        Ok(document) => document,
        Err(error) => {
            prepared.validation.push(
                "compile_layout",
                crate::ir::Severity::Blocking,
                error.to_string(),
            );
            return Ok(prepared);
        }
    };
    validate_layout(ir, &document, &mut prepared.validation, &options.validation)?;
    if !prepared.validation.is_passing() {
        prepared.document = Some(document);
        return Ok(prepared);
    }
    let scene = match crate::scene::scene(&document, ir, style, &layout_options) {
        Ok(scene) => scene,
        Err(error) => {
            prepared.validation.push(
                "compile_render",
                crate::ir::Severity::Blocking,
                error.to_string(),
            );
            prepared.document = Some(document);
            return Ok(prepared);
        }
    };
    prepared.svg_pages = scene
        .pages()
        .iter()
        .map(|page| annotate_svg(typst_svg::svg(page, &Default::default()), &ir.title))
        .collect();
    prepared.document = Some(document);
    prepared.scene = Some(scene);
    Ok(prepared)
}

pub fn compile(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    options: &CompileOptions,
) -> Result<ArtifactSet> {
    if !options.raster_dpi.is_finite() || options.raster_dpi < 1.0 || options.raster_dpi > 600.0 {
        return Err(EntwurfError::Render(
            "raster DPI must be between 1 and 600".into(),
        ));
    }
    let prepared = prepare(ir, style, options)?;
    if !prepared.validation.is_passing() {
        return Err(EntwurfError::ValidationBlocked {
            blocking: prepared.validation.blocking_count(),
        });
    }
    let validation = prepared.validation;
    let document = prepared.document.expect("passing preparation has geometry");
    let scene = prepared.scene.expect("passing preparation has scene");
    let pdf = typst_pdf::pdf(
        &scene,
        &typst_pdf::PdfOptions {
            tagged: false,
            ..Default::default()
        },
    )
    .map_err(|error| EntwurfError::Render(format!("PDF export: {error:?}")))?;
    let mut data: Vec<(String, String, Vec<u8>)> = Vec::new();
    for (index, page) in scene.pages().iter().enumerate() {
        let svg = prepared.svg_pages[index].clone();
        let png = rasterize_svg(
            svg.as_bytes(),
            page.frame.width().to_pt(),
            page.frame.height().to_pt(),
            options.raster_dpi,
        )?;
        data.push((
            format!("svg:page-{}", index + 1),
            format!("page-{:02}.svg", index + 1),
            svg.into_bytes(),
        ));
        data.push((
            format!("png:page-{}", index + 1),
            format!("preview-{:02}.png", index + 1),
            png,
        ));
    }
    data.push(("pdf".into(), "entwurf.pdf".into(), pdf));
    let ir_json =
        serde_json::to_vec_pretty(ir).map_err(|error| EntwurfError::Export(error.to_string()))?;
    let ir_hash = crate::ingest::hash_bytes(&ir_json);
    data.push(("ir".into(), "knowledge.ir.json".into(), ir_json));
    data.push((
        "layout".into(),
        "layout.json".into(),
        serde_json::to_vec_pretty(&document).map_err(|e| EntwurfError::Export(e.to_string()))?,
    ));
    let files = data
        .iter()
        .map(|(role, path, bytes)| ManifestFile {
            role: role.clone(),
            path: path.clone(),
            bytes: bytes.len() as u64,
            sha256: crate::ingest::hash_bytes(bytes),
        })
        .collect();
    if options.write_manifest {
        let manifest = ArtifactManifest {
            schema_version: 1,
            project_id: ir.project_id.clone(),
            task_id: ir.task_id.clone(),
            ir_id: ir.id.clone(),
            ir_hash,
            generated_at: ir.updated_at,
            page_count: scene.pages().len() as u16,
            width_mm: style.paper.width_mm,
            height_mm: style.paper.height_mm,
            files,
            block_ids: document.all_nodes().map(|n| n.node_id.clone()).collect(),
        };
        data.push((
            "manifest".into(),
            "manifest.json".into(),
            serde_json::to_vec_pretty(&manifest)
                .map_err(|e| EntwurfError::Export(e.to_string()))?,
        ));
    }
    // Finish every validation, shape, and raster operation before publishing any
    // artifact. A failed formula or glyph never leaves a half-exported bundle.
    fs::create_dir_all(&options.output_dir)
        .map_err(|e| EntwurfError::write(&options.output_dir, e))?;
    for (_, name, bytes) in &data {
        let path = options.output_dir.join(name);
        let mut pending = tempfile::NamedTempFile::new_in(&options.output_dir)
            .map_err(|e| EntwurfError::write(&path, e))?;
        use std::io::Write;
        pending
            .write_all(bytes)
            .map_err(|e| EntwurfError::write(&path, e))?;
        pending
            .persist(&path)
            .map_err(|e| EntwurfError::write(&path, e.error))?;
    }
    let managed = regex::Regex::new(r"^(page-[0-9]+\.svg|preview-[0-9]+\.png|manifest\.json)$")
        .expect("static pattern");
    for entry in
        fs::read_dir(&options.output_dir).map_err(|e| EntwurfError::read(&options.output_dir, e))?
    {
        let entry = entry.map_err(|e| EntwurfError::read(&options.output_dir, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if managed.is_match(&name) && !data.iter().any(|(_, current, _)| *current == name) {
            fs::remove_file(entry.path()).map_err(|e| EntwurfError::write(entry.path(), e))?;
        }
    }
    Ok(ArtifactSet {
        svg_paths: (1..=scene.pages().len())
            .map(|i| options.output_dir.join(format!("page-{i:02}.svg")))
            .collect(),
        png_paths: (1..=scene.pages().len())
            .map(|i| options.output_dir.join(format!("preview-{i:02}.png")))
            .collect(),
        pdf_path: options.output_dir.join("entwurf.pdf"),
        manifest_path: options
            .write_manifest
            .then(|| options.output_dir.join("manifest.json")),
        ir_path: options.output_dir.join("knowledge.ir.json"),
        page_count: scene.pages().len() as u16,
        validation_passed: true,
        validation_report: validation,
        layout_document: document,
    })
}

/// Outlined glyphs need an accessible document name for SVG readers.
fn annotate_svg(mut svg: String, title: &str) -> String {
    let title = title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    svg = svg.replacen("<svg ", "<svg role=\"img\" ", 1);
    if let Some(index) = svg.find('>') {
        svg.insert_str(index + 1, &format!("<title>{title}</title>"));
    }
    svg
}

fn rasterize_svg(svg: &[u8], width_pt: f64, height_pt: f64, dpi: f32) -> Result<Vec<u8>> {
    let tree = usvg::Tree::from_data(svg, &usvg::Options::default())
        .map_err(|e| EntwurfError::Render(e.to_string()))?;
    // SVG's intrinsic dimensions may be CSS pixels. Derive physical output size
    // from the authoritative Typst page in points, not a guessed SVG unit.
    let width = (width_pt * f64::from(dpi) / 72.0).round() as u32;
    let height = (height_pt * f64::from(dpi) / 72.0).round() as u32;
    if u64::from(width) * u64::from(height) > 100_000_000 {
        return Err(EntwurfError::Render(
            "raster canvas exceeds 100 megapixels".into(),
        ));
    }
    let mut pixmap = tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| EntwurfError::Render("cannot allocate raster canvas".into()))?;
    let transform = tiny_skia::Transform::from_scale(
        width as f32 / tree.size().width(),
        height as f32 / tree.size().height(),
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let rgba: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let pixels_per_meter = (dpi / 0.0254).round() as u32;
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: pixels_per_meter,
            yppu: pixels_per_meter,
            unit: png::Unit::Meter,
        }));
        let mut writer = encoder
            .write_header()
            .map_err(|e| EntwurfError::Export(e.to_string()))?;
        writer
            .write_image_data(&rgba)
            .map_err(|e| EntwurfError::Export(e.to_string()))?;
    }
    Ok(bytes)
}

pub fn render_svg(
    document: &LayoutDocument,
    ir: &KnowledgeIr,
    style: &StyleSpec,
    options: &LayoutOptions,
) -> Result<String> {
    let scene = crate::scene::scene(document, ir, style, options)?;
    Ok(typst_svg::svg_merged(
        &scene,
        &Default::default(),
        typst::layout::Abs::pt(12.0),
    ))
}
pub fn export_svg_bundle(
    document: &LayoutDocument,
    ir: &KnowledgeIr,
    style: &StyleSpec,
    options: &LayoutOptions,
    out: &Path,
) -> Result<()> {
    fs::write(out, render_svg(document, ir, style, options)?)
        .map_err(|e| EntwurfError::write(out, e))
}
pub fn a3_dimensions(style: &StyleSpec) -> (f32, f32) {
    (
        mm_to_pt(style.paper.width_mm),
        mm_to_pt(style.paper.height_mm),
    )
}
