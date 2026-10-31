//! The delimiter grammar shared by rendered prose, review queues and provenance.
use crate::{EntwurfError, Result};
#[derive(Debug, PartialEq)]
pub enum Span<'a> {
    Text(&'a str),
    Math { latex: &'a str, block: bool },
}
pub fn parse(text: &str) -> Result<Vec<Span<'_>>> {
    let (mut out, mut cursor, mut start) = (Vec::new(), 0, 0);
    while cursor < text.len() {
        let rest = &text[cursor..];
        if rest.starts_with(r"\$") {
            cursor += 2;
            continue;
        }
        let pair = [
            ("$$", "$$", true),
            ("$", "$", false),
            (r"\(", r"\)", false),
            (r"\[", r"\]", true),
        ]
        .into_iter()
        .find(|(open, _, _)| rest.starts_with(open));
        if let Some((open, close, block)) = pair {
            if cursor > start {
                out.push(Span::Text(&text[start..cursor]));
            }
            let inner = cursor + open.len();
            let end = text[inner..]
                .find(close)
                .map(|n| inner + n)
                .ok_or_else(|| EntwurfError::Render("unclosed inline math delimiter".into()))?;
            out.push(Span::Math {
                latex: &text[inner..end],
                block,
            });
            cursor = end + close.len();
            start = cursor;
        } else {
            cursor += rest.chars().next().unwrap().len_utf8();
        }
    }
    if start < text.len() {
        out.push(Span::Text(&text[start..]));
    }
    Ok(out)
}
pub fn formulas(text: &str) -> Result<Vec<&str>> {
    Ok(parse(text)?
        .into_iter()
        .filter_map(|s| match s {
            Span::Math { latex, .. } => Some(latex),
            _ => None,
        })
        .collect())
}
pub fn contains_math(text: &str) -> bool {
    formulas(text).map_or(true, |v| !v.is_empty())
}
