//! Embeds the canonical JSON Schemas used for offline contract validation.
//! Runtime Rust declarations and TypeScript types are generated from these same
//! schemas. Behavioral implementations remain in their original Rust modules.

use jsonschema::{Draft, Validator};
use serde_json::Value;

use crate::error::{EntwurfError, Result};
use crate::ir::{KnowledgeIr, ResultEnvelope, StyleSpec, TaskEnvelope};

const SCHEMAS: &[(&str, &str)] = &[
    (
        "project.schema.json",
        include_str!("../../../packages/contracts/schemas/project.schema.json"),
    ),
    (
        "knowledge-ir.schema.json",
        include_str!("../../../packages/contracts/schemas/knowledge-ir.schema.json"),
    ),
    (
        "style.schema.json",
        include_str!("../../../packages/contracts/schemas/style.schema.json"),
    ),
    (
        "task-envelope.schema.json",
        include_str!("../../../packages/contracts/schemas/task-envelope.schema.json"),
    ),
    (
        "result-envelope.schema.json",
        include_str!("../../../packages/contracts/schemas/result-envelope.schema.json"),
    ),
];

pub fn load_schema(name: &str) -> Result<Value> {
    let (_, schema) = SCHEMAS
        .iter()
        .find(|(key, _)| *key == name)
        .ok_or_else(|| EntwurfError::Contract(format!("unknown schema {name}")))?;
    serde_json::from_str(schema).map_err(|error| EntwurfError::Contract(error.to_string()))
}

pub fn validator(name: &str) -> Result<Validator> {
    let schema = load_schema(name)?;
    let mut options = Validator::options()
        .with_draft(Draft::Draft202012)
        .should_validate_formats(true);
    for (key, _) in SCHEMAS {
        let resource = jsonschema::Resource::from_contents(load_schema(key)?)
            .map_err(|error| EntwurfError::Contract(error.to_string()))?;
        options = options.with_resource(format!("https://entwurf.local/schemas/{key}"), resource);
    }
    options
        .build(&schema)
        .map_err(|error| EntwurfError::Contract(format!("{name}: {error}")))
}

pub fn validate_ir(ir: &KnowledgeIr) -> Result<()> {
    validate_json(
        "knowledge-ir.schema.json",
        &serde_json::to_value(ir).map_err(|error| EntwurfError::Contract(error.to_string()))?,
    )
}

pub fn validate_style(style: &StyleSpec) -> Result<()> {
    validate_json(
        "style.schema.json",
        &serde_json::to_value(style).map_err(|error| EntwurfError::Contract(error.to_string()))?,
    )
}

pub fn validate_task_envelope(envelope: &TaskEnvelope) -> Result<()> {
    validate_json(
        "task-envelope.schema.json",
        &serde_json::to_value(envelope)
            .map_err(|error| EntwurfError::Contract(error.to_string()))?,
    )
}

pub fn validate_result_envelope(result: &ResultEnvelope) -> Result<()> {
    validate_json(
        "result-envelope.schema.json",
        &serde_json::to_value(result).map_err(|error| EntwurfError::Contract(error.to_string()))?,
    )
}

pub fn validate_json(name: &str, value: &Value) -> Result<()> {
    let validator = validator(name)?;
    let errors = validator
        .iter_errors(value)
        .map(|error| error.to_string())
        .collect::<Vec<_>>();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(EntwurfError::Contract(format!(
            "{name} validation failed: {}",
            errors.join("; ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Importance, KnowledgeNode, NodeKind, NodeOrigin, SourceAnchor, SourceFile, SourceKind,
    };

    fn sample() -> KnowledgeIr {
        let mut ir = KnowledgeIr::empty("p", "t", "Probability");
        ir.sources.push(SourceFile {
            id: "s".into(),
            original_name: "s.md".into(),
            kind: SourceKind::Markdown,
            content_hash: "a".repeat(64),
            stored_path: "sources/s.md".into(),
            byte_len: 10,
            imported_at: chrono::Utc::now(),
            notes: vec![],
        });
        ir.nodes.push(KnowledgeNode {
            id: "n".into(),
            kind: NodeKind::Concept,
            title: "Concept".into(),
            body: "Body".into(),
            latex: None,
            anchors: vec![SourceAnchor {
                source_id: "s".into(),
                page: None,
                line: Some(1),
                char_start: None,
                char_end: None,
                quote_hash: None,
                label: "s:1".into(),
                region: None,
            }],
            importance: Importance::Critical,
            origin: NodeOrigin::Source,
            parent_id: None,
            tags: vec![],
            verified: false,
            review_note: None,
        });
        ir
    }

    #[test]
    fn result_patch_and_visual_plan_fields_are_validated() {
        let value = serde_json::json!({
            "schema_version": 1, "task_id": "t", "revision": 1,
            "ir_patch": { "add_nodes": [], "update_nodes": [ {"id": "n", "importance": "invented"} ], "add_edges": [], "remove_node_ids": [] },
            "visual_plan": { "page_count": 0, "groups": [], "notes": [] },
            "corrections": [], "image_path": null, "notes": []
        });
        assert!(validate_json("result-envelope.schema.json", &value).is_err());
    }

    #[test]
    fn every_embedded_schema_resolves_offline() {
        for (name, _) in SCHEMAS {
            validator(name).unwrap();
        }
        assert!(load_schema("../../outside.json").is_err());
    }

    #[test]
    fn generated_types_match_embedded_schema_manifest() {
        use sha2::{Digest, Sha256};
        let mut sorted = SCHEMAS.to_vec();
        sorted.sort_by_key(|(name, _)| *name);
        let mut hasher = Sha256::new();
        for (name, source) in sorted {
            hasher.update(format!("{name}\n{source}\n"));
        }
        let manifest: Value = serde_json::from_str(include_str!(
            "../../../packages/contracts/generated/manifest.json"
        ))
        .unwrap();
        assert_eq!(
            manifest["schema_sha256"],
            format!("{:x}", hasher.finalize()),
            "run node packages/contracts/generate.mjs"
        );
        assert_eq!(
            manifest["types_sha256"],
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../../packages/contracts/generated/types.ts"
                ))
            )
        );
        assert_eq!(
            manifest["generator_sha256"],
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../../packages/contracts/generate.mjs"))
            )
        );
        for (name, source) in [
            (
                "ir.rs",
                include_str!("../../../packages/contracts/generated/ir.rs"),
            ),
            (
                "model.rs",
                include_str!("../../../packages/contracts/generated/model.rs"),
            ),
            (
                "project.rs",
                include_str!("../../../packages/contracts/generated/project.rs"),
            ),
        ] {
            assert_eq!(
                manifest["rust_sha256"][name],
                format!("{:x}", Sha256::digest(source)),
                "regenerate {name}"
            );
        }
    }

    fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned>(
        schema: &str,
        value: &T,
    ) -> Value {
        let json = serde_json::to_value(value).unwrap();
        validate_json(schema, &json).unwrap();
        let restored: T = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(serde_json::to_value(restored).unwrap(), json);
        json
    }

    #[test]
    fn generated_runtime_contracts_roundtrip_complete_project_and_envelopes() {
        use crate::ir::{
            Correction, CorrectionField, EdgeKind, GraphEdge, RegionUnit, SourceRegion,
            UnverifiedItem, UnverifiedKind,
        };
        use crate::model::{
            LayoutDocument, LayoutPage, PlacedEdge, PlacedGroup, PlacedNode, Point, Rect, TextLine,
        };
        use crate::project::{ArtifactPaths, Project, VersionReason};
        let mut ir = sample();
        // Integers must not silently pass through a lossy floating-point stand-in.
        ir.sources[0].byte_len = 9_007_199_254_740_993;
        ir.nodes[0].latex = Some("P(A)=1".into());
        ir.nodes[0].anchors[0].region = Some(SourceRegion {
            left: 12_500,
            top: 20_000,
            width: 30_000,
            height: 5_000,
            unit: RegionUnit::PdfMillipoints,
        });
        let anchor = ir.nodes[0].anchors[0].clone();
        ir.edges.push(GraphEdge {
            id: "edge-identity".into(),
            from: "n".into(),
            to: "n".into(),
            edge_type: EdgeKind::DependsOn,
            label: Some("dependency".into()),
            anchors: vec![anchor.clone()],
        });
        ir.corrections.push(Correction {
            id: "correction-identity".into(),
            node_id: "n".into(),
            field: CorrectionField::Latex,
            before: "P(A)=0".into(),
            after: "P(A)=1".into(),
            reason: "source correction".into(),
            anchors: vec![anchor],
            accepted: true,
            reverted: false,
            created_at: ir.updated_at,
        });
        ir.unverified.push(UnverifiedItem {
            id: "review-identity".into(),
            node_id: Some("n".into()),
            kind: UnverifiedKind::Formula,
            text: "P(A)=1".into(),
            reason: "confirm transcription".into(),
        });
        let json = roundtrip("knowledge-ir.schema.json", &ir);
        assert_eq!(json["edges"][0]["type"], "depends_on");
        assert_eq!(
            json["sources"][0]["byte_len"].as_u64(),
            Some(9_007_199_254_740_993)
        );
        assert_eq!(
            json["nodes"][0]["anchors"][0]["region"]["unit"],
            "pdf_millipoints"
        );

        let style = StyleSpec::default();
        roundtrip("style.schema.json", &style);
        let envelope = crate::prompt::build_task_envelope(&ir, &style, 17, "source");
        let json = roundtrip("task-envelope.schema.json", &envelope);
        assert_eq!(json["revision"], 17);
        assert_eq!(json["requested_artifact"]["kind"], "knowledge_sheet");
        assert_eq!(json["requested_artifact"]["render_mode"], "native_svg");
        let result_json = serde_json::json!({
            "schema_version": 1, "task_id": "t", "revision": 17,
            "ir_patch": { "add_nodes": ir.nodes, "update_nodes": [{"id":"n","title":"Updated","body":null,"latex":"P(A)=1","importance":"high"}], "add_edges": ir.edges, "remove_node_ids": ["removed-identity"] },
            "visual_plan": { "page_count": 1, "groups": [{"id":"g","title":"Group","node_ids":["n"],"page":1}], "notes": ["layout note"] },
            "corrections": [{"node_id":"n","field":"body","before":"old","after":"new","reason":"source","anchors":[]}],
            "image_path": "imports/result.png", "notes": ["review"]
        });
        let result = crate::prompt::parse_result_envelope(&result_json.to_string()).unwrap();
        assert_eq!(
            roundtrip("result-envelope.schema.json", &result),
            result_json
        );

        let layout = LayoutDocument {
            width_pt: 100.5,
            height_pt: 200.25,
            margin_pt: 8.0,
            diagnostics: vec!["layout diagnostic".into()],
            pages: vec![LayoutPage {
                index: 0,
                width_pt: 100.5,
                height_pt: 200.25,
                margin_pt: 8.0,
                groups: vec![PlacedGroup {
                    id: "g".into(),
                    title: "Group".into(),
                    page: 0,
                    rect: Rect::new(8.0, 8.0, 80.0, 80.0),
                }],
                nodes: vec![PlacedNode {
                    node_id: "n".into(),
                    kind: NodeKind::Concept,
                    title: "Title".into(),
                    body: "Body".into(),
                    latex: Some("P(A)=1".into()),
                    page: 0,
                    rect: Rect::new(10.0, 10.0, 50.0, 40.0),
                    title_lines: vec![TextLine {
                        text: "Title".into(),
                        x: 10.0,
                        baseline: 20.0,
                        font_size: 10.5,
                        bold: true,
                    }],
                    body_lines: vec![],
                    latex_lines: vec![],
                    text_clipped: false,
                    measured_content_rect: None,
                    measured_font_pt: None,
                }],
                edges: vec![PlacedEdge {
                    edge_id: "edge-identity".into(),
                    from_node: "n".into(),
                    to_node: "n".into(),
                    page: 0,
                    points: vec![Point::new(10.0, 20.0), Point::new(20.0, 30.0)],
                    label: None,
                }],
            }],
        };
        assert_eq!(layout.node_count(), 1);
        let mut report = crate::ir::ValidationReport::new();
        report.push("review", crate::ir::Severity::Warning, "confirm formula");
        report.metrics.insert("precision".into(), 0.123456789012345);
        let mut project = Project::new("Complete project");
        let task_id = project.add_task("Chapter", Some("1".into())).id.clone();
        let version_id = project
            .save_version(&task_id, ir, VersionReason::UserEdited)
            .unwrap();
        project
            .attach_version_outputs(
                &task_id,
                &version_id,
                layout,
                report,
                ArtifactPaths {
                    svg: vec!["page.svg".into()],
                    png: vec!["page.png".into()],
                    pdf: "page.pdf".into(),
                    manifest: "manifest.json".into(),
                    ir: "knowledge.ir.json".into(),
                },
            )
            .unwrap();
        project
            .transition(&task_id, crate::ir::TaskState::Ingesting)
            .unwrap();
        project
            .transition(&task_id, crate::ir::TaskState::Paused)
            .unwrap();
        let json = roundtrip("project.schema.json", &project);
        assert_eq!(json["tasks"][0]["id"], task_id);
        assert_eq!(json["tasks"][0]["current_version"], version_id);
        assert_eq!(json["tasks"][0]["paused_from"], "INGESTING");
        assert_eq!(json["tasks"][0]["state"], "PAUSED");
        assert_eq!(
            json["tasks"][0]["versions"][0]["validation"]["metrics"]["precision"],
            0.123456789012345
        );
    }

    #[test]
    fn generated_models_reopen_legacy_immutable_snapshots_without_rewriting_ir() {
        use crate::project::{Project, VersionReason};
        use crate::storage::{load_project, save_project, ProjectPaths, IR_FILE};
        let temp = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(temp.path());
        paths.ensure().unwrap();
        let mut project = Project::new("Legacy snapshot");
        let task_id = project.add_task("Chapter", None).id.clone();
        let mut ir = sample();
        ir.project_id = project.id.clone();
        ir.task_id = task_id.clone();
        let version_id = project
            .save_version(&task_id, ir.clone(), VersionReason::Ingested)
            .unwrap();
        // Pre-region snapshots used the same ordered serde fields, with no region key.
        let legacy_bytes = |text: String| {
            regex::Regex::new(r#",\n[ \t]*"region": null"#)
                .unwrap()
                .replace_all(&text, "")
                .into_owned()
        };
        let old_ir = legacy_bytes(serde_json::to_string_pretty(&ir).unwrap());
        assert!(!old_ir.contains("\"region\""));
        let old_project = legacy_bytes(serde_json::to_string_pretty(&project).unwrap());
        std::fs::write(paths.project_file(), old_project).unwrap();
        let ir_path = paths
            .versions_dir()
            .join(&task_id)
            .join(&version_id)
            .join(IR_FILE);
        std::fs::create_dir_all(ir_path.parent().unwrap()).unwrap();
        std::fs::write(&ir_path, &old_ir).unwrap();
        let reopened = load_project(&paths).unwrap();
        assert_eq!(
            reopened.tasks[0].versions[0].ir.nodes[0].anchors[0].region,
            None
        );
        save_project(&paths, &reopened).unwrap();
        assert_eq!(std::fs::read_to_string(&ir_path).unwrap(), old_ir);
        std::fs::remove_file(paths.database_file()).unwrap();
        let rebuilt = load_project(&paths).unwrap();
        assert_eq!(rebuilt.tasks[0].versions[0].id, version_id);
        assert_eq!(std::fs::read_to_string(&ir_path).unwrap(), old_ir);
    }

    #[test]
    fn generated_types_preserve_optional_fields_and_native_ranges() {
        let legacy = serde_json::json!({"source_id":"stable-source","label":"page 1"});
        let anchor: SourceAnchor = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(anchor.region, None);
        assert_eq!(anchor.page, None);
        let full = serde_json::to_value(&anchor).unwrap();
        assert!(full["region"].is_null());
        assert_eq!(
            serde_json::from_value::<SourceAnchor>(full).unwrap(),
            anchor
        );
        for value in [
            serde_json::json!(-1),
            serde_json::json!(4_294_967_296_u64),
            serde_json::json!(1.5),
            serde_json::json!("1"),
        ] {
            let mut invalid = legacy.clone();
            invalid["page"] = value;
            assert!(serde_json::from_value::<SourceAnchor>(invalid).is_err());
        }
        let mut ir = serde_json::to_value(sample()).unwrap();
        ir.as_object_mut().unwrap().remove("schema_version");
        assert_eq!(
            serde_json::from_value::<KnowledgeIr>(ir.clone())
                .unwrap()
                .schema_version,
            1
        );
        // Import validation still rejects missing required version before serde defaults.
        assert!(validate_json("knowledge-ir.schema.json", &ir).is_err());
        let mut envelope = serde_json::to_value(crate::prompt::build_task_envelope(
            &sample(),
            &StyleSpec::default(),
            1,
            "source",
        ))
        .unwrap();
        envelope.as_object_mut().unwrap().remove("task_version");
        assert!(serde_json::from_value::<TaskEnvelope>(envelope.clone())
            .unwrap()
            .task_version
            .is_none());
        envelope["requested_artifact"]["pages"] = serde_json::json!(65536);
        assert!(serde_json::from_value::<TaskEnvelope>(envelope).is_err());
    }

    #[test]
    fn generated_types_reject_unknown_enums_fields_and_wrong_value_types() {
        let ir = serde_json::to_value(sample()).unwrap();
        for (pointer, value) in [
            ("/nodes/0/kind", serde_json::json!("invented")),
            ("/nodes/0/importance", serde_json::json!("urgent")),
            ("/nodes/0/verified", serde_json::json!("true")),
            ("/sources/0/imported_at", serde_json::json!("yesterday")),
            ("/nodes/0/tags", serde_json::json!([7])),
        ] {
            let mut invalid = ir.clone();
            *invalid.pointer_mut(pointer).unwrap() = value;
            assert!(
                serde_json::from_value::<KnowledgeIr>(invalid.clone()).is_err(),
                "{pointer}"
            );
            assert!(
                validate_json("knowledge-ir.schema.json", &invalid).is_err(),
                "{pointer}"
            );
        }
        let mut unknown_field = ir;
        unknown_field["nodes"][0]["invented"] = true.into();
        assert!(serde_json::from_value::<KnowledgeIr>(unknown_field).is_err());
        assert!(serde_json::from_value::<crate::ir::TaskState>("paused".into()).is_err());
        assert!(serde_json::from_value::<crate::ir::NodePatch>(
            serde_json::json!({"id":"n","importance":"urgent"})
        )
        .is_err());
        assert!(serde_json::from_value::<crate::ir::RegionUnit>("points".into()).is_err());
    }

    #[test]
    fn rust_ir_matches_schema() {
        validate_ir(&sample()).unwrap();
    }

    #[test]
    fn invalid_enum_is_rejected_by_schema() {
        let mut value = serde_json::to_value(sample()).unwrap();
        value["nodes"][0]["kind"] = "made_up_kind".into();
        assert!(validate_json("knowledge-ir.schema.json", &value).is_err());
    }
}
