//! Embeds the canonical JSON Schemas used for offline contract validation.
//! TypeScript is generated from these schemas; hand-authored Rust serde models
//! have sampled compatibility tests. Generated Rust and exhaustive parity remain
//! pending.

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
