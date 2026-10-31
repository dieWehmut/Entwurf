use entwurf_core::{ingest::IngestOptions, workspace::Workspace};
fn fixture() -> (tempfile::TempDir, Workspace, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(dir.path().join("项目"), "Repairs").unwrap();
    let id = ws
        .create_task("Long chapter", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let path = dir.path().join("source.md");
    std::fs::write(
        &path,
        format!("# Long chapter\n\n{}\n", "可读内容".repeat(550)),
    )
    .unwrap();
    ws.import_sources(&id, &[path], &IngestOptions::standard())
        .unwrap();
    (dir, ws, id)
}
#[test]
fn repaired_options_and_history_survive_reopen_and_repeated_preview() {
    let (_dir, mut ws, id) = fixture();
    let baseline = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
    let before = serde_json::to_vec(&baseline.ir).unwrap();
    let fixed = ws.preview_current(&id).unwrap();
    let v = fixed.current_version.unwrap();
    let history = v
        .visual_repairs
        .clone()
        .expect("persist actual repair history");
    assert!(!history.is_empty() && history.len() <= 3);
    assert!(v.validation.as_ref().unwrap().is_passing());
    assert!(v.layout_options.is_some());
    assert_eq!(serde_json::to_vec(&v.ir).unwrap(), before);
    let mut reopened = Workspace::open(&fixed.root).unwrap();
    let again = reopened
        .preview_current(&id)
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(again.visual_repairs.as_ref(), Some(&history));
    assert_eq!(again.layout_options, v.layout_options);
    assert_eq!(again.id, baseline.id);
    let exported = reopened
        .export_current(&id, None)
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(exported.visual_repairs.as_ref(), Some(&history));
    assert!(exported.artifacts.is_some());
}
#[test]
fn failed_repairs_never_exceed_three_active_rounds_across_retries() {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(dir.path().join("project"), "Blocked").unwrap();
    let id = ws
        .create_task("Huge", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let path = dir.path().join("source.md");
    std::fs::write(&path, format!("# Huge\n\n{}\n", "内容".repeat(12000))).unwrap();
    ws.import_sources(
        &id,
        &[path],
        &IngestOptions {
            max_chars_per_node: 100_000,
            ..IngestOptions::standard()
        },
    )
    .unwrap();
    let first = ws.preview_current(&id).unwrap().current_version.unwrap();
    assert_eq!(first.visual_repairs.as_ref().unwrap().len(), 3);
    assert!(!first.validation.as_ref().unwrap().is_passing());
    let again = ws.preview_current(&id).unwrap().current_version.unwrap();
    assert_eq!(again.visual_repairs, first.visual_repairs);
    assert!(!again.validation.unwrap().is_passing());
    assert!(again.artifacts.is_none());
}
#[test]
fn selected_repair_undo_is_new_version_with_identical_knowledge_and_no_automatic_redo() {
    let (_dir, mut ws, id) = fixture();
    let prepared = ws.prepare_handoff(&id).unwrap().current_version.unwrap();
    let result = serde_json::json!({"schema_version":1,"task_id":id,"revision":prepared.revision,"ir_patch":{"add_nodes":[],"update_nodes":[],"add_edges":[],"remove_node_ids":[]},"visual_plan":{"page_count":1,"groups":[],"notes":[]},"corrections":[],"image_path":null,"notes":[]});
    ws.import_result(&id, &result.to_string()).unwrap();
    let fixed = ws.preview_current(&id).unwrap();
    let before = fixed.current_version.unwrap();
    assert!(before.source_prompt_hash.is_some());
    let record = before.visual_repairs.as_ref().unwrap().last().unwrap();
    let ir = serde_json::to_vec(&before.ir).unwrap();
    assert!(ws.undo_visual_repair(&id, &before.id, "unknown").is_err());
    let undone = ws.undo_visual_repair(&id, &before.id, &record.id).unwrap();
    let v = undone.current_version.unwrap();
    assert_ne!(v.id, before.id);
    assert_eq!(v.source_prompt_hash, before.source_prompt_hash);
    assert_eq!(serde_json::to_vec(&v.ir).unwrap(), ir);
    assert_eq!(v.layout_options.as_ref(), Some(&record.before));
    assert_eq!(v.visual_repair_enabled, Some(false));
    assert!(v.visual_repairs.as_ref().unwrap().last().unwrap().reverted);
    assert!(!v.validation.as_ref().unwrap().is_passing());
    assert!(v.artifacts.is_none());
    let old = undone
        .project
        .task(&id)
        .unwrap()
        .versions
        .iter()
        .find(|v| v.id == before.id)
        .unwrap();
    assert_eq!(old.visual_repairs, before.visual_repairs);
    let mut reopened = Workspace::open(&undone.root).unwrap();
    let again = reopened
        .preview_current(&id)
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(again.visual_repairs, v.visual_repairs);
    assert_eq!(again.layout_options, v.layout_options);
    assert!(!again.validation.unwrap().is_passing());
    assert!(reopened
        .undo_visual_repair(&id, &before.id, &record.id)
        .is_err());
}
#[test]
fn saved_visual_history_cannot_be_rewritten_or_removed() {
    use entwurf_core::storage::{save_project, ProjectPaths};
    let (_dir, mut ws, id) = fixture();
    let fixed = ws.preview_current(&id).unwrap();
    let mut tampered = fixed.project.clone();
    tampered
        .task_mut(&id)
        .unwrap()
        .versions
        .last_mut()
        .unwrap()
        .visual_repairs
        .as_mut()
        .unwrap()[0]
        .reason = "rewrite history".into();
    assert!(save_project(&ProjectPaths::new(&fixed.root), &tampered).is_err());
    let mut erased = fixed.project;
    erased
        .task_mut(&id)
        .unwrap()
        .versions
        .last_mut()
        .unwrap()
        .visual_repairs = None;
    assert!(save_project(&ProjectPaths::new(&fixed.root), &erased).is_err());
}
#[test]
fn version_rollback_preserves_selected_visual_undo_state() {
    let (_dir, mut ws, id) = fixture();
    let fixed = ws.preview_current(&id).unwrap().current_version.unwrap();
    let undone = ws
        .undo_visual_repair(
            &id,
            &fixed.id,
            &fixed.visual_repairs.as_ref().unwrap()[0].id,
        )
        .unwrap()
        .current_version
        .unwrap();
    let restored = ws
        .rollback_version(&id, &undone.id)
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(restored.visual_repair_enabled, Some(false));
    assert_eq!(restored.layout_options, undone.layout_options);
    assert_eq!(restored.visual_repairs, undone.visual_repairs);
    assert_eq!(
        serde_json::to_vec(&restored.ir).unwrap(),
        serde_json::to_vec(&undone.ir).unwrap()
    );
    assert!(restored.artifacts.is_none());
    let checked = ws.preview_current(&id).unwrap().current_version.unwrap();
    assert!(!checked.validation.unwrap().is_passing());
}
#[test]
fn rollback_under_changed_style_invalidates_only_visual_proof_with_audit() {
    let (_dir, mut ws, id) = fixture();
    let fixed = ws.preview_current(&id).unwrap();
    let source = fixed.current_version.unwrap();
    let mut style = fixed.project.style;
    style.typography.body_pt = 11.0;
    ws.save_style(style).unwrap();
    let restored = ws.rollback_version(&id, &source.id).unwrap();
    let v = restored.current_version.unwrap();
    assert_eq!(
        serde_json::to_vec(&v.ir).unwrap(),
        serde_json::to_vec(&source.ir).unwrap()
    );
    assert!(v.visual_repairs.is_none());
    assert!(v.layout_options.is_none());
    assert!(restored
        .project
        .audit
        .iter()
        .any(|a| a.detail.contains("different style")));
    let prior = restored
        .project
        .task(&id)
        .unwrap()
        .versions
        .iter()
        .find(|v| v.id == source.id)
        .unwrap();
    assert_eq!(prior.visual_repairs, source.visual_repairs);
}
