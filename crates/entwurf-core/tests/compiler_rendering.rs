use entwurf_core::{
    compiler::{compile, prepare, ArtifactManifest, CompileOptions},
    ingest::{ingest_paths, IngestOptions},
    ir::*,
    layout::{layout, LayoutOptions},
};
use std::{fs, path::PathBuf};
fn fixture() -> KnowledgeIr {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/public/probability-chapter.md");
    let report = ingest_paths(&[path], &IngestOptions::default()).unwrap();
    let mut ir = report.into_ir(
        "probability-project",
        "chapter-task",
        "概率论：随机变量与条件概率",
    );
    // Explicit test-author review of the checked-in synthetic formulas. This
    // models approval; production ingestion leaves its review queue intact.
    let formulas: Vec<_> = ir.nodes.iter().filter_map(|n| n.latex.as_deref()).collect();
    assert_eq!(
        formulas,
        vec![
            r"P(X \le x) = F_X(x)",
            r"P(A \mid B) = \frac{P(A \cap B)}{P(B)}",
            r"P(A) = \sum_{i=1}^{n} P(A \mid B_i) P(B_i)",
            r"P(B_i \mid A) = \frac{P(A \mid B_i) P(B_i)}{\sum_{j=1}^{n} P(A \mid B_j) P(B_j)}",
        ]
    );
    for item in &ir.unverified {
        assert!(matches!(item.kind, UnverifiedKind::Formula));
    }
    ir.unverified.clear();
    for node in &mut ir.nodes {
        if node.latex.is_some() {
            node.verified = true;
        }
    }
    ir
}
#[test]
fn public_probability_exports_readable_vector_scene_at_300dpi() {
    let dir = tempfile::tempdir().unwrap();
    let ir = fixture();
    let before = serde_json::to_vec(&ir).unwrap();
    let style = StyleSpec::default();
    let prepared = prepare(&ir, &style, &CompileOptions::new(dir.path())).unwrap();
    assert!(prepared.validation.is_passing());
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    let artifact = compile(&ir, &style, &CompileOptions::new(dir.path())).unwrap();
    assert!(artifact.page_count <= 3);
    let reader = png::Decoder::new(std::io::BufReader::new(
        fs::File::open(&artifact.png_paths[0]).unwrap(),
    ))
    .read_info()
    .unwrap();
    assert_eq!(reader.info().pixel_dims.unwrap().xppu, 11811);
    let png = image::open(&artifact.png_paths[0]).unwrap();
    assert_eq!((png.width(), png.height()), (4961, 3508));
    let svg = fs::read_to_string(&artifact.svg_paths[0]).unwrap();
    assert_eq!(svg, prepared.svg_pages[0]);
    assert!(svg.contains("<use xlink:href=\"#g"));
    assert!(!svg.contains("<image"));
    assert!(!svg.contains("\\frac"));
    let exported: KnowledgeIr =
        serde_json::from_slice(&fs::read(&artifact.ir_path).unwrap()).unwrap();
    assert_eq!(before, serde_json::to_vec(&exported).unwrap());
    let manifest: ArtifactManifest =
        serde_json::from_slice(&fs::read(artifact.manifest_path.unwrap()).unwrap()).unwrap();
    assert_eq!(manifest.block_ids.len(), ir.nodes.len());
    for file in manifest.files {
        let bytes = fs::read(dir.path().join(file.path)).unwrap();
        assert_eq!(file.bytes, bytes.len() as u64);
        assert_eq!(file.sha256, entwurf_core::ingest::hash_bytes(&bytes));
    }
    let layout = layout(&ir, &style, &LayoutOptions::default()).unwrap();
    assert_eq!(layout.node_count(), ir.nodes.len());
    for n in layout.all_nodes() {
        assert!(n.measured_font_pt.unwrap() >= 8.0);
        let b = n.measured_content_rect.unwrap();
        assert!(
            b.x >= n.rect.x - 0.5
                && b.right() <= n.rect.right() + 0.5
                && b.y >= n.rect.y - 0.5
                && b.bottom() <= n.rect.bottom() + 0.5
        );
    }
}
#[test]
fn deterministic_exports_and_content_style_changes_invalidate_cache() {
    let dir = tempfile::tempdir().unwrap();
    let mut ir = fixture();
    let mut opts = CompileOptions::new(dir.path());
    opts.raster_dpi = 72.0;
    let mut style = StyleSpec::default();
    compile(&ir, &style, &opts).unwrap();
    let snapshot = |name: &str| fs::read(dir.path().join(name)).unwrap();
    let pdf = snapshot("entwurf.pdf");
    let svg = snapshot("page-01.svg");
    let manifest = snapshot("manifest.json");
    compile(&ir, &style, &opts).unwrap();
    assert_eq!(pdf, snapshot("entwurf.pdf"));
    assert_eq!(svg, snapshot("page-01.svg"));
    assert_eq!(manifest, snapshot("manifest.json"));
    ir.nodes[0].body.push_str("新增可追溯内容");
    compile(&ir, &style, &opts).unwrap();
    assert_ne!(pdf, snapshot("entwurf.pdf"));
    let changed = snapshot("page-01.svg");
    style.colors.accent = "#007755".into();
    style.colors.text = "#101010".into();
    compile(&ir, &style, &opts).unwrap();
    assert_ne!(changed, snapshot("page-01.svg"));
}
#[test]
fn blockers_and_unrenderable_content_publish_no_artifacts() {
    let style = StyleSpec::default();
    for mode in 0..4 {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("export");
        let mut ir = fixture();
        match mode {
            0 => ir.nodes[0].anchors.clear(),
            1 => ir.nodes[0].latex = Some(r"\unsupported{x}".into()),
            2 => ir.nodes[0].body = "永不截断的内容".repeat(5000),
            _ => ir.nodes[0].body = "\u{10ffff}".into(),
        }
        let prepared = prepare(&ir, &style, &CompileOptions::new(&output)).unwrap();
        assert!(!prepared.validation.is_passing());
        assert!(prepared.svg_pages.is_empty());
        assert!(
            compile(&ir, &style, &CompileOptions::new(&output)).is_err(),
            "mode {mode}"
        );
        assert!(!output.exists());
    }
}
#[test]
fn content_preserving_pagination_respects_budget() {
    let mut ir = fixture();
    let seed = ir
        .nodes
        .iter()
        .find(|n| n.body.chars().count() > 50)
        .unwrap()
        .clone();
    ir.nodes.clear();
    ir.edges.clear();
    for i in 0..30 {
        let mut n = seed.clone();
        n.id = format!("node-{i}");
        n.parent_id = None;
        ir.nodes.push(n);
    }
    let style = StyleSpec::default();
    let doc = layout(&ir, &style, &LayoutOptions::default()).unwrap();
    assert!(doc.pages.len() > 1 && doc.pages.len() <= 3);
    assert_eq!(doc.node_count(), 30);
    for page in &doc.pages {
        for (i, n) in page.nodes.iter().enumerate() {
            for other in page.nodes.iter().skip(i + 1) {
                assert!(!n.rect.intersects(&other.rect));
            }
        }
    }
    assert!(layout(
        &ir,
        &style,
        &LayoutOptions {
            max_pages: 1,
            ..Default::default()
        }
    )
    .is_err());
    let dir = tempfile::tempdir().unwrap();
    let mut options = CompileOptions::new(dir.path());
    options.raster_dpi = 72.0;
    let many = compile(&ir, &style, &options).unwrap();
    assert!(many.page_count > 1);
    assert!(dir.path().join("page-02.svg").exists());
    fs::write(dir.path().join("user-note.txt"), "keep").unwrap();
    let one = compile(&fixture(), &style, &options).unwrap();
    assert_eq!(one.page_count, 1);
    assert!(!dir.path().join("page-02.svg").exists());
    assert!(!dir.path().join("preview-02.png").exists());
    assert!(dir.path().join("user-note.txt").exists());
}
#[test]
#[ignore = "requires ENTWURF_PDFIUM_DIR pointing to pinned PDFium 7881"]
fn pdfium_verifies_chinese_math_vector_pdf_and_page_counts() {
    use pdfium_render::prelude::*;
    let dir = tempfile::tempdir().unwrap();
    let ir = fixture();
    let opts = CompileOptions::new(dir.path());
    let artifacts = compile(&ir, &StyleSpec::default(), &opts).unwrap();
    let bindings = Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(
        &std::env::var("ENTWURF_PDFIUM_DIR").unwrap(),
    ))
    .unwrap();
    let pdfium = Pdfium::new(bindings);
    let pdf = pdfium
        .load_pdf_from_file(&artifacts.pdf_path, None)
        .unwrap();
    assert_eq!(pdf.pages().len(), artifacts.page_count as i32);
    let mut text = String::new();
    for p in pdf.pages().iter() {
        assert!((p.width().value - 1190.551).abs() < 0.1);
        assert!((p.height().value - 841.89).abs() < 0.1);
        text.push_str(&p.text().unwrap().all());
        assert!(p
            .objects()
            .iter()
            .all(|o| !matches!(o, PdfPageObject::Image(_))));
    }
    for word in ["概率论", "随机变量", "条件概率", "贝叶斯", "∩"] {
        assert!(text.contains(word), "missing {word}: {text}");
    }
    if let Some(out) = std::env::var_os("ENTWURF_COMPILER_EVIDENCE_DIR") {
        fs::create_dir_all(&out).unwrap();
        for entry in fs::read_dir(dir.path()).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), PathBuf::from(&out).join(entry.file_name())).unwrap();
        }
    }
}
