use entwurf_core::{
    ingest::{ingest_paths, IngestOptions},
    prompt::apply_result,
    NodeOrigin, ResultEnvelope,
};
fn sample() -> (tempfile::TempDir, entwurf_core::KnowledgeIr) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.md");
    std::fs::write(&p, "$x=1$").unwrap();
    let ir = ingest_paths(&[p], &IngestOptions::standard())
        .unwrap()
        .into_ir("p", "t", "Sample");
    (d, ir)
}
fn result() -> ResultEnvelope {
    serde_json::from_value(serde_json::json!({"schema_version":1,"task_id":"t","revision":1,"ir_patch":{"add_nodes":[],"update_nodes":[],"add_edges":[],"remove_node_ids":[]},"visual_plan":{"page_count":1,"groups":[],"notes":[]},"corrections":[],"image_path":null,"notes":[]})).unwrap()
}
#[test]
fn unaudited_formula_update_is_rejected() {
    let (_d, ir) = sample();
    let mut r = result();
    r.ir_patch.update_nodes.push(entwurf_core::NodePatch {
        id: ir.nodes[0].id.clone(),
        title: None,
        body: None,
        latex: Some("x=2".into()),
        importance: None,
    });
    assert!(
        apply_result(&ir, &r).is_err(),
        "formula changes must require explicit correction evidence"
    );
}
#[test]
fn corrections_never_overwrite_manual_nodes() {
    let (_d, mut ir) = sample();
    ir.nodes[0].origin = NodeOrigin::UserEdit;
    let title = ir.nodes[0].title.clone();
    let mut r = result();
    r.corrections.push(entwurf_core::CorrectionProposal {
        node_id: ir.nodes[0].id.clone(),
        field: entwurf_core::CorrectionField::Title,
        before: title.clone(),
        after: "Overwritten".into(),
        reason: "model preference".into(),
        anchors: ir.nodes[0].anchors.clone(),
    });
    let (merged, corrections) = apply_result(&ir, &r).unwrap();
    assert_eq!(merged.nodes[0].title, title);
    assert!(corrections.is_empty());
}
#[test]
fn explicit_formula_correction_records_exact_before_and_can_revert() {
    let (_d, ir) = sample();
    let mut r = result();
    r.corrections.push(entwurf_core::CorrectionProposal {
        node_id: ir.nodes[0].id.clone(),
        field: entwurf_core::CorrectionField::Latex,
        before: "x=1".into(),
        after: "x=2".into(),
        reason: "Verified against source errata".into(),
        anchors: ir.nodes[0].anchors.clone(),
    });
    let (mut merged, cs) = apply_result(&ir, &r).unwrap();
    assert_eq!(merged.nodes[0].latex.as_deref(), Some("x=2"));
    assert_eq!(cs[0].before, "x=1");
    entwurf_core::prompt::revert_correction(&mut merged, &cs[0].id).unwrap();
    assert_eq!(merged.nodes[0].latex.as_deref(), Some("x=1"));
    assert!(merged.corrections[0].reverted);
}
#[test]
fn stale_correction_before_and_duplicate_addition_are_rejected() {
    let (_d, ir) = sample();
    let mut r = result();
    r.corrections.push(entwurf_core::CorrectionProposal {
        node_id: ir.nodes[0].id.clone(),
        field: entwurf_core::CorrectionField::Latex,
        before: "not the original".into(),
        after: "x=2".into(),
        reason: "Reason".into(),
        anchors: ir.nodes[0].anchors.clone(),
    });
    assert!(apply_result(&ir, &r).is_err());
    let mut r = result();
    r.ir_patch.add_nodes.push(ir.nodes[0].clone());
    assert!(apply_result(&ir, &r).is_err());
}
#[test]
fn reverting_old_correction_cannot_overwrite_later_edit() {
    let (_d, ir) = sample();
    let mut r = result();
    r.corrections.push(entwurf_core::CorrectionProposal {
        node_id: ir.nodes[0].id.clone(),
        field: entwurf_core::CorrectionField::Latex,
        before: "x=1".into(),
        after: "x=2".into(),
        reason: "Reason".into(),
        anchors: ir.nodes[0].anchors.clone(),
    });
    let (mut merged, cs) = apply_result(&ir, &r).unwrap();
    merged.nodes[0].latex = Some("x=3".into());
    assert!(entwurf_core::prompt::revert_correction(&mut merged, &cs[0].id).is_err());
    assert_eq!(merged.nodes[0].latex.as_deref(), Some("x=3"));
}
#[test]
fn ocr_formula_candidate_body_cannot_change_without_audit() {
    let (_d, mut ir) = sample();
    ir.nodes[0].latex = None;
    ir.nodes[0].tags.push("formula_candidate".into());
    ir.nodes[0].body = "x = 1".into();
    let mut r = result();
    r.ir_patch.update_nodes.push(entwurf_core::NodePatch {
        id: ir.nodes[0].id.clone(),
        title: None,
        body: Some("x = 2".into()),
        latex: None,
        importance: None,
    });
    assert!(apply_result(&ir, &r).is_err());
}
#[test]
fn newly_added_gpt_formula_enters_review_queue() {
    let (_d, ir) = sample();
    let mut r = result();
    let mut node = ir.nodes[0].clone();
    node.id = "new-formula".into();
    r.ir_patch.add_nodes.push(node);
    let (merged, _) = apply_result(&ir, &r).unwrap();
    assert!(merged
        .unverified
        .iter()
        .any(|u| u.node_id.as_deref() == Some("new-formula")));
}
#[test]
fn gpt_cannot_delete_source_baseline() {
    let (_d, ir) = sample();
    let mut r = result();
    r.ir_patch.remove_node_ids.push(ir.nodes[0].id.clone());
    assert!(apply_result(&ir, &r).is_err());
}
#[test]
fn gpt_relation_formula_enters_review_queue() {
    let (_d, ir) = sample();
    let mut r = result();
    r.ir_patch.add_edges.push(entwurf_core::GraphEdge {
        id: "relation".into(),
        from: ir.nodes[0].id.clone(),
        to: ir.nodes[0].id.clone(),
        edge_type: entwurf_core::EdgeKind::Related,
        label: Some("$x=1$".into()),
        anchors: ir.nodes[0].anchors.clone(),
    });
    let (merged, _) = apply_result(&ir, &r).unwrap();
    assert!(merged
        .unverified
        .iter()
        .any(|u| u.reason.contains("relation")));
}
