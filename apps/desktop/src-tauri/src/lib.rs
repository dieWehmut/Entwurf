mod bridge;
use entwurf_core::{
    ingest::IngestOptions,
    workspace::{NodeEdit, Workspace, WorkspaceSnapshot},
    StyleSpec,
};
use std::{path::PathBuf, sync::Mutex};
use tauri::Manager;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

type AppState = Mutex<Option<Workspace>>;
type CapabilityState = Mutex<entwurf_core::diagnostics::DiagnosticCapabilities>;
async fn state_call(
    app: tauri::AppHandle,
    action: impl FnOnce(&mut Option<Workspace>) -> Result<WorkspaceSnapshot, String> + Send + 'static,
) -> Result<WorkspaceSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut state = state.lock().map_err(|e| e.to_string())?;
        action(&mut state)
    })
    .await
    .map_err(|e| e.to_string())?
}
async fn workspace_call(
    app: tauri::AppHandle,
    action: impl FnOnce(&mut Workspace) -> entwurf_core::Result<WorkspaceSnapshot> + Send + 'static,
) -> Result<WorkspaceSnapshot, String> {
    state_call(app, move |state| {
        action(state.as_mut().ok_or("no project is open")?).map_err(|e| e.to_string())
    })
    .await
}
fn tesseract_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(root) = std::env::var_os("ProgramFiles") {
            let path = PathBuf::from(root).join("Tesseract-OCR");
            if path.join("tesseract.exe").is_file() {
                return Some(path);
            }
        }
        if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            let path = PathBuf::from(root).join("Entwurf").join("tesseract");
            if path.join("tesseract.exe").is_file() {
                return Some(path);
            }
        }
    }
    None
}
fn ocr_command() -> std::process::Command {
    let mut c = std::process::Command::new(
        tesseract_dir()
            .map(|p| p.join("tesseract.exe"))
            .unwrap_or_else(|| "tesseract".into()),
    );
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    c
}
#[tauri::command]
async fn bootstrap(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    let mut value=tauri::async_runtime::spawn_blocking(|| {
        let available=ocr_command().arg("--version").output().is_ok_and(|o|o.status.success());
        let languages=ocr_command().arg("--list-langs").output().ok().filter(|o|o.status.success()).map(|o|String::from_utf8_lossy(&o.stdout).lines().filter(|l|!l.starts_with("List of")).map(str::to_owned).collect::<Vec<_>>()).unwrap_or_default();
        serde_json::json!({"version":entwurf_core::VERSION,"platform":std::env::consts::OS,"tesseract_available":available,"ocr_languages":languages,"automation_enabled":false})
    }).await.map_err(|e|e.to_string())?;
    let probe = bridge::probe(
        app.path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("resources/gpt-bridge/Entwurf.Bridge.exe"),
    )
    .await;
    value["bridge_status"] = probe.status.into();
    value["bridge_detail"] = probe.detail.into();
    let capabilities = entwurf_core::diagnostics::DiagnosticCapabilities {
        pdfium_available: app
            .path()
            .resource_dir()
            .map_err(|e| e.to_string())?
            .join("resources/pdfium/pdfium.dll")
            .is_file(),
        tesseract_available: value["tesseract_available"].as_bool().unwrap_or(false),
        bridge_available: probe.status == "ready_manual",
        chatgpt_detected: probe
            .capabilities
            .as_ref()
            .and_then(|c| c.get("app_found"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        automation_enabled: false,
    };
    *app.state::<CapabilityState>()
        .lock()
        .map_err(|e| e.to_string())? = capabilities;
    value["bridge"] = serde_json::to_value(probe).map_err(|e| e.to_string())?;
    Ok(value)
}

fn last_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("last-workspace.json"))
}
fn remember(app: &tauri::AppHandle, root: &str) -> Result<(), String> {
    let path = last_root(app)?;
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    entwurf_core::storage::write_atomic(
        &path,
        &serde_json::to_vec(root).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
#[tauri::command]
async fn open_workspace(
    app: tauri::AppHandle,
    root: Option<String>,
    name: String,
) -> Result<WorkspaceSnapshot, String> {
    let app2 = app.clone();
    state_call(app, move |state| {
        let root = match root.filter(|s| !s.trim().is_empty()) {
            Some(r) => PathBuf::from(r),
            None => {
                let last = last_root(&app2)?;
                if last.exists() {
                    PathBuf::from(
                        serde_json::from_slice::<String>(
                            &std::fs::read(last).map_err(|e| e.to_string())?,
                        )
                        .map_err(|e| e.to_string())?,
                    )
                } else {
                    app2.path()
                        .app_data_dir()
                        .map_err(|e| e.to_string())?
                        .join("workspace")
                }
            }
        };
        let paths = entwurf_core::storage::ProjectPaths::new(&root);
        let ws = if paths.database_file().exists() || paths.project_file().exists() {
            Workspace::open(root)
        } else {
            Workspace::create(root, &name)
        }
        .map_err(|e| e.to_string())?;
        let snapshot = ws.snapshot(None).map_err(|e| e.to_string())?;
        remember(&app2, &snapshot.root)?;
        *state = Some(ws);
        Ok(snapshot)
    })
    .await
}
#[tauri::command]
async fn create_project(
    app: tauri::AppHandle,
    root: String,
    name: String,
) -> Result<WorkspaceSnapshot, String> {
    let app2 = app.clone();
    state_call(app, move |state| {
        let ws = Workspace::create(root, &name).map_err(|e| e.to_string())?;
        let snapshot = ws.snapshot(None).map_err(|e| e.to_string())?;
        remember(&app2, &snapshot.root)?;
        *state = Some(ws);
        Ok(snapshot)
    })
    .await
}
#[tauri::command]
async fn load_existing_project(
    app: tauri::AppHandle,
    root: String,
) -> Result<WorkspaceSnapshot, String> {
    let app2 = app.clone();
    state_call(app, move |state| {
        let ws = Workspace::open(root).map_err(|e| e.to_string())?;
        let snapshot = ws.snapshot(None).map_err(|e| e.to_string())?;
        remember(&app2, &snapshot.root)?;
        *state = Some(ws);
        Ok(snapshot)
    })
    .await
}
#[tauri::command]
async fn workspace_snapshot(
    app: tauri::AppHandle,
    task_id: Option<String>,
) -> Result<WorkspaceSnapshot, String> {
    let app2 = app.clone();
    state_call(app, move |state| {
        if state.is_none() && task_id.is_none() {
            let path = last_root(&app2)?;
            if !path.is_file() {
                return Err("no saved project; create or open a project".into());
            }
            let root: String =
                serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("cannot read last workspace: {e}"))?;
            *state = Some(
                Workspace::open(root).map_err(|e| format!("cannot restore last workspace: {e}"))?,
            );
        }
        state
            .as_mut()
            .ok_or("no project is open")?
            .select_task(task_id.as_deref())
            .map_err(|e| e.to_string())
    })
    .await
}
#[tauri::command]
async fn create_task(
    app: tauri::AppHandle,
    title: String,
    chapter: Option<String>,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| ws.create_task(&title, chapter)).await
}
fn ingestion_options(app: &tauri::AppHandle) -> Result<IngestOptions, String> {
    Ok(IngestOptions {
        pdfium_dir: Some(
            app.path()
                .resource_dir()
                .map_err(|e| e.to_string())?
                .join("resources/pdfium"),
        ),
        binary_dir: tesseract_dir(),
        ..IngestOptions::standard()
    })
}
#[tauri::command]
async fn import_sources(
    app: tauri::AppHandle,
    task_id: String,
    paths: Vec<String>,
) -> Result<WorkspaceSnapshot, String> {
    let options = ingestion_options(&app)?;
    workspace_call(app, move |ws| {
        ws.import_sources(
            &task_id,
            &paths.into_iter().map(PathBuf::from).collect::<Vec<_>>(),
            &options,
        )
    })
    .await
}
#[tauri::command]
async fn reimport_source(
    app: tauri::AppHandle,
    task_id: String,
    base_version_id: String,
    source_id: String,
) -> Result<WorkspaceSnapshot, String> {
    let options = ingestion_options(&app)?;
    workspace_call(app, move |ws| {
        ws.reimport_source(&task_id, &base_version_id, &source_id, &options)
    })
    .await
}
#[tauri::command]
async fn update_nodes(
    app: tauri::AppHandle,
    task_id: String,
    base_version_id: String,
    edits: Vec<NodeEdit>,
    node_order: Option<Vec<String>>,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| {
        ws.update_nodes(&task_id, &base_version_id, edits, node_order)
    })
    .await
}
#[tauri::command]
async fn rollback_version(
    app: tauri::AppHandle,
    task_id: String,
    version_id: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| ws.rollback_version(&task_id, &version_id)).await
}
#[tauri::command]
async fn revert_correction(
    app: tauri::AppHandle,
    task_id: String,
    base_version_id: String,
    correction_id: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| {
        ws.revert_correction(&task_id, &base_version_id, &correction_id)
    })
    .await
}
#[tauri::command]
async fn resolve_review(
    app: tauri::AppHandle,
    task_id: String,
    base_version_id: String,
    item_id: String,
    note: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| {
        ws.resolve_review(&task_id, &base_version_id, &item_id, &note)
    })
    .await
}
#[tauri::command]
async fn import_style_images(
    app: tauri::AppHandle,
    paths: Vec<String>,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| {
        ws.import_style_images(&paths.into_iter().map(PathBuf::from).collect::<Vec<_>>())
    })
    .await
}
#[tauri::command]
async fn save_settings(
    app: tauri::AppHandle,
    ocr_languages: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| ws.save_settings(&ocr_languages)).await
}
#[tauri::command]
async fn save_style(app: tauri::AppHandle, style: StyleSpec) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| ws.save_style(style)).await
}
#[tauri::command]
async fn prepare_handoff(
    app: tauri::AppHandle,
    task_id: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| ws.prepare_handoff(&task_id)).await
}
#[tauri::command]
async fn import_result(
    app: tauri::AppHandle,
    task_id: String,
    json: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| ws.import_result(&task_id, &json)).await
}
#[tauri::command]
async fn undo_visual_repair(
    app: tauri::AppHandle,
    task_id: String,
    base_version_id: String,
    repair_id: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| {
        ws.undo_visual_repair(&task_id, &base_version_id, &repair_id)
    })
    .await
}
#[tauri::command]
async fn export_diagnostics(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let picker = app.clone();
    let chosen = tauri::async_runtime::spawn_blocking(move || {
        picker
            .dialog()
            .file()
            .add_filter("Diagnostics JSON", &["json"])
            .set_file_name("entwurf-diagnostics.json")
            .blocking_save_file()
    })
    .await
    .map_err(|e| e.to_string())?;
    let Some(chosen) = chosen else {
        return Ok(None);
    };
    let path = chosen.into_path().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        let capabilities = *app
            .state::<CapabilityState>()
            .lock()
            .map_err(|e| e.to_string())?;
        let state = app.state::<AppState>();
        let state = state.lock().map_err(|e| e.to_string())?;
        let snapshot = state
            .as_ref()
            .map(|ws| ws.snapshot(None))
            .transpose()
            .map_err(|e| e.to_string())?;
        let report =
            entwurf_core::diagnostics::build_redacted_diagnostics(snapshot.as_ref(), capabilities);
        let bytes = serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?;
        entwurf_core::storage::write_atomic(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn validate_current(
    app: tauri::AppHandle,
    task_id: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| ws.preview_current(&task_id)).await
}
#[tauri::command]
async fn preview_current(
    app: tauri::AppHandle,
    task_id: String,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| ws.preview_current(&task_id)).await
}
#[tauri::command]
async fn export_current(
    app: tauri::AppHandle,
    task_id: String,
    output_dir: Option<String>,
) -> Result<WorkspaceSnapshot, String> {
    workspace_call(app, move |ws| {
        ws.export_current(&task_id, output_dir.map(PathBuf::from))
    })
    .await
}
#[tauri::command]
async fn choose_directory(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .map(|p| {
                p.into_path()
                    .map(|p| p.display().to_string())
                    .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn choose_sources(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .add_filter(
                "Sources",
                &[
                    "pdf", "md", "markdown", "tex", "latex", "png", "jpg", "jpeg", "bmp", "tif",
                    "tiff", "webp",
                ],
            )
            .blocking_pick_files()
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                p.into_path()
                    .map(|p| p.display().to_string())
                    .map_err(|e| e.to_string())
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn choose_result_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(p) = app
            .dialog()
            .file()
            .add_filter("ResultEnvelope JSON", &["json"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = p.into_path().map_err(|e| e.to_string())?;
        if std::fs::metadata(&path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
            return Err("Result file exceeds 16 MiB".into());
        }
        std::fs::read_to_string(path)
            .map(Some)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
async fn copy_prompt(app: tauri::AppHandle, task_id: String) -> Result<(), String> {
    let app2 = app.clone();
    workspace_call(app, move |ws| {
        let snapshot = ws.snapshot(Some(&task_id))?;
        let text = snapshot
            .handoff
            .as_ref()
            .ok_or_else(|| {
                entwurf_core::EntwurfError::InvalidInput("prepare a prompt first".into())
            })?
            .markdown
            .clone();
        app2.clipboard()
            .write_text(text)
            .map_err(|e| entwurf_core::EntwurfError::InvalidInput(e.to_string()))?;
        Ok(snapshot)
    })
    .await
    .map(|_| ())
}
#[tauri::command]
async fn open_export(
    app: tauri::AppHandle,
    task_id: String,
    kind: String,
    index: Option<usize>,
) -> Result<(), String> {
    let app2 = app.clone();
    workspace_call(app, move |ws| {
        let path = ws.artifact_path(&task_id, &kind, index.unwrap_or(0))?;
        app2.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|e| entwurf_core::EntwurfError::InvalidInput(e.to_string()))?;
        ws.snapshot(Some(&task_id))
    })
    .await
    .map(|_| ())
}
#[tauri::command]
async fn open_chatgpt() -> Result<(), String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(||{use std::os::windows::process::CommandExt;let output=std::process::Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command","[Console]::OutputEncoding=[Text.UTF8Encoding]::new(); $apps=@(Get-StartApps | Where-Object { ($_.Name -eq 'ChatGPT') -and ($_.AppID -match '^OpenAI[.]ChatGPT(-Desktop)?_2p2nqsd0c76g0!') }); if($apps.Count -ne 1){exit 3}; $apps[0].AppID"]).creation_flags(0x08000000).output().map_err(|e|e.to_string())?;let id=String::from_utf8_lossy(&output.stdout).trim().to_owned();if !output.status.success()||id.is_empty(){return Err("ChatGPT desktop app was not uniquely found. Open it manually from Windows Start.".into());}std::process::Command::new("explorer.exe").arg(format!("shell:AppsFolder\\{id}")).spawn().map_err(|e|e.to_string())?;Ok(())}).await.map_err(|e|e.to_string())?
    }
    #[cfg(not(windows))]
    {
        Err("ChatGPT desktop launch is available on Windows. Open ChatGPT manually.".into())
    }
}
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .manage(AppState::new(None))
        .manage(CapabilityState::default())
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            open_workspace,
            create_project,
            load_existing_project,
            workspace_snapshot,
            create_task,
            import_sources,
            reimport_source,
            update_nodes,
            rollback_version,
            revert_correction,
            resolve_review,
            save_style,
            import_style_images,
            save_settings,
            prepare_handoff,
            import_result,
            validate_current,
            undo_visual_repair,
            export_diagnostics,
            preview_current,
            export_current,
            choose_directory,
            choose_sources,
            choose_result_file,
            copy_prompt,
            open_export,
            open_chatgpt
        ])
        .run(tauri::generate_context!())
        .expect("error while running Entwurf");
}
