//! Deterministic A3 composition: grouping, measured text wrapping, column
//! packing, automatic pagination, and orthogonal edge routing.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::Result;
use crate::ir::{Id, KnowledgeIr, KnowledgeNode, NodeKind, StyleSpec};
use crate::model::{
    mm_to_pt, LayoutDocument, LayoutPage, PlacedEdge, PlacedGroup, PlacedNode, Point, Rect,
    TextLine,
};

#[derive(Debug, Clone)]
pub struct LayoutOptions {
    pub max_pages: u16,
    pub min_font_pt: f32,
    pub max_font_pt: f32,
    pub column_gap_pt: f32,
    pub node_gap_pt: f32,
    pub card_padding_pt: f32,
    pub preferred_columns: usize,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            max_pages: 3,
            min_font_pt: 8.0,
            max_font_pt: 11.5,
            column_gap_pt: 14.0,
            node_gap_pt: 10.0,
            card_padding_pt: 8.0,
            preferred_columns: 3,
        }
    }
}

impl LayoutOptions {
    pub fn from_style(style: &StyleSpec) -> Self {
        Self {
            max_pages: style.paper.max_pages.max(1),
            max_font_pt: style.typography.body_pt.max(9.0),
            ..Self::default()
        }
    }
}

pub fn layout(
    ir: &KnowledgeIr,
    style: &StyleSpec,
    options: &LayoutOptions,
) -> Result<LayoutDocument> {
    let width_pt = mm_to_pt(style.paper.width_mm);
    let height_pt = mm_to_pt(style.paper.height_mm);
    let margin_pt = mm_to_pt(style.paper.margin_mm);
    let content = Rect::new(
        margin_pt,
        margin_pt,
        width_pt - margin_pt * 2.0,
        height_pt - margin_pt * 2.0,
    );

    let title_height = 58.0;
    let column_count = options.preferred_columns.max(1);
    let columns = column_rects(content, column_count, options.column_gap_pt);
    let mut pages: Vec<LayoutPage> = Vec::new();
    let mut current = new_page(1, width_pt, height_pt, margin_pt);
    let mut column_heights = vec![title_height + 14.0; column_count];
    let mut group_map: BTreeMap<String, PlacedGroup> = BTreeMap::new();
    let mut diagnostics = Vec::new();

    for group in build_groups(ir) {
        for node_id in group.node_ids {
            let Some(node) = ir.node(&node_id) else {
                continue;
            };
            let estimated = measure_node(node, columns[0].width - 16.0, style, options).1;
            let column_index = choose_column(
                &column_heights,
                estimated + 34.0,
                content.height - title_height - 24.0,
            );
            let column_index = match column_index {
                Some(index) => index,
                None if current.nodes.is_empty() => {
                    diagnostics.push(format!("node {} cannot fit within one A3 page", node.title));
                    0
                }
                None => {
                    finalize_page(&mut current, ir, &mut group_map);
                    let next_index = pages.len() as u16 + 2;
                    pages.push(std::mem::replace(
                        &mut current,
                        new_page(next_index, width_pt, height_pt, margin_pt),
                    ));
                    column_heights = vec![title_height + 14.0; column_count];
                    0
                }
            };
            let column = columns[column_index];
            let top = content.y + column_heights[column_index];
            let card_width = column.width - 18.0;
            let card_height = measure_node(node, card_width, style, options).1;
            let padding = 10.0;
            let group_key = group.id.clone();
            let existing = group_map.get(&group_key).cloned();
            let group_top = existing
                .as_ref()
                .and_then(|placed| {
                    if placed.page == current.index {
                        Some(placed.rect.y)
                    } else {
                        None
                    }
                })
                .unwrap_or(top);
            let node_rect = Rect::new(column.x + 9.0, top + 24.0, card_width, card_height);
            let placed = place_node(node, node_rect, current.index, style, options)?;
            let new_group_rect = match existing {
                Some(mut placed_group) => {
                    placed_group.rect.height = (placed.rect.bottom() + padding
                        - placed_group.rect.y)
                        .max(placed_group.rect.height);
                    group_map.insert(group_key.clone(), placed_group);
                    None
                }
                None => Some(Rect::new(
                    column.x,
                    group_top,
                    column.width,
                    placed.rect.bottom() + padding - group_top,
                )),
            };
            if let Some(rect) = new_group_rect {
                group_map.insert(
                    group_key,
                    PlacedGroup {
                        id: group.id.clone(),
                        title: group.title.clone(),
                        page: current.index,
                        rect,
                    },
                );
            }
            current.nodes.push(placed);
            column_heights[column_index] =
                (node_rect.bottom() + options.node_gap_pt + padding) - content.y;
        }
    }
    if !current.nodes.is_empty() || pages.is_empty() {
        finalize_page(&mut current, ir, &mut group_map);
        pages.push(current);
    }

    if pages.len() as u16 > options.max_pages {
        diagnostics.push(format!(
            "page budget {} exceeded; {} pages are required",
            options.max_pages,
            pages.len()
        ));
    }
    Ok(LayoutDocument {
        width_pt,
        height_pt,
        margin_pt,
        pages,
        diagnostics,
    })
}

fn new_page(index: u16, width_pt: f32, height_pt: f32, margin_pt: f32) -> LayoutPage {
    LayoutPage {
        index,
        width_pt,
        height_pt,
        margin_pt,
        groups: Vec::new(),
        nodes: Vec::new(),
        edges: Vec::new(),
    }
}

fn choose_column(heights: &[f32], needed: f32, max_height: f32) -> Option<usize> {
    let mut candidates: Vec<usize> = (0..heights.len())
        .filter(|index| heights[*index] + needed <= max_height)
        .collect();
    candidates.sort_by(|left, right| {
        heights[*left]
            .partial_cmp(&heights[*right])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.cmp(right))
    });
    candidates.first().copied()
}

fn finalize_page(
    page: &mut LayoutPage,
    ir: &KnowledgeIr,
    group_map: &mut BTreeMap<String, PlacedGroup>,
) {
    page.groups = group_map.values().cloned().collect();
    page.groups.sort_by(|left, right| {
        left.rect
            .y
            .partial_cmp(&right.rect.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.rect
                    .x
                    .partial_cmp(&right.rect.x)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    page.edges = route_edges(ir, page, page.index);
}

#[derive(Debug, Clone)]
struct Group {
    id: Id,
    title: String,
    node_ids: Vec<Id>,
}

fn build_groups(ir: &KnowledgeIr) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    let chapter_ids: BTreeSet<&str> = ir
        .nodes
        .iter()
        .filter(|node| matches!(node.kind, NodeKind::Chapter | NodeKind::Section))
        .map(|node| node.id.as_str())
        .collect();
    for node in &ir.nodes {
        if node.kind == NodeKind::Chapter {
            groups.push(Group {
                id: format!("group:{}", node.id),
                title: node.title.clone(),
                node_ids: vec![node.id.clone()],
            });
        }
    }
    for node in &ir.nodes {
        if node.kind == NodeKind::Chapter {
            continue;
        }
        let group_index = match &node.parent_id {
            Some(parent) if chapter_ids.contains(parent.as_str()) => groups
                .iter()
                .position(|group| group.node_ids.first() == Some(parent)),
            _ => None,
        };
        if let Some(index) = group_index {
            groups[index].node_ids.push(node.id.clone());
        } else {
            groups.push(Group {
                id: format!("group:{}", node.id),
                title: node.title.clone(),
                node_ids: vec![node.id.clone()],
            });
        }
    }
    if groups.is_empty() {
        groups.push(Group {
            id: "group:all".to_string(),
            title: ir.title.clone(),
            node_ids: ir.nodes.iter().map(|node| node.id.clone()).collect(),
        });
    }
    groups
}

fn measure_node(
    node: &KnowledgeNode,
    width: f32,
    style: &StyleSpec,
    options: &LayoutOptions,
) -> (f32, f32) {
    let title_size = (style.typography.body_pt * 1.24).clamp(9.0, options.max_font_pt + 2.0);
    let body_size = style
        .typography
        .body_pt
        .clamp(options.min_font_pt, options.max_font_pt);
    let title_lines = wrap_text(
        &node.title,
        width - options.card_padding_pt * 2.0,
        title_size,
    );
    let body_lines = wrap_text(&node.body, width - options.card_padding_pt * 2.0, body_size);
    let latex_lines = node
        .latex
        .as_deref()
        .map(|latex| wrap_text(latex, width - options.card_padding_pt * 2.0, body_size))
        .unwrap_or_default();
    let line_gap = body_size * style.typography.line_height;
    let height = options.card_padding_pt * 2.0
        + title_lines.len() as f32 * title_size * 1.28
        + body_lines.len() as f32 * line_gap
        + latex_lines.len() as f32 * line_gap
        + 12.0;
    (width, height.max(42.0))
}

fn place_node(
    node: &KnowledgeNode,
    rect: Rect,
    page: u16,
    style: &StyleSpec,
    options: &LayoutOptions,
) -> Result<PlacedNode> {
    let title_size = (style.typography.body_pt * 1.24).clamp(9.0, options.max_font_pt + 2.0);
    let body_size = style
        .typography
        .body_pt
        .clamp(options.min_font_pt, options.max_font_pt);
    let line_gap = body_size * style.typography.line_height;
    let inner_width = rect.width - options.card_padding_pt * 2.0;
    let title_lines = positioned_lines(
        &node.title,
        rect.x + options.card_padding_pt,
        rect.y + options.card_padding_pt + title_size,
        inner_width,
        title_size,
        line_gap,
        true,
    );
    let body_top =
        rect.y + options.card_padding_pt + title_lines.len() as f32 * title_size * 1.28 + 6.0;
    let body_lines = positioned_lines(
        &node.body,
        rect.x + options.card_padding_pt,
        body_top + body_size,
        inner_width,
        body_size,
        line_gap,
        false,
    );
    let latex_top = body_top + body_lines.len() as f32 * line_gap + 4.0;
    let latex_lines = node
        .latex
        .as_deref()
        .map(|latex| {
            positioned_lines(
                latex,
                rect.x + options.card_padding_pt,
                latex_top + body_size,
                inner_width,
                body_size,
                line_gap,
                false,
            )
        })
        .unwrap_or_default();
    let text_bottom = latex_lines
        .last()
        .map(|line| line.baseline + line_gap * 0.35)
        .or_else(|| {
            body_lines
                .last()
                .map(|line| line.baseline + line_gap * 0.35)
        })
        .unwrap_or(body_top);
    Ok(PlacedNode {
        node_id: node.id.clone(),
        kind: node.kind.clone(),
        title: node.title.clone(),
        body: node.body.clone(),
        latex: node.latex.clone(),
        page,
        rect: Rect::new(
            rect.x,
            rect.y,
            rect.width,
            (text_bottom - rect.y + options.card_padding_pt).max(42.0),
        ),
        title_lines,
        body_lines,
        latex_lines,
        text_clipped: false,
    })
}

fn positioned_lines(
    text: &str,
    x: f32,
    first_baseline: f32,
    width: f32,
    font_size: f32,
    line_gap: f32,
    bold: bool,
) -> Vec<TextLine> {
    wrap_text(text, width, font_size)
        .into_iter()
        .enumerate()
        .map(|(index, text)| TextLine {
            text,
            x,
            baseline: first_baseline + index as f32 * line_gap,
            font_size,
            bold,
        })
        .collect()
}

pub fn wrap_text(text: &str, width: f32, font_size: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let width_units = (width / font_size.max(1.0)).max(1.0);
    for paragraph in text.lines() {
        if paragraph.trim().is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            continue;
        }
        let mut line = String::new();
        let mut units = 0.0f32;
        for character in paragraph.chars() {
            let character_units = if character.is_ascii() {
                0.56
            } else if matches!(
                character,
                '，' | '。' | '、' | '；' | '：' | '！' | '？' | '）' | '」' | '』'
            ) {
                0.85
            } else {
                1.0
            };
            if units + character_units > width_units && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                units = 0.0;
            }
            line.push(character);
            units += character_units;
        }
        if !line.is_empty() {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn column_rects(content: Rect, count: usize, gap: f32) -> Vec<Rect> {
    let count = count.max(1);
    let width = (content.width - gap * (count - 1) as f32) / count as f32;
    (0..count)
        .map(|index| {
            Rect::new(
                content.x + index as f32 * (width + gap),
                content.y,
                width,
                content.height,
            )
        })
        .collect()
}

fn route_edges(ir: &KnowledgeIr, page: &LayoutPage, page_number: u16) -> Vec<PlacedEdge> {
    let mut positions = BTreeMap::new();
    for node in &page.nodes {
        positions.insert(node.node_id.clone(), node.rect);
    }
    let mut edges = Vec::new();
    for edge in &ir.edges {
        let (Some(from), Some(to)) = (positions.get(&edge.from), positions.get(&edge.to)) else {
            continue;
        };
        let start = Point::new(
            from.right(),
            (from.y + from.height * 0.5).min(from.bottom() - 6.0),
        );
        let end = Point::new(to.x, (to.y + to.height * 0.5).min(to.bottom() - 6.0));
        let mid_x = (start.x + end.x) * 0.5;
        edges.push(PlacedEdge {
            edge_id: edge.id.clone(),
            from_node: edge.from.clone(),
            to_node: edge.to.clone(),
            page: page_number,
            points: vec![
                start,
                Point::new(mid_x, start.y),
                Point::new(mid_x, end.y),
                end,
            ],
            label: edge.label.clone(),
        });
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Importance, NodeOrigin, SourceAnchor, SourceFile, SourceKind, StyleSpec};

    fn ir_with_nodes() -> KnowledgeIr {
        let mut ir = KnowledgeIr::empty("p", "t", "Probability");
        let source = SourceFile {
            id: "s".into(),
            original_name: "source.md".into(),
            kind: SourceKind::Markdown,
            content_hash: "h".into(),
            stored_path: "source.md".into(),
            byte_len: 0,
            imported_at: chrono::Utc::now(),
            notes: vec![],
        };
        ir.sources.push(source);
        let anchor = SourceAnchor {
            region: None,
            source_id: "s".into(),
            page: Some(1),
            line: Some(1),
            char_start: Some(0),
            char_end: Some(10),
            quote_hash: None,
            label: "source".into(),
        };
        ir.nodes.push(KnowledgeNode {
            id: "chapter".into(),
            kind: NodeKind::Chapter,
            title: "Probability".into(),
            body: String::new(),
            latex: None,
            anchors: vec![anchor.clone()],
            importance: Importance::Critical,
            origin: NodeOrigin::Source,
            parent_id: None,
            tags: vec![],
            verified: false,
            review_note: None,
        });
        for index in 0..5 {
            ir.nodes.push(KnowledgeNode {
                id: format!("n{index}"),
                kind: NodeKind::Concept,
                title: format!("Concept {index}"),
                body: "Every outcome has a probability. Conditional probability updates belief."
                    .into(),
                latex: Some("P(A \\mid B) = \\frac{P(A \\cap B)}{P(B)}".into()),
                anchors: vec![anchor.clone()],
                importance: Importance::High,
                origin: NodeOrigin::Source,
                parent_id: Some("chapter".into()),
                tags: vec![],
                verified: false,
                review_note: None,
            });
        }
        ir.edges.push(crate::ir::GraphEdge {
            id: "e".into(),
            from: "n0".into(),
            to: "n1".into(),
            edge_type: crate::ir::EdgeKind::DependsOn,
            label: Some("uses".into()),
            anchors: vec![anchor],
        });
        ir
    }

    #[test]
    fn lays_out_a3_and_keeps_nodes_inside_pages() {
        let ir = ir_with_nodes();
        let style = StyleSpec::default();
        let options = LayoutOptions::from_style(&style);
        let document = layout(&ir, &style, &options).unwrap();
        assert!(!document.pages.is_empty());
        assert_eq!(document.node_count(), 6);
        for page in &document.pages {
            for node in &page.nodes {
                assert!(node.rect.x >= 0.0);
                assert!(node.rect.right() <= page.width_pt + 0.5);
                assert!(node.rect.bottom() <= page.height_pt + 0.5);
            }
        }
    }

    #[test]
    fn wrap_text_respects_width() {
        let lines = wrap_text(
            "这是一段比较长的中文文本，用于测试确定性的排版换行逻辑。",
            100.0,
            10.0,
        );
        assert!(lines.len() > 1);
    }
}
