#[cfg(test)]
mod tests {
    use entwurf_core::{workspace::Workspace, *};
    use std::fs;
    fn task(ws: &mut Workspace, root: &std::path::Path, name: &str) -> String {
        let id = ws
            .create_task(name, None)
            .unwrap()
            .selected_task_id
            .unwrap();
        let path = root.join(format!("{name}.md"));
        fs::write(&path, format!("# {name}\n\nGrounded source text.\n")).unwrap();
        ws.import_sources(&id, &[path], &ingest::IngestOptions::standard())
            .unwrap();
        id
    }
    fn result(ws: &Workspace, id: &str) -> serde_json::Value {
        let v = ws.snapshot(Some(id)).unwrap().current_version.unwrap();
        serde_json::json!({"schema_version":1,"task_id":id,"revision":v.revision,"ir_patch":{"add_nodes":[],"update_nodes":[],"add_edges":[],"remove_node_ids":[]},"visual_plan":{"page_count":1,"groups":[],"notes":[]},"corrections":[],"image_path":null,"notes":[]})
    }
    #[test]
    fn exports_of_two_tasks_to_same_directory_must_not_open_wrong_ir() {
        let d = tempfile::tempdir().unwrap();
        let mut ws = Workspace::create(d.path().join("project"), "Review").unwrap();
        let a = task(&mut ws, d.path(), "Alpha");
        let out = d.path().join("out");
        let exported = ws.export_current(&a, Some(out.clone())).unwrap();
        assert!(exported.current_version.unwrap().artifacts.is_some());
        let b = task(&mut ws, d.path(), "Beta");
        ws.export_current(&b, Some(out)).unwrap();
        let path = ws.artifact_path(&a, "ir", 0).unwrap();
        let opened: KnowledgeIr = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(opened.task_id, a, "opening Alpha returns Beta's artifact");
    }
    #[test]
    fn new_gpt_inline_formula_must_require_source_proof_and_review() {
        let d = tempfile::tempdir().unwrap();
        let mut ws = Workspace::create(d.path().join("project"), "Review").unwrap();
        let id = task(&mut ws, d.path(), "Alpha");
        let v = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
        let mut node = v.ir.nodes[1].clone();
        node.id = "gpt-inline".into();
        node.title = "Invented equation".into();
        node.body = "The answer is $x=987654$.".into();
        node.latex = None;
        node.kind = NodeKind::Concept;
        node.origin = NodeOrigin::GptCandidate;
        let mut r = result(&ws, &id);
        r["ir_patch"]["add_nodes"] = serde_json::json!([node]);
        ws.prepare_handoff(&id).unwrap();
        let imported = ws.import_result(&id, &r.to_string()).unwrap();
        assert_eq!(imported.current_version.unwrap().ir.nodes.len(), 3);
        let checked = ws.preview_current(&id).unwrap();
        assert!(
            !checked
                .current_version
                .unwrap()
                .validation
                .unwrap()
                .is_passing(),
            "invented inline formula was approved without exact source or review"
        );
    }
    #[test]
    fn formula_bearing_body_correction_must_validate_correction_anchor() {
        let d = tempfile::tempdir().unwrap();
        let mut ws = Workspace::create(d.path().join("project"), "Review").unwrap();
        let id = task(&mut ws, d.path(), "Alpha");
        let v = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
        let node = &v.ir.nodes[1];
        let mut anchors = node.anchors.clone();
        anchors[0].quote_hash = Some("0".repeat(64));
        let mut r = result(&ws, &id);
        r["corrections"] = serde_json::json!([{"node_id":node.id,"field":"body","before":node.body,"after":"The answer is $x=987654$.","reason":"Source says so","anchors":anchors}]);
        ws.prepare_handoff(&id).unwrap();
        ws.import_result(&id, &r.to_string()).unwrap();
        let checked = ws.preview_current(&id).unwrap();
        assert!(
            !checked
                .current_version
                .unwrap()
                .validation
                .unwrap()
                .is_passing(),
            "formula-bearing correction with invalid quote hash was approved"
        );
    }

    #[test]
    fn every_rendered_inline_delimiter_in_title_and_body_requires_review_and_exact_proof() {
        for expression in [
            "$x=987654$",
            "$$x=987654$$",
            r"\(x=987654\)",
            r"\[x=987654\]",
        ] {
            for field in ["title", "body"] {
                let d = tempfile::tempdir().unwrap();
                let mut ws = Workspace::create(d.path().join("project"), "Review").unwrap();
                let id = task(&mut ws, d.path(), "Alpha");
                let v = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
                let mut node = v.ir.nodes[1].clone();
                node.id = "new-inline".into();
                node.latex = None;
                node.kind = NodeKind::Concept;
                if field == "title" {
                    node.title = format!("Answer {expression}");
                } else {
                    node.body = format!("Answer {expression}");
                }
                let mut r = result(&ws, &id);
                r["ir_patch"]["add_nodes"] = serde_json::json!([node]);
                ws.prepare_handoff(&id).unwrap();
                let imported = ws
                    .import_result(&id, &r.to_string())
                    .unwrap()
                    .current_version
                    .unwrap();
                let pending = imported
                    .ir
                    .unverified
                    .iter()
                    .find(|u| u.node_id.as_deref() == Some("new-inline"))
                    .expect("new rendered formula must enter review");
                assert!(
                    ws.resolve_review(&id, &imported.id, &pending.id, "Reviewed")
                        .is_err(),
                    "unproven math cannot be acknowledged: {field} {expression}"
                );
                let mut ir = imported.ir;
                ir.unverified.clear();
                let report =
                    validate::validate_ir(&ir, &validate::ValidationOptions::default()).unwrap();
                assert!(
                    report
                        .findings
                        .iter()
                        .any(|f| f.code == "unaudited_formula"),
                    "queue clearing cannot hide {field} {expression}"
                );
            }
        }
    }
    #[test]
    fn title_and_body_corrections_require_real_evidence_but_allow_complete_audits() {
        for field in ["title", "body"] {
            let d = tempfile::tempdir().unwrap();
            let mut ws = Workspace::create(d.path().join("project"), "Review").unwrap();
            let id = task(&mut ws, d.path(), "Alpha");
            let v = ws.snapshot(Some(&id)).unwrap().current_version.unwrap();
            let node = &v.ir.nodes[1];
            let mut r = result(&ws, &id);
            let before = if field == "title" {
                &node.title
            } else {
                &node.body
            };
            let mut anchors = node.anchors.clone();
            anchors[0].quote_hash = Some("0".repeat(64));
            r["corrections"] = serde_json::json!([{"node_id":node.id,"field":field,"before":before,"after":"Answer $x=2$","reason":"Explicit source correction","anchors":anchors}]);
            ws.prepare_handoff(&id).unwrap();
            let imported = ws
                .import_result(&id, &r.to_string())
                .unwrap()
                .current_version
                .unwrap();
            let report =
                validate::validate_ir(&imported.ir, &validate::ValidationOptions::default())
                    .unwrap();
            assert!(report
                .findings
                .iter()
                .any(|f| f.code == "invalid_correction_evidence"));
            ws.rollback_version(&id, &v.id).unwrap();
            let current = ws.prepare_handoff(&id).unwrap().current_version.unwrap();
            r["revision"] = current.revision.into();
            r["corrections"][0]["anchors"] = serde_json::to_value(&node.anchors).unwrap();
            let corrected = ws
                .import_result(&id, &r.to_string())
                .unwrap()
                .current_version
                .unwrap();
            assert!(
                validate::validate_ir(&corrected.ir, &validate::ValidationOptions::default())
                    .unwrap()
                    .is_passing(),
                "valid audited {field} correction remains usable"
            );
        }
    }
}
