//! Deterministic A3 composition: grouping, measured text wrapping, column
//! packing, automatic pagination, and orthogonal edge routing.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::Result;
use crate::ir::{Id, KnowledgeIr, NodeKind, StyleSpec};
use crate::model::{
    mm_to_pt, LayoutDocument, LayoutPage, PlacedEdge, PlacedGroup, PlacedNode, Point, Rect,
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
    if ![
        width_pt,
        height_pt,
        margin_pt,
        options.column_gap_pt,
        options.node_gap_pt,
        options.card_padding_pt,
        options.min_font_pt,
        options.max_font_pt,
        style.typography.body_pt,
        style.typography.line_height,
    ]
    .iter()
    .all(|v| v.is_finite() && *v >= 0.0)
        || options.max_pages == 0
        || options.column_gap_pt < 8.0
        || options.min_font_pt < 8.0
        || options.max_font_pt < options.min_font_pt
        || options.preferred_columns == 0
        || options.preferred_columns > 12
        || width_pt <= 2.0 * margin_pt
        || height_pt <= 2.0 * margin_pt + 100.0
    {
        return Err(crate::error::EntwurfError::Render(
            "invalid canvas or layout options".into(),
        ));
    }
    let content = Rect::new(
        margin_pt,
        margin_pt + 72.0,
        width_pt - 2.0 * margin_pt,
        height_pt - 2.0 * margin_pt - 88.0,
    );
    let columns = column_rects(content, options.preferred_columns, options.column_gap_pt);
    let card_width = columns[0].width;
    let inner_width = card_width - options.card_padding_pt * 2.0;
    if inner_width < 32.0 {
        return Err(crate::error::EntwurfError::Render(
            "columns leave no readable card width".into(),
        ));
    }
    let mut pages = Vec::new();
    let mut current = new_page(1, width_pt, height_pt, margin_pt);
    let mut heights = vec![0.0; columns.len()];
    // Groups are kept in stable chapter order. Each page/column fragment owns
    // its own group geometry, avoiding stale cross-page rectangles.
    for group in build_groups(ir) {
        let mut group_column = None;
        for node_id in group.node_ids {
            let node = ir.node(&node_id).expect("group ids come from IR");
            let card = crate::rendering::measure_card(
                &crate::rendering::display_node(ir, node),
                inner_width,
                style,
                options,
            )?;
            let card_height = card.height + 2.0 * options.card_padding_pt;
            if card_height > content.height {
                return Err(crate::error::EntwurfError::Render(format!(
                    "node {} cannot fit on one page at readable size",
                    node.id
                )));
            }
            let selected = group_column
                .filter(|&index| heights[index] + card_height <= content.height)
                .or_else(|| choose_column(&heights, card_height, content.height));
            let index = match selected {
                Some(index) => index,
                None => {
                    current.edges = route_edges(ir, &current, current.index);
                    pages.push(current);
                    if pages.len() >= options.max_pages as usize {
                        return Err(crate::error::EntwurfError::Render(format!(
                            "page budget {} exceeded; all content must be preserved",
                            options.max_pages
                        )));
                    }
                    current = new_page(pages.len() as u16 + 1, width_pt, height_pt, margin_pt);
                    heights.fill(0.0);
                    0
                }
            };
            group_column = Some(index);
            let rect = Rect::new(
                columns[index].x,
                content.y + heights[index],
                card_width,
                card_height,
            );
            let dx = rect.x + options.card_padding_pt;
            let dy = rect.y + options.card_padding_pt;
            let lines = card
                .lines
                .iter()
                .cloned()
                .map(|mut line| {
                    line.x += dx;
                    line.baseline += dy;
                    line
                })
                .collect();
            current.nodes.push(PlacedNode {
                node_id: node.id.clone(),
                kind: node.kind.clone(),
                title: node.title.clone(),
                body: node.body.clone(),
                latex: node.latex.clone(),
                page: current.index,
                rect,
                title_lines: Vec::new(),
                body_lines: lines,
                latex_lines: Vec::new(),
                text_clipped: false,
                measured_content_rect: Some(Rect::new(
                    dx + card.bounds.x,
                    dy + card.bounds.y,
                    card.bounds.width,
                    card.bounds.height,
                )),
                measured_font_pt: Some(card.minimum_font),
            });
            let key = format!("{}:p{}:c{}", group.id, current.index, index);
            if let Some(fragment) = current.groups.iter_mut().find(|g| g.id == key) {
                fragment.rect.height = rect.bottom() - fragment.rect.y;
            } else {
                current.groups.push(PlacedGroup {
                    id: key,
                    title: group.title.clone(),
                    page: current.index,
                    rect,
                });
            }
            heights[index] += card_height + options.node_gap_pt;
        }
    }
    if !current.nodes.is_empty() || pages.is_empty() {
        current.edges = route_edges(ir, &current, current.index);
        pages.push(current);
    }
    Ok(LayoutDocument {
        width_pt,
        height_pt,
        margin_pt,
        pages,
        diagnostics: Vec::new(),
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

#[derive(Debug, Clone)]
struct Group {
    id: Id,
    title: String,
    node_ids: Vec<Id>,
}

fn build_groups(ir: &KnowledgeIr) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for node in &ir.nodes {
        let mut root = node;
        let mut seen = BTreeSet::new();
        while !matches!(root.kind, NodeKind::Chapter | NodeKind::Section) {
            if !seen.insert(root.id.as_str()) {
                break;
            }
            let Some(parent) = root.parent_id.as_deref().and_then(|id| ir.node(id)) else {
                break;
            };
            root = parent;
        }
        let id = format!("group:{}", root.id);
        if let Some(group) = groups.iter_mut().find(|g| g.id == id) {
            group.node_ids.push(node.id.clone());
        } else {
            groups.push(Group {
                id,
                title: root.title.clone(),
                node_ids: vec![node.id.clone()],
            });
        }
    }
    groups
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
    let positions: BTreeMap<_, _> = page
        .nodes
        .iter()
        .map(|n| (n.node_id.as_str(), n.rect))
        .collect();
    let mut edges = Vec::new();
    for edge in &ir.edges {
        let (Some(from), Some(to)) = (
            positions.get(edge.from.as_str()),
            positions.get(edge.to.as_str()),
        ) else {
            continue;
        };
        let sy = from.y + from.height / 2.0;
        let ty = to.y + to.height / 2.0;
        let lane = 4.0;
        let points = if (from.x - to.x).abs() < 0.1 {
            let x = from.right() + lane;
            vec![
                Point::new(from.right(), sy),
                Point::new(x, sy),
                Point::new(x, ty),
                Point::new(to.right(), ty),
            ]
        } else {
            let (sx, tx) = if from.x < to.x {
                (from.right(), to.x)
            } else {
                (from.x, to.right())
            };
            let sign = if from.x < to.x { 1.0 } else { -1.0 };
            let x1 = sx + sign * lane;
            let x2 = tx - sign * lane;
            // All columns share x extents. Their outer lanes are free of
            // cards; the band above the first card connects distant columns.
            let y = page.margin_pt + 64.0;
            vec![
                Point::new(sx, sy),
                Point::new(x1, sy),
                Point::new(x1, y),
                Point::new(x2, y),
                Point::new(x2, ty),
                Point::new(tx, ty),
            ]
        };
        edges.push(PlacedEdge {
            edge_id: edge.id.clone(),
            from_node: edge.from.clone(),
            to_node: edge.to.clone(),
            page: page_number,
            points,
            label: edge.label.clone(),
        });
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        Importance, KnowledgeNode, NodeOrigin, SourceAnchor, SourceFile, SourceKind, StyleSpec,
    };

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
