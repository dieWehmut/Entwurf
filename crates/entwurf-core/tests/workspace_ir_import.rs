use entwurf_core::{
    ingest::IngestOptions,
    workspace::{NodeEdit, Workspace},
    NodeOrigin,
};
use std::{
    fs,
    time::{Duration, SystemTime},
};

fn fixture() -> (tempfile::TempDir, Workspace, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(dir.path().join("项目"), "Import").unwrap();
    let task = ws
        .create_task("Original", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let source = dir.path().join("chapter.md");
    fs::write(&source, "# Chapter\n\nA source paragraph.\n\n$x=1$\n").unwrap();
    ws.import_sources(&task, &[source], &IngestOptions::standard())
        .unwrap();
    (dir, ws, task)
}

#[test]
fn imported_snapshot_uses_retained_sources_and_preserves_reviews_and_manual_edits() {
    let (_dir, mut ws, task) = fixture();
    let initial = ws.snapshot(Some(&task)).unwrap().current_version.unwrap();
    let node = initial.ir.nodes.iter().find(|n| n.latex.is_none()).unwrap();
    let mut edit = NodeEdit::from(node);
    edit.title = "My manual title".into();
    let edited = ws
        .update_nodes(&task, &initial.id, vec![edit], None)
        .unwrap()
        .current_version
        .unwrap();
    let new_task = ws
        .create_task("Imported", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let json = serde_json::to_string(&edited.ir).unwrap();
    let imported = ws.import_ir(&new_task, None, &json).unwrap();
    let current = imported.current_version.unwrap();
    assert_ne!(current.ir.id, edited.ir.id);
    assert_eq!(current.ir.task_id, new_task);
    assert_eq!(current.ir.nodes[0].origin, NodeOrigin::UserEdit);
    for field in ["nodes", "edges", "corrections", "unverified", "sources"] {
        assert_eq!(
            serde_json::to_value(&current.ir).unwrap()[field],
            serde_json::to_value(&edited.ir).unwrap()[field]
        );
    }
    assert!(current.layout.is_none() && current.artifacts.is_none());
    assert!(!current.validation.as_ref().unwrap().is_passing());
    let mut reopened = Workspace::open(&imported.root).unwrap();
    assert!(!reopened
        .preview_current(&new_task)
        .unwrap()
        .current_version
        .unwrap()
        .validation
        .unwrap()
        .is_passing());
}

#[test]
fn exported_knowledge_roundtrips_into_new_task_and_can_export_again() {
    let (_dir, mut ws, task) = fixture();
    loop {
        let v = ws.snapshot(Some(&task)).unwrap().current_version.unwrap();
        let Some(item) = v.ir.unverified.first() else {
            break;
        };
        ws.resolve_review(&task, &v.id, &item.id, "Checked exact original")
            .unwrap();
    }
    let exported = ws
        .export_current(&task, None)
        .unwrap()
        .current_version
        .unwrap();
    let json = fs::read_to_string(exported.artifacts.unwrap().ir).unwrap();
    let new_task = ws
        .create_task("Roundtrip", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let imported = ws
        .import_ir(&new_task, None, &json)
        .unwrap()
        .current_version
        .unwrap();
    assert!(imported.validation.unwrap().is_passing());
    assert!(imported.artifacts.is_none());
    assert!(ws
        .export_current(&new_task, None)
        .unwrap()
        .current_version
        .unwrap()
        .artifacts
        .is_some());
}

#[test]
fn incoming_stored_path_is_never_read_even_when_it_exists() {
    let (dir, mut ws, task) = fixture();
    let mut ir = ws
        .snapshot(Some(&task))
        .unwrap()
        .current_version
        .unwrap()
        .ir;
    let sentinel = dir.path().join("private-sentinel");
    fs::write(&sentinel, "private data must not be read").unwrap();
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
    fs::File::options()
        .write(true)
        .open(&sentinel)
        .unwrap()
        .set_times(fs::FileTimes::new().set_accessed(old).set_modified(old))
        .unwrap();
    ir.sources[0].stored_path = sentinel.to_string_lossy().into();
    let new_task = ws
        .create_task("Safe import", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let imported = ws
        .import_ir(&new_task, None, &serde_json::to_string(&ir).unwrap())
        .unwrap()
        .current_version
        .unwrap();
    assert_ne!(
        imported.ir.sources[0].stored_path,
        ir.sources[0].stored_path
    );
    assert_eq!(fs::metadata(sentinel).unwrap().accessed().unwrap(), old);
}

#[test]
fn missing_tampered_sources_and_invalid_references_leave_task_empty() {
    let (_dir, mut ws, task) = fixture();
    let ir = ws
        .snapshot(Some(&task))
        .unwrap()
        .current_version
        .unwrap()
        .ir;
    let new_task = ws
        .create_task("Destination", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let mut missing = ir.clone();
    missing.sources[0].content_hash = "a".repeat(64);
    let err = ws
        .import_ir(&new_task, None, &serde_json::to_string(&missing).unwrap())
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("import the original sources first"));
    let mut invalid = ir.clone();
    invalid.nodes[0].anchors[0].source_id = "missing-source".into();
    assert!(ws
        .import_ir(&new_task, None, &serde_json::to_string(&invalid).unwrap())
        .is_err());
    fs::write(&ir.sources[0].stored_path, "modified retained bytes").unwrap();
    assert!(ws
        .import_ir(&new_task, None, &serde_json::to_string(&ir).unwrap())
        .is_err());
    assert!(ws
        .snapshot(Some(&new_task))
        .unwrap()
        .current_version
        .is_none());
}

#[test]
fn populated_task_or_stale_baseline_cannot_be_replaced() {
    let (_dir, mut ws, task) = fixture();
    let before = ws.snapshot(Some(&task)).unwrap().current_version.unwrap();
    let json = serde_json::to_string(&before.ir).unwrap();
    assert!(ws.import_ir(&task, Some("stale"), &json).is_err());
    assert!(ws.import_ir(&task, Some(&before.id), &json).is_err());
    assert!(ws.import_ir(&task, None, &json).is_err());
    assert_eq!(
        ws.snapshot(Some(&task))
            .unwrap()
            .current_version
            .unwrap()
            .id,
        before.id
    );
}

#[test]
fn inbound_pdf_transcript_and_diagnostic_edits_are_not_trusted() {
    let (dir, mut ws, task) = fixture();
    let pdf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/public/ingestion/two-pages.pdf");
    // Deliberately unavailable engine produces retained failure metadata. An
    // imported snapshot cannot remove it or replace it with a forged transcript.
    ws.import_sources(
        &task,
        &[pdf],
        &IngestOptions {
            pdfium_dir: Some(dir.path().into()),
            ..IngestOptions::standard()
        },
    )
    .unwrap();
    let mut ir = ws
        .snapshot(Some(&task))
        .unwrap()
        .current_version
        .unwrap()
        .ir;
    let source = ir
        .sources
        .iter_mut()
        .find(|s| s.kind == entwurf_core::SourceKind::Pdf)
        .unwrap();
    let trusted_notes = source.notes.clone();
    assert!(trusted_notes
        .iter()
        .any(|n| n.contains("pdfium_unavailable")));
    source.notes = vec!["ingestion_transcript:{\"text\":\"forged\",\"page\":1}".into()];
    ir.unverified.clear();
    let new_task = ws
        .create_task("PDF imported", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let imported = ws
        .import_ir(&new_task, None, &serde_json::to_string(&ir).unwrap())
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(
        imported
            .ir
            .sources
            .iter()
            .find(|s| s.kind == entwurf_core::SourceKind::Pdf)
            .unwrap()
            .notes,
        trusted_notes
    );
    assert!(!imported.validation.unwrap().is_passing());
}

#[test]
fn source_identity_can_be_rebound_without_rewriting_anchor_history() {
    let (_dir, mut ws, task) = fixture();
    let mut ir = ws
        .snapshot(Some(&task))
        .unwrap()
        .current_version
        .unwrap()
        .ir;
    let imported_id = "external-source-id";
    ir.sources[0].id = imported_id.into();
    for node in &mut ir.nodes {
        for anchor in &mut node.anchors {
            anchor.source_id = imported_id.into();
        }
    }
    for edge in &mut ir.edges {
        for anchor in &mut edge.anchors {
            anchor.source_id = imported_id.into();
        }
    }
    let new_task = ws
        .create_task("Mapped", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let imported = ws
        .import_ir(&new_task, None, &serde_json::to_string(&ir).unwrap())
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(imported.ir.sources[0].id, imported_id);
    assert_eq!(
        serde_json::to_value(imported.ir.nodes).unwrap(),
        serde_json::to_value(ir.nodes).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn retained_source_symlink_escape_is_rejected_before_read() {
    let (dir, mut ws, task) = fixture();
    let ir = ws
        .snapshot(Some(&task))
        .unwrap()
        .current_version
        .unwrap()
        .ir;
    let outside = dir.path().join("outside.md");
    fs::copy(&ir.sources[0].stored_path, &outside).unwrap();
    fs::remove_file(&ir.sources[0].stored_path).unwrap();
    std::os::unix::fs::symlink(&outside, &ir.sources[0].stored_path).unwrap();
    let new_task = ws
        .create_task("Destination", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    assert!(ws
        .import_ir(&new_task, None, &serde_json::to_string(&ir).unwrap())
        .is_err());
}

#[test]
#[ignore = "requires pinned PDFium runtime via ENTWURF_PDFIUM_DIR"]
fn later_successful_pdf_evidence_is_not_replaced_by_earlier_failure() {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(dir.path().join("project"), "Historical evidence").unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/public/ingestion/two-pages.pdf");
    let failed = ws
        .create_task("Earlier missing PDFium", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    ws.import_sources(
        &failed,
        &[source.clone()],
        &IngestOptions {
            pdfium_dir: Some(dir.path().join("missing-engine")),
            ..IngestOptions::standard()
        },
    )
    .unwrap();
    let failed_before = ws.snapshot(Some(&failed)).unwrap().current_version.unwrap();
    let successful = ws
        .create_task("Later successful parse", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let original = ws
        .import_sources(&successful, &[source], &IngestOptions::standard())
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(
        original.ir.nodes.len(),
        2,
        "real PDFium extraction is required"
    );
    let report = entwurf_core::validate::validate_ir(&original.ir, &Default::default()).unwrap();
    assert!(report.is_passing());
    let task = ws
        .create_task("Reimport successful snapshot", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let imported = ws
        .import_ir(&task, None, &serde_json::to_string(&original.ir).unwrap())
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(imported.ir.sources[0].notes, original.ir.sources[0].notes);
    assert!(imported.validation.unwrap().is_passing());
    let failed_after = ws.snapshot(Some(&failed)).unwrap().current_version.unwrap();
    assert_eq!(
        serde_json::to_value(failed_after).unwrap(),
        serde_json::to_value(failed_before).unwrap()
    );
    for forged_notes in [
        vec![],
        vec!["ingestion_transcript:{\"page\":1,\"text\":\"forged\"}".into()],
    ] {
        let mut forged = original.ir.clone();
        forged.sources[0].notes = forged_notes;
        let task = ws
            .create_task("Unmatched evidence", None)
            .unwrap()
            .selected_task_id
            .unwrap();
        let imported = ws
            .import_ir(&task, None, &serde_json::to_string(&forged).unwrap())
            .unwrap()
            .current_version
            .unwrap();
        assert!(!imported.validation.unwrap().is_passing());
        assert!(imported.ir.sources[0]
            .notes
            .iter()
            .any(|n| n.contains("pdfium_unavailable")));
    }
}
