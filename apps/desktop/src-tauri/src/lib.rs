use std::path::PathBuf;

use entwurf_core::compiler::{ArtifactSet, CompileOptions};
use entwurf_core::ingest::{ingest_paths, IngestOptions};
use entwurf_core::project::{Project, VersionReason};
use entwurf_core::storage::{load_project, save_project, ProjectPaths};
use entwurf_core::StyleSpec;
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

#[derive(Default)]
struct AppState {
    project: Option<Project>,
    root: Option<PathBuf>,
}

#[derive(Serialize)]
struct BootstrapResponse {
    version: String,
    schemas: Vec<String>,
}

#[tauri::command]
fn bootstrap() -> BootstrapResponse {
    BootstrapResponse {
        version: entwurf_core::VERSION.to_string(),
        schemas: ["knowledge-ir", "task-envelope", "result-envelope", "style"]
            .into_iter()
            .map(str::to_string)
            .collect(),
    }
}

#[tauri::command]
fn create_project(
    state: State<'_, std::sync::Mutex<AppState>>,
    root: String,
    name: String,
) -> Result<Project, String> {
    let project = Project::new(name);
    let paths = ProjectPaths::new(&root);
    save_project(&paths, &project).map_err(|error| error.to_string())?;
    let mut state = state.lock().map_err(|error| error.to_string())?;
    state.project = Some(project.clone());
    state.root = Some(root.into());
    Ok(project)
}

#[tauri::command]
fn load_existing_project(
    state: State<'_, std::sync::Mutex<AppState>>,
    root: String,
) -> Result<Project, String> {
    let paths = ProjectPaths::new(&root);
    let project = load_project(&paths).map_err(|error| error.to_string())?;
    let mut state = state.lock().map_err(|error| error.to_string())?;
    state.project = Some(project.clone());
    state.root = Some(root.into());
    Ok(project)
}

#[tauri::command]
fn import_sources(
    state: State<'_, std::sync::Mutex<AppState>>,
    task_id: String,
    paths: Vec<String>,
) -> Result<entwurf_core::KnowledgeIr, String> {
    let mut state = state.lock().map_err(|error| error.to_string())?;
    let project = state.project.as_mut().ok_or("no project is open")?;
    let report = ingest_paths(
        &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
        &IngestOptions::standard(),
    )
    .map_err(|error| error.to_string())?;
    let mut ir = report.into_ir(&project.id, &task_id, &project.name);
    let version = project
        .save_version(&task_id, ir.clone(), VersionReason::Ingested)
        .map_err(|error| error.to_string())?;
    ir.task_id = task_id;
    ir.id = version;
    Ok(ir)
}

#[tauri::command]
fn compile_current(
    state: State<'_, std::sync::Mutex<AppState>>,
    task_id: String,
    output_dir: String,
) -> Result<entwurf_core::compiler::ArtifactSet, String> {
    let mut state = state.lock().map_err(|error| error.to_string())?;
    let AppState { project, root } = &mut *state;
    let project = project.as_mut().ok_or("no project is open")?;
    let artifacts = project
        .compile_current(&task_id, &CompileOptions::new(output_dir))
        .map_err(|error| error.to_string())?;
    if let Some(root) = root {
        save_project(&ProjectPaths::new(root.as_path()), project)
            .map_err(|error| error.to_string())?;
    }
    Ok(artifacts)
}

#[tauri::command]
fn default_style() -> StyleSpec {
    StyleSpec::default()
}

#[derive(Debug, Clone, Serialize)]
struct ExportResult {
    artifacts: ArtifactSet,
    validation_passed: bool,
    blocking_findings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct NodeEdit {
    id: String,
    title: String,
    body: String,
    latex: Option<String>,
}

fn save_current_project(root: &Option<PathBuf>, project: &Project) -> Result<(), String> {
    if let Some(root) = root {
        save_project(&ProjectPaths::new(root), project).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_workspace(
    state: State<'_, std::sync::Mutex<AppState>>,
    root: Option<String>,
    name: String,
    app: tauri::AppHandle,
) -> Result<Project, String> {
    let root = match root.filter(|value| !value.trim().is_empty()) {
        Some(root) => PathBuf::from(root),
        None => app
            .path()
            .app_data_dir()
            .map_err(|error| error.to_string())?
            .join("workspace"),
    };
    let paths = ProjectPaths::new(&root);
    let project = if paths.database_file().exists() || paths.project_file().exists() {
        load_project(&paths).map_err(|error| error.to_string())?
    } else {
        let project = Project::new(name);
        save_project(&paths, &project).map_err(|error| error.to_string())?;
        project
    };
    let mut state = state.lock().map_err(|error| error.to_string())?;
    state.project = Some(project.clone());
    state.root = Some(root);
    Ok(project)
}

#[tauri::command]
fn create_task(
    state: State<'_, std::sync::Mutex<AppState>>,
    title: String,
    chapter: Option<String>,
) -> Result<String, String> {
    let mut state = state.lock().map_err(|error| error.to_string())?;
    let AppState { project, root } = &mut *state;
    let project = project.as_mut().ok_or("no project is open")?;
    let task_id = project.add_task(title, chapter).id.clone();
    save_current_project(root, project)?;
    Ok(task_id)
}

#[tauri::command]
fn save_ir(
    state: State<'_, std::sync::Mutex<AppState>>,
    task_id: String,
    ir: entwurf_core::KnowledgeIr,
) -> Result<String, String> {
    let mut state = state.lock().map_err(|error| error.to_string())?;
    let AppState { project, root } = &mut *state;
    let project = project.as_mut().ok_or("no project is open")?;
    let version = project
        .save_version(&task_id, ir, VersionReason::UserEdited)
        .map_err(|error| error.to_string())?;
    save_current_project(root, project)?;
    Ok(version)
}

#[tauri::command]
fn update_nodes(
    state: State<'_, std::sync::Mutex<AppState>>,
    task_id: String,
    nodes: Vec<NodeEdit>,
) -> Result<String, String> {
    let mut state = state.lock().map_err(|error| error.to_string())?;
    let AppState { project, root } = &mut *state;
    let project = project.as_mut().ok_or("no project is open")?;
    let mut ir = project
        .current_ir(&task_id)
        .cloned()
        .ok_or("task has no current IR")?;
    for edit in nodes {
        let node = ir
            .node_mut(&edit.id)
            .ok_or_else(|| format!("unknown node {}", edit.id))?;
        node.title = edit.title;
        node.body = edit.body;
        node.latex = edit.latex;
        node.origin = entwurf_core::NodeOrigin::UserEdit;
    }
    ir.touch();
    let version = project
        .save_version(&task_id, ir, VersionReason::UserEdited)
        .map_err(|error| error.to_string())?;
    save_current_project(root, project)?;
    Ok(version)
}

#[tauri::command]
fn export_current(
    state: State<'_, std::sync::Mutex<AppState>>,
    task_id: String,
    output_dir: String,
) -> Result<ExportResult, String> {
    let mut state = state.lock().map_err(|error| error.to_string())?;
    let AppState { project, root } = &mut *state;
    let project = project.as_mut().ok_or("no project is open")?;
    let artifacts = project
        .compile_current(&task_id, &CompileOptions::new(output_dir))
        .map_err(|error| error.to_string())?;
    save_current_project(root, project)?;
    let task = project
        .task(&task_id)
        .ok_or_else(|| format!("task not found: {task_id}"))?;
    let version_id = task
        .current_version
        .as_deref()
        .ok_or("task has no current version")?;
    let validation = task
        .versions
        .iter()
        .find(|version| version.id == version_id)
        .and_then(|version| version.validation.clone())
        .ok_or("compile did not attach validation")?;
    let blocking_findings = validation
        .findings
        .iter()
        .filter(|finding| finding.severity == entwurf_core::Severity::Blocking)
        .map(|finding| finding.message.clone())
        .collect();
    Ok(ExportResult {
        artifacts,
        validation_passed: validation.is_passing(),
        blocking_findings,
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(std::sync::Mutex::new(AppState::default()))
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            create_project,
            load_existing_project,
            import_sources,
            compile_current,
            default_style,
            open_workspace,
            create_task,
            save_ir,
            update_nodes,
            export_current
        ])
        .run(tauri::generate_context!())
        .expect("error while running Entwurf");
}
