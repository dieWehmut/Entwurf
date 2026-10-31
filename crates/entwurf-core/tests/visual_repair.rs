use entwurf_core::{
    compiler::{prepare, CompileOptions},
    ingest::{ingest_paths, IngestOptions},
    layout::{layout, LayoutOptions},
    model::Point,
    validate::{validate_layout, ValidationOptions},
    *,
};
fn fixture() -> (tempfile::TempDir, KnowledgeIr) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chapter.md");
    std::fs::write(
        &path,
        "# Chapter\n\nEvidence preserves every word.\n\n## Section\n\nOther exact evidence.\n",
    )
    .unwrap();
    let ir = ingest_paths(&[path], &IngestOptions::default())
        .unwrap()
        .into_ir("p", "t", "Readable chapter");
    (dir, ir)
}
#[test]
fn low_contrast_is_blocking_without_color_rewrite() {
    let (dir, ir) = fixture();
    let mut style = StyleSpec::default();
    style.colors.text = style.colors.surface.clone();
    let before = serde_json::to_vec(&style).unwrap();
    let p = prepare(&ir, &style, &CompileOptions::new(dir.path().join("out"))).unwrap();
    assert!(
        !p.validation.is_passing(),
        "low contrast must block actual card text"
    );
    assert!(p.svg_pages.is_empty());
    assert_eq!(before, serde_json::to_vec(&style).unwrap());
}
#[test]
fn edges_through_card_interiors_are_blocking() {
    let (_, ir) = fixture(); // keep source evidence alive below via direct layout validation report
    let style = StyleSpec::default();
    let mut doc = layout(&ir, &style, &LayoutOptions::default()).unwrap();
    let p = &mut doc.pages[0];
    let edge = p.edges.first_mut().expect("hierarchy edge");
    let a = p
        .nodes
        .iter()
        .find(|n| n.node_id == edge.from_node)
        .unwrap()
        .rect;
    let b = p
        .nodes
        .iter()
        .find(|n| n.node_id == edge.to_node)
        .unwrap()
        .rect;
    edge.points = vec![
        Point::new(a.x + a.width / 2.0, a.y + a.height / 2.0),
        Point::new(b.x + b.width / 2.0, a.y + a.height / 2.0),
        Point::new(b.x + b.width / 2.0, b.y + b.height / 2.0),
    ];
    let mut report = ValidationReport::new();
    validate_layout(&ir, &doc, &mut report, &ValidationOptions::default()).unwrap();
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.code == "edge_crosses_node" && f.severity == Severity::Blocking),
        "{:#?}",
        report.findings
    );
}
#[test]
fn overflowing_card_is_reflowed_without_changing_ir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.md");
    std::fs::write(
        &path,
        format!("# Long chapter\n\n{}\n", "可读内容".repeat(550)),
    )
    .unwrap();
    let ir = ingest_paths(&[path], &IngestOptions::default())
        .unwrap()
        .into_ir("p", "t", "Long chapter");
    let before = serde_json::to_vec(&ir).unwrap();
    let result = prepare(
        &ir,
        &StyleSpec::default(),
        &CompileOptions::new(dir.path().join("out")),
    )
    .unwrap();
    assert!(
        result.validation.is_passing(),
        "{:?}",
        result.validation.findings
    );
    assert_eq!(before, serde_json::to_vec(&ir).unwrap());
}
#[test]
fn records_are_deterministic_and_selected_undo_restores_prior_options() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("long.md");
    std::fs::write(
        &path,
        format!("# Long chapter\n\n{}\n", "可读内容".repeat(550)),
    )
    .unwrap();
    let ir = ingest_paths(&[path], &IngestOptions::default())
        .unwrap()
        .into_ir("p", "t", "Long chapter");
    let before = serde_json::to_vec(&ir).unwrap();
    let style = StyleSpec::default();
    let options = CompileOptions::new(dir.path().join("out"));
    let p = prepare(&ir, &style, &options).unwrap();
    assert!(p.validation.is_passing());
    assert!(!p.visual_repairs.is_empty() && p.visual_repairs.len() <= 3);
    let repeated = prepare(&ir, &style, &options).unwrap();
    assert_eq!(p.visual_repairs, repeated.visual_repairs);
    assert_eq!(p.effective_layout, repeated.effective_layout);
    let record = p.visual_repairs.last().unwrap();
    let (restored, history) =
        entwurf_core::visual_repair::rollback(&ir, &style, &p.visual_repairs, &record.id).unwrap();
    assert_eq!(restored, record.before);
    assert!(history.last().unwrap().reverted);
    assert!(!p.visual_repairs.last().unwrap().reverted);
    let mut exact = options.clone();
    exact.layout = restored.clone();
    exact.auto_repair = false;
    let undone = prepare(&ir, &style, &exact).unwrap();
    assert_eq!(undone.effective_layout, restored);
    assert!(undone.visual_repairs.is_empty());
    assert!(
        !undone.validation.is_passing(),
        "undo must not silently redo the necessary repair"
    );
    let mut edited = ir.clone();
    edited.title.push('!');
    assert!(
        entwurf_core::visual_repair::rollback(&edited, &style, &p.visual_repairs, &record.id)
            .is_err()
    );
    let (first, all_reverted) = entwurf_core::visual_repair::rollback(
        &ir,
        &style,
        &p.visual_repairs,
        &p.visual_repairs[0].id,
    )
    .unwrap();
    assert_eq!(first, p.visual_repairs[0].before);
    assert!(all_reverted.iter().all(|r| r.reverted));
    assert_eq!(before, serde_json::to_vec(&ir).unwrap());
    let artifact = entwurf_core::compiler::compile(&ir, &style, &options).unwrap();
    assert_eq!(artifact.visual_repairs, p.visual_repairs);
    assert_eq!(artifact.effective_layout, p.effective_layout);
    assert_eq!(artifact.layout_document.node_count(), ir.nodes.len());
}
#[test]
fn unfixable_content_stops_after_three_rounds_without_exports() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.md");
    std::fs::write(&path, format!("# Huge\n\n{}\n", "内容".repeat(12000))).unwrap();
    let ingestion = IngestOptions {
        max_chars_per_node: 100_000,
        ..Default::default()
    };
    let ir = ingest_paths(&[path], &ingestion)
        .unwrap()
        .into_ir("p", "t", "Huge");
    let before = serde_json::to_vec(&ir).unwrap();
    let options = CompileOptions::new(dir.path().join("out"));
    let p = prepare(&ir, &StyleSpec::default(), &options).unwrap();
    assert!(!p.validation.is_passing());
    assert_eq!(p.visual_repairs.len(), 3);
    assert!(p
        .visual_repairs
        .iter()
        .all(|r| !r.passed && r.after.max_font_pt >= 8.0));
    assert!(p.svg_pages.is_empty());
    assert!(!options.output_dir.exists());
    assert_eq!(before, serde_json::to_vec(&ir).unwrap());
}
#[test]
fn obstacle_rerouting_removes_collisions_and_is_deterministic() {
    let (_dir, mut ir) = fixture();
    ir.nodes.truncate(3);
    for n in &mut ir.nodes {
        n.parent_id = None;
    }
    ir.edges = vec![GraphEdge {
        id: "detour".into(),
        from: ir.nodes[0].id.clone(),
        to: ir.nodes[2].id.clone(),
        edge_type: EdgeKind::Related,
        label: None,
        anchors: vec![],
    }];
    let mut doc = layout(&ir, &StyleSpec::default(), &LayoutOptions::default()).unwrap();
    let page = &mut doc.pages[0];
    page.width_pt = 800.0;
    page.height_pt = 400.0;
    let rs = [
        entwurf_core::model::Rect::new(50.0, 150.0, 120.0, 80.0),
        entwurf_core::model::Rect::new(300.0, 110.0, 160.0, 160.0),
        entwurf_core::model::Rect::new(600.0, 150.0, 120.0, 80.0),
    ];
    for (n, r) in page.nodes.iter_mut().zip(rs) {
        n.rect = r;
        n.measured_content_rect = None;
        n.measured_font_pt = None;
        n.body_lines.clear();
        n.title_lines.clear();
        n.latex_lines.clear();
    }
    let original = doc.clone();
    entwurf_core::visual_repair::reroute(&ir, &mut doc).unwrap();
    let mut other = original;
    entwurf_core::visual_repair::reroute(&ir, &mut other).unwrap();
    assert_eq!(
        serde_json::to_vec(&doc).unwrap(),
        serde_json::to_vec(&other).unwrap()
    );
    let mut report = ValidationReport::new();
    validate_layout(&ir, &doc, &mut report, &ValidationOptions::default()).unwrap();
    assert!(report.is_passing(), "{:?}", report.findings);
    assert!(doc.pages[0].edges[0].points.len() >= 4);
}

#[test]
fn exhausted_version_does_not_repeat_automatic_repairs() {
    let (dir, ir) = fixture();
    let mut options = CompileOptions::new(dir.path().join("out"));
    options.layout.preferred_columns = 12;
    options.repair_rounds_used = 3;
    let prepared = prepare(&ir, &StyleSpec::default(), &options).unwrap();
    assert!(prepared.visual_repairs.is_empty());
}

#[test]
fn heading_footer_contrast_and_invalid_color_are_blocking() {
    let (dir, ir) = fixture();
    let options = CompileOptions::new(dir.path().join("out"));
    let mut style = StyleSpec::default();
    style.colors.surface = "#000000".into();
    style.colors.text = "#ffffff".into();
    style.colors.background = "#ffffff".into();
    let p = prepare(&ir, &style, &options).unwrap();
    assert!(p
        .validation
        .findings
        .iter()
        .any(|f| f.code == "text_contrast" && f.message.contains("heading/footer")));
    assert!(p.visual_repairs.is_empty());
    assert!(p.svg_pages.is_empty());
    style.colors.text = "#中a00".into();
    let p = prepare(&ir, &style, &options).unwrap();
    assert!(p
        .validation
        .findings
        .iter()
        .any(|f| f.code == "invalid_color"));
    assert!(p.visual_repairs.is_empty());
}

#[test]
fn invisible_self_route_is_repaired_through_prepare() {
    let (dir, mut ir) = fixture();
    let id = ir.nodes[0].id.clone();
    ir.edges = vec![GraphEdge {
        id: "self-route".into(),
        from: id.clone(),
        to: id,
        edge_type: EdgeKind::Related,
        label: None,
        anchors: vec![],
    }];
    let before = serde_json::to_vec(&ir).unwrap();
    let prepared = prepare(
        &ir,
        &StyleSpec::default(),
        &CompileOptions::new(dir.path().join("out")),
    )
    .unwrap();
    assert!(
        prepared.validation.is_passing(),
        "{:?}",
        prepared.validation.findings
    );
    assert_eq!(prepared.visual_repairs.len(), 1);
    assert!(prepared.effective_layout.obstacle_routing);
    assert!(prepared.document.unwrap().pages[0].edges[0].points.len() >= 4);
    assert_eq!(before, serde_json::to_vec(&ir).unwrap());
}

#[test]
fn overflowing_document_heading_is_resized_without_touching_title() {
    let (dir, mut ir) = fixture();
    ir.title = "可读标题".repeat(40);
    let before = serde_json::to_vec(&ir).unwrap();
    let prepared = prepare(
        &ir,
        &StyleSpec::default(),
        &CompileOptions::new(dir.path().join("out")),
    )
    .unwrap();
    assert!(
        prepared.validation.is_passing(),
        "{:?}",
        prepared.validation.findings
    );
    assert!(!prepared.visual_repairs.is_empty());
    assert!(prepared.visual_repairs.len() <= 3);
    assert!(prepared.effective_layout.max_heading_font_pt >= 8.0);
    assert_eq!(before, serde_json::to_vec(&ir).unwrap());
}
