use entwurf_core::{
    ingest::IngestOptions,
    workspace::{NodeEdit, Workspace},
    NodeOrigin,
};
use std::fs;

fn opened() -> (tempfile::TempDir, Workspace, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(dir.path().join("中文 项目"), "概率论").unwrap();
    let snap = ws.create_task("第一章", None).unwrap();
    let id = snap.selected_task_id.unwrap();
    (dir, ws, id)
}
fn imported() -> (tempfile::TempDir, Workspace, String) {
    let (dir, mut ws, id) = opened();
    let source = dir.path().join("教材.md");
    fs::write(&source, "# 概率\n\n事件的概率在零与一之间。\n\n$x=1$\n").unwrap();
    ws.import_sources(&id, &[source.clone()], &IngestOptions::standard())
        .unwrap();
    fs::remove_file(source).unwrap();
    (dir, ws, id)
}
#[test]
fn draft_and_import_survive_reopening_without_original_source() {
    let (_dir, ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap();
    let reopened = Workspace::open(&before.root).unwrap();
    let after = reopened.snapshot(Some(&id)).unwrap();
    assert_eq!(after.project.name, "概率论");
    assert!(after.current_version.as_ref().unwrap().ir.nodes.len() >= 3);
    assert!(after.current_version.as_ref().unwrap().validation.is_none());
    assert!(
        std::path::Path::new(&after.current_version.unwrap().ir.sources[0].stored_path).exists()
    );
}
#[test]
fn formula_edit_records_original_and_stale_edit_is_atomic() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let node = before.ir.nodes.iter().find(|n| n.latex.is_some()).unwrap();
    let mut edit = NodeEdit::from(node);
    edit.latex = Some("x=2".into());
    let edited = ws
        .update_nodes(&id, &before.id, vec![edit.clone()], None)
        .unwrap();
    let current = edited.current_version.unwrap();
    let changed = current.ir.node(&node.id).unwrap();
    assert_eq!(changed.latex.as_deref(), Some("x=2"));
    assert_eq!(changed.origin, NodeOrigin::UserEdit);
    assert_eq!(current.ir.corrections.last().unwrap().before, "x=1");
    assert_eq!(current.ir.corrections.last().unwrap().after, "x=2");
    assert!(current.artifacts.is_none());
    assert!(ws.update_nodes(&id, &before.id, vec![edit], None).is_err());
    assert_eq!(
        ws.snapshot(Some(&id)).unwrap().current_version.unwrap().id,
        current.id
    );
}
#[test]
fn handoff_rejection_budget_persists_and_cannot_reset_on_prepare() {
    let (_dir, mut ws, id) = imported();
    let prepared = ws.prepare_handoff(&id).unwrap();
    assert_eq!(prepared.handoff.as_ref().unwrap().correction_attempts, 0);
    let rejected = ws.import_result(&id, "not JSON").unwrap();
    assert_eq!(
        rejected.handoff.as_ref().unwrap().status,
        "correction_required"
    );
    assert_eq!(rejected.handoff.as_ref().unwrap().correction_attempts, 1);
    let mut ws = Workspace::open(&prepared.root).unwrap();
    assert_eq!(
        ws.prepare_handoff(&id)
            .unwrap()
            .handoff
            .unwrap()
            .correction_attempts,
        1
    );
    let twice = ws.import_result(&id, "still not JSON").unwrap();
    assert_eq!(twice.handoff.unwrap().status, "manual_required");
    assert_eq!(
        twice.current_version.unwrap().id,
        prepared.current_version.unwrap().id
    );
}
#[test]
fn rollback_makes_new_version_and_never_reuses_export_outputs() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let mut edit = NodeEdit::from(&before.ir.nodes[0]);
    edit.title = "我的标题".into();
    ws.update_nodes(&id, &before.id, vec![edit], None).unwrap();
    let rolled = ws.rollback_version(&id, &before.id).unwrap();
    let current = rolled.current_version.unwrap();
    assert_eq!(current.revision, 3);
    assert_eq!(current.ir.nodes[0].title, before.ir.nodes[0].title);
    assert!(current.artifacts.is_none());
    assert!(current.validation.is_none());
    assert!(rolled
        .project
        .audit
        .iter()
        .any(|a| a.detail.contains("rollback")));
}
#[test]
fn unknown_node_and_invalid_order_leave_saved_version_unchanged() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let mut edit = NodeEdit::from(&before.ir.nodes[0]);
    edit.id = "unknown".into();
    assert!(ws.update_nodes(&id, &before.id, vec![edit], None).is_err());
    assert!(ws
        .update_nodes(
            &id,
            &before.id,
            vec![],
            Some(vec![before.ir.nodes[0].id.clone()])
        )
        .is_err());
    assert_eq!(
        ws.snapshot(Some(&id)).unwrap().current_version.unwrap().id,
        before.id
    );
}
#[test]
fn formula_confirmation_requires_note_and_preserves_immutable_history() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let review = before
        .ir
        .unverified
        .iter()
        .find(|u| u.node_id.is_some())
        .unwrap();
    assert!(ws.resolve_review(&id, &before.id, &review.id, "").is_err());
    let checked = ws
        .resolve_review(&id, &before.id, &review.id, "已核对原文")
        .unwrap();
    assert!(!checked
        .current_version
        .unwrap()
        .ir
        .unverified
        .iter()
        .any(|u| u.id == review.id));
    assert!(checked.project.tasks[0].versions[0]
        .ir
        .unverified
        .iter()
        .any(|u| u.id == review.id));
}
#[test]
fn unreadable_source_review_cannot_be_approved_with_note() {
    let (dir, mut ws, id) = opened();
    let png = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/public/ingestion/bilingual.png");
    ws.import_sources(
        &id,
        &[png],
        &IngestOptions {
            binary_dir: Some(dir.path().join("no-ocr")),
            ..IngestOptions::standard()
        },
    )
    .unwrap();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let fatal = before
        .ir
        .unverified
        .iter()
        .find(|u| u.node_id.is_none())
        .unwrap();
    assert!(ws
        .resolve_review(&id, &before.id, &fatal.id, "忽略不可读内容")
        .is_err());
    assert_eq!(
        ws.snapshot(Some(&id)).unwrap().current_version.unwrap().id,
        before.id
    );
}
#[test]
fn formula_confirmation_rejects_forged_source_evidence() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let review = before
        .ir
        .unverified
        .iter()
        .find(|u| u.node_id.is_some())
        .unwrap()
        .clone();
    let mut edit = NodeEdit::from(before.ir.node(review.node_id.as_deref().unwrap()).unwrap());
    edit.anchors[0].quote_hash = Some("0".repeat(64));
    let edited = ws
        .update_nodes(&id, &before.id, vec![edit], None)
        .unwrap()
        .current_version
        .unwrap();
    assert!(ws
        .resolve_review(&id, &edited.id, &review.id, "Checked")
        .is_err());
}
#[test]
fn style_change_creates_new_versions_without_rewriting_history() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let mut style = entwurf_core::StyleSpec::default();
    style.typography.body_pt = 11.0;
    let changed = ws.save_style(style).unwrap();
    assert_eq!(changed.project.style.typography.body_pt, 11.0);
    assert_ne!(changed.current_version.as_ref().unwrap().id, before.id);
    assert_eq!(changed.project.tasks[0].versions[0].id, before.id);
    assert!(changed.current_version.unwrap().validation.is_none());
}
#[test]
fn reimport_uses_retained_original_and_protects_manual_nodes() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let source = before.ir.sources[0].id.clone();
    let fresh = ws
        .reimport_source(&id, &before.id, &source, &IngestOptions::standard())
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(fresh.ir.sources.len(), 1);
    assert_eq!(fresh.ir.nodes.len(), before.ir.nodes.len());
    let mut edit = NodeEdit::from(&fresh.ir.nodes[0]);
    edit.title = "Manual".into();
    let edited = ws
        .update_nodes(&id, &fresh.id, vec![edit], None)
        .unwrap()
        .current_version
        .unwrap();
    assert!(ws
        .reimport_source(&id, &edited.id, &source, &IngestOptions::standard())
        .is_err());
}
#[test]
fn valid_result_keeps_manual_edits_and_records_prompt_identity() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let mut edit = NodeEdit::from(&before.ir.nodes[0]);
    edit.title = "我的标题".into();
    let edited = ws
        .update_nodes(&id, &before.id, vec![edit], None)
        .unwrap()
        .current_version
        .unwrap();
    ws.prepare_handoff(&id).unwrap();
    let result = serde_json::json!({"schema_version":1,"task_id":id,"revision":edited.revision,"ir_patch":{"add_nodes":[],"update_nodes":[{"id":edited.ir.nodes[0].id,"title":"GPT title","body":null,"latex":null,"importance":null}],"add_edges":[],"remove_node_ids":[edited.ir.nodes[0].id]},"visual_plan":{"page_count":1,"groups":[],"notes":[]},"corrections":[],"image_path":null,"notes":[]});
    let imported = ws.import_result(&id, &result.to_string()).unwrap();
    let v = imported.current_version.unwrap();
    assert_eq!(v.ir.nodes[0].title, "我的标题");
    assert!(v.source_prompt_hash.as_ref().is_some_and(|h| h.len() == 64));
    let reopened = Workspace::open(&imported.root)
        .unwrap()
        .snapshot(Some(&id))
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(reopened.id, v.id);
    assert_eq!(reopened.source_prompt_hash, v.source_prompt_hash);
    let imported_json = std::path::Path::new(&imported.root)
        .join("imports")
        .join(format!("entwurf-task-{id}-v{}-plan.json", edited.revision));
    let persisted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(imported_json).unwrap()).unwrap();
    assert_eq!(persisted["task_id"], id);
    assert_eq!(persisted["visual_plan"]["page_count"], 1);
}
#[test]
fn wrong_result_identity_consumes_retry_without_mutating_ir() {
    let (_dir, mut ws, id) = imported();
    let p = ws.prepare_handoff(&id).unwrap();
    let before = p.current_version.unwrap();
    let r = serde_json::json!({"schema_version":1,"task_id":"another-task","revision":before.revision,"ir_patch":{"add_nodes":[],"update_nodes":[],"add_edges":[],"remove_node_ids":[]},"visual_plan":{"page_count":1,"groups":[],"notes":[]},"corrections":[],"image_path":null,"notes":[]});
    let rejected = ws.import_result(&id, &r.to_string()).unwrap();
    assert_eq!(rejected.handoff.unwrap().status, "correction_required");
    assert_eq!(rejected.current_version.unwrap().id, before.id);
}
#[test]
fn undoing_manual_formula_addition_restores_absent_formula() {
    let (_dir, mut ws, id) = imported();
    let before = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let node = before.ir.nodes.iter().find(|n| n.latex.is_none()).unwrap();
    let mut edit = NodeEdit::from(node);
    edit.latex = Some("x=1".into());
    let edited = ws
        .update_nodes(&id, &before.id, vec![edit], None)
        .unwrap()
        .current_version
        .unwrap();
    let c = edited
        .ir
        .corrections
        .iter()
        .find(|c| c.field == entwurf_core::CorrectionField::Latex)
        .unwrap();
    let undone = ws
        .revert_correction(&id, &edited.id, &c.id)
        .unwrap()
        .current_version
        .unwrap();
    assert!(undone.ir.node(&node.id).unwrap().latex.is_none());
}
#[test]
fn blocked_export_persists_report_without_artifacts() {
    let (dir, mut ws, id) = imported();
    let output = dir.path().join("blocked");
    let blocked = ws.export_current(&id, Some(output.clone())).unwrap();
    let v = blocked.current_version.unwrap();
    assert!(!v.validation.unwrap().is_passing());
    assert!(v.artifacts.is_none());
    assert!(!output.exists());
    let reopened = Workspace::open(&blocked.root)
        .unwrap()
        .snapshot(Some(&id))
        .unwrap()
        .current_version
        .unwrap();
    assert!(reopened.validation.is_some());
    assert!(reopened.artifacts.is_none());
}
#[test]
fn approved_export_is_real_and_edit_invalidates_it() {
    let (dir, mut ws, id) = imported();
    let v = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    for item in v.ir.unverified {
        let current = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
        ws.resolve_review(&id, &current.id, &item.id, "核对原文")
            .unwrap();
    }
    let preview = ws.preview_current(&id).unwrap();
    assert!(!preview.preview_pages.is_empty());
    assert!(preview.preview_pages[0].svg.contains("<svg"));
    let output = dir.path().join("导出 文件");
    let exported = ws.export_current(&id, Some(output)).unwrap();
    let v = exported.current_version.unwrap();
    let files = v.artifacts.as_ref().unwrap();
    assert!(std::fs::read(&files.pdf).unwrap().starts_with(b"%PDF"));
    assert!(!std::fs::read(&files.png[0]).unwrap().is_empty());
    assert!(v.validation.as_ref().unwrap().is_passing());
    assert!(ws.artifact_path(&id, "pdf", 0).unwrap().exists());
    let mut edit = NodeEdit::from(&v.ir.nodes[0]);
    edit.title = "New title".into();
    let changed = ws.update_nodes(&id, &v.id, vec![edit], None).unwrap();
    assert!(changed.current_version.unwrap().artifacts.is_none());
    assert!(ws.artifact_path(&id, "pdf", 0).is_err());
}
#[test]
fn style_references_are_copied_without_knowledge_ingestion() {
    let (dir, mut ws, id) = opened();
    let image = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/public/ingestion/bilingual.png");
    let selected = dir.path().join("风格.png");
    std::fs::copy(image, &selected).unwrap();
    let saved = ws.import_style_images(&[selected.clone()]).unwrap();
    std::fs::remove_file(selected).unwrap();
    assert!(saved.current_version.is_none());
    assert_eq!(saved.project.style.reference_images.len(), 1);
    assert!(std::path::Path::new(&saved.project.style.reference_images[0]).is_file());
    assert!(ws.snapshot(Some(&id)).unwrap().current_version.is_none());
}
#[test]
fn ocr_language_setting_survives_reopen() {
    let (_dir, mut ws, _id) = opened();
    assert!(ws.save_settings("").is_err());
    let saved = ws.save_settings("eng").unwrap();
    let reopened = Workspace::open(&saved.root)
        .unwrap()
        .snapshot(None)
        .unwrap();
    assert_eq!(reopened.project.settings.ocr_languages, "eng");
    assert!(!reopened.project.settings.automation_enabled);
}
#[test]
fn task_selection_survives_project_settings_and_style_updates() {
    let (_dir, mut ws, first) = opened();
    let second = ws
        .create_task("Second", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    assert_ne!(first, second);
    ws.select_task(Some(&first)).unwrap();
    assert_eq!(
        ws.save_settings("eng").unwrap().selected_task_id.as_deref(),
        Some(first.as_str())
    );
    assert_eq!(
        ws.save_style(entwurf_core::StyleSpec::default())
            .unwrap()
            .selected_task_id
            .as_deref(),
        Some(first.as_str())
    );
}
#[test]
fn tasks_exported_to_same_directory_never_open_each_others_artifact() {
    let (dir, mut ws, first) = imported();
    let current = ws.snapshot(Some(&first)).unwrap().current_version.unwrap();
    for item in current.ir.unverified {
        let v = ws.snapshot(Some(&first)).unwrap().current_version.unwrap();
        ws.resolve_review(&first, &v.id, &item.id, "Reviewed")
            .unwrap();
    }
    let selected = dir.path().join("exports");
    let a = ws
        .export_current(&first, Some(selected.clone()))
        .unwrap()
        .current_version
        .unwrap();
    let original = std::fs::read(&a.artifacts.unwrap().pdf).unwrap();
    let second = ws
        .create_task("Other task", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let source = dir.path().join("second.md");
    std::fs::write(&source, "# Different chapter\n\nDifferent content").unwrap();
    ws.import_sources(&second, &[source], &IngestOptions::standard())
        .unwrap();
    ws.export_current(&second, Some(selected)).unwrap();
    match ws.artifact_path(&first, "pdf", 0) {
        Ok(path) => assert!(
            std::fs::read(path).unwrap() == original,
            "opened another task artifact"
        ),
        Err(_) => {}
    }
}
#[test]
fn modified_artifact_is_rejected_before_open() {
    let (dir, mut ws, id) = opened();
    let source = dir.path().join("source.md");
    std::fs::write(&source, "# Source\n\nBody").unwrap();
    ws.import_sources(&id, &[source], &IngestOptions::standard())
        .unwrap();
    ws.export_current(&id, None).unwrap();
    let path = ws.artifact_path(&id, "pdf", 0).unwrap();
    std::fs::write(path, b"%PDF changed outside Entwurf").unwrap();
    assert!(ws.artifact_path(&id, "pdf", 0).is_err());
}
