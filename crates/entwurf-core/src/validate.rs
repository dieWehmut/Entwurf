//! Content-first validation. A layout may never pass by visual score alone if a
//! source or formula invariant is broken.

use std::collections::{BTreeMap, BTreeSet};

use regex::Regex;

use crate::error::{EntwurfError, Result};
use crate::ir::{CorrectionField, Importance, KnowledgeIr, NodeKind, Severity, ValidationReport};
use crate::model::LayoutDocument;

#[derive(Debug, Clone)]
pub struct ValidationOptions {
    pub minimum_font_pt: f32,
    pub minimum_contrast: f32,
    pub require_critical_anchors: bool,
    pub require_formula_anchors: bool,
    pub max_pages: u16,
}

impl Default for ValidationOptions {
    fn default() -> Self {
        Self {
            minimum_font_pt: 8.0,
            minimum_contrast: 4.5,
            require_critical_anchors: true,
            require_formula_anchors: true,
            max_pages: 3,
        }
    }
}

pub fn validate_ir(ir: &KnowledgeIr, options: &ValidationOptions) -> Result<ValidationReport> {
    let mut report = ValidationReport::new();
    if ir.schema_version != crate::ir::IR_SCHEMA_VERSION {
        report.push(
            "schema_version",
            Severity::Blocking,
            format!(
                "expected schema version {}, found {}",
                crate::ir::IR_SCHEMA_VERSION,
                ir.schema_version
            ),
        );
    }
    let source_ids: BTreeSet<&str> = ir.sources.iter().map(|source| source.id.as_str()).collect();
    let node_ids: BTreeSet<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    let reviewed_formula_ids: BTreeSet<&str> = ir
        .corrections
        .iter()
        .filter(|correction| {
            correction.accepted
                && !correction.reverted
                && correction.field == CorrectionField::Latex
        })
        .map(|correction| correction.node_id.as_str())
        .collect();

    if ir.nodes.is_empty() {
        report.push(
            "empty_ir",
            Severity::Blocking,
            "the knowledge IR has no nodes",
        );
    }
    // Ingestion can preserve useful partial content while recording unresolved
    // formulas, OCR uncertainty, or unreadable pages. Those explicit review
    // items must block export until the caller records review by clearing them.
    // Grounded source nodes default to verified=false; that flag alone is not
    // an unresolved-review item and must not become a blanket source gate.
    for item in &ir.unverified {
        report.findings.push(crate::ir::Finding {
            id: uuid::Uuid::new_v4().to_string(),
            code: "unresolved_review".into(),
            severity: Severity::Blocking,
            message: item.reason.clone(),
            node_id: item.node_id.clone(),
            evidence: Some(item.text.clone()),
        });
    }
    for node in &ir.nodes {
        if node.title.trim().is_empty() {
            report.push(
                "empty_title",
                Severity::Blocking,
                format!("node {} has an empty title", node.id),
            );
        }
        let must_anchor = options.require_critical_anchors
            && (node.importance == Importance::Critical
                || matches!(node.kind, NodeKind::Chapter | NodeKind::Section));
        let source_anchor_exists = node.anchors.iter().any(|anchor| {
            source_ids.contains(anchor.source_id.as_str())
                && (anchor.page.is_some()
                    || anchor.line.is_some()
                    || anchor.char_start.is_some()
                    || anchor.quote_hash.is_some())
        });
        if must_anchor && !source_anchor_exists {
            let mut finding = crate::ir::Finding {
                id: uuid::Uuid::new_v4().to_string(),
                code: "missing_critical_anchor".to_string(),
                severity: Severity::Blocking,
                message: format!(
                    "critical node {} has no verifiable source anchor",
                    node.title
                ),
                node_id: Some(node.id.clone()),
                evidence: None,
            };
            finding.evidence = Some(node.body.chars().take(160).collect());
            report.findings.push(finding);
        }
        if options.require_formula_anchors && node.kind == NodeKind::Formula {
            let has_formula = node
                .latex
                .as_deref()
                .map(|latex| !latex.trim().is_empty())
                .unwrap_or(false);
            if !has_formula {
                report.push(
                    "empty_formula",
                    Severity::Blocking,
                    format!("formula node {} has no LaTeX", node.title),
                );
            }
            if !source_anchor_exists && !reviewed_formula_ids.contains(node.id.as_str()) {
                report.push(
                    "missing_formula_anchor",
                    Severity::Blocking,
                    format!("formula node {} is not traceable to a source", node.title),
                );
            }
            if let Some(latex) = &node.latex {
                if !balanced_delimiters(latex) {
                    report.push(
                        "unbalanced_formula",
                        Severity::Blocking,
                        format!("formula {} has unbalanced braces or delimiters", node.title),
                    );
                }
            }
        }
        if let Some(parent) = &node.parent_id {
            if !node_ids.contains(parent.as_str()) {
                report.push(
                    "orphan_node",
                    Severity::Blocking,
                    format!("node {} points to unknown parent {}", node.title, parent),
                );
            }
        }
    }
    for edge in &ir.edges {
        if !node_ids.contains(edge.from.as_str()) || !node_ids.contains(edge.to.as_str()) {
            report.push(
                "dangling_edge",
                Severity::Blocking,
                format!("edge {} references a node that does not exist", edge.id),
            );
        }
    }
    for correction in &ir.corrections {
        if correction.accepted && correction.reverted && !ir.node(&correction.node_id).is_some() {
            report.push(
                "correction_orphan",
                Severity::Warning,
                format!("correction {} points to a removed node", correction.id),
            );
        }
        if correction.accepted && correction.before == correction.after {
            report.push(
                "noop_correction",
                Severity::Warning,
                format!("correction {} does not change anything", correction.id),
            );
        }
    }
    crate::provenance::validate_provenance(ir, &mut report);
    report.metrics.insert("nodes".into(), ir.nodes.len() as f64);
    report
        .metrics
        .insert("sources".into(), ir.sources.len() as f64);
    report
        .metrics
        .insert("critical_nodes".into(), ir.critical_nodes().count() as f64);
    report.metrics.insert(
        "formula_nodes".into(),
        ir.nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Formula)
            .count() as f64,
    );
    Ok(report)
}

pub fn validate_layout(
    ir: &KnowledgeIr,
    layout: &LayoutDocument,
    ir_report: &mut ValidationReport,
    options: &ValidationOptions,
) -> Result<ValidationReport> {
    if layout.pages.len() as u16 > options.max_pages {
        ir_report.push(
            "page_budget",
            Severity::Blocking,
            format!(
                "layout uses {} pages but the limit is {}",
                layout.pages.len(),
                options.max_pages
            ),
        );
    }
    let node_ids: BTreeSet<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    let mut placed_ids = BTreeSet::new();
    let mut placed_rects: BTreeMap<&str, (u16, crate::model::Rect)> = BTreeMap::new();
    for page in &layout.pages {
        let page_rect = crate::model::Rect::new(0.0, 0.0, page.width_pt, page.height_pt);
        for node in &page.nodes {
            if !node_ids.contains(node.node_id.as_str()) {
                ir_report.push(
                    "stale_layout_node",
                    Severity::Blocking,
                    format!("layout contains unknown node {}", node.node_id),
                );
            }
            if !placed_ids.insert(node.node_id.clone()) {
                ir_report.push(
                    "duplicate_placement",
                    Severity::Blocking,
                    format!("node {} is placed more than once", node.node_id),
                );
            }
            let rect = node.rect;
            if rect.x < 0.0
                || rect.y < 0.0
                || !page_rect
                    .inset(0.5)
                    .contains(crate::model::Point::new(rect.right(), rect.bottom()))
            {
                ir_report.push(
                    "out_of_bounds",
                    Severity::Blocking,
                    format!("node {} extends beyond page {}", node.title, page.index),
                );
            }
            if node.text_clipped {
                ir_report.push(
                    "text_clipping",
                    Severity::Blocking,
                    format!("node {} reports clipped content", node.title),
                );
            }
            if let Some(bounds) = node.measured_content_rect {
                if ![bounds.x, bounds.y, bounds.width, bounds.height]
                    .iter()
                    .all(|v| v.is_finite())
                    || bounds.width < 0.0
                    || bounds.height < 0.0
                    || bounds.x < rect.x - 0.5
                    || bounds.y < rect.y - 0.5
                    || bounds.right() > rect.right() + 0.5
                    || bounds.bottom() > rect.bottom() + 0.5
                {
                    ir_report.push(
                        "text_clipping",
                        Severity::Blocking,
                        format!("shaped glyph bounds exceed node {}", node.title),
                    );
                }
                if node
                    .measured_font_pt
                    .is_none_or(|size| !size.is_finite() || size < options.minimum_font_pt)
                {
                    ir_report.push(
                        "font_too_small",
                        Severity::Blocking,
                        format!(
                            "node {} has invalid or too-small measured glyphs",
                            node.title
                        ),
                    );
                }
            }
            for line in node
                .title_lines
                .iter()
                .chain(node.body_lines.iter())
                .chain(node.latex_lines.iter())
            {
                if line.font_size < options.minimum_font_pt {
                    ir_report.push(
                        "font_too_small",
                        Severity::Blocking,
                        format!(
                            "node {} uses {:.1}pt text below the {:.1}pt print minimum",
                            node.title, line.font_size, options.minimum_font_pt
                        ),
                    );
                }
                let estimated_width = line
                    .text
                    .chars()
                    .map(|character| if character.is_ascii() { 0.56 } else { 1.0 })
                    .sum::<f32>()
                    * line.font_size;
                if node.measured_content_rect.is_none()
                    && line.x + estimated_width > rect.right() + line.font_size
                {
                    ir_report.push(
                        "text_clipping",
                        Severity::Blocking,
                        format!("text overflows node {}", node.title),
                    );
                }
            }
            placed_rects.insert(node.node_id.as_str(), (page.index, rect));
        }
        let mut routed = std::collections::BTreeSet::new();
        for edge in &page.edges {
            if !routed.insert(edge.edge_id.as_str()) {
                ir_report.push(
                    "duplicate_route",
                    Severity::Blocking,
                    format!("edge {} is routed twice", edge.edge_id),
                );
            }
            let original = ir.edges.iter().find(|e| e.id == edge.edge_id);
            let from = page.nodes.iter().find(|n| n.node_id == edge.from_node);
            let to = page.nodes.iter().find(|n| n.node_id == edge.to_node);
            if !original.is_some_and(|e| e.from == edge.from_node && e.to == edge.to_node)
                || from.is_none()
                || to.is_none()
                || edge.page != page.index
                || edge.points.len() < 2
            {
                ir_report.push(
                    "invalid_route",
                    Severity::Blocking,
                    format!("invalid edge route {}", edge.edge_id),
                );
                continue;
            }
            let degenerate = edge
                .points
                .windows(2)
                .all(|p| (p[0].x - p[1].x).abs() + (p[0].y - p[1].y).abs() < 0.01)
                || edge.from_node == edge.to_node
                    && (edge
                        .points
                        .iter()
                        .all(|p| (p.x - edge.points[0].x).abs() < 0.01)
                        || edge
                            .points
                            .iter()
                            .all(|p| (p.y - edge.points[0].y).abs() < 0.01));
            if degenerate {
                ir_report.push(
                    "edge_endpoint",
                    Severity::Blocking,
                    format!("edge {} has no visible route", edge.edge_id),
                );
            }
            let on_boundary = |p: crate::model::Point, r: crate::model::Rect| {
                r.contains(p)
                    && [
                        (p.x - r.x).abs(),
                        (p.x - r.right()).abs(),
                        (p.y - r.y).abs(),
                        (p.y - r.bottom()).abs(),
                    ]
                    .into_iter()
                    .any(|d| d < 0.1)
            };
            if !on_boundary(edge.points[0], from.unwrap().rect)
                || !on_boundary(*edge.points.last().unwrap(), to.unwrap().rect)
            {
                ir_report.push(
                    "edge_endpoint",
                    Severity::Blocking,
                    format!("edge {} must terminate at card boundaries", edge.edge_id),
                );
            }
            if edge.points.iter().any(|p| {
                !p.x.is_finite()
                    || !p.y.is_finite()
                    || p.x < 0.0
                    || p.y < 0.0
                    || p.x > page.width_pt
                    || p.y > page.height_pt
            }) {
                ir_report.push(
                    "edge_out_of_bounds",
                    Severity::Blocking,
                    format!("edge {} leaves its page", edge.edge_id),
                );
            }
            for (segment_index, pair) in edge.points.windows(2).enumerate() {
                if (pair[0].x - pair[1].x).abs() > 0.01 && (pair[0].y - pair[1].y).abs() > 0.01 {
                    ir_report.push(
                        "edge_nonorthogonal",
                        Severity::Blocking,
                        format!("edge {} contains a diagonal segment", edge.edge_id),
                    );
                    continue;
                }
                for node in &page.nodes {
                    // A 0.6pt connector may meet its endpoint border, but its
                    // stroke must clear every other card, not just its centreline.
                    let terminal = (segment_index == 0 && node.node_id == edge.from_node)
                        || (segment_index + 2 == edge.points.len() && node.node_id == edge.to_node);
                    let obstacle = if terminal {
                        node.rect
                    } else {
                        crate::model::Rect::new(
                            node.rect.x - 0.3,
                            node.rect.y - 0.3,
                            node.rect.width + 0.6,
                            node.rect.height + 0.6,
                        )
                    };
                    if crate::obstacle_routes::segment_hits(pair[0], pair[1], obstacle) {
                        ir_report.push(
                            "edge_crosses_node",
                            Severity::Blocking,
                            format!("edge {} crosses card {}", edge.edge_id, node.node_id),
                        );
                    }
                }
            }
        }
        for edge in &ir.edges {
            if page.nodes.iter().any(|n| n.node_id == edge.from)
                && page.nodes.iter().any(|n| n.node_id == edge.to)
                && !routed.contains(edge.id.as_str())
            {
                ir_report.push(
                    "missing_route",
                    Severity::Blocking,
                    format!("missing route for edge {}", edge.id),
                );
            }
        }
        for (index, node) in page.nodes.iter().enumerate() {
            for other in page.nodes.iter().skip(index + 1) {
                if node.rect.intersects(&other.rect) {
                    ir_report.push(
                        "node_overlap",
                        Severity::Blocking,
                        format!("nodes {} and {} overlap", node.title, other.title),
                    );
                }
            }
        }
    }
    for node in &ir.nodes {
        if !placed_ids.contains(&node.id) {
            ir_report.push(
                "missing_placement",
                Severity::Blocking,
                format!("node {} is missing from the layout", node.title),
            );
        }
    }
    ir_report
        .metrics
        .insert("pages".into(), layout.pages.len() as f64);
    ir_report
        .metrics
        .insert("placed_nodes".into(), placed_ids.len() as f64);
    Ok(ir_report.clone())
}

/// Validate exactly the painted text/background pairs: cards use surface;
/// document heading/footer use background. Colors are never repaired implicitly.
pub fn validate_style(
    style: &crate::ir::StyleSpec,
    report: &mut ValidationReport,
    options: &ValidationOptions,
) {
    for color in [
        &style.colors.accent,
        &style.colors.border,
        &style.colors.muted,
    ] {
        if let Err(error) = parse_hex_color(color) {
            report.push("invalid_color", Severity::Blocking, error.to_string());
        }
    }
    for (name, background) in [
        ("cards", &style.colors.surface),
        ("heading/footer", &style.colors.background),
    ] {
        match contrast_ratio(&style.colors.text, background) {
            Ok(ratio) if ratio >= options.minimum_contrast => {
                report
                    .metrics
                    .insert(format!("text_contrast_{name}"), ratio as f64);
            }
            Ok(ratio) => report.push(
                "text_contrast",
                Severity::Blocking,
                format!(
                    "{name} text contrast {ratio:.2} is below {:.2}",
                    options.minimum_contrast
                ),
            ),
            Err(error) => report.push("invalid_color", Severity::Blocking, error.to_string()),
        }
    }
}

pub fn contrast_ratio(foreground: &str, background: &str) -> Result<f32> {
    let fg = parse_hex_color(foreground)?;
    let bg = parse_hex_color(background)?;
    let fg_luminance = relative_luminance(fg);
    let bg_luminance = relative_luminance(bg);
    let (lighter, darker) = if fg_luminance > bg_luminance {
        (fg_luminance, bg_luminance)
    } else {
        (bg_luminance, fg_luminance)
    };
    Ok((lighter + 0.05) / (darker + 0.05))
}

fn parse_hex_color(value: &str) -> Result<(f32, f32, f32)> {
    let value = value.trim().trim_start_matches('#');
    if value.len() != 6 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(EntwurfError::InvalidInput(format!(
            "expected #RRGGBB color, got {value}"
        )));
    }
    let red = u8::from_str_radix(&value[0..2], 16)
        .map_err(|error| EntwurfError::InvalidInput(error.to_string()))?;
    let green = u8::from_str_radix(&value[2..4], 16)
        .map_err(|error| EntwurfError::InvalidInput(error.to_string()))?;
    let blue = u8::from_str_radix(&value[4..6], 16)
        .map_err(|error| EntwurfError::InvalidInput(error.to_string()))?;
    Ok((
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    ))
}

fn relative_luminance((red, green, blue): (f32, f32, f32)) -> f32 {
    fn channel(value: f32) -> f32 {
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue)
}

fn balanced_delimiters(latex: &str) -> bool {
    let mut braces = 0i32;
    let mut brackets = 0i32;
    let mut in_escape = false;
    for character in latex.chars() {
        if in_escape {
            in_escape = false;
            continue;
        }
        match character {
            '\\' => in_escape = true,
            '{' => braces += 1,
            '}' => braces -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            _ => {}
        }
        if braces < 0 || brackets < 0 {
            return false;
        }
    }
    braces == 0 && brackets == 0
}

pub fn normalize_formula(formula: &str) -> String {
    let whitespace = Regex::new(r"\s+").expect("valid regex");
    whitespace
        .replace_all(formula.trim(), " ")
        .replace("\\(", "\\left(")
        .replace("\\)", "\\right)")
}

pub fn source_coverage(ir: &KnowledgeIr) -> f64 {
    if ir.nodes.is_empty() {
        return 0.0;
    }
    let anchored = ir
        .nodes
        .iter()
        .filter(|node| !node.anchors.is_empty())
        .count();
    anchored as f64 / ir.nodes.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::StyleSpec;
    use crate::layout::{layout, LayoutOptions};

    fn sample() -> (tempfile::TempDir, KnowledgeIr) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.md");
        std::fs::write(&path, "# Concept\n\nBody").unwrap();
        let ir = crate::ingest::ingest_paths(&[path], &crate::ingest::IngestOptions::standard())
            .unwrap()
            .into_ir("p", "t", "Test");
        (dir, ir)
    }

    #[test]
    fn critical_nodes_without_anchor_are_blocking() {
        let (_dir, mut ir) = sample();
        ir.nodes[0].anchors.clear();
        let report = validate_ir(&ir, &ValidationOptions::default()).unwrap();
        assert!(report
            .findings
            .iter()
            .any(|item| item.code == "missing_critical_anchor"
                && item.severity == Severity::Blocking));
    }

    #[test]
    fn contrast_ratio_matches_wcag_example() {
        let ratio = contrast_ratio("#000000", "#ffffff").unwrap();
        assert!(ratio > 20.0);
    }

    #[test]
    fn layout_for_sample_passes_content_gate() {
        let (_dir, ir) = sample();
        let style = StyleSpec::default();
        let doc = layout(&ir, &style, &LayoutOptions::from_style(&style)).unwrap();
        let mut report = validate_ir(&ir, &ValidationOptions::default()).unwrap();
        validate_layout(&ir, &doc, &mut report, &ValidationOptions::default()).unwrap();
        assert!(report.is_passing(), "{:?}", report.findings);
    }
}
