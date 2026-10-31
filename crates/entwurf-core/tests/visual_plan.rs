use entwurf_core::{
    compiler::{prepare, CompileOptions},
    ingest::{ingest_paths, IngestOptions},
    layout::{layout, LayoutOptions},
    project::VisualPlanStatus,
    *,
};

fn fixture() -> (tempfile::TempDir, KnowledgeIr) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("chapter.md");
    std::fs::write(&path, "# Chapter\n\nFirst evidence.\n\n## Second\n\nSecond evidence.\n\n## Third\n\nThird evidence.\n\n## Fourth\n\nFourth evidence.\n").unwrap();
    let mut ir = ingest_paths(&[path], &IngestOptions::default())
        .unwrap()
        .into_ir("p", "t", "Chapter");
    ir.edges.clear(); // isolate card packing from separately tested edge routing
    assert!(ir.nodes.len() >= 4);
    (dir, ir)
}
fn plan(ir: &KnowledgeIr, chunks: &[usize], pages: &[u16]) -> VisualPlan {
    let mut start = 0;
    let groups = chunks
        .iter()
        .zip(pages)
        .enumerate()
        .map(|(i, (&n, &page))| {
            let group = VisualGroup {
                id: format!("proposal-{i}"),
                title: "Metadata $not_a_formula$ #include(\"secret\")".into(),
                node_ids: ir.nodes[start..start + n]
                    .iter()
                    .map(|n| n.id.clone())
                    .collect(),
                page,
            };
            start += n;
            group
        })
        .collect();
    assert_eq!(start, ir.nodes.len());
    VisualPlan {
        page_count: *pages.iter().max().unwrap(),
        groups,
        notes: vec![],
    }
}
fn options(dir: &tempfile::TempDir, proposal: VisualPlan) -> CompileOptions {
    let mut opts = CompileOptions::new(dir.path().join("out"));
    opts.auto_repair = false;
    opts.visual_plan = Some(proposal);
    opts
}
#[test]
fn compatible_groups_and_pages_change_geometry_without_changing_ir() {
    let (dir, ir) = fixture();
    let original = serde_json::to_vec(&ir).unwrap();
    let style = StyleSpec::default();
    let a = prepare(
        &ir,
        &style,
        &options(&dir, plan(&ir, &[ir.nodes.len()], &[1])),
    )
    .unwrap();
    let b = prepare(
        &ir,
        &style,
        &options(
            &dir,
            plan(&ir, &vec![1; ir.nodes.len()], &vec![1; ir.nodes.len()]),
        ),
    )
    .unwrap();
    assert!(a.validation.is_passing(), "{:?}", a.validation.findings);
    assert!(b.validation.is_passing(), "{:?}", b.validation.findings);
    assert_ne!(
        a.document.as_ref().unwrap().pages[0].nodes[1].rect,
        b.document.as_ref().unwrap().pages[0].nodes[1].rect,
        "GPT group boundaries must affect measured packing"
    );
    let c = prepare(
        &ir,
        &style,
        &options(&dir, plan(&ir, &[2, ir.nodes.len() - 2], &[1, 2])),
    )
    .unwrap();
    assert_eq!(
        c.document.as_ref().unwrap().pages.len(),
        2,
        "compatible page hints must create page breaks"
    );
    assert_eq!(c.svg_pages.len(), 2);
    assert_eq!(c.visual_plan_status, Some(VisualPlanStatus::Applied));
    assert!(
        c.visual_plan_warnings.is_empty(),
        "{:?}",
        c.visual_plan_warnings
    );
    for result in [a, b, c] {
        assert_eq!(
            result
                .document
                .unwrap()
                .all_nodes()
                .map(|n| n.node_id.clone())
                .collect::<Vec<_>>(),
            ir.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()
        );
        assert!(result
            .svg_pages
            .iter()
            .all(|s| !s.contains("not_a_formula")));
    }
    assert_eq!(original, serde_json::to_vec(&ir).unwrap());
}
#[test]
fn parent_groups_cannot_reorder_manual_canvas_order() {
    let (_dir, mut ir) = fixture();
    for node in ir.nodes.iter_mut().skip(1) {
        node.kind = NodeKind::Concept;
    }
    let parent = ir.nodes[0].id.clone();
    ir.nodes[1].parent_id = Some(parent.clone());
    ir.nodes[2].parent_id = None;
    ir.nodes[3].parent_id = Some(parent);
    let doc = layout(&ir, &StyleSpec::default(), &LayoutOptions::default()).unwrap();
    assert_eq!(
        doc.all_nodes()
            .map(|n| n.node_id.clone())
            .collect::<Vec<_>>(),
        ir.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn malformed_proposals_are_explicitly_ignored_with_complete_content() {
    let (dir, ir) = fixture();
    let original = serde_json::to_vec(&ir).unwrap();
    let good = plan(&ir, &[2, ir.nodes.len() - 2], &[1, 1]);
    let mut variants = Vec::new();
    let mut missing = good.clone();
    missing.groups[0].node_ids.pop();
    variants.push(missing);
    let mut repeated = good.clone();
    repeated.groups[1].node_ids.push(ir.nodes[0].id.clone());
    variants.push(repeated);
    let mut unknown = good.clone();
    unknown.groups[0].node_ids[0] = "unknown-source-node".into();
    variants.push(unknown);
    let mut duplicate_group = good.clone();
    duplicate_group.groups[1].id = duplicate_group.groups[0].id.clone();
    variants.push(duplicate_group);
    let mut bad_page = good.clone();
    bad_page.groups[1].page = 2;
    variants.push(bad_page);
    let mut too_many = good.clone();
    too_many.page_count = 4;
    variants.push(too_many);
    let mut zero = good.clone();
    zero.page_count = 0;
    variants.push(zero);
    let mut empty = good.clone();
    empty.groups.push(VisualGroup {
        id: "empty".into(),
        title: "Empty".into(),
        node_ids: vec![],
        page: 1,
    });
    variants.push(empty);
    for bad in variants {
        let result = prepare(&ir, &StyleSpec::default(), &options(&dir, bad)).unwrap();
        assert!(
            result.validation.is_passing(),
            "{:?}",
            result.validation.findings
        );
        assert_eq!(result.visual_plan_status, Some(VisualPlanStatus::Ignored));
        assert!(result
            .visual_plan_warnings
            .iter()
            .any(|w| w.code == "visual_plan_invalid"));
        assert!(result
            .validation
            .findings
            .iter()
            .any(|f| f.code == "visual_plan_invalid" && f.severity == Severity::Warning));
        let document = result.document.unwrap();
        assert!(
            document
                .diagnostics
                .iter()
                .any(|d| d.contains("visual_plan_invalid")),
            "exported layout must retain the fallback explanation"
        );
        assert_eq!(document.all_nodes().count(), ir.nodes.len());
    }
    assert_eq!(original, serde_json::to_vec(&ir).unwrap());
}

#[test]
fn each_incompatible_order_hint_is_reported_and_canonical_order_wins() {
    let (dir, ir) = fixture();
    let mut proposal = plan(&ir, &[2, 2, ir.nodes.len() - 4], &[1, 1, 1]);
    proposal.groups[0].node_ids.reverse();
    proposal.groups.swap(1, 2);
    let result = prepare(&ir, &StyleSpec::default(), &options(&dir, proposal.clone())).unwrap();
    assert!(result.validation.is_passing());
    assert_eq!(
        result.visual_plan_status,
        Some(VisualPlanStatus::PartiallyApplied)
    );
    for group in &proposal.groups {
        assert!(
            result
                .visual_plan_warnings
                .iter()
                .any(|w| w.code == "visual_plan_order_protected" && w.message.contains(&group.id)),
            "{:?}",
            result.visual_plan_warnings
        );
    }
    assert_eq!(
        result
            .document
            .unwrap()
            .all_nodes()
            .map(|n| n.node_id.clone())
            .collect::<Vec<_>>(),
        ir.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn unused_page_budget_has_visible_reason_without_blank_pages() {
    let (dir, ir) = fixture();
    let mut proposal = plan(&ir, &[ir.nodes.len()], &[1]);
    proposal.page_count = 3;
    let result = prepare(&ir, &StyleSpec::default(), &options(&dir, proposal)).unwrap();
    assert_eq!(result.document.unwrap().pages.len(), 1);
    assert_eq!(
        result.visual_plan_status,
        Some(VisualPlanStatus::PartiallyApplied)
    );
    assert!(result
        .visual_plan_warnings
        .iter()
        .any(|w| w.code == "visual_plan_page_count_fallback"));
}

#[test]
fn unfit_page_breaks_fall_back_to_measured_layout_without_losing_nodes() {
    let (dir, ir) = fixture();
    let original = serde_json::to_vec(&ir).unwrap();
    let mut opts = options(&dir, plan(&ir, &[1, ir.nodes.len() - 1], &[1, 2]));
    opts.layout.max_pages = 2;
    opts.layout.preferred_columns = 1;
    opts.layout.node_gap_pt = 100.;
    let result = prepare(&ir, &StyleSpec::default(), &opts).unwrap();
    assert!(
        result.validation.is_passing(),
        "{:?}",
        result.validation.findings
    );
    assert_eq!(result.visual_plan_status, Some(VisualPlanStatus::Ignored));
    assert!(
        result
            .visual_plan_warnings
            .iter()
            .any(|w| w.code == "visual_plan_measured_fallback"),
        "{:?}",
        result.visual_plan_warnings
    );
    assert_eq!(
        result
            .document
            .unwrap()
            .all_nodes()
            .map(|n| n.node_id.clone())
            .collect::<Vec<_>>(),
        ir.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()
    );
    assert_eq!(original, serde_json::to_vec(&ir).unwrap());
}

#[test]
fn exact_plan_scene_and_status_reach_export() {
    let (dir, ir) = fixture();
    let mut opts = options(&dir, plan(&ir, &[2, ir.nodes.len() - 2], &[1, 2]));
    opts.raster_dpi = 12.;
    let prepared = prepare(&ir, &StyleSpec::default(), &opts).unwrap();
    let result = entwurf_core::compiler::compile(&ir, &StyleSpec::default(), &opts).unwrap();
    assert_eq!(result.page_count, 2);
    assert_eq!(result.visual_plan_status, Some(VisualPlanStatus::Applied));
    assert_eq!(
        serde_json::to_vec(&result.layout_document).unwrap(),
        serde_json::to_vec(&prepared.document.unwrap()).unwrap()
    );
    assert_eq!(
        std::fs::read(&result.ir_path).unwrap(),
        serde_json::to_vec_pretty(&ir).unwrap()
    );
    for (path, preview) in result.svg_paths.iter().zip(prepared.svg_pages) {
        assert_eq!(std::fs::read_to_string(path).unwrap(), preview);
    }
}

#[test]
fn layout_proposals_cannot_bypass_source_verification_or_write_blocked_exports() {
    let (dir, ir) = fixture();
    let opts = options(&dir, plan(&ir, &[2, ir.nodes.len() - 2], &[1, 2]));
    let original = serde_json::to_vec(&ir).unwrap();
    std::fs::write(&ir.sources[0].stored_path, "Tampered evidence").unwrap();
    let result = prepare(&ir, &StyleSpec::default(), &opts).unwrap();
    assert!(!result.validation.is_passing());
    assert!(result.document.is_none());
    assert!(result.svg_pages.is_empty());
    assert_eq!(
        result.visual_plan_status,
        Some(VisualPlanStatus::NotEvaluated)
    );
    assert!(entwurf_core::compiler::compile(&ir, &StyleSpec::default(), &opts).is_err());
    assert!(!opts.output_dir.exists());
    assert_eq!(serde_json::to_vec(&ir).unwrap(), original);
}
