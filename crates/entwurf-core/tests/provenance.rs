use entwurf_core::{provenance::validate_provenance, *};
fn sample() -> (tempfile::TempDir, KnowledgeIr) {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("a.md");
    std::fs::write(&p, "$x=10$").unwrap();
    let ir = ingest::ingest_paths(&[p], &ingest::IngestOptions::standard())
        .unwrap()
        .into_ir("p", "t", "Math");
    (d, ir)
}
fn report(ir: &KnowledgeIr) -> ValidationReport {
    let mut r = ValidationReport::new();
    validate_provenance(ir, &mut r);
    r
}
#[test]
fn retained_source_tampering_blocks_provenance() {
    let (_d, ir) = sample();
    std::fs::write(&ir.sources[0].stored_path, "$x=11$").unwrap();
    assert!(!report(&ir).is_passing());
}
#[test]
fn anchor_quote_hash_and_range_are_verified() {
    let (_d, mut ir) = sample();
    ir.nodes[0].anchors[0].quote_hash = Some("0".repeat(64));
    assert!(!report(&ir).is_passing());
}
#[test]
fn formula_prefix_is_not_an_exact_source_match() {
    let (_d, mut ir) = sample();
    ir.nodes[0].latex = Some("x=1".into());
    assert!(!report(&ir).is_passing());
}
#[test]
fn original_formula_and_complete_correction_chain_are_accepted() {
    let (_d, mut ir) = sample();
    assert!(report(&ir).is_passing());
    let id = ir.nodes[0].id.clone();
    ir.corrections.push(Correction {
        id: new_id(),
        node_id: id,
        field: CorrectionField::Latex,
        before: "x=10".into(),
        after: "x=11".into(),
        reason: "Source correction".into(),
        anchors: ir.nodes[0].anchors.clone(),
        accepted: true,
        reverted: false,
        created_at: chrono::Utc::now(),
    });
    ir.nodes[0].latex = Some("x=11".into());
    assert!(report(&ir).is_passing());
    ir.corrections[0].before = "forged".into();
    assert!(!report(&ir).is_passing());
}
#[test]
fn extraction_error_in_source_notes_cannot_be_hidden_by_clearing_review_queue() {
    let (_d, mut ir) = sample();
    ir.unverified.clear();
    ir.sources[0].notes.push("ingestion_diagnostic:{\"level\":\"error\",\"code\":\"pdf_page_load_failed\",\"message\":\"page missing\"}".into());
    assert!(!report(&ir).is_passing());
}
#[test]
fn superseded_formula_correction_cannot_validate_an_old_value() {
    let (_d, mut ir) = sample();
    for (before, after) in [("x=10", "x=11"), ("x=11", "x=12")] {
        ir.corrections.push(Correction {
            id: new_id(),
            node_id: ir.nodes[0].id.clone(),
            field: CorrectionField::Latex,
            before: before.into(),
            after: after.into(),
            reason: "Correction".into(),
            anchors: ir.nodes[0].anchors.clone(),
            accepted: true,
            reverted: false,
            created_at: chrono::Utc::now(),
        });
    }
    ir.nodes[0].latex = Some("x=11".into());
    assert!(!report(&ir).is_passing());
}
