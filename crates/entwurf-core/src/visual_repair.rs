//! Content-preserving visual repair policies and reversible, deterministic records.
use crate::{
    error::{EntwurfError, Result},
    ir::{KnowledgeIr, StyleSpec, ValidationReport},
    layout::LayoutOptions,
    model::{LayoutDocument, VisualRepairRecord},
};

pub fn ir_hash(ir: &KnowledgeIr) -> String {
    crate::ingest::hash_bytes(&serde_json::to_vec(ir).expect("serializable IR"))
}
pub fn style_hash(style: &StyleSpec) -> String {
    crate::ingest::hash_bytes(&serde_json::to_vec(style).expect("serializable style"))
}
pub(crate) fn geometry_hash(doc: Option<&LayoutDocument>) -> Option<String> {
    doc.map(|d| crate::ingest::hash_bytes(&serde_json::to_vec(d).expect("serializable geometry")))
}

pub(crate) fn repairable(report: &ValidationReport) -> bool {
    let blocking: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == crate::ir::Severity::Blocking)
        .collect();
    !blocking.is_empty()
        && blocking.iter().all(|f| {
            matches!(
                f.code.as_str(),
                "compile_layout"
                    | "compile_render"
                    | "text_clipping"
                    | "font_too_small"
                    | "node_overlap"
                    | "out_of_bounds"
                    | "edge_crosses_node"
                    | "edge_out_of_bounds"
                    | "edge_nonorthogonal"
                    | "missing_route"
                    | "edge_endpoint"
            ) && ![
                "unsupported",
                "unclosed",
                "missing glyph",
                "invalid canvas",
                "font ",
                "Typst:",
            ]
            .iter()
            .any(|s| f.message.contains(s))
        })
}

/// At most three ordered changes, each derived only from current geometry and
/// diagnostics. Page budget, source content, colors and style are never changed.
pub(crate) fn next_options(
    current: &LayoutOptions,
    round: u16,
    report: &ValidationReport,
) -> (LayoutOptions, String) {
    let mut next = current.clone();
    next.obstacle_routing = true;
    if report
        .findings
        .iter()
        .any(|f| f.message.contains("document heading exceeds"))
    {
        next.max_heading_font_pt = current.max_heading_font_pt.min(match round {
            1 => 24.0,
            2 => 16.0,
            _ => 8.0,
        });
        return (
            next,
            format!("round {round}: reduce overflowing page heading within the 8pt floor"),
        );
    }
    if report
        .findings
        .iter()
        .filter(|f| f.severity == crate::ir::Severity::Blocking)
        .all(|f| f.code.starts_with("edge_") || f.code == "missing_route")
    {
        return (
            next,
            "reroute orthogonal connectors around card obstacles".into(),
        );
    }
    next.max_font_pt = current
        .max_font_pt
        .min(11.0 - round as f32)
        .max(current.min_font_pt.max(8.0));
    next.card_padding_pt = current
        .card_padding_pt
        .min(if round == 1 { 6.0 } else { 4.0 });
    next.node_gap_pt = current.node_gap_pt.min(4.0);
    if report.findings.iter().any(|f| {
        f.message.contains("exceeds its card") || f.message.contains("cannot fit on one page")
    }) {
        next.preferred_columns = current.preferred_columns.saturating_sub(1).clamp(1, 2);
    }
    (
        next,
        format!("round {round}: compact spacing, readable font size and wider cards when required"),
    )
}

pub(crate) fn record(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    round: u16,
    reason: String,
    before: LayoutOptions,
    after: LayoutOptions,
    before_geometry: Option<String>,
    after_geometry: Option<String>,
    passed: bool,
) -> VisualRepairRecord {
    let ir_hash = ir_hash(ir);
    let style_hash = style_hash(style);
    let id = crate::ingest::hash_bytes(
        &serde_json::to_vec(&(&ir_hash, &style_hash, round, &before, &after))
            .expect("serializable repair"),
    );
    VisualRepairRecord {
        id,
        round,
        reason,
        ir_hash,
        style_hash,
        before,
        after,
        before_geometry_hash: before_geometry,
        after_geometry_hash: after_geometry,
        passed,
        reverted: false,
    }
}

/// Undo a selected adjustment and dependent later adjustments. Caller persists
/// the returned history in a NEW version and disables automatic repair for it.
/// The existing history and IR are never mutated.
pub fn rollback(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    records: &[VisualRepairRecord],
    id: &str,
) -> Result<(LayoutOptions, Vec<VisualRepairRecord>)> {
    let index = records
        .iter()
        .position(|r| r.id == id && !r.reverted)
        .ok_or_else(|| {
            EntwurfError::InvalidInput("unknown or already reverted visual repair".into())
        })?;
    let expected_ir = ir_hash(ir);
    let expected_style = style_hash(style);
    if records.iter().filter(|r| !r.reverted).count() > 3 {
        return Err(EntwurfError::InvalidInput(
            "visual repair history exceeds three active rounds".into(),
        ));
    }
    let mut previous: Option<&LayoutOptions> = None;
    for r in records.iter().filter(|r| !r.reverted) {
        if r.ir_hash != expected_ir
            || r.style_hash != expected_style
            || r.round == 0
            || r.round > 3
            || previous.is_some_and(|options| options != &r.before)
        {
            return Err(EntwurfError::InvalidInput(
                "visual repair history does not match current IR/style or option chain".into(),
            ));
        }
        let expected = record(
            ir,
            style,
            r.round,
            r.reason.clone(),
            r.before.clone(),
            r.after.clone(),
            r.before_geometry_hash.clone(),
            r.after_geometry_hash.clone(),
            r.passed,
        );
        if expected.id != r.id {
            return Err(EntwurfError::InvalidInput(
                "visual repair record identity is inconsistent".into(),
            ));
        }
        previous = Some(&r.after);
    }
    let options = records[index].before.clone();
    let mut history = records.to_vec();
    for r in &mut history[index..] {
        r.reverted = true;
    }
    Ok((options, history))
}

/// Reroute a supplied page geometry without moving cards or changing knowledge.
pub fn reroute(ir: &KnowledgeIr, document: &mut LayoutDocument) -> Result<()> {
    for page in &mut document.pages {
        page.edges = crate::obstacle_routes::reroute(ir, page)?;
    }
    Ok(())
}
