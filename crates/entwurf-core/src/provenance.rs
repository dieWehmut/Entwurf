//! Verify source bytes and exact formula history, independently of visual quality.
use crate::files as fs;
use crate::{ingest::hash_bytes, ir::*};
use std::collections::BTreeMap;

fn blocked(report: &mut ValidationReport, code: &str, message: impl Into<String>) {
    report.push(code, Severity::Blocking, message);
}
pub fn validate_provenance(ir: &KnowledgeIr, report: &mut ValidationReport) {
    let mut data = BTreeMap::new();
    for source in &ir.sources {
        match fs::read(&source.stored_path) {
            Ok(bytes)
                if bytes.len() as u64 == source.byte_len
                    && hash_bytes(&bytes) == source.content_hash =>
            {
                data.insert(source.id.as_str(), bytes);
            }
            _ => blocked(
                report,
                "source_integrity",
                format!(
                    "retained source {} is missing or differs from its imported hash",
                    source.original_name
                ),
            ),
        }
        for note in &source.notes {
            if let Some(raw) = note.strip_prefix("ingestion_diagnostic:") {
                match serde_json::from_str::<serde_json::Value>(raw) {
                    Ok(d) if d["level"] != "info" && d["code"] != "empty_source" => blocked(
                        report,
                        "incomplete_source",
                        format!(
                            "{}: {}",
                            source.original_name,
                            d["message"]
                                .as_str()
                                .unwrap_or("unresolved source extraction diagnostic")
                        ),
                    ),
                    Err(_) => blocked(
                        report,
                        "invalid_source_diagnostic",
                        format!(
                            "{} contains an unreadable extraction diagnostic",
                            source.original_name
                        ),
                    ),
                    _ => {}
                }
            }
        }
    }
    let mut quotes = BTreeMap::<String, Vec<String>>::new();
    for node in &ir.nodes {
        let mut found = vec![];
        for a in &node.anchors {
            match quote(ir, a, &data) {
                Ok(q) => found.push(q),
                Err(e) => blocked(report, "invalid_anchor", format!("{}: {e}", node.title)),
            }
        }
        if node.anchors.is_empty() {
            blocked(
                report,
                "untraceable_node",
                format!("{} needs a source anchor", node.title),
            );
        }
        quotes.insert(node.id.clone(), found);
    }
    for edge in &ir.edges {
        for a in &edge.anchors {
            if let Err(e) = quote(ir, a, &data) {
                blocked(report, "invalid_edge_anchor", format!("{}: {e}", edge.id));
            }
        }
    }
    // Every active correction is evidence, regardless of which field it changes.
    for c in ir.corrections.iter().filter(|c| c.accepted && !c.reverted) {
        if c.reason.trim().is_empty()
            || c.anchors.is_empty()
            || c.anchors.iter().any(|a| quote(ir, a, &data).is_err())
        {
            blocked(
                report,
                "invalid_correction_evidence",
                format!("correction {} has missing or invalid source evidence", c.id),
            );
        }
    }
    for node in &ir.nodes {
        let node_quotes = &quotes[&node.id];
        let mut originals: Vec<String> = node_quotes
            .iter()
            .map(|q| unwrap_formula(q).to_owned())
            .collect();
        for q in node_quotes {
            if let Ok(values) = crate::math_text::formulas(q) {
                originals.extend(values.into_iter().map(str::to_owned));
            }
        }
        for (field, value) in [
            (CorrectionField::Title, Some(node.title.as_str())),
            (CorrectionField::Body, Some(node.body.as_str())),
            (CorrectionField::Latex, node.latex.as_deref()),
        ] {
            let Some(value) = value else { continue };
            let current_math = if field == CorrectionField::Latex {
                Ok(vec![value])
            } else {
                crate::math_text::formulas(value)
            };
            let Ok(current_math) = current_math else {
                blocked(
                    report,
                    "invalid_formula_delimiter",
                    format!("{} has unclosed math", node.title),
                );
                continue;
            };
            if current_math.is_empty()
                || current_math
                    .iter()
                    .all(|formula| originals.iter().any(|q| q == formula))
            {
                continue;
            }
            let (mut previous, mut broken): (Option<String>, bool) = (None, false);
            for c in ir
                .corrections
                .iter()
                .filter(|c| c.node_id == node.id && c.field == field && c.accepted && !c.reverted)
            {
                let anchored =
                    !c.anchors.is_empty() && c.anchors.iter().all(|a| quote(ir, a, &data).is_ok());
                let starts = if field == CorrectionField::Latex {
                    originals.contains(&c.before)
                        || (c.before.is_empty()
                            && node.origin == NodeOrigin::UserEdit
                            && c.reason == "Explicit manual edit")
                } else {
                    crate::math_text::formulas(&c.before)
                        .is_ok_and(|values| values.iter().all(|f| originals.iter().any(|q| q == f)))
                };
                let follows = previous
                    .as_ref()
                    .map_or(starts, |before| before == &c.before);
                if anchored && !c.reason.trim().is_empty() && follows {
                    previous = Some(c.after.clone());
                } else {
                    broken = true;
                    break;
                }
            }
            if broken || previous.as_deref() != Some(value) {
                blocked(report,"unaudited_formula",format!("formula in {} {:?} differs from its exact source and has no complete correction history",node.title,field));
            }
        }
    }
    for edge in &ir.edges {
        if let Some(label) = &edge.label {
            match crate::math_text::formulas(label) {
                Ok(formulas) => {
                    let source_quotes: Vec<_> = edge
                        .anchors
                        .iter()
                        .filter_map(|a| quote(ir, a, &data).ok())
                        .collect();
                    for formula in formulas {
                        if !source_quotes.iter().any(|q| {
                            unwrap_formula(q) == formula
                                || crate::math_text::formulas(q).is_ok_and(|v| v.contains(&formula))
                        }) {
                            blocked(
                                report,
                                "unaudited_edge_formula",
                                format!("edge {} contains an unproven formula", edge.id),
                            );
                        }
                    }
                }
                Err(e) => blocked(report, "invalid_formula_delimiter", e.to_string()),
            }
        }
    }
}
fn unwrap_formula(quote: &str) -> &str {
    for (open, close) in [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")] {
        if let Some(q) = quote.strip_prefix(open).and_then(|q| q.strip_suffix(close)) {
            return q;
        }
    }
    quote
}
fn quote(
    ir: &KnowledgeIr,
    a: &SourceAnchor,
    data: &BTreeMap<&str, Vec<u8>>,
) -> std::result::Result<String, String> {
    let source = ir
        .sources
        .iter()
        .find(|s| s.id == a.source_id)
        .ok_or("unknown source")?;
    let bytes = data
        .get(a.source_id.as_str())
        .ok_or("source integrity not verified")?;
    let text = match source.kind {
        SourceKind::Markdown | SourceKind::Latex => {
            String::from_utf8(bytes.clone()).map_err(|_| "source is not UTF-8")?
        }
        SourceKind::Pdf | SourceKind::Image => {
            let candidates: Vec<_> = source
                .notes
                .iter()
                .filter_map(|n| n.strip_prefix("ingestion_transcript:"))
                .filter_map(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                .filter(|t| t["page"].as_u64() == a.page.map(u64::from))
                .collect();
            if candidates.len() != 1 {
                return Err("anchor page has no unique retained transcript".into());
            }
            let t = &candidates[0];
            let text = t["text"].as_str().ok_or("missing transcript text")?;
            if t["sha256"].as_str() != Some(hash_bytes(text.as_bytes()).as_str())
                || t["offset_unit"] != "utf8_bytes"
            {
                return Err("transcript integrity failed".into());
            }
            text.to_owned()
        }
    };
    let (start, end) = (
        a.char_start.ok_or("anchor needs exact byte start")? as usize,
        a.char_end.ok_or("anchor needs exact byte end")? as usize,
    );
    if start >= end {
        return Err("empty or reversed anchor byte range".into());
    }
    let q = text
        .get(start..end)
        .ok_or("anchor bytes are outside source or not UTF-8 boundaries")?;
    if a.quote_hash.as_deref() != Some(hash_bytes(q.as_bytes()).as_str()) {
        return Err("anchor quote hash differs from source bytes".into());
    }
    if let Some(line) = a.line {
        if line == 0 || text[..start].bytes().filter(|b| *b == b'\n').count() + 1 != line as usize {
            return Err("anchor line disagrees with byte position".into());
        }
    }
    if a.page == Some(0) {
        return Err("page numbers start at one".into());
    }
    Ok(q.into())
}
