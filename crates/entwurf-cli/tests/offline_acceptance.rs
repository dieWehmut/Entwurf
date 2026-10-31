use std::fs;
use std::path::PathBuf;
use std::process::Command;

use entwurf_core::compiler::{compile, CompileOptions};
use entwurf_core::ingest::{ingest_path, IngestOptions};
use entwurf_core::layout::{layout, LayoutOptions};
use entwurf_core::project::{Project, VersionReason};
use entwurf_core::storage::{load_ir, load_project, save_ir, save_project, ProjectPaths};
use entwurf_core::validate::{validate_ir, validate_layout, ValidationOptions};
use entwurf_core::StyleSpec;

#[test]
fn public_probability_chapter_compiles_offline() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = root.join("fixtures/public/probability-chapter.md");
    let ingested = ingest_path(&source, &IngestOptions::standard()).unwrap();
    assert!(
        ingested.nodes.len() >= 8,
        "expected a full chapter, got {}",
        ingested.nodes.len()
    );
    assert!(ingested
        .nodes
        .iter()
        .any(|node| node.kind == entwurf_core::NodeKind::Formula));
    assert!(ingested.nodes.iter().all(|node| !node.anchors.is_empty()));

    let mut ir = entwurf_core::KnowledgeIr::empty(
        "project-acceptance",
        "task-acceptance",
        "概率论：随机变量与条件概率",
    );
    ir.sources.push(ingested.source);
    ir.nodes.extend(ingested.nodes);
    let style = StyleSpec::default();
    let mut report = validate_ir(&ir, &ValidationOptions::default()).unwrap();
    assert!(report.is_passing(), "{:?}", report.findings);
    let document = layout(&ir, &style, &LayoutOptions::from_style(&style)).unwrap();
    validate_layout(&ir, &document, &mut report, &ValidationOptions::default()).unwrap();
    assert!(report.is_passing(), "{:?}", report.findings);
    assert!(document.pages.len() <= 3);

    let dir = tempfile::tempdir().unwrap();
    let artifacts = compile(&ir, &style, &CompileOptions::new(dir.path())).unwrap();
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
    assert_eq!(reloaded.nodes.len(), ir.nodes.len());
}

#[test]
fn project_persists_ir_and_manual_edit() {
    let dir = tempfile::tempdir().unwrap();
    let paths = ProjectPaths::new(dir.path().join("中文 项目"));
    let mut project = Project::new("概率论");
    let task_id = project.add_task("第一章", Some("概率论".into())).id.clone();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/public/probability-chapter.md");
    let ingested = ingest_path(&source, &IngestOptions::standard()).unwrap();
    let mut ir = entwurf_core::KnowledgeIr::empty(&project.id, &task_id, "第一章");
    ir.sources.push(ingested.source);
    ir.nodes.extend(ingested.nodes);
    let version = project
        .save_version(&task_id, ir.clone(), VersionReason::Ingested)
        .unwrap();
    let ir_path = save_ir(&paths, &task_id, &ir).unwrap();
    save_project(&paths, &project).unwrap();
    let mut loaded = load_project(&paths).unwrap();
    let mut edited = loaded.current_ir(&task_id).unwrap().clone();
    edited.nodes[1].title = "手工修改".to_string();
    let edited_version = loaded
        .save_version(&task_id, edited, VersionReason::UserEdited)
        .unwrap();
    save_project(&paths, &loaded).unwrap();
    let again = load_project(&paths).unwrap();
    assert_eq!(
        again.current_ir(&task_id).unwrap().nodes[1].title,
        "手工修改"
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
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/public/probability-chapter.md");
    let binary = env!("CARGO_BIN_EXE_entwurf");
    let result = Command::new(binary)
        .arg("compile")
        .arg(&source)
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
    assert!(output.join("entwurf.pdf").exists());
    assert!(output.join("manifest.json").exists());
    assert!(output.join("knowledge.ir.json").exists());
    assert!(output.join("project.json").exists());
}
