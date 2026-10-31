//! Deterministic construction and validation of the GPT task/result contract.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::error::{EntwurfError, Result};
use crate::ir::{
    new_id, ArtifactKind, Correction, CorrectionField, CorrectionProposal, KnowledgeIr,
    KnowledgeNode, NodeOrigin, RenderMode, ResultEnvelope, SourceAnchor, StyleSpec, TaskEnvelope,
    ENVELOPE_SCHEMA_VERSION,
};

pub const RESULT_SCHEMA: &str = r#"{
  "schema_version": 1,
  "task_id": "string",
  "revision": 1,
  "ir_patch": {
    "add_nodes": [{
      "id": "string", "kind": "concept|definition|formula|theorem|example|warning|note|chapter|section",
      "title": "string", "body": "string", "latex": null,
      "anchors": [{ "source_id": "string", "page": null, "line": null, "char_start": null, "char_end": null, "quote_hash": null, "label": "string" }],
      "importance": "critical|high|normal|low", "origin": "gpt_candidate",
      "parent_id": null, "tags": [], "verified": false, "review_note": null
    }],
    "update_nodes": [{ "id": "string", "title": null, "body": null, "latex": null, "importance": null }],
    "add_edges": [{ "id": "string", "from": "string", "to": "string", "type": "contains|depends_on|related|example_of|contradicts", "label": null, "anchors": [] }],
    "remove_node_ids": []
  },
  "visual_plan": { "page_count": 1, "groups": [{ "id": "string", "title": "string", "node_ids": [], "page": 1 }], "notes": [] },
  "corrections": [{ "node_id": "string", "field": "title|body|latex|anchors", "before": "string", "after": "string", "reason": "string", "anchors": [] }],
  "image_path": null,
  "notes": []
}"#;

pub const TASK_RULES: &[&str] = &[
    "Use only facts supported by the supplied sources; never invent new knowledge claims.",
    "Every critical node and every chapter or section node must carry at least one source anchor.",
    "Preserve formulas exactly. Put any change in corrections with before, after, reason and anchors.",
    "Return one JSON object that conforms to response_schema, without Markdown fences.",
    "Keep the requested task_id and revision unchanged.",
    "Keep the output within the requested A3 page budget.",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptBundle {
    pub envelope: TaskEnvelope,
    pub envelope_json: String,
    pub markdown: String,
}

pub fn build_task_envelope(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    revision: u32,
    source_excerpt: impl Into<String>,
) -> TaskEnvelope {
    let mut rules: Vec<String> = TASK_RULES.iter().map(|rule| rule.to_string()).collect();
    rules.extend(style.rules.iter().cloned());
    TaskEnvelope {
        schema_version: ENVELOPE_SCHEMA_VERSION,
        task_version: Some(format!("{}:r{}", ir.task_id, revision)),
        task_id: ir.task_id.clone(),
        project_id: ir.project_id.clone(),
        revision,
        title: ir.title.clone(),
        locale: ir.language.clone(),
        source_excerpt: source_excerpt.into(),
        baseline_ir: ir.clone(),
        style: style.clone(),
        requested_artifact: crate::ir::RequestedArtifact {
            kind: ArtifactKind::KnowledgeSheet,
            pages: style.paper.max_pages,
            render_mode: RenderMode::NativeSvg,
        },
        response_schema: RESULT_SCHEMA.to_string(),
        rules,
    }
}

pub fn build_prompt(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    revision: u32,
    source_excerpt: impl Into<String>,
) -> Result<PromptBundle> {
    let envelope = build_task_envelope(ir, style, revision, source_excerpt);
    let envelope_json = serde_json::to_string_pretty(&envelope)
        .map_err(|error| EntwurfError::Contract(error.to_string()))?;
    let markdown = format!(
        "# Entwurf task {}\n\n\
         You are the knowledge planner for a deterministic local compiler. Compile the supplied \
         source into a traceable Knowledge IR patch and a page/group visual plan. Do not answer \
         the exercises or add knowledge that is absent from the sources.\n\n\
         ## Response contract\n\n\
         Return exactly one JSON object matching `response_schema` in the envelope. Do not wrap \
         it in Markdown. If you find a source error, propose a correction; the local app records \
         the original text and can undo it.\n\n\
         ## Task envelope\n\n```json\n{}\n```\n",
        envelope.task_id, envelope_json
    );
    Ok(PromptBundle {
        envelope,
        envelope_json,
        markdown,
    })
}

pub fn envelope_sha256(envelope: &TaskEnvelope) -> Result<String> {
    use sha2::{Digest, Sha256};
    let json =
        serde_json::to_vec(envelope).map_err(|error| EntwurfError::Contract(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(json)))
}

pub fn parse_result_envelope(input: &str) -> Result<ResultEnvelope> {
    let cleaned = strip_code_fence(input.trim());
    let value: serde_json::Value = serde_json::from_str(cleaned).map_err(|error| {
        EntwurfError::Contract(format!("result envelope is not valid JSON: {error}"))
    })?;
    crate::contracts::validate_json("result-envelope.schema.json", &value)?;
    serde_json::from_value(value).map_err(|error| EntwurfError::Contract(error.to_string()))
}

fn strip_code_fence(input: &str) -> &str {
    if !input.starts_with("```") {
        return input;
    }
    let after_first_line = input.find('\n').map(|index| index + 1).unwrap_or(0);
    let body = &input[after_first_line..];
    match body.rfind("```") {
        Some(index) => body[..index].trim(),
        None => body.trim(),
    }
}

pub fn validate_result(
    envelope: &TaskEnvelope,
    result: &ResultEnvelope,
) -> Result<Vec<ResultWarning>> {
    let mut warnings = Vec::new();
    if result.schema_version != ENVELOPE_SCHEMA_VERSION {
        return Err(EntwurfError::Contract(format!(
            "unsupported result schema version {}",
            result.schema_version
        )));
    }
    if result.task_id != envelope.task_id {
        return Err(EntwurfError::Contract(format!(
            "result task_id {} does not match {}",
            result.task_id, envelope.task_id
        )));
    }
    if result.revision != envelope.revision {
        return Err(EntwurfError::Contract(format!(
            "result revision {} does not match {}",
            result.revision, envelope.revision
        )));
    }

    let known_ids: BTreeSet<&str> = envelope
        .baseline_ir
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    for added in &result.ir_patch.add_nodes {
        if added.id.trim().is_empty() || added.title.trim().is_empty() {
            return Err(EntwurfError::Contract(
                "added nodes need a non-empty id and title".to_string(),
            ));
        }
        if added.origin == NodeOrigin::GptCorrection {
            warnings.push(ResultWarning::new(
                "gpt_origin_rewritten",
                "GPT candidate was reclassified as an AI candidate on import",
            ));
        }
    }
    let mut provided_ids = known_ids.clone();
    provided_ids.extend(
        result
            .ir_patch
            .add_nodes
            .iter()
            .map(|node| node.id.as_str()),
    );
    for patch in &result.ir_patch.update_nodes {
        if !provided_ids.contains(patch.id.as_str()) {
            return Err(EntwurfError::Contract(format!(
                "patch references unknown node {}",
                patch.id
            )));
        }
    }
    for edge in &result.ir_patch.add_edges {
        if !provided_ids.contains(edge.from.as_str()) || !provided_ids.contains(edge.to.as_str()) {
            return Err(EntwurfError::Contract(format!(
                "edge {} references an unknown node",
                edge.id
            )));
        }
    }
    if result.ir_patch.add_nodes.is_empty()
        && result.ir_patch.update_nodes.is_empty()
        && result.corrections.is_empty()
    {
        warnings.push(ResultWarning::new(
            "empty_result",
            "the result contained no knowledge changes",
        ));
    }
    if result.visual_plan.page_count == 0 {
        return Err(EntwurfError::Contract(
            "visual plan must contain at least one page".to_string(),
        ));
    }
    if result.visual_plan.page_count > envelope.style.paper.max_pages {
        warnings.push(ResultWarning::new(
            "page_budget_exceeded",
            format!(
                "GPT planned {} pages but the style allows {}",
                result.visual_plan.page_count, envelope.style.paper.max_pages
            ),
        ));
    }
    for group in &result.visual_plan.groups {
        for node_id in &group.node_ids {
            if !provided_ids.contains(node_id.as_str()) {
                return Err(EntwurfError::Contract(format!(
                    "visual group {} references unknown node {}",
                    group.id, node_id
                )));
            }
        }
    }
    Ok(warnings)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResultWarning {
    pub code: String,
    pub message: String,
}

impl ResultWarning {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

fn correction_value(node: &KnowledgeNode, field: &CorrectionField) -> Result<String> {
    Ok(match field {
        CorrectionField::Title => node.title.clone(),
        CorrectionField::Body => node.body.clone(),
        CorrectionField::Latex => node.latex.clone().unwrap_or_default(),
        CorrectionField::Anchors => serde_json::to_string(&node.anchors)
            .map_err(|e| EntwurfError::Contract(e.to_string()))?,
    })
}

fn validate_safe_changes(baseline: &KnowledgeIr, result: &ResultEnvelope) -> Result<()> {
    let mut ids: BTreeSet<&str> = baseline.nodes.iter().map(|n| n.id.as_str()).collect();
    for node in &result.ir_patch.add_nodes {
        if node.id.trim().is_empty() || !ids.insert(&node.id) {
            return Err(EntwurfError::Contract(format!(
                "duplicate or empty added node {}",
                node.id
            )));
        }
    }
    for id in &result.ir_patch.remove_node_ids {
        let node = baseline.node(id).ok_or_else(|| {
            EntwurfError::Contract("cannot remove an unknown baseline node".into())
        })?;
        if node.origin == NodeOrigin::Source {
            return Err(EntwurfError::Contract(
                "GPT cannot delete source baseline knowledge; use an explicit manual version edit"
                    .into(),
            ));
        }
    }
    let sources: BTreeSet<&str> = baseline.sources.iter().map(|s| s.id.as_str()).collect();
    let mut corrected = BTreeSet::new();
    for proposal in &result.corrections {
        let node = baseline.node(&proposal.node_id).ok_or_else(|| {
            EntwurfError::Contract(format!(
                "correction references unknown baseline node {}",
                proposal.node_id
            ))
        })?;
        if node.origin == NodeOrigin::UserEdit {
            continue;
        }
        let field = format!("{:?}", proposal.field);
        if !corrected.insert((proposal.node_id.as_str(), field))
            || correction_value(node, &proposal.field)? != proposal.before
            || proposal.reason.trim().is_empty()
            || !proposal.anchors.iter().any(|a| anchors_match(a, &sources))
        {
            return Err(EntwurfError::Contract(
                "correction needs exact before text, a unique field, reason and source anchors"
                    .into(),
            ));
        }
        if proposal.field == CorrectionField::Anchors {
            let _: Vec<SourceAnchor> = serde_json::from_str(&proposal.after)
                .map_err(|e| EntwurfError::Contract(format!("invalid corrected anchors: {e}")))?;
        }
        if result.ir_patch.remove_node_ids.contains(&proposal.node_id) {
            return Err(EntwurfError::Contract(
                "cannot correct and delete the same node".into(),
            ));
        }
    }
    let mut patched = BTreeSet::new();
    for patch in &result.ir_patch.update_nodes {
        if !patched.insert(patch.id.as_str()) || !ids.contains(patch.id.as_str()) {
            return Err(EntwurfError::Contract(
                "duplicate or unknown patched node".into(),
            ));
        }
        let Some(node) = baseline.node(&patch.id) else {
            continue;
        };
        if node.origin == NodeOrigin::UserEdit {
            continue;
        }
        if let Some(after) = &patch.latex {
            if node.latex.as_deref() != Some(after.as_str())
                && !result.corrections.iter().any(|c| {
                    c.node_id == node.id && c.field == CorrectionField::Latex && c.after == *after
                })
            {
                return Err(EntwurfError::Contract(
                    "formula changes require an explicit audited correction".into(),
                ));
            }
        }
        let math_text = crate::math_text::contains_math;
        for (field, before, after) in [
            (CorrectionField::Title, &node.title, patch.title.as_ref()),
            (CorrectionField::Body, &node.body, patch.body.as_ref()),
        ] {
            if let Some(after) = after {
                let formula_bearing = node.latex.is_some()
                    || node.tags.iter().any(|t| t == "formula_candidate")
                    || math_text(before)
                    || math_text(after);
                if after != before
                    && formula_bearing
                    && !result
                        .corrections
                        .iter()
                        .any(|c| c.node_id == node.id && c.field == field && c.after == *after)
                {
                    return Err(EntwurfError::Contract(
                        "formula-bearing text changes require an audited correction".into(),
                    ));
                }
            }
        }
        for c in result.corrections.iter().filter(|c| c.node_id == patch.id) {
            let overlap = match c.field {
                CorrectionField::Title => patch.title.as_ref(),
                CorrectionField::Body => patch.body.as_ref(),
                _ => None,
            };
            if overlap.is_some_and(|value| *value != c.after) {
                return Err(EntwurfError::Contract(
                    "patch conflicts with correction".into(),
                ));
            }
        }
    }
    Ok(())
}

/// Merge a validated result into the baseline. User edits are never overwritten:
/// GPT patches only replace fields on nodes whose origin is still GPT/source.
pub fn apply_result(
    baseline: &KnowledgeIr,
    result: &ResultEnvelope,
) -> Result<(KnowledgeIr, Vec<Correction>)> {
    validate_safe_changes(baseline, result)?;
    let mut ir = baseline.clone();
    ir.schema_version = crate::ir::IR_SCHEMA_VERSION;

    for mut node in result.ir_patch.add_nodes.clone() {
        if node.id.trim().is_empty() {
            node.id = new_id();
        }
        if ir.node(&node.id).is_none() {
            node.origin = NodeOrigin::GptCandidate;
            if node.verified {
                node.verified = false;
            }
            if node.kind == crate::ir::NodeKind::Formula
                || node.latex.is_some()
                || crate::math_text::contains_math(&node.title)
                || crate::math_text::contains_math(&node.body)
            {
                ir.unverified.push(crate::ir::UnverifiedItem {
                    id: new_id(),
                    node_id: Some(node.id.clone()),
                    kind: crate::ir::UnverifiedKind::Formula,
                    text: node.latex.clone().unwrap_or_else(|| node.body.clone()),
                    reason: "Confirm GPT formula against its exact source".into(),
                });
            }
            ir.nodes.push(node);
        }
    }

    for patch in &result.ir_patch.update_nodes {
        if let Some(node) = ir.node_mut(&patch.id) {
            if node.origin == NodeOrigin::UserEdit {
                continue;
            }
            if let Some(title) = &patch.title {
                node.title = title.clone();
            }
            if let Some(body) = &patch.body {
                node.body = body.clone();
            }
            // Formula replacements are applied only by the audited correction below.
            // A matching proposal is required by validate_safe_changes.

            if let Some(importance) = &patch.importance {
                node.importance = importance.clone();
            }
            node.verified = false;
        }
    }

    for edge in &result.ir_patch.add_edges {
        if ir.edges.iter().all(|existing| existing.id != edge.id) {
            if edge
                .label
                .as_deref()
                .is_some_and(crate::math_text::contains_math)
            {
                ir.unverified.push(crate::ir::UnverifiedItem {
                    id: new_id(),
                    node_id: Some(edge.from.clone()),
                    kind: crate::ir::UnverifiedKind::Formula,
                    text: edge.label.clone().unwrap_or_default(),
                    reason: "Confirm GPT relation formula against its source".into(),
                });
            }
            ir.edges.push(edge.clone());
        }
    }
    for node_id in &result.ir_patch.remove_node_ids {
        if let Some(node) = ir.node(node_id) {
            if node.origin != NodeOrigin::UserEdit {
                ir.nodes.retain(|candidate| candidate.id != *node_id);
                ir.edges
                    .retain(|edge| edge.from != *node_id && edge.to != *node_id);
                ir.unverified
                    .retain(|item| item.node_id.as_ref() != Some(node_id));
            }
        }
    }

    let mut corrections = Vec::new();
    for proposal in &result.corrections {
        let Some(node) = ir.node(&proposal.node_id) else {
            continue;
        };
        if node.origin == NodeOrigin::UserEdit {
            continue;
        }
        let before = proposal.before.clone();
        let correction = Correction {
            id: new_id(),
            node_id: proposal.node_id.clone(),
            field: proposal.field.clone(),
            before,
            after: proposal.after.clone(),
            reason: proposal.reason.clone(),
            anchors: proposal.anchors.clone(),
            accepted: true,
            reverted: false,
            created_at: chrono::Utc::now(),
        };
        apply_correction(&mut ir, &correction);
        corrections.push(correction);
    }
    ir.corrections.extend(corrections.clone());
    ir.touch();
    Ok((ir, corrections))
}

pub fn apply_correction(ir: &mut KnowledgeIr, correction: &Correction) {
    let Some(node) = ir.node_mut(&correction.node_id) else {
        return;
    };
    match correction.field {
        CorrectionField::Title => node.title = correction.after.clone(),
        CorrectionField::Body => node.body = correction.after.clone(),
        CorrectionField::Latex => node.latex = Some(correction.after.clone()),
        CorrectionField::Anchors => {
            if let Ok(anchors) = serde_json::from_str(&correction.after) {
                node.anchors = anchors;
            }
        }
    }
    node.verified = false;
}

pub fn revert_correction(ir: &mut KnowledgeIr, correction_id: &str) -> Result<()> {
    let Some(index) = ir
        .corrections
        .iter()
        .position(|item| item.id == correction_id)
    else {
        return Err(EntwurfError::Contract(format!(
            "correction {correction_id} does not exist"
        )));
    };
    if ir.corrections[index].reverted {
        return Ok(());
    }
    let correction = ir.corrections[index].clone();
    let Some(node) = ir.node_mut(&correction.node_id) else {
        return Err(EntwurfError::Contract(format!(
            "node {} no longer exists",
            correction.node_id
        )));
    };
    if correction_value(node, &correction.field)? != correction.after {
        return Err(EntwurfError::Contract(
            "correction has been superseded; undo later changes first".into(),
        ));
    }
    match correction.field {
        CorrectionField::Title => node.title = correction.before.clone(),
        CorrectionField::Body => node.body = correction.before.clone(),
        CorrectionField::Latex => {
            node.latex = if correction.before.is_empty() {
                None
            } else {
                Some(correction.before.clone())
            }
        }
        CorrectionField::Anchors => {
            node.anchors = serde_json::from_str(&correction.before)
                .map_err(|e| EntwurfError::Contract(e.to_string()))?
        }
    }
    node.origin = NodeOrigin::UserEdit;
    node.verified = false;
    ir.corrections[index].reverted = true;
    ir.touch();
    Ok(())
}

pub fn anchors_satisfy(node: &KnowledgeNode, source_ids: &BTreeSet<&str>) -> bool {
    node.anchors
        .iter()
        .any(|anchor| anchors_match(anchor, source_ids))
}

fn anchors_match(anchor: &SourceAnchor, source_ids: &BTreeSet<&str>) -> bool {
    source_ids.contains(anchor.source_id.as_str())
        && (anchor.page.is_some()
            || anchor.line.is_some()
            || anchor.char_start.is_some()
            || anchor.quote_hash.is_some())
}

pub fn correction_for_patch(
    node: &KnowledgeNode,
    patch: &crate::ir::NodePatch,
    reason: impl Into<String>,
) -> Vec<CorrectionProposal> {
    let mut proposals = Vec::new();
    if let Some(after) = &patch.title {
        if *after != node.title {
            proposals.push(CorrectionProposal {
                node_id: node.id.clone(),
                field: CorrectionField::Title,
                before: node.title.clone(),
                after: after.clone(),
                reason: reason.into(),
                anchors: node.anchors.clone(),
            });
        }
    }
    proposals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{GraphEdge, Importance, IrPatch, NodeKind, VisualGroup, VisualPlan};

    fn sample_ir() -> KnowledgeIr {
        let mut ir = KnowledgeIr::empty("p", "t", "Probability");
        ir.nodes.push(KnowledgeNode {
            id: "n1".into(),
            kind: NodeKind::Concept,
            title: "Random variable".into(),
            body: "A function from outcomes to real numbers.".into(),
            latex: Some("X: \\Omega \\to \\mathbb{R}".into()),
            anchors: vec![SourceAnchor {
                region: None,
                source_id: "s1".into(),
                page: Some(2),
                line: None,
                char_start: Some(4),
                char_end: Some(18),
                quote_hash: None,
                label: "definition".into(),
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

    fn sample_result() -> ResultEnvelope {
        ResultEnvelope {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            task_id: "t".into(),
            revision: 1,
            ir_patch: IrPatch {
                add_nodes: vec![],
                update_nodes: vec![],
                add_edges: vec![GraphEdge {
                    id: "e1".into(),
                    from: "n1".into(),
                    to: "n1".into(),
                    edge_type: crate::ir::EdgeKind::Related,
                    label: None,
                    anchors: vec![],
                }],
                remove_node_ids: vec![],
            },
            visual_plan: VisualPlan {
                page_count: 1,
                groups: vec![VisualGroup {
                    id: "g1".into(),
                    title: "Core".into(),
                    node_ids: vec!["n1".into()],
                    page: 1,
                }],
                notes: vec![],
            },
            corrections: vec![],
            image_path: None,
            notes: vec![],
        }
    }

    #[test]
    fn result_parser_rejects_invalid_raw_contracts() {
        let mut value = serde_json::to_value(sample_result()).unwrap();
        value["schema_version"] = 2.into();
        assert!(parse_result_envelope(&value.to_string()).is_err());
        value["schema_version"] = 1.into();
        value["task_id"] = "".into();
        assert!(parse_result_envelope(&value.to_string()).is_err());
        value["task_id"] = "t".into();
        value["unexpected"] = true.into();
        assert!(parse_result_envelope(&value.to_string()).is_err());
        assert!(parse_result_envelope(&format!(
            "```json\n{}\n```",
            serde_json::to_string(&sample_result()).unwrap()
        ))
        .is_ok());
    }

    #[test]
    fn validates_identity_and_references() {
        let ir = sample_ir();
        let style = StyleSpec::default();
        let envelope = build_task_envelope(&ir, &style, 1, "source");
        assert!(validate_result(&envelope, &sample_result()).is_ok());

        let mut wrong = sample_result();
        wrong.revision = 2;
        assert!(validate_result(&envelope, &wrong).is_err());
    }

    #[test]
    fn merges_without_overwriting_user_edit() {
        let mut ir = sample_ir();
        ir.nodes[0].origin = NodeOrigin::UserEdit;
        ir.nodes[0].title = "My title".into();
        let mut result = sample_result();
        result.ir_patch.update_nodes.push(crate::ir::NodePatch {
            id: "n1".into(),
            title: Some("Model title".into()),
            body: None,
            latex: None,
            importance: None,
        });
        let (merged, _) = apply_result(&ir, &result).unwrap();
        assert_eq!(merged.nodes[0].title, "My title");
    }

    #[test]
    fn correction_can_be_reverted() {
        let mut ir = sample_ir();
        let correction = Correction {
            id: "c1".into(),
            node_id: "n1".into(),
            field: CorrectionField::Title,
            before: "Random variable".into(),
            after: "Random Variable".into(),
            reason: "case".into(),
            anchors: vec![],
            accepted: true,
            reverted: false,
            created_at: chrono::Utc::now(),
        };
        apply_correction(&mut ir, &correction);
        ir.corrections.push(correction);
        revert_correction(&mut ir, "c1").unwrap();
        assert_eq!(ir.nodes[0].title, "Random variable");
    }
}
