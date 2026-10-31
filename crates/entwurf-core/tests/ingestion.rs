use entwurf_core::ingest::{hash_bytes, ingest_path, ingest_paths, IngestOptions};
use entwurf_core::ir::{NodeKind, UnverifiedKind};
use std::fs;

fn ingest(text: &str, ext: &str, max: usize) -> entwurf_core::ingest::IngestedSource {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(format!("中文.{ext}"));
    fs::write(&path, text).unwrap();
    ingest_path(
        &path,
        &IngestOptions {
            max_chars_per_node: max,
            ..IngestOptions::standard()
        },
    )
    .unwrap()
}

#[test]
fn every_long_utf8_paragraph_byte_survives_chunking_with_exact_hashes() {
    let text = "概率论与随机变量🙂。".repeat(500);
    let result = ingest(&text, "md", 80);
    assert!(result.nodes.len() > 1);
    assert_eq!(
        result
            .nodes
            .iter()
            .map(|n| n.body.as_str())
            .collect::<String>(),
        text
    );
    for node in result.nodes {
        let a = &node.anchors[0];
        let quote = &text[a.char_start.unwrap() as usize..a.char_end.unwrap() as usize];
        assert_eq!(a.quote_hash, Some(hash_bytes(quote.as_bytes())));
        assert_eq!(node.body, quote);
        assert!(node.body.chars().count() <= 80);
    }
}

#[test]
fn commonmark_hierarchy_math_and_code_are_distinct() {
    let text = "概率论\n======\n\n### 三级\n\n前 $P(A)$ 后。\n\n$$\nP(B) = 1\n$$\n\n```latex\n$not_math$\n# not_heading\n```\n\n`$also_code$`\n\n\\[x+y\\]\n";
    let result = ingest(text, "md", 80);
    let formulas: Vec<_> = result
        .nodes
        .iter()
        .filter(|n| n.kind == NodeKind::Formula)
        .collect();
    assert_eq!(formulas.len(), 3);
    assert!(formulas.iter().any(|n| n.latex.as_deref() == Some("P(A)")));
    assert!(formulas
        .iter()
        .any(|n| n.latex.as_deref().unwrap().contains("P(B) = 1")));
    let headings: Vec<_> = result
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Chapter | NodeKind::Section))
        .collect();
    assert_eq!(headings.len(), 2);
    assert_eq!(
        headings[1].parent_id.as_deref(),
        Some(headings[0].id.as_str())
    );
    assert!(result.nodes.iter().any(|n| n.body.contains("$not_math$")));
    for n in &result.nodes {
        let a = &n.anchors[0];
        assert_eq!(
            a.quote_hash,
            Some(hash_bytes(
                text[a.char_start.unwrap() as usize..a.char_end.unwrap() as usize].as_bytes()
            ))
        );
    }
}

#[test]
fn latex_keeps_full_nested_environments_and_chapters() {
    let env =
        "\\begin{align}\na &= \\begin{aligned}b+c\\\\d+e\\end{aligned} \\\\\nz &= 2\n\\end{align}";
    let text = format!("\\chapter{{概率论}}\n\\section{{随机变量}}\n{env}\n尾部文字");
    let result = ingest(&text, "tex", 20);
    assert!(result.nodes.iter().any(|n| n.kind == NodeKind::Chapter));
    assert!(result.nodes.iter().any(|n| n.latex.as_deref() == Some(env)));
    assert!(result.nodes.iter().any(|n| n.body.contains("尾部文字")));
}

#[test]
fn formula_confirmation_survives_into_ir() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("formula.md");
    fs::write(&path, "$x=1$").unwrap();
    let ir = ingest_paths(&[path], &IngestOptions::standard())
        .unwrap()
        .into_ir("p", "t", "title");
    assert_eq!(
        ir.unverified
            .iter()
            .filter(|u| u.kind == UnverifiedKind::Formula)
            .count(),
        1
    );
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/public/ingestion")
        .join(name)
}

#[test]
fn missing_ocr_and_pdfium_are_diagnostics_persisted_in_ir() {
    let dir = tempfile::tempdir().unwrap();
    let options = IngestOptions {
        binary_dir: Some(dir.path().into()),
        pdfium_dir: Some(dir.path().into()),
        ..IngestOptions::standard()
    };
    let report = ingest_paths(
        &[fixture("bilingual.png"), fixture("two-pages.pdf")],
        &options,
    )
    .unwrap();
    assert_eq!(report.node_count(), 0);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == "ocr_unavailable"));
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == "pdfium_unavailable"));
    let ir = report.into_ir("p", "t", "title");
    assert!(ir.sources[0]
        .notes
        .iter()
        .any(|n| n.contains("ocr_unavailable")));
    assert!(ir.unverified.iter().any(|u| u.kind == UnverifiedKind::Ocr));
    assert!(ir
        .unverified
        .iter()
        .any(|u| u.reason.contains("pdfium_unavailable")));
}

#[test]
fn project_import_copies_deduplicates_and_reopens_without_originals() {
    use entwurf_core::{
        ingest::ingest_project_paths,
        project::{Project, VersionReason},
        storage::{load_project, save_project, ProjectPaths},
    };
    let dir = tempfile::tempdir().unwrap();
    let paths = ProjectPaths::new(dir.path().join("项目"));
    let a = dir.path().join("a.md");
    let b = dir.path().join("b.txt");
    fs::write(&a, "# 概率\n\n$x=1$").unwrap();
    fs::copy(&a, &b).unwrap();
    let report =
        ingest_project_paths(&paths, &[a.clone(), b.clone()], &IngestOptions::standard()).unwrap();
    assert_eq!(report.sources.len(), 1);
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.code == "duplicate_source"));
    assert!(std::path::Path::new(&report.sources[0].source.stored_path)
        .starts_with(paths.sources_dir()));
    let mut project = Project::new("概率");
    let task = project.add_task("第一章", None).id.clone();
    let ir = report.into_ir(&project.id, &task, "第一章");
    project
        .save_version(&task, ir, VersionReason::Ingested)
        .unwrap();
    save_project(&paths, &project).unwrap();
    fs::remove_file(a).unwrap();
    fs::remove_file(b).unwrap();
    let loaded = load_project(&paths).unwrap();
    let ir = &loaded.tasks[0].versions[0].ir;
    assert_eq!(ir.sources.len(), 1);
    assert_eq!(ir.unverified.len(), 1);
    assert!(!ir.edges.is_empty());
    let source = &ir.sources[0];
    let stored = std::path::Path::new(&source.stored_path);
    let bytes = fs::read(stored).unwrap();
    assert_eq!(hash_bytes(&bytes), source.content_hash);
    let again = entwurf_core::ingest::ingest_source(
        source.clone(),
        &bytes,
        stored,
        &IngestOptions::standard(),
    )
    .unwrap();
    assert_eq!(again.nodes.len(), ir.nodes.len());
}

#[test]
#[ignore = "requires pinned PDFium: ENTWURF_PDFIUM_DIR"]
fn real_pdfium_extracts_actual_two_page_anchors() {
    let result = ingest_path(&fixture("two-pages.pdf"), &IngestOptions::standard()).unwrap();
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    assert_eq!(result.nodes.len(), 2);
    for (i, n) in result.nodes.iter().enumerate() {
        assert_eq!(n.anchors[0].page, Some(i as u32 + 1));
        assert!(n.anchors[0].region.is_some());
        assert!(n
            .body
            .contains(if i == 0 { "PAGE ONE" } else { "PAGE TWO" }));
        let note = &result.source.notes[i];
        let transcript: serde_json::Value =
            serde_json::from_str(note.strip_prefix("ingestion_transcript:").unwrap()).unwrap();
        let text = transcript["text"].as_str().unwrap();
        let a = &n.anchors[0];
        assert_eq!(
            a.quote_hash,
            Some(hash_bytes(
                text[a.char_start.unwrap() as usize..a.char_end.unwrap() as usize].as_bytes()
            ))
        );
    }
}

#[test]
#[ignore = "requires pinned PDFium: ENTWURF_PDFIUM_DIR"]
fn real_scanned_pdf_reports_missing_ocr() {
    let dir = tempfile::tempdir().unwrap();
    let options = IngestOptions {
        binary_dir: Some(dir.path().into()),
        ..IngestOptions::standard()
    };
    let result = ingest_path(&fixture("scanned.pdf"), &options).unwrap();
    assert!(result.nodes.is_empty());
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "ocr_unavailable"),
        "{:?}",
        result.diagnostics
    );
}

#[test]
#[ignore = "requires pinned PDFium and Tesseract chi_sim+eng; ENTWURF_TEST_OCR_DIR optionally selects Tesseract"]
fn real_bilingual_image_and_scanned_pdf_keep_ocr_uncertainty() {
    let options = IngestOptions {
        binary_dir: std::env::var_os("ENTWURF_TEST_OCR_DIR").map(Into::into),
        ..IngestOptions::standard()
    };
    let report = ingest_paths(
        &[fixture("bilingual.png"), fixture("scanned.pdf")],
        &options,
    )
    .unwrap();
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    assert_eq!(report.sources.len(), 2);
    for source in &report.sources {
        let content = source
            .nodes
            .iter()
            .map(|n| n.body.as_str())
            .collect::<String>();
        assert!(content.contains("Probability"), "{content}");
        assert!(content.contains("概率"), "{content}");
        assert!(source.nodes.iter().all(|n| !n.verified
            && n.tags.iter().any(|t| t == "ocr")
            && n.anchors[0].region.is_some()));
    }
    let ir = report.into_ir("p", "t", "OCR");
    assert_eq!(
        ir.unverified
            .iter()
            .filter(|u| u.kind == UnverifiedKind::Ocr)
            .count(),
        ir.nodes.len()
    );
}

#[test]
fn math_inside_headings_and_nested_theorems_still_requires_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let md = dir.path().join("math.md");
    let tex = dir.path().join("math.tex");
    fs::write(&md, "# 条件概率 $P(A)$\n").unwrap();
    fs::write(
        &tex,
        "\\begin{theorem}Exact claim. \\begin{equation}x=1\\end{equation}\\end{theorem}",
    )
    .unwrap();
    let report = ingest_paths(&[md, tex], &IngestOptions::standard()).unwrap();
    assert!(report.sources[0]
        .nodes
        .iter()
        .any(|n| n.latex.as_deref() == Some("P(A)")));
    let ir = report.into_ir("p", "t", "title");
    assert_eq!(
        ir.unverified
            .iter()
            .filter(|u| u.kind == UnverifiedKind::Formula)
            .count(),
        2
    );
}

#[test]
#[ignore = "requires pinned PDFium: ENTWURF_PDFIUM_DIR"]
fn real_pdf_page_load_failure_keeps_later_pages_and_blocks_review() {
    let report = ingest_paths(&[fixture("bad-middle.pdf")], &IngestOptions::standard()).unwrap();
    assert_eq!(
        report.sources[0]
            .nodes
            .iter()
            .map(|n| n.anchors[0].page)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(3)]
    );
    assert!(report.sources[0].nodes[1].body.contains("PAGE THREE"));
    assert!(report.has_errors());
    let failure = report
        .diagnostics
        .iter()
        .find(|d| d.code == "pdf_page_load_failed")
        .unwrap();
    assert!(failure.message.contains("Page 2"));
    let ir = report.into_ir("p", "t", "partial PDF");
    assert!(ir.sources[0]
        .notes
        .iter()
        .any(|n| n.contains("pdf_page_load_failed")));
    assert!(ir
        .unverified
        .iter()
        .any(|u| u.reason.contains("pdf_page_load_failed") && u.reason.contains("Page 2")));
    assert_review_blocks_export_then_can_be_cleared(ir);
}

#[test]
fn latex_and_markdown_tex_heading_math_has_exact_linked_review_nodes() {
    for (ext, text, expected) in [
        ("tex", r"\section{The $P(A)$ theorem}", "P(A)"),
        ("tex", r"\chapter{The \(P(A)\) theorem}", "P(A)"),
        ("md", r"# The \(P(A)\) theorem", "P(A)"),
        ("md", r"# The \[P(A)\] theorem", "P(A)"),
    ] {
        let source = ingest(text, ext, 80);
        let heading = source
            .nodes
            .iter()
            .find(|n| matches!(n.kind, NodeKind::Chapter | NodeKind::Section))
            .unwrap();
        assert!(
            heading.title.contains(if text.contains(r"\(") {
                r"\(P(A)\)"
            } else if text.contains(r"\[") {
                r"\[P(A)\]"
            } else {
                "$P(A)$"
            }),
            "{}",
            heading.title
        );
        let formulas: Vec<_> = source
            .nodes
            .iter()
            .filter(|n| n.kind == NodeKind::Formula)
            .collect();
        assert_eq!(formulas.len(), 1, "{ext}: {text}");
        assert_eq!(formulas[0].latex.as_deref(), Some(expected));
        assert_eq!(formulas[0].parent_id.as_deref(), Some(heading.id.as_str()));
        let a = &formulas[0].anchors[0];
        let quote = &text[a.char_start.unwrap() as usize..a.char_end.unwrap() as usize];
        assert_eq!(a.quote_hash, Some(hash_bytes(quote.as_bytes())));
        assert_eq!(formulas[0].body, quote);
        let ir = entwurf_core::ingest::IngestReport {
            sources: vec![source],
            diagnostics: vec![],
        }
        .into_ir("p", "t", "heading");
        assert_eq!(
            ir.unverified
                .iter()
                .filter(|u| u.kind == UnverifiedKind::Formula)
                .count(),
            1
        );
    }
    for (ext, text) in [
        ("md", r"# Code `\(not_math\)` and \\(escaped\\)"),
        ("tex", r"\section{Code \verb|$not_math$| and \$escaped\$}"),
    ] {
        assert!(
            ingest(text, ext, 80)
                .nodes
                .iter()
                .all(|n| n.kind != NodeKind::Formula),
            "{text}"
        );
    }
}

#[test]
#[ignore = "requires Tesseract eng; ENTWURF_TEST_OCR_DIR optionally selects Tesseract"]
fn real_partial_ocr_language_failure_retains_text_and_warning() {
    let options = IngestOptions {
        binary_dir: std::env::var_os("ENTWURF_TEST_OCR_DIR").map(Into::into),
        ocr_languages: Some("missing_entwurf_language+eng".into()),
        ..IngestOptions::standard()
    };
    let report = ingest_paths(&[fixture("bilingual.png")], &options).unwrap();
    assert!(report.sources[0]
        .nodes
        .iter()
        .any(|n| n.body.contains("Probability")));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| d.code == "ocr_language_unavailable"
                && d.message.contains("missing_entwurf_language")),
        "{:?}",
        report.diagnostics
    );
    let ir = report.into_ir("p", "t", "partial OCR");
    assert!(ir.sources[0]
        .notes
        .iter()
        .any(|n| n.contains("ocr_language_unavailable")));
    assert!(ir
        .unverified
        .iter()
        .any(|u| u.kind == UnverifiedKind::Ocr && u.reason.contains("missing_entwurf_language")));
    assert!(ir.nodes.iter().all(|n| !n.verified));
    assert_review_blocks_export_then_can_be_cleared(ir);
}

#[test]
fn latex_optional_heading_arguments_and_spaces_preserve_hierarchy() {
    let text =
        "\\chapter [Short $c$]{Actual chapter}\n\\section* [Short $s$ title] {Actual $x$ title}";
    let result = ingest(text, "tex", 80);
    let headings: Vec<_> = result
        .nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Chapter | NodeKind::Section))
        .collect();
    assert_eq!(headings.len(), 2);
    assert_eq!(headings[0].title, "Actual chapter");
    assert_eq!(headings[1].title, "Actual $x$ title");
    assert_eq!(
        headings[1].parent_id.as_deref(),
        Some(headings[0].id.as_str())
    );
    for formula in ["c", "s", "x"] {
        assert!(
            result
                .nodes
                .iter()
                .any(|n| n.latex.as_deref() == Some(formula)),
            "missing {formula}"
        );
    }
}

#[test]
fn latex_nested_verbatim_does_not_break_outer_environment() {
    for verbatim in ["verbatim", "lstlisting", "minted"] {
        let text=format!("\\begin{{example}}\n\\begin{{{verbatim}}}\n\\begin{{fake}}\n$literal$\n\\end{{{verbatim}}}\ntext\n\\end{{example}}");
        let result = ingest(&text, "tex", 20);
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
        assert_eq!(result.nodes.len(), 1);
        assert_eq!(result.nodes[0].kind, NodeKind::Example);
        assert_eq!(result.nodes[0].body, text);
    }
}

#[test]
fn reparse_replaces_generated_notes_but_preserves_user_notes() {
    let input = "# original";
    let mut old = ingest(input, "md", 80).source;
    old.notes = vec![
        "user annotation".into(),
        "ingestion_transcript:{\"page\":1,\"text\":\"obsolete\"}".into(),
        "ingestion_diagnostic:{\"code\":\"old_failure\"}".into(),
    ];
    let result = entwurf_core::ingest::ingest_source(
        old,
        input.as_bytes(),
        std::path::Path::new("unused.md"),
        &IngestOptions::standard(),
    )
    .unwrap();
    assert_eq!(result.source.notes, vec!["user annotation"]);
}

fn assert_review_blocks_export_then_can_be_cleared(mut ir: entwurf_core::KnowledgeIr) {
    use entwurf_core::{
        compiler::{compile, CompileOptions},
        ir::Severity,
        validate::{validate_ir, ValidationOptions},
        EntwurfError, StyleSpec,
    };
    assert!(!ir.unverified.is_empty());
    let report = validate_ir(&ir, &ValidationOptions::default()).unwrap();
    for item in &ir.unverified {
        assert!(
            report.findings.iter().any(|f| f.code == "unresolved_review"
                && f.severity == Severity::Blocking
                && f.message == item.reason
                && f.node_id == item.node_id
                && f.evidence.as_deref() == Some(item.text.as_str())),
            "missing review finding: {}",
            item.reason
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("export");
    let mut options = CompileOptions::new(&output);
    options.raster_dpi = 36.0;
    assert!(matches!(
        compile(&ir, &StyleSpec::default(), &options),
        Err(EntwurfError::ValidationBlocked { .. })
    ));
    assert!(!output.exists(), "blocked export created output files");
    // The caller's explicit review clears the queue; grounded source nodes still
    // have their original verified=false flag and are not blanket-blocked.
    ir.unverified.clear();
    assert!(ir.nodes.iter().any(|n| !n.verified));
    let artifacts = compile(&ir, &StyleSpec::default(), &options).unwrap();
    assert!(artifacts.pdf_path.exists());
    assert!(artifacts.svg_paths[0].exists());
    assert!(artifacts.png_paths[0].exists());
}

#[test]
fn unresolved_ingestion_and_formula_reviews_block_export_until_cleared() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("probability.md");
    fs::write(&path, "# Probability\n\n$P(A)=1$\n").unwrap();
    let options = IngestOptions {
        binary_dir: Some(dir.path().into()),
        ..IngestOptions::standard()
    };
    let report = ingest_paths(&[path, fixture("bilingual.png")], &options).unwrap();
    assert_review_blocks_export_then_can_be_cleared(report.into_ir("p", "t", "source review"));
}
