//! Host-independent command adapter. One module instance belongs to one worker.
#![cfg(target_arch = "wasm32")]
use entwurf_core::{
    files,
    ingest::IngestOptions,
    workspace::{Workspace, WorkspaceSnapshot},
    EntwurfError,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::{cell::RefCell, path::PathBuf};
use wasm_bindgen::prelude::*;
thread_local! { static WORKSPACE: RefCell<Option<Workspace>> = const { RefCell::new(None) }; }
fn error(e: impl ToString) -> JsValue {
    JsValue::from_str(&e.to_string())
}
#[wasm_bindgen]
pub fn browser_reset() {
    WORKSPACE.with(|ws| *ws.borrow_mut() = None);
    files::reset();
}
#[wasm_bindgen]
pub fn browser_put_file(path: &str, bytes: &[u8]) -> Result<(), JsValue> {
    files::write(path, bytes).map_err(error)
}
#[wasm_bindgen]
pub fn browser_get_file(path: &str) -> Result<Vec<u8>, JsValue> {
    files::read(path).map_err(error)
}
#[wasm_bindgen]
pub fn browser_delete_file(path: &str) -> Result<(), JsValue> {
    files::remove_file(path).map_err(error)
}
#[wasm_bindgen]
pub fn browser_list_files() -> String {
    serde_json::to_string(&files::list_files()).expect("paths serialize")
}
fn required<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, EntwurfError> {
    serde_json::from_value(
        args.get(key)
            .cloned()
            .ok_or_else(|| EntwurfError::InvalidInput(format!("missing argument {key}")))?,
    )
    .map_err(|e| EntwurfError::InvalidInput(format!("invalid argument {key}: {e}")))
}
fn optional<T: DeserializeOwned>(args: &Value, key: &str) -> Result<Option<T>, EntwurfError> {
    if args.get(key).is_none_or(Value::is_null) {
        Ok(None)
    } else {
        required(args, key).map(Some)
    }
}
fn snapshot(
    ws: &mut Workspace,
    command: &str,
    a: &Value,
) -> entwurf_core::Result<WorkspaceSnapshot> {
    if command == "workspace_snapshot" {
        return ws.select_task(optional::<String>(a, "taskId")?.as_deref());
    }
    if command == "create_task" {
        return ws.create_task(&required::<String>(a, "title")?, optional(a, "chapter")?);
    }
    if command == "save_style" {
        return ws.save_style(required(a, "style")?);
    }
    if command == "save_settings" {
        return ws.save_settings(&required::<String>(a, "ocrLanguages")?);
    }
    if command == "import_style_images" {
        return ws.import_style_images(&required::<Vec<PathBuf>>(a, "paths")?);
    }
    let task: String = required(a, "taskId")?;
    match command {
        "import_sources" => ws.import_sources(
            &task,
            &required::<Vec<PathBuf>>(a, "paths")?,
            &IngestOptions::standard(),
        ),
        "import_ir" => ws.import_ir(
            &task,
            optional::<String>(a, "baseVersionId")?.as_deref(),
            &required::<String>(a, "json")?,
        ),
        "reimport_source" => ws.reimport_source(
            &task,
            &required::<String>(a, "baseVersionId")?,
            &required::<String>(a, "sourceId")?,
            &IngestOptions::standard(),
        ),
        "update_nodes" => ws.update_nodes(
            &task,
            &required::<String>(a, "baseVersionId")?,
            required(a, "edits")?,
            optional(a, "nodeOrder")?,
        ),
        "resolve_review" => ws.resolve_review(
            &task,
            &required::<String>(a, "baseVersionId")?,
            &required::<String>(a, "itemId")?,
            &required::<String>(a, "note")?,
        ),
        "rollback_version" => ws.rollback_version(&task, &required::<String>(a, "versionId")?),
        "revert_correction" => ws.revert_correction(
            &task,
            &required::<String>(a, "baseVersionId")?,
            &required::<String>(a, "correctionId")?,
        ),
        "undo_visual_repair" => ws.undo_visual_repair(
            &task,
            &required::<String>(a, "baseVersionId")?,
            &required::<String>(a, "repairId")?,
        ),
        "prepare_handoff" => ws.prepare_handoff(&task),
        "import_result" => ws.import_result(&task, &required::<String>(a, "json")?),
        "preview_current" | "validate_current" => ws.preview_current(&task),
        "export_current" => ws.export_current(&task, optional::<PathBuf>(a, "outputDir")?),
        _ => Err(EntwurfError::Unsupported(format!(
            "unknown browser command {command}"
        ))),
    }
}
#[wasm_bindgen]
pub fn browser_call(command: &str, args_json: &str) -> Result<String, JsValue> {
    let args: Value = serde_json::from_str(args_json).map_err(error)?;
    let result: entwurf_core::Result<Value> = WORKSPACE.with(|state| {
        let mut state = state.borrow_mut();
        if command == "diagnostics" {
            let current = state.as_ref().map(|ws| ws.snapshot(None)).transpose()?;
            return serde_json::to_value(entwurf_core::diagnostics::build_redacted_diagnostics(
                current.as_ref(),
                entwurf_core::diagnostics::DiagnosticCapabilities::default(),
            ))
            .map_err(|e| EntwurfError::InvalidInput(e.to_string()));
        }
        if command == "register_extraction" {
            entwurf_core::ingest::register_extraction(
                &required::<PathBuf>(&args, "path")?,
                required(&args, "extraction")?,
            )?;
            return Ok(Value::Null);
        }
        if command == "list_projects" {
            let mut projects = Vec::new();
            for path in files::list_files()
                .into_iter()
                .filter(|p| p.starts_with("/projects/") && p.ends_with("/project.json"))
            {
                let root = PathBuf::from(&path).parent().unwrap().to_path_buf();
                let project = entwurf_core::storage::load_project(
                    &entwurf_core::storage::ProjectPaths::new(&root),
                )?;
                projects.push(json!({"root":root,"name":project.name,"id":project.id}));
            }
            return Ok(Value::Array(projects));
        }
        if command == "create_project" || command == "load_existing_project" {
            let root: String = required(&args, "root")?;
            let ws = if command == "create_project" {
                Workspace::create(root, &required::<String>(&args, "name")?)?
            } else {
                Workspace::open(root)?
            };
            let next = ws.snapshot(None)?;
            *state = Some(ws);
            return serde_json::to_value(next)
                .map_err(|e| EntwurfError::InvalidInput(e.to_string()));
        }
        let ws = state
            .as_mut()
            .ok_or_else(|| EntwurfError::InvalidInput("no project is open".into()))?;
        if command == "artifact_path" {
            return Ok(json!(ws.artifact_path(
                &required::<String>(&args, "taskId")?,
                &required::<String>(&args, "kind")?,
                optional::<usize>(&args, "index")?.unwrap_or(0)
            )?));
        }
        serde_json::to_value(snapshot(ws, command, &args)?)
            .map_err(|e| EntwurfError::InvalidInput(e.to_string()))
    });
    serde_json::to_string(&result.map_err(error)?).map_err(error)
}
