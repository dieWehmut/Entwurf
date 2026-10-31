use super::*;
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

struct Part {
    range: Range<usize>,
    kind: NodeKind,
    title: String,
    latex: Option<String>,
    level: Option<usize>,
}

pub(super) fn markdown(
    source: &SourceFile,
    input: &str,
    options: &IngestOptions,
) -> Vec<KnowledgeNode> {
    let mut parts = vec![];
    let mut excluded: Vec<Range<usize>> = vec![];
    let mut heading: Option<(Range<usize>, usize, String)> = None;
    let mut code: Option<usize> = None;
    for (event, range) in Parser::new_ext(input, Options::ENABLE_MATH).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((range, level as usize, String::new()))
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((r, level, title)) = heading.take() {
                    parts.push(Part {
                        range: r,
                        kind: if level == 1 {
                            NodeKind::Chapter
                        } else {
                            NodeKind::Section
                        },
                        title,
                        latex: None,
                        level: Some(level),
                    });
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                excluded.push(range.clone());
                code = Some(range.start);
            }
            Event::End(TagEnd::CodeBlock) => {
                code = None;
            }
            Event::Code(t) => {
                excluded.push(range);
                if let Some((_, _, title)) = &mut heading {
                    title.push_str(&t);
                }
            }
            Event::Text(t) => {
                if let Some((_, _, title)) = &mut heading {
                    title.push_str(&t);
                }
            }
            Event::InlineMath(_) | Event::DisplayMath(_) if code.is_none() => {
                let raw = &input[range.clone()];
                let count = if raw.starts_with("$$") { 2 } else { 1 };
                let latex = raw[count..raw.len() - count].to_string();
                if let Some((_, _, title)) = &mut heading {
                    title.push_str(raw);
                }
                {
                    excluded.push(range.clone());
                    parts.push(Part {
                        range,
                        kind: NodeKind::Formula,
                        title: "公式".into(),
                        latex: Some(latex),
                        level: None,
                    });
                }
            }
            _ => {}
        }
    }
    // CommonMark does not define TeX delimiters. Recognize them in ordinary
    // prose and headings, while excluding code and already parsed dollar math.
    for part in delimited_math(
        input,
        0..input.len(),
        &[("\\[", "\\]"), ("\\(", "\\)")],
        &excluded,
        false,
    ) {
        parts.push(part);
    }
    // CommonMark unescapes TeX delimiters in Text events. Render headings with
    // the exact recognized formulas temporarily protected so display titles
    // cannot silently turn \(x\) into (x). Anchors always use original bytes.
    let formula_ranges: Vec<_> = parts
        .iter()
        .filter(|p| p.kind == NodeKind::Formula)
        .map(|p| p.range.clone())
        .collect();
    for part in parts.iter_mut().filter(|p| p.level.is_some()) {
        part.title =
            markdown_title_with_math(input, part.range.clone(), &formula_ranges, &part.title);
    }
    assemble(source, input, options, parts)
}
fn escaped(input: &str, at: usize) -> bool {
    input[..at]
        .bytes()
        .rev()
        .take_while(|b| *b == b'\\')
        .count()
        % 2
        == 1
}
fn assemble(
    source: &SourceFile,
    input: &str,
    options: &IngestOptions,
    mut parts: Vec<Part>,
) -> Vec<KnowledgeNode> {
    parts.sort_by_key(|p| p.range.start);
    let mut nodes = vec![];
    let mut hierarchy: Vec<(usize, String)> = vec![];
    let mut at = 0;
    let mut title = source.original_name.clone();
    for p in parts {
        if p.range.start < at && p.kind != NodeKind::Formula {
            continue;
        }
        if p.range.start >= at {
            chunks(
                source,
                input,
                at..p.range.start,
                &title,
                hierarchy.last().map(|x| x.1.clone()),
                options,
                &mut nodes,
            );
        }
        if let Some(level) = p.level {
            while hierarchy.last().is_some_and(|x| x.0 >= level) {
                hierarchy.pop();
            }
            title = p.title.clone();
        }
        let end = p.range.end;
        let mut n = node(
            source,
            input,
            p.range,
            p.kind,
            &p.title,
            hierarchy.last().map(|x| x.1.clone()),
            p.latex,
        );
        if let Some(level) = p.level {
            n.importance = if level == 1 {
                Importance::Critical
            } else {
                Importance::High
            };
            // The full heading syntax remains recoverable from the anchor. Body
            // is empty to avoid printing the heading twice in downstream layouts.
            n.body.clear();
            hierarchy.push((level, n.id.clone()));
        }
        if n.kind == NodeKind::Formula {
            n.importance = Importance::High;
            n.review_note = Some("Formula requires source confirmation".into());
        }
        nodes.push(n);
        at = at.max(end);
    }
    chunks(
        source,
        input,
        at..input.len(),
        &title,
        hierarchy.last().map(|x| x.1.clone()),
        options,
        &mut nodes,
    );
    nodes
}

pub(super) fn latex(
    source: &SourceFile,
    input: &str,
    options: &IngestOptions,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<KnowledgeNode> {
    let mut parts = vec![];
    let mut at = 0;
    while at < input.len() {
        let rest = &input[at..];
        if rest.starts_with('%') && !escaped(input, at) {
            at = input[at..].find('\n').map_or(input.len(), |i| at + i + 1);
            continue;
        }
        if let Some(end) = verb_end(input, at) {
            at = end;
            continue;
        }
        if rest.starts_with('\\') && !escaped(input, at) {
            let mut matched = false;
            for (command, level) in [
                ("chapter", 1),
                ("section", 2),
                ("subsection", 3),
                ("subsubsection", 4),
                ("paragraph", 5),
                ("subparagraph", 6),
            ] {
                let prefix = format!("\\{command}");
                if rest.starts_with(&prefix) {
                    let mut pos = at + prefix.len();
                    if input[pos..].starts_with('*') {
                        pos += 1;
                    }
                    pos = skip_space(input, pos);
                    let short_title = if let Some((_, end)) = group(input, pos, '[', ']') {
                        let range = pos + 1..end - 1;
                        pos = skip_space(input, end);
                        Some(range)
                    } else {
                        None
                    };
                    if let Some((name, end)) = group(input, pos, '{', '}') {
                        parts.push(Part {
                            range: at..end,
                            kind: if level == 1 {
                                NodeKind::Chapter
                            } else {
                                NodeKind::Section
                            },
                            title: name.into(),
                            latex: None,
                            level: Some(level),
                        });
                        if let Some(range) = short_title {
                            parts.extend(delimited_math(
                                input,
                                range,
                                &[("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")],
                                &[],
                                true,
                            ));
                        }
                        parts.extend(delimited_math(
                            input,
                            pos + 1..end - 1,
                            &[("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")],
                            &[],
                            true,
                        ));
                        at = end;
                        matched = true;
                        break;
                    }
                }
            }
            if matched {
                continue;
            }
            if rest.starts_with("\\begin") {
                if let Some((name, begin_end)) = group(input, skip_space(input, at + 6), '{', '}') {
                    // Document is a container; retain its markers as raw source
                    // and parse its children instead of swallowing all chapters.
                    if name != "document" {
                        if let Some(end) = environment_end(input, begin_end, name) {
                            let base = name.trim_end_matches('*');
                            let kind = match base {
                                "equation" | "align" | "alignat" | "gather" | "multline"
                                | "displaymath" | "math" | "eqnarray" => NodeKind::Formula,
                                "definition" => NodeKind::Definition,
                                "theorem" | "lemma" | "corollary" | "proposition" => {
                                    NodeKind::Theorem
                                }
                                "example" => NodeKind::Example,
                                _ => NodeKind::Note,
                            };
                            let raw = input[at..end].to_string();
                            // Preserve complete environments even above the chunk target.
                            parts.push(Part {
                                range: at..end,
                                kind,
                                title: name.into(),
                                latex: if matches!(base, "verbatim" | "lstlisting" | "minted") {
                                    None
                                } else {
                                    Some(raw)
                                },
                                level: None,
                            });
                            at = end;
                            continue;
                        } else {
                            diagnostics.push(Diagnostic{level:DiagnosticLevel::Warning,code:"latex_unclosed_environment".into(),message:format!("Unclosed environment {name} at byte {at}; raw content retained")});
                        }
                    }
                }
            }
        }
        let delim = [("$$", "$$"), ("\\[", "\\]"), ("\\(", "\\)"), ("$", "$")]
            .into_iter()
            .find(|(open, _)| rest.starts_with(open) && !escaped(input, at));
        if let Some((open, close)) = delim {
            let content = at + open.len();
            let mut seek = content;
            while let Some(i) = input[seek..].find(close) {
                let stop = seek + i;
                if escaped(input, stop) {
                    seek = stop + close.len();
                    continue;
                }
                let end = stop + close.len();
                parts.push(Part {
                    range: at..end,
                    kind: NodeKind::Formula,
                    title: "公式".into(),
                    latex: Some(input[content..stop].into()),
                    level: None,
                });
                at = end;
                break;
            }
            if at != content - open.len() {
                continue;
            }
        }
        at += input[at..].chars().next().unwrap().len_utf8();
    }
    assemble(source, input, options, parts)
}
fn group(input: &str, at: usize, open: char, close: char) -> Option<(&str, usize)> {
    if !input[at..].starts_with(open) {
        return None;
    }
    let mut depth = 0;
    for (i, c) in input[at..].char_indices() {
        if escaped(input, at + i) {
            continue;
        }
        if c == open {
            depth += 1;
        }
        if c == close {
            depth -= 1;
            if depth == 0 {
                return Some((
                    &input[at + open.len_utf8()..at + i],
                    at + i + close.len_utf8(),
                ));
            }
        }
    }
    None
}
fn environment_end(input: &str, begin_end: usize, name: &str) -> Option<usize> {
    let mut stack = vec![name.to_string()];
    let mut at = begin_end;
    // Verbatim environments have literal bodies (including percent and begin).
    if matches!(name, "verbatim" | "lstlisting" | "minted") {
        let close = format!("\\end{{{name}}}");
        return input[at..].find(&close).map(|i| at + i + close.len());
    }
    while at < input.len() {
        if input[at..].starts_with('%') && !escaped(input, at) {
            at = input[at..].find('\n').map_or(input.len(), |i| at + i + 1);
            continue;
        }
        let mut consumed = false;
        for (marker, is_begin) in [("\\begin", true), ("\\end", false)] {
            if input[at..].starts_with(marker) && !escaped(input, at) {
                if let Some((env, end)) =
                    group(input, skip_space(input, at + marker.len()), '{', '}')
                {
                    if is_begin && matches!(env, "verbatim" | "lstlisting" | "minted") {
                        // An opaque nested body may contain arbitrary fake begin/end
                        // commands. Jump directly to its own literal terminator.
                        at = environment_end(input, end, env)?;
                        consumed = true;
                        break;
                    }
                    if is_begin {
                        stack.push(env.into());
                    } else if stack.last().is_some_and(|n| n == env) {
                        stack.pop();
                        if stack.is_empty() {
                            return Some(end);
                        }
                    }
                    at = end;
                    consumed = true;
                    break;
                }
            }
        }
        if consumed {
            continue;
        }
        if at < input.len() {
            at += input[at..].chars().next().unwrap().len_utf8();
        }
    }
    None
}

fn skip_space(input: &str, at: usize) -> usize {
    at + input[at..].len() - input[at..].trim_start().len()
}

/// Inline TeX verbatim must remain opaque even inside a heading argument.
fn verb_end(input: &str, at: usize) -> Option<usize> {
    if !input[at..].starts_with("\\verb") || escaped(input, at) {
        return None;
    }
    let mut content = at + "\\verb".len();
    if input[content..].starts_with('*') {
        content += 1;
    }
    let delimiter = input[content..].chars().next()?;
    if delimiter.is_alphabetic() || delimiter.is_whitespace() {
        return None;
    }
    content += delimiter.len_utf8();
    let end = input[content..].find(delimiter)?;
    Some(content + end + delimiter.len_utf8())
}

fn delimited_math(
    input: &str,
    range: Range<usize>,
    delimiters: &[(&str, &str)],
    excluded: &[Range<usize>],
    tex: bool,
) -> Vec<Part> {
    let mut parts = vec![];
    let mut at = range.start;
    while at < range.end {
        if let Some(exclusion) = excluded.iter().find(|r| r.contains(&at)) {
            at = exclusion.end;
            continue;
        }
        if tex {
            if input[at..].starts_with('%') && !escaped(input, at) {
                at = input[at..range.end]
                    .find('\n')
                    .map_or(range.end, |n| at + n + 1);
                continue;
            }
            if let Some(end) = verb_end(input, at) {
                at = end;
                continue;
            }
        }
        let delimiter = delimiters
            .iter()
            .find(|(open, _)| input[at..range.end].starts_with(open) && !escaped(input, at));
        if let Some((open, close)) = delimiter {
            let content = at + open.len();
            let mut seek = content;
            let mut matched = false;
            while seek < range.end {
                let Some(relative) = input[seek..range.end].find(close) else {
                    break;
                };
                let stop = seek + relative;
                let end = stop + close.len();
                if escaped(input, stop) {
                    seek = end;
                    continue;
                }
                if excluded.iter().any(|r| r.start < end && r.end > at) {
                    break;
                }
                parts.push(Part {
                    range: at..end,
                    kind: NodeKind::Formula,
                    title: "公式".into(),
                    latex: Some(input[content..stop].into()),
                    level: None,
                });
                at = end;
                matched = true;
                break;
            }
            if matched {
                continue;
            }
        }
        at += input[at..]
            .chars()
            .next()
            .expect("character boundary")
            .len_utf8();
    }
    parts
}

fn markdown_title_with_math(
    input: &str,
    heading: Range<usize>,
    formulas: &[Range<usize>],
    fallback: &str,
) -> String {
    let mut formulas: Vec<_> = formulas
        .iter()
        .filter(|r| r.start >= heading.start && r.end <= heading.end)
        .collect();
    if formulas.is_empty() {
        return fallback.into();
    }
    formulas.sort_by_key(|r| r.start);
    let raw = &input[heading.clone()];
    let mut prefix = String::from("ENTWURFMATHTOKEN");
    while raw.contains(&prefix) {
        prefix.push('X');
    }
    let replacements: Vec<_> = formulas
        .iter()
        .enumerate()
        .map(|(i, r)| (format!("{prefix}{i}END"), input[(*r).clone()].to_string()))
        .collect();
    let mut protected = raw.to_string();
    for (range, (token, _)) in formulas.iter().zip(&replacements).rev() {
        protected.replace_range(
            range.start - heading.start..range.end - heading.start,
            token,
        );
    }
    let mut title = String::new();
    for event in Parser::new(&protected) {
        match event {
            Event::Text(value) | Event::Code(value) => title.push_str(&value),
            Event::SoftBreak | Event::HardBreak => title.push(' '),
            _ => {}
        }
    }
    for (token, raw) in replacements {
        title = title.replace(&token, &raw);
    }
    title
}
