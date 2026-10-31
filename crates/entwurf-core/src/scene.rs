use crate::error::{EntwurfError, Result};
use crate::ir::{KnowledgeIr, StyleSpec};
use crate::layout::LayoutOptions;
use crate::model::LayoutDocument;
use crate::rendering::{compile_source, literal, measure_card, prelude};
use typst::layout::{Abs, Point};
use typst::model::Document;
use typst_layout::PagedDocument;

/// Shapes and metadata are generated only by the renderer; all user strings
/// enter Typst through quoted text values. Card frames are exactly those measured.
pub fn scene(
    document: &LayoutDocument,
    ir: &KnowledgeIr,
    style: &StyleSpec,
    options: &LayoutOptions,
) -> Result<PagedDocument> {
    let mut pages = Vec::new();
    let mut info = None;
    for page in &document.pages {
        let mut source = prelude(style, style.typography.body_pt.max(8.0))?;
        source.push_str(&format!("#set document(title: {}, date: none)\n#set page(width: {}pt, height: {}pt, margin: 0pt, fill: rgb({}))\n",literal(&ir.title),page.width_pt,page.height_pt,literal(&style.colors.background)));
        source.push_str(&format!(
            "#place(top + left, dx: {}pt, dy: {}pt, text(size: 8pt, {}))\n",
            page.margin_pt,
            page.height_pt - page.margin_pt.max(12.0),
            literal(&format!(
                "Entwurf · {} / {}",
                page.index,
                document.pages.len()
            ))
        ));
        for edge in &page.edges {
            for pair in edge.points.windows(2) {
                source.push_str(&format!("#place(top + left, dx: {}pt, dy: {}pt, line(start: (0pt, 0pt), end: ({}pt, {}pt), stroke: (paint: rgb({}), thickness: 0.6pt)))\n",pair[0].x,pair[0].y,pair[1].x-pair[0].x,pair[1].y-pair[0].y,literal(&style.colors.accent)));
            }
        }
        for node in &page.nodes {
            source.push_str(&format!("#place(top + left, dx: {}pt, dy: {}pt, rect(width: {}pt, height: {}pt, radius: 4pt, fill: rgb({}), stroke: (paint: rgb({}), thickness: 0.5pt)))\n",node.rect.x,node.rect.y,node.rect.width,node.rect.height,literal(&style.colors.surface),literal(&style.colors.border)));
        }
        let base = compile_source(&source)?;
        if base.pages().len() != 1 {
            return Err(EntwurfError::Render("scene unexpectedly paginated".into()));
        }
        info = Some(base.info().clone());
        let mut rendered = base.pages()[0].clone();
        rendered.number = page.index as u64;
        let heading = crate::rendering::heading_frame(
            &ir.title,
            page.width_pt - 2.0 * page.margin_pt,
            style,
        )?;
        rendered.frame.push_frame(
            Point::new(
                Abs::pt(page.margin_pt as f64),
                Abs::pt(page.margin_pt as f64),
            ),
            heading,
        );
        for node in &page.nodes {
            let original = ir
                .node(&node.node_id)
                .ok_or_else(|| EntwurfError::Render("stale node in scene".into()))?;
            let padding = options.card_padding_pt;
            let card = measure_card(
                &crate::rendering::display_node(ir, original),
                node.rect.width - 2.0 * padding,
                style,
                options,
            )?;
            if card.height + 2.0 * padding > node.rect.height + 0.1 {
                return Err(EntwurfError::Render(format!(
                    "stale measured geometry for {}",
                    node.node_id
                )));
            }
            rendered.frame.push_frame(
                Point::new(
                    Abs::pt((node.rect.x + padding) as f64),
                    Abs::pt((node.rect.y + padding) as f64),
                ),
                card.frame.clone(),
            );
        }
        pages.push(rendered);
    }
    Ok(PagedDocument::new(
        pages.into_iter().collect(),
        info.ok_or_else(|| EntwurfError::Render("empty scene".into()))?,
    ))
}
