use entwurf_core::{
    ingest::IngestOptions,
    workspace::{NodeEdit, Workspace},
};

fn prepared() -> (tempfile::TempDir, Workspace, String, serde_json::Value) {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(dir.path().join("project"), "Plans").unwrap();
    let task = ws
        .create_task("Chapter", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let path = dir.path().join("source.md");
    std::fs::write(&path, "# Chapter\n\nA precise paragraph.\n").unwrap();
    let first = ws
        .import_sources(&task, &[path], &IngestOptions::standard())
        .unwrap()
        .current_version
        .unwrap();
    let mut edit = NodeEdit::from(&first.ir.nodes[0]);
    edit.title = "Manual heading".into();
    ws.update_nodes(&task, &first.id, vec![edit], None).unwrap();
    let mut style = ws.snapshot(Some(&task)).unwrap().project.style;
    style.paper.max_pages = 1;
    ws.save_style(style).unwrap();
    let v = ws.prepare_handoff(&task).unwrap().current_version.unwrap();
    let result = serde_json::json!({"schema_version":1,"task_id":task,"revision":v.revision,
        "ir_patch":{"add_nodes":[],"update_nodes":[{"id":v.ir.nodes[0].id,"title":"Ignored GPT heading"}],"add_edges":[],"remove_node_ids":[]},
        "visual_plan":{"page_count":2,"groups":[],"notes":["A requested plan"]},
        "corrections":[],"image_path":"/untrusted/not-selected/image.png","notes":[]});
    (dir, ws, task, result)
}

#[test]
fn proposal_warnings_and_unapplied_image_survive_reopen() {
    let (_dir, mut ws, task, result) = prepared();
    let imported = ws.import_result(&task, &result.to_string()).unwrap();
    assert!(
        imported.handoff.is_none(),
        "result rejected: {:?}",
        imported.handoff
    );
    let version = imported.current_version.as_ref().unwrap();
    assert_eq!(version.ir.nodes[0].title, "Manual heading");
    let json = serde_json::to_value(version).unwrap();
    let meta = &json["gpt_import"];
    assert_eq!(meta["visual_plan"], result["visual_plan"]);
    assert_eq!(meta["image_reference_present"], true);
    assert_eq!(meta["image_reference_applied"], false);
    assert_eq!(meta["visual_plan_status"], "not_evaluated");
    for code in [
        "page_budget_exceeded",
        "manual_update_ignored",
        "image_reference_unapplied",
    ] {
        assert!(
            meta["warnings"]
                .as_array()
                .unwrap()
                .iter()
                .any(|w| w["code"] == code),
            "missing {code}"
        );
    }
    let reopened = Workspace::open(&imported.root)
        .unwrap()
        .snapshot(Some(&task))
        .unwrap();
    assert_eq!(
        serde_json::to_value(reopened.current_version.unwrap()).unwrap()["gpt_import"],
        *meta
    );
}

#[test]
fn evaluated_plan_status_is_durable_and_original_metadata_is_immutable() {
    let (_dir, mut ws, task, result) = prepared();
    ws.import_result(&task, &result.to_string()).unwrap();
    let checked = ws.preview_current(&task).unwrap();
    let current = serde_json::to_value(checked.current_version.as_ref().unwrap()).unwrap();
    assert_eq!(current["gpt_import"]["visual_plan_status"], "ignored");
    let repeated = ws.preview_current(&task).unwrap();
    assert_eq!(
        serde_json::to_value(repeated.current_version.unwrap()).unwrap()["gpt_import"],
        current["gpt_import"]
    );
    let mut forged = serde_json::to_value(&checked.project).unwrap();
    let history = forged["tasks"][0]["versions"].as_array_mut().unwrap();
    history.last_mut().unwrap()["gpt_import"]["visual_plan"]["notes"] =
        serde_json::json!(["rewritten"]);
    let project = serde_json::from_value(forged).unwrap();
    assert!(entwurf_core::storage::save_project(
        &entwurf_core::storage::ProjectPaths::new(&checked.root),
        &project
    )
    .is_err());
    for field in ["warnings", "image_reference_applied"] {
        let mut forged = serde_json::to_value(&checked.project).unwrap();
        let history = forged["tasks"][0]["versions"].as_array_mut().unwrap();
        history.last_mut().unwrap()["gpt_import"][field] = if field == "warnings" {
            serde_json::json!([])
        } else {
            serde_json::json!(true)
        };
        assert!(entwurf_core::storage::save_project(
            &entwurf_core::storage::ProjectPaths::new(&checked.root),
            &serde_json::from_value(forged).unwrap()
        )
        .is_err());
    }
}

#[test]
fn valid_page_advice_drives_preview_export_and_version_rollback() {
    let (_dir, mut ws, task, mut result) = prepared();
    let mut style = ws.snapshot(Some(&task)).unwrap().project.style;
    style.paper.max_pages = 2;
    ws.save_style(style).unwrap();
    let baseline = ws.prepare_handoff(&task).unwrap().current_version.unwrap();
    result["revision"] = baseline.revision.into();
    result["visual_plan"] = serde_json::json!({"page_count":2,"groups":baseline.ir.nodes.iter().enumerate().map(|(i,n)|serde_json::json!({"id":format!("g{i}"),"title":format!("Group {i}"),"node_ids":[n.id],"page":i+1})).collect::<Vec<_>>(),"notes":[]});
    ws.import_result(&task, &result.to_string()).unwrap();
    let prepared = ws.preview_current(&task).unwrap();
    assert_eq!(prepared.preview_pages.len(), 2);
    let v = prepared.current_version.unwrap();
    assert_eq!(
        v.gpt_import.as_ref().unwrap().visual_plan_status,
        entwurf_core::project::VisualPlanStatus::Applied
    );
    let exported = ws
        .export_current(&task, None)
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(exported.artifacts.as_ref().unwrap().svg.len(), 2);
    assert_eq!(
        serde_json::to_value(&exported.gpt_import).unwrap(),
        serde_json::to_value(&v.gpt_import).unwrap()
    );
    let rolled = ws
        .rollback_version(&task, &v.id)
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(
        rolled.gpt_import.as_ref().unwrap().visual_plan_status,
        entwurf_core::project::VisualPlanStatus::NotEvaluated
    );
    assert_eq!(
        serde_json::to_value(&rolled.ir.nodes).unwrap(),
        serde_json::to_value(&exported.ir.nodes).unwrap()
    );
    assert_eq!(ws.preview_current(&task).unwrap().preview_pages.len(), 2);
}

#[test]
fn confirming_grounded_gpt_formula_keeps_visual_plan_until_real_preview() {
    let dir = tempfile::tempdir().unwrap();
    let mut ws = Workspace::create(dir.path().join("project"), "Formula plan").unwrap();
    let task = ws
        .create_task("Formula chapter", None)
        .unwrap()
        .selected_task_id
        .unwrap();
    let source = dir.path().join("source.md");
    std::fs::write(&source, "# Chapter\n\n$x=1$\n").unwrap();
    ws.import_sources(&task, &[source], &IngestOptions::standard())
        .unwrap();
    loop {
        let v = ws.snapshot(Some(&task)).unwrap().current_version.unwrap();
        let Some(item) = v.ir.unverified.first() else {
            break;
        };
        ws.resolve_review(&task, &v.id, &item.id, "Checked original formula")
            .unwrap();
    }
    let baseline = ws.prepare_handoff(&task).unwrap().current_version.unwrap();
    let mut formula = baseline
        .ir
        .nodes
        .iter()
        .find(|n| n.latex.is_some())
        .unwrap()
        .clone();
    formula.id = "gpt-grounded-formula".into();
    formula.title = "Grounded GPT formula".into();
    formula.origin = entwurf_core::NodeOrigin::GptCandidate;
    formula.verified = false;
    formula.review_note = None;
    let result = serde_json::json!({"schema_version":1,"task_id":task,"revision":baseline.revision,
        "ir_patch":{"add_nodes":[formula],"update_nodes":[],"add_edges":[],"remove_node_ids":[]},
        "visual_plan":{"page_count":2,"groups":[{"id":"original","title":"Original","node_ids":baseline.ir.nodes.iter().map(|n|&n.id).collect::<Vec<_>>(),"page":1},{"id":"proposed","title":"Proposed","node_ids":[formula.id],"page":2}],"notes":[]},
        "corrections":[],"image_path":"/untrusted/reference.png","notes":[]});
    let imported = ws
        .import_result(&task, &result.to_string())
        .unwrap()
        .current_version
        .unwrap();
    assert_eq!(imported.ir.unverified.len(), 1);
    let checked = ws
        .resolve_review(
            &task,
            &imported.id,
            &imported.ir.unverified[0].id,
            "Compared exact source equation",
        )
        .unwrap();
    let reviewed = checked.current_version.unwrap();
    assert!(
        reviewed.gpt_import.is_some(),
        "review-only confirmation must retain the proposal"
    );
    assert_eq!(
        serde_json::to_value(&reviewed.gpt_import).unwrap(),
        serde_json::to_value(&imported.gpt_import).unwrap()
    );
    assert_eq!(reviewed.source_prompt_hash, imported.source_prompt_hash);
    assert!(
        reviewed.layout.is_none() && reviewed.validation.is_none() && reviewed.artifacts.is_none()
    );
    assert!(reviewed.layout_options.is_none() && reviewed.visual_repairs.is_none());
    let mut reopened = Workspace::open(&checked.root).unwrap();
    let preview = reopened.preview_current(&task).unwrap();
    assert_eq!(preview.preview_pages.len(), 2);
    assert_eq!(
        preview
            .current_version
            .unwrap()
            .gpt_import
            .unwrap()
            .visual_plan_status,
        entwurf_core::project::VisualPlanStatus::Applied
    );
    assert_eq!(
        checked
            .project
            .task(&task)
            .unwrap()
            .versions
            .iter()
            .find(|v| v.id == imported.id)
            .unwrap()
            .ir
            .unverified
            .len(),
        1
    );
}

#[test]
fn substantive_edits_discard_proposal_with_explicit_audit() {
    let (_dir, mut ws, task, result) = prepared();
    let imported = ws
        .import_result(&task, &result.to_string())
        .unwrap()
        .current_version
        .unwrap();
    let mut edit = NodeEdit::from(&imported.ir.nodes[0]);
    edit.title = "Changed after GPT import".into();
    let changed = ws
        .update_nodes(&task, &imported.id, vec![edit], None)
        .unwrap();
    assert!(changed.current_version.unwrap().gpt_import.is_none());
    assert!(changed
        .project
        .audit
        .iter()
        .any(|a| a.detail.contains("GPT visual proposal invalidated")
            && a.detail.contains(&imported.id)));
    ws.rollback_version(&task, &imported.id).unwrap();
    let mut style = ws.snapshot(Some(&task)).unwrap().project.style;
    style.typography.body_pt += 1.0;
    let changed = ws.save_style(style).unwrap();
    assert!(changed.current_version.unwrap().gpt_import.is_none());
    assert!(changed.project.audit.iter().any(|a| a
        .detail
        .contains("GPT visual proposal invalidated by style change")));
}
