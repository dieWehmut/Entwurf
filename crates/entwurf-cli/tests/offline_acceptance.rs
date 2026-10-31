use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use entwurf_core::compiler::{compile, CompileOptions};
use entwurf_core::ingest::{hash_bytes, ingest_paths, IngestOptions};
use entwurf_core::layout::{layout, LayoutOptions};
use entwurf_core::project::{Project, VersionReason};
use entwurf_core::storage::{load_ir, load_project, save_ir, save_project, ProjectPaths};
use entwurf_core::validate::{validate_ir, validate_layout, ValidationOptions};
use entwurf_core::{KnowledgeIr, NodeKind, StyleSpec, UnverifiedKind};

const PUBLIC_SOURCE: &[u8] = include_bytes!("../../../fixtures/public/probability-chapter.md");
// Explicit, independently reviewed acceptance-fixture formulas. This is test
// review data, never an approval mechanism available to the CLI or desktop.
const REVIEWED_FORMULAS: [&str; 4] = [
    r"P(X \le x) = F_X(x)",
    r"P(A \mid B) = \frac{P(A \cap B)}{P(B)}",
    r"P(A) = \sum_{i=1}^{n} P(A \mid B_i) P(B_i)",
    r"P(B_i \mid A) = \frac{P(A \mid B_i) P(B_i)}{\sum_{j=1}^{n} P(A \mid B_j) P(B_j)}",
];

fn public_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/public/probability-chapter.md")
}

fn ingest_public(source: &Path, project_id: &str, task_id: &str) -> KnowledgeIr {
    let report = ingest_paths(&[source.into()], &IngestOptions::standard()).unwrap();
    assert!(!report.has_errors());
    let ir = report.into_ir(project_id, task_id, "概率论：随机变量与条件概率");
    assert!(
        ir.nodes.len() >= 8,
        "expected full chapter, got {}",
        ir.nodes.len()
    );
    assert!(ir.nodes.iter().all(|node| !node.anchors.is_empty()));
    assert!(!ir.edges.is_empty());
    assert_eq!(ir.unverified.len(), REVIEWED_FORMULAS.len());
    assert!(!validate_ir(&ir, &ValidationOptions::default())
        .unwrap()
        .is_passing());
    ir
}

fn review_public_probability_formulas(ir: &mut KnowledgeIr) -> Result<(), String> {
    if ir.sources.len() != 1 {
        return Err("expected one public fixture source".into());
    }
    let source = &ir.sources[0];
    let bytes = fs::read(&source.stored_path).map_err(|e| e.to_string())?;
    if bytes != PUBLIC_SOURCE || source.content_hash != hash_bytes(&bytes) {
        return Err("fixture source bytes/hash differ".into());
    }
    let input = std::str::from_utf8(&bytes).map_err(|e| e.to_string())?;
    if ir
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Formula)
        .count()
        != REVIEWED_FORMULAS.len()
    {
        return Err("unexpected formula set requires separate review".into());
    }
    let mut reviewed_nodes = BTreeSet::new();
    let mut reviewed_items = BTreeSet::new();
    for formula in REVIEWED_FORMULAS {
        let candidates: Vec<_> = ir
            .nodes
            .iter()
            .filter(|node| node.kind == NodeKind::Formula && node.latex.as_deref() == Some(formula))
            .collect();
        if candidates.len() != 1 {
            return Err(format!("formula differs from reviewed fixture: {formula}"));
        }
        let node = candidates[0];
        if node.anchors.len() != 1 {
            return Err("ambiguous source anchor".into());
        }
        let anchor = &node.anchors[0];
        if anchor.source_id != source.id {
            return Err("wrong source reference".into());
        }
        let start = anchor.char_start.ok_or("missing start")? as usize;
        let end = anchor.char_end.ok_or("missing end")? as usize;
        let quote = input.get(start..end).ok_or("invalid UTF-8 source span")?;
        if quote != format!("$${formula}$$")
            || node.body != quote
            || anchor.quote_hash.as_deref() != Some(hash_bytes(quote.as_bytes()).as_str())
        {
            return Err("formula original span/body/quote hash differs".into());
        }
        let items: Vec<_> = ir
            .unverified
            .iter()
            .filter(|item| {
                item.kind == UnverifiedKind::Formula
                    && item.node_id.as_deref() == Some(node.id.as_str())
                    && item.text == formula
            })
            .collect();
        if items.len() != 1 {
            return Err("missing or ambiguous formula review item".into());
        }
        reviewed_nodes.insert(node.id.clone());
        reviewed_items.insert(items[0].id.clone());
    }
    // Mutate only after every explicit formula review succeeded; unrelated
    // uncertainty remains blocking, even when it points at a reviewed node.
    for node in &mut ir.nodes {
        if reviewed_nodes.contains(&node.id) {
            node.verified = true;
            node.review_note = Some(
                "Acceptance fixture review: exact original UTF-8 span and quote SHA-256 confirmed"
                    .into(),
            );
        }
    }
    ir.unverified
        .retain(|item| !reviewed_items.contains(&item.id));
    Ok(())
}

fn persisted_reviewed_public(
    source: &Path,
    paths: &ProjectPaths,
    project_id: &str,
    task_id: &str,
) -> (KnowledgeIr, PathBuf) {
    let mut ir = ingest_public(source, project_id, task_id);
    let original = save_ir(paths, task_id, &ir).unwrap();
    review_public_probability_formulas(&mut ir).unwrap();
    assert!(ir.unverified.is_empty());
    assert!(ir
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Formula)
        .all(|node| node.verified));
    let reviewed = save_ir(paths, task_id, &ir).unwrap();
    assert_ne!(original, reviewed);
    assert_eq!(
        load_ir(&original).unwrap().unverified.len(),
        4,
        "review mutated original snapshot"
    );
    let reloaded = load_ir(&reviewed).unwrap();
    assert_eq!(
        serde_json::to_value(&ir).unwrap(),
        serde_json::to_value(&reloaded).unwrap()
    );
    (reloaded, reviewed)
}

#[test]
fn public_probability_chapter_compiles_offline() {
    let dir = tempfile::tempdir().unwrap();
    let (ir, _) = persisted_reviewed_public(
        &public_source(),
        &ProjectPaths::new(dir.path().join("review")),
        "project-acceptance",
        "task-acceptance",
    );
    let style = StyleSpec::default();
    let mut report = validate_ir(&ir, &ValidationOptions::default()).unwrap();
    assert!(report.is_passing(), "{:?}", report.findings);
    let document = layout(&ir, &style, &LayoutOptions::from_style(&style)).unwrap();
    validate_layout(&ir, &document, &mut report, &ValidationOptions::default()).unwrap();
    assert!(report.is_passing(), "{:?}", report.findings);
    assert!(document.pages.len() <= 3);
    let artifacts = compile(&ir, &style, &CompileOptions::new(dir.path().join("export"))).unwrap();
    assert!(artifacts.validation_passed);
    assert_eq!(artifacts.svg_paths.len(), document.pages.len());
    assert_eq!(artifacts.png_paths.len(), document.pages.len());
    for path in &artifacts.svg_paths {
        let svg = fs::read_to_string(path).unwrap();
        assert!(svg.contains("概率论"));
        assert!(svg.contains("<svg"));
    }
    assert!(fs::metadata(&artifacts.pdf_path).unwrap().len() > 1000);
    assert!(
        fs::metadata(artifacts.manifest_path.as_ref().unwrap())
            .unwrap()
            .len()
            > 100
    );
    let reloaded = load_ir(&artifacts.ir_path).unwrap();
    assert_eq!(
        serde_json::to_value(&reloaded).unwrap(),
        serde_json::to_value(&ir).unwrap()
    );
}

#[test]
fn project_persists_ir_and_manual_edit() {
    let dir = tempfile::tempdir().unwrap();
    let paths = ProjectPaths::new(dir.path().join("中文 项目"));
    let mut project = Project::new("概率论");
    let task_id = project.add_task("第一章", Some("概率论".into())).id.clone();
    let ir = ingest_public(&public_source(), &project.id, &task_id);
    let version = project
        .save_version(&task_id, ir.clone(), VersionReason::Ingested)
        .unwrap();
    let ir_path = save_ir(&paths, &task_id, &ir).unwrap();
    save_project(&paths, &project).unwrap();
    let mut loaded = load_project(&paths).unwrap();
    let mut edited = loaded.current_ir(&task_id).unwrap().clone();
    assert_eq!(edited.unverified.len(), 4);
    edited.nodes[1].title = "手工修改".into();
    let edited_version = loaded
        .save_version(&task_id, edited, VersionReason::UserEdited)
        .unwrap();
    save_project(&paths, &loaded).unwrap();
    let again = load_project(&paths).unwrap();
    assert_eq!(
        again.current_ir(&task_id).unwrap().nodes[1].title,
        "手工修改"
    );
    assert_eq!(
        again.current_ir(&task_id).unwrap().unverified.len(),
        4,
        "text edits must not silently approve formulas"
    );
    assert!(ir_path.exists());
    assert_eq!(
        again.task(&task_id).unwrap().current_version.as_deref(),
        Some(edited_version.as_str())
    );
    assert_eq!(again.task(&task_id).unwrap().versions[0].id, version);
    assert_ne!(
        again.task(&task_id).unwrap().versions[0].ir.nodes[1].title,
        "手工修改"
    );
}

#[test]
fn cli_compiles_chinese_path_and_space() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("输出 目录");
    let source = dir.path().join("概率论 完整章节.md");
    fs::write(&source, PUBLIC_SOURCE).unwrap();
    let (reviewed, snapshot) = persisted_reviewed_public(
        &source,
        &ProjectPaths::new(dir.path().join("已审核 输入")),
        "fixture-project",
        "fixture-task",
    );
    let result = Command::new(env!("CARGO_BIN_EXE_entwurf"))
        .arg("compile")
        .arg("--ir")
        .arg(&snapshot)
        .arg("--out")
        .arg(&output)
        .arg("--title")
        .arg("概率论验收")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for file in [
        "entwurf.pdf",
        "manifest.json",
        "knowledge.ir.json",
        "project.json",
    ] {
        assert!(output.join(file).exists());
    }
    let exported = load_ir(&output.join("knowledge.ir.json")).unwrap();
    let project = load_project(&ProjectPaths::new(&output)).unwrap();
    let task = &project.tasks[0];
    assert_eq!(exported.project_id, project.id);
    assert_eq!(exported.task_id, task.id);
    assert_eq!(
        serde_json::to_value(&exported).unwrap(),
        serde_json::to_value(project.current_ir(&task.id).unwrap()).unwrap()
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["project_id"], project.id);
    assert_eq!(manifest["task_id"], task.id);
    assert_eq!(manifest["ir_id"], exported.id);
    assert_eq!(exported.nodes.len(), reviewed.nodes.len());
    assert_eq!(
        serde_json::to_value(&exported.nodes).unwrap(),
        serde_json::to_value(&reviewed.nodes).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&exported.edges).unwrap(),
        serde_json::to_value(&reviewed.edges).unwrap()
    );
    assert!(exported.unverified.is_empty());
    let imported_snapshot = load_ir(&snapshot).unwrap();
    assert_eq!(imported_snapshot.project_id, "fixture-project");
    assert_eq!(imported_snapshot.task_id, "fixture-task");
}

#[test]
fn cli_rejects_unreviewed_raw_chapter_and_ir_without_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let raw = ingest_public(&public_source(), "p", "t");
    let snapshot = save_ir(&ProjectPaths::new(dir.path().join("unreviewed")), "t", &raw).unwrap();
    for reviewed_input in [false, true] {
        let output = dir.path().join(if reviewed_input {
            "blocked IR"
        } else {
            "blocked raw"
        });
        let mut command = Command::new(env!("CARGO_BIN_EXE_entwurf"));
        command.arg("compile");
        if reviewed_input {
            command.arg("--ir").arg(&snapshot);
        } else {
            command.arg(public_source());
        }
        let result = command.arg("--out").arg(&output).output().unwrap();
        assert!(!result.status.success());
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("blocking finding"),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            !output.exists(),
            "unreviewed input created output artifacts"
        );
    }
    assert_eq!(load_ir(&snapshot).unwrap().unverified.len(), 4);
}

#[test]
fn fixture_review_refuses_changed_formula_and_retains_unrelated_uncertainty() {
    let original = ingest_public(&public_source(), "p", "t");
    let mut altered = original.clone();
    let first = altered
        .nodes
        .iter_mut()
        .find(|node| node.kind == NodeKind::Formula)
        .unwrap();
    first.latex = Some("P(X)=0".into());
    assert!(review_public_probability_formulas(&mut altered).is_err());
    assert_eq!(altered.unverified.len(), 4);
    assert!(altered
        .nodes
        .iter()
        .filter(|node| node.kind == NodeKind::Formula)
        .all(|node| !node.verified));
    let mut with_ocr = original;
    let mut extra = with_ocr.unverified[0].clone();
    extra.id = "unreviewed-ocr".into();
    extra.kind = UnverifiedKind::Ocr;
    extra.reason = "Separate OCR review required".into();
    with_ocr.unverified.push(extra);
    review_public_probability_formulas(&mut with_ocr).unwrap();
    assert_eq!(with_ocr.unverified.len(), 1);
    assert_eq!(with_ocr.unverified[0].id, "unreviewed-ocr");
    assert!(!validate_ir(&with_ocr, &ValidationOptions::default())
        .unwrap()
        .is_passing());
}
