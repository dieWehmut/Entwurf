//! A sealed, offline Typst world. No source can access the host filesystem,
//! packages, clock, environment or system font installation.
use crate::error::{EntwurfError, Result};
use crate::ir::{KnowledgeIr, KnowledgeNode, StyleSpec};
use crate::layout::LayoutOptions;
use crate::model::{Rect, TextLine};
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::layout::{Abs, Frame, FrameItem, Point, Size};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

pub const ENGINE: &str = "entwurf-typst-0.15.1-mitex-0.2.4-scene-v1";
static FONTS: LazyLock<Vec<Font>> = LazyLock::new(|| {
    typst_assets::fonts()
        .chain(std::iter::once(
            include_bytes!("../../../assets/fonts/LXGWWenKai-Regular.ttf").as_slice(),
        ))
        .flat_map(|data| Font::iter(Bytes::new(data)))
        .collect()
});
static BOOK: LazyLock<LazyHash<FontBook>> =
    LazyLock::new(|| LazyHash::new(FontBook::from_fonts(FONTS.iter())));
static LIBRARY: LazyLock<LazyHash<Library>> = LazyLock::new(|| LazyHash::new(Library::default()));
struct SealedWorld {
    source: Source,
}
impl World for SealedWorld {
    fn library(&self) -> &LazyHash<Library> {
        &LIBRARY
    }
    fn book(&self) -> &LazyHash<FontBook> {
        &BOOK
    }
    fn main(&self) -> FileId {
        self.source.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main() {
            Ok(self.source.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        if id == self.main() {
            Ok(Bytes::from_string(self.source.text().to_owned()))
        } else {
            Err(FileError::AccessDenied)
        }
    }
    fn font(&self, index: usize) -> Option<Font> {
        FONTS.get(index).cloned()
    }
    fn today(&self, _: Option<Duration>) -> Option<Datetime> {
        None
    }
}

pub fn compile_source(source: &str) -> Result<PagedDocument> {
    let world = SealedWorld {
        source: Source::detached(source),
    };
    let result = typst::compile::<PagedDocument>(&world);
    typst::comemo::evict(10);
    // Missing fonts/glyphs and layout warnings cannot become successful exports.
    if !result.warnings.is_empty() {
        return Err(EntwurfError::Render(format!(
            "Typst warnings: {:?}",
            result.warnings
        )));
    }
    result
        .output
        .map_err(|errors| EntwurfError::Render(format!("Typst: {errors:?}")))
}

/// Escape as a Typst string literal, never as executable markup.
pub fn literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Intentionally bounded TeX subset, with no macro expansion or Typst escapes.
/// The original LaTeX remains byte-for-byte in Knowledge IR.
pub fn convert_math(latex: &str) -> Result<String> {
    let fail = |message: String| EntwurfError::Render(format!("unsupported LaTeX: {message}"));
    if latex.len() > 32_768 || latex.contains(['#', '$', '%', '`']) {
        return Err(fail(
            "formula is too large or contains code/delimiter/comment syntax".into(),
        ));
    }
    if latex.contains(r"\\") && !latex.contains(r"\begin{") {
        return Err(fail(
            "line breaks require an explicit supported math environment".into(),
        ));
    }
    let allowed = "frac dfrac tfrac sqrt sum prod int iint iiint lim min max sup inf log ln exp sin cos tan cot sec csc arcsin arccos arctan sinh cosh tanh det gcd Pr mid cap cup bigcap bigcup subset subseteq supset supseteq in notin ni emptyset varnothing Omega omega alpha beta gamma Gamma delta Delta epsilon varepsilon zeta eta theta Theta vartheta iota kappa lambda Lambda mu nu xi Xi pi Pi varpi rho varrho sigma Sigma varsigma tau upsilon Upsilon phi Phi varphi chi psi Psi partial nabla infty forall exists nexists neg land lor lnot wedge vee implies iff Rightarrow Leftarrow Leftrightarrow rightarrow leftarrow leftrightarrow to mapsto approx sim simeq equiv cong ne neq le leq ge geq ll gg propto pm mp times div cdot cdots ldots vdots ddots ast star circ bullet oplus otimes perp parallel angle degree left right big Big bigg Bigg overline underline hat widehat bar vec dot ddot tilde widetilde mathbb mathcal mathrm mathbf mathit mathsf mathtt text operatorname underbrace overbrace binom dbinom tbinom begin end quad qquad lvert rvert vert Vert lVert rVert langle rangle lbrace rbrace braces limits nolimits";
    let commands = regex::Regex::new(r"\\([a-zA-Z]+|.)").expect("static regex");
    for capture in commands.captures_iter(latex) {
        let command = &capture[1];
        if !allowed.split_whitespace().any(|name| name == command)
            && !["{", "}", "|", ",", ";", ":", "!", " ", "_", "\\"].contains(&command)
        {
            return Err(fail(format!("command \\{command}")));
        }
    }
    let env = regex::Regex::new(r"\\(?:begin|end)\{([^}]+)\}").expect("static regex");
    for capture in env.captures_iter(latex) {
        if ![
            "matrix", "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix", "cases", "aligned",
        ]
        .contains(&&capture[1])
        {
            return Err(fail(format!("environment {}", &capture[1])));
        }
    }
    let converted = mitex::convert_math_no_macro(latex, None).map_err(fail)?;
    if converted.contains(['#', '$', '`']) {
        return Err(fail("conversion contains executable syntax".into()));
    }
    Ok(converted)
}

/// Parse only explicit inline/display TeX delimiters, preserving every prose
/// segment as a literal. Unmatched delimiters are errors, never raw math output.
fn prose(text: &str) -> Result<String> {
    let mut out = String::new();
    for span in crate::math_text::parse(text)? {
        match span {
            crate::math_text::Span::Text(text) => {
                out.push_str(&format!("#text({})", literal(text)))
            }
            crate::math_text::Span::Math { latex, block } => {
                let converted = convert_math(latex)?;
                out.push_str(&format!("#math.equation(block: {block}, $ {converted} $)"));
            }
        }
    }
    Ok(out)
}

pub fn prelude(style: &StyleSpec, size: f32) -> Result<String> {
    let family = &style.typography.family;
    if !FONTS
        .iter()
        .any(|font| font.info().family.eq_ignore_ascii_case(family))
    {
        return Err(EntwurfError::Render(format!(
            "font {family:?} is not bundled"
        )));
    }
    for color in [
        &style.colors.background,
        &style.colors.surface,
        &style.colors.text,
        &style.colors.muted,
        &style.colors.accent,
        &style.colors.border,
    ] {
        if color.len() != 7
            || !color.starts_with('#')
            || !color[1..].bytes().all(|c| c.is_ascii_hexdigit())
        {
            return Err(EntwurfError::Render(format!("invalid color {color:?}")));
        }
    }
    Ok(format!("#let sect = sym.inter\n#set text(font: ({}, \"Libertinus Serif\", \"LXGW WenKai\"), size: {size}pt, fill: rgb({}), lang: \"zh\")\n#set par(leading: {}pt, spacing: 0pt)\n#show math.equation: set text(font: \"New Computer Modern Math\")\n", literal(family), literal(&style.colors.text), size * (style.typography.line_height-1.0).max(0.0)))
}

#[derive(Clone)]
pub struct MeasuredCard {
    pub frame: Frame,
    pub height: f32,
    pub lines: Vec<TextLine>,
    pub bounds: Rect,
    pub minimum_font: f32,
}
static CACHE: LazyLock<Mutex<BTreeMap<String, Arc<MeasuredCard>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// Labeled graph relationships remain readable even when their endpoints land
/// on different pages. This display-only annotation does not change the IR.
pub fn display_node(ir: &KnowledgeIr, node: &KnowledgeNode) -> KnowledgeNode {
    let mut display = node.clone();
    for edge in ir.edges.iter().filter(|e| e.from == node.id) {
        let relation = match edge.edge_type {
            crate::ir::EdgeKind::Contains if edge.label.is_none() => continue,
            crate::ir::EdgeKind::Contains => "包含",
            crate::ir::EdgeKind::DependsOn => "依赖",
            crate::ir::EdgeKind::Related => "关联",
            crate::ir::EdgeKind::ExampleOf => "示例",
            crate::ir::EdgeKind::Contradicts => "矛盾",
        };
        if let Some(target) = ir.node(&edge.to) {
            let label = edge.label.as_deref().unwrap_or(relation);
            display
                .body
                .push_str(&format!("\n→ {label}：{}", target.title));
        }
    }
    display
}

pub fn measure_card(
    node: &KnowledgeNode,
    width: f32,
    style: &StyleSpec,
    options: &LayoutOptions,
) -> Result<Arc<MeasuredCard>> {
    let body_size = style.typography.body_pt.clamp(
        options.min_font_pt.max(8.0),
        options.max_font_pt.max(options.min_font_pt).max(8.0),
    );
    let title_size = (body_size * 1.24).max(9.0);
    let mut source = prelude(style, body_size)?;
    let body = if node.latex.as_deref().is_some_and(|latex| {
        let raw = node.body.trim();
        raw == latex
            || ["$$", "$"].iter().any(|d| {
                raw.strip_prefix(d)
                    .and_then(|r| r.strip_suffix(d))
                    .is_some_and(|r| r.trim() == latex.trim())
            })
            || raw
                .strip_prefix(r"\[")
                .and_then(|r| r.strip_suffix(r"\]"))
                .is_some_and(|r| r.trim() == latex.trim())
    }) {
        ""
    } else {
        &node.body
    };
    source.push_str(&format!("#show math.equation: set text(size: 18pt)\n#set page(width: {width}pt, height: auto, margin: 0pt)\n#text(size: {title_size}pt, weight: \"bold\")[{}]\n#parbreak()\n#v(4pt)\n{}\n", prose(&node.title)?, prose(body)?));
    if let Some(latex) = node.latex.as_deref().filter(|s| !s.is_empty()) {
        let converted = convert_math(latex)?;
        source.push_str("#show math.equation: set text(size: 18pt)\n");
        source.push_str(&format!(
            "\n#v(4pt)\n#math.equation(block: true, $ {converted} $)\n"
        ));
    }
    let key = crate::ingest::hash_bytes(format!("{ENGINE}\n{source}").as_bytes());
    if let Some(value) = CACHE.lock().expect("cache lock").get(&key).cloned() {
        return Ok(value);
    }
    let doc = compile_source(&source)?;
    if doc.pages().len() != 1 {
        return Err(EntwurfError::Render("card unexpectedly paginated".into()));
    }
    let original_frame = doc.pages()[0].frame.clone();
    let mut lines = Vec::new();
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut minimum_font = f32::INFINITY;
    inspect(
        &original_frame,
        Point::zero(),
        &mut lines,
        &mut bounds,
        &mut minimum_font,
    )?;
    // Account for glyph ink protruding beyond Typst's nominal ascent.
    let dx = (-bounds[0]).max(0.0);
    let dy = (-bounds[1]).max(0.0);
    let height = (original_frame.height().to_pt() as f32).max(bounds[3]) + dy;
    let mut frame = Frame::soft(Size::new(Abs::pt(width as f64), Abs::pt(height as f64)));
    frame.push_frame(
        Point::new(Abs::pt(dx as f64), Abs::pt(dy as f64)),
        original_frame,
    );
    for line in &mut lines {
        line.x += dx;
        line.baseline += dy;
    }
    bounds[0] += dx;
    bounds[2] += dx;
    bounds[1] += dy;
    bounds[3] += dy;
    if minimum_font < 8.0 {
        return Err(EntwurfError::Render(format!("node {} has math scripts below 8pt; simplify visual nesting or widen/enlarge the formula",node.id)));
    }
    if bounds[0] < -0.5 || bounds[2] > width + 0.5 || bounds[1] < -0.5 || bounds[3] > height + 0.5 {
        return Err(EntwurfError::Render(format!(
            "node {} shaped content exceeds its card: {:?}, available {width} × {height}pt",
            node.id, bounds
        )));
    }
    let measured = Arc::new(MeasuredCard {
        frame,
        height,
        lines,
        bounds: Rect::new(
            bounds[0],
            bounds[1],
            bounds[2] - bounds[0],
            bounds[3] - bounds[1],
        ),
        minimum_font,
    });
    let mut cache = CACHE.lock().expect("cache lock");
    if cache.len() >= 256 {
        cache.clear();
    }
    cache.insert(key, measured.clone());
    Ok(measured)
}

pub fn heading_frame(text: &str, width: f32, style: &StyleSpec) -> Result<Frame> {
    let size = style.typography.title_pt;
    if !size.is_finite() || size < 8.0 || size > 56.0 {
        return Err(EntwurfError::Render("title font must be 8–56pt".into()));
    }
    let source = format!(
        "{}#set page(width: {width}pt, height: auto, margin: 0pt)\n{}",
        prelude(style, size)?,
        prose(text)?
    );
    let doc = compile_source(&source)?;
    let frame = doc.pages()[0].frame.clone();
    let mut lines = Vec::new();
    let mut bounds = [
        f32::INFINITY,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
    ];
    let mut minimum = f32::INFINITY;
    inspect(&frame, Point::zero(), &mut lines, &mut bounds, &mut minimum)?;
    let dx = (-bounds[0]).max(0.0);
    let dy = (-bounds[1]).max(0.0);
    let height = (frame.height().to_pt() as f32).max(bounds[3]) + dy;
    if bounds[2] + dx > width + 0.5 || height > 56.0 || minimum < 8.0 {
        return Err(EntwurfError::Render(
            "document heading exceeds the reserved header at readable size".into(),
        ));
    }
    let mut padded = Frame::soft(Size::new(Abs::pt(width as f64), Abs::pt(height as f64)));
    padded.push_frame(Point::new(Abs::pt(dx as f64), Abs::pt(dy as f64)), frame);
    Ok(padded)
}

fn inspect(
    frame: &Frame,
    offset: Point,
    lines: &mut Vec<TextLine>,
    bounds: &mut [f32; 4],
    minimum_font: &mut f32,
) -> Result<()> {
    for (pos, item) in frame.items() {
        let pos = offset + *pos;
        match item {
            FrameItem::Group(group) => {
                if group.transform != typst::layout::Transform::identity() || group.clip.is_some() {
                    return Err(EntwurfError::Render(
                        "unsupported transformed/clipped text frame".into(),
                    ));
                }
                inspect(&group.frame, pos, lines, bounds, minimum_font)?;
            }
            FrameItem::Shape(shape, _) => {
                let b = shape.bbox(true);
                bounds[0] = bounds[0].min((pos.x + b.min.x).to_pt() as f32);
                bounds[1] = bounds[1].min((pos.y + b.min.y).to_pt() as f32);
                bounds[2] = bounds[2].max((pos.x + b.max.x).to_pt() as f32);
                bounds[3] = bounds[3].max((pos.y + b.max.y).to_pt() as f32);
            }
            FrameItem::Text(text) => {
                if text.glyphs.iter().any(|glyph| glyph.id == 0) {
                    return Err(EntwurfError::Render(format!(
                        "missing glyph in {:?}",
                        text.text
                    )));
                }
                // Actual script glyph sizes are exposed and checked against the print floor.
                *minimum_font = minimum_font.min(text.size.to_pt() as f32);
                let b = text.bbox();
                for (i, v) in [
                    (0, pos.x + b.min.x),
                    (1, pos.y + b.min.y.min(b.max.y)),
                    (2, pos.x + b.max.x),
                    (3, pos.y + b.max.y.max(b.min.y)),
                ] {
                    if i < 2 {
                        bounds[i] = bounds[i].min(v.to_pt() as f32);
                    } else {
                        bounds[i] = bounds[i].max(v.to_pt() as f32);
                    }
                }
                lines.push(TextLine {
                    text: text.text.to_string(),
                    x: pos.x.to_pt() as f32,
                    baseline: pos.y.to_pt() as f32,
                    font_size: text.size.to_pt() as f32,
                    bold: text.font.font().info().variant.weight >= typst::text::FontWeight::BOLD,
                });
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;
    fn node() -> KnowledgeNode {
        KnowledgeNode {
            id: "n".into(),
            kind: NodeKind::Formula,
            title: "条件概率".into(),
            body: "条件概率公式保留原文。".into(),
            latex: Some(r"P(A \mid B)=\frac{P(A \cap B)}{P(B)}".into()),
            anchors: vec![],
            importance: Importance::High,
            origin: NodeOrigin::Source,
            parent_id: None,
            tags: vec![],
            verified: true,
            review_note: None,
        }
    }
    #[test]
    fn chinese_math_are_shaped_with_fonts_and_cached() {
        let node = node();
        let style = StyleSpec::default();
        let opts = LayoutOptions::default();
        let a = measure_card(&node, 300.0, &style, &opts).unwrap();
        assert!(a.lines.iter().any(|l| l.text.contains("条件概率")));
        assert!(a.lines.iter().any(|l| l.text.contains('∩')));
        assert!(a.minimum_font >= 8.0, "{}", a.minimum_font);
        let b = measure_card(&node, 300.0, &style, &opts).unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        let c = measure_card(&node, 200.0, &style, &opts).unwrap();
        assert!(!Arc::ptr_eq(&a, &c));
        let mut changed = node.clone();
        changed.body.push_str("新内容");
        let d = measure_card(&changed, 300.0, &style, &opts).unwrap();
        assert!(!Arc::ptr_eq(&a, &d));
    }
    #[test]
    fn rejects_unknown_commands_and_code() {
        for value in [
            r"\\frac{1}{2}",
            r"\unknown{x}",
            r"\def\x{1}",
            "#read(\"/etc/passwd\")",
            r"\begin{document}x\end{document}",
            "x%hidden",
        ] {
            assert!(convert_math(value).is_err(), "{value}");
        }
        assert!(compile_source("#read(\"/etc/passwd\")").is_err());
        assert!(compile_source("#import \"@preview/mitex:0.2.4\": *").is_err());
    }
    #[test]
    fn untrusted_text_stays_literal() {
        let mut n = node();
        n.title = "#read(\"secret\") [$x$]".into();
        let card =
            measure_card(&n, 300.0, &StyleSpec::default(), &LayoutOptions::default()).unwrap();
        assert!(card.lines.iter().any(|l| l.text.contains("#read")));
    }
}

#[cfg(test)]
mod inline_tests {
    use super::*;
    #[test]
    fn inline_math_delimiters_convert_and_unclosed_math_fails() {
        for source in [
            "公式 $x^2$ 保留",
            r"公式 \(x^2\) 保留",
            r"公式 \[x^2\] 保留",
            "公式 $$x^2$$ 保留",
        ] {
            let converted = prose(source).unwrap();
            assert!(converted.contains("math.equation"));
            assert!(converted.contains("保留"));
        }
        assert!(prose("公式 $x").is_err());
        assert!(prose(r"公式 $\unsupported{x}$").is_err());
    }
}
