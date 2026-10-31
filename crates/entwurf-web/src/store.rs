use crate::access;
use entwurf_core::{
    ingest::IngestOptions,
    project::Project,
    storage,
    workspace::{Workspace, WorkspaceSnapshot},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Default, Clone, Serialize, Deserialize)]
struct Selection {
    root: Option<String>,
    task_id: Option<String>,
}

pub struct Store {
    data: PathBuf,
    clients: HashMap<String, Selection>,
    workspaces: HashMap<String, Workspace>,
}

fn text<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required"))
}
fn optional<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

impl Store {
    pub fn new(data: PathBuf) -> Result<Self, String> {
        for name in ["projects", "clients", "uploads"] {
            fs::create_dir_all(data.join(name)).map_err(|e| e.to_string())?;
        }
        let data = fs::canonicalize(data).map_err(|e| e.to_string())?;
        let store = Self {
            data,
            clients: HashMap::new(),
            workspaces: HashMap::new(),
        };
        store.prune_uploads();
        Ok(store)
    }
    pub fn projects(&self) -> Result<Value, String> {
        let mut projects = Vec::new();
        for entry in fs::read_dir(self.data.join("projects")).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let handle = format!("/projects/{name}");
            let Ok(path) = access::project_path(&self.data, &handle) else {
                continue;
            };
            let mirror = path.join("project.json");
            if mirror.is_file() {
                let Ok(bytes) = fs::read(mirror) else {
                    continue;
                };
                let Ok(project) = serde_json::from_slice::<Project>(&bytes) else {
                    continue;
                };
                projects.push(json!({"root":handle,"name":project.name,"id":project.id}));
            }
        }
        Ok(Value::Array(projects))
    }
    fn selection(&mut self, client: &str) -> Result<Selection, String> {
        access::component(client)?;
        if let Some(selection) = self.clients.get(client) {
            return Ok(selection.clone());
        }
        let path = self.data.join("clients").join(format!("{client}.json"));
        let selection = if path.is_file() {
            serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?
        } else {
            Selection::default()
        };
        self.clients.insert(client.into(), selection.clone());
        Ok(selection)
    }
    fn save_selection(
        &mut self,
        client: &str,
        root: &str,
        snapshot: &WorkspaceSnapshot,
    ) -> Result<(), String> {
        let selection = Selection {
            root: Some(root.into()),
            task_id: snapshot.selected_task_id.clone(),
        };
        storage::write_atomic(
            &self.data.join("clients").join(format!("{client}.json")),
            &serde_json::to_vec(&selection).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        self.clients.insert(client.into(), selection);
        Ok(())
    }
    fn open(&mut self, root: &str) -> Result<&mut Workspace, String> {
        let path = access::project_path(&self.data, root)?;
        if !self.workspaces.contains_key(root) {
            self.workspaces.insert(
                root.into(),
                Workspace::open(path).map_err(|e| e.to_string())?,
            );
        }
        Ok(self.workspaces.get_mut(root).unwrap())
    }
    pub fn upload(&self, client: &str, name: &str, bytes: &[u8]) -> Result<Value, String> {
        access::component(client)?;
        access::upload_name(name)?;
        let id = uuid::Uuid::new_v4().to_string();
        let directory = self.data.join("uploads").join(client).join(&id);
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        if let Err(error) = storage::write_atomic(&directory.join(name), bytes) {
            let _ = fs::remove_dir_all(&directory);
            return Err(error.to_string());
        }
        Ok(json!({"path":format!("/uploads/{client}/{id}/{name}"),"name":name}))
    }
    // Failed multipart batches are all-or-nothing from the client's perspective.
    pub fn discard_uploads(&self, client: &str, files: &[Value]) {
        for file in files {
            if let Ok(paths) = self.upload_paths(client, &json!({"paths":[file["path"]]})) {
                Self::remove_staging(&paths);
            }
        }
    }
    fn remove_staging(paths: &[PathBuf]) {
        for path in paths {
            if let Some(directory) = path.parent() {
                let _ = fs::remove_dir_all(directory);
            }
        }
    }
    fn prune_uploads(&self) {
        let Ok(clients) = fs::read_dir(self.data.join("uploads")) else {
            return;
        };
        for client in clients.flatten() {
            if !client.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let Ok(entries) = fs::read_dir(client.path()) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }
                if entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.elapsed().ok())
                    .is_some_and(|age| age.as_secs() > 86_400)
                {
                    let _ = fs::remove_dir_all(entry.path());
                }
            }
        }
    }
    fn upload_paths(&self, client: &str, args: &Value) -> Result<Vec<PathBuf>, String> {
        let paths = args
            .get("paths")
            .and_then(Value::as_array)
            .ok_or("paths array is required")?;
        if paths.is_empty() || paths.len() > 64 {
            return Err("select between 1 and 64 files".into());
        }
        paths
            .iter()
            .map(|item| {
                let handle = item.as_str().ok_or("file handle must be a string")?;
                let prefix = format!("/uploads/{client}/");
                let relative = handle
                    .strip_prefix(&prefix)
                    .ok_or("upload belongs to another client or is not staged")?;
                let (id, name) = relative.split_once('/').ok_or("invalid upload handle")?;
                access::component(id)?;
                access::upload_name(name)?;
                let owned = self.data.join("uploads").join(client).join(id);
                let target = owned.join(name);
                let resolved = fs::canonicalize(&target).map_err(|e| e.to_string())?;
                if resolved != target || !resolved.is_file() {
                    return Err("upload escaped staging directory".into());
                }
                Ok(resolved)
            })
            .collect()
    }
    pub fn command(
        &mut self,
        client: &str,
        command: &str,
        args: Value,
        options: &IngestOptions,
        capabilities: entwurf_core::diagnostics::DiagnosticCapabilities,
    ) -> Result<Value, String> {
        let selection = self.selection(client)?;
        if command == "list_projects" {
            return self.projects();
        }
        let root = if matches!(command, "create_project" | "load_existing_project") {
            text(&args, "root")?.to_owned()
        } else {
            selection
                .root
                .clone()
                .ok_or("no saved project; create or open a project")?
        };
        let paths = if matches!(command, "import_sources" | "import_style_images") {
            self.upload_paths(client, &args)?
        } else {
            Vec::new()
        };
        if command == "create_project" {
            let path = access::project_path(&self.data, &root)?;
            let workspace =
                Workspace::create(path, text(&args, "name")?).map_err(|e| e.to_string())?;
            self.workspaces.insert(root.clone(), workspace);
        }
        if command == "load_existing_project" {
            // Explicit reopen is also the recovery path after another process edits SQLite.
            self.workspaces.remove(&root);
        }
        let workspace = self.open(&root)?;
        if selection.root.as_deref() == Some(root.as_str()) {
            workspace
                .select_task(selection.task_id.as_deref())
                .map_err(|e| e.to_string())?;
        }
        let mut snapshot = match command {
            "create_project" | "load_existing_project" => workspace.snapshot(None),
            "workspace_snapshot" => workspace.select_task(optional(&args, "taskId")),
            "create_task" => workspace.create_task(
                text(&args, "title")?,
                optional(&args, "chapter").map(str::to_owned),
            ),
            "import_sources" => workspace.import_sources(text(&args, "taskId")?, &paths, options),
            "import_style_images" => workspace.import_style_images(&paths),
            "import_ir" => workspace.import_ir(
                text(&args, "taskId")?,
                optional(&args, "baseVersionId"),
                text(&args, "json")?,
            ),
            "reimport_source" => workspace.reimport_source(
                text(&args, "taskId")?,
                text(&args, "baseVersionId")?,
                text(&args, "sourceId")?,
                options,
            ),
            "update_nodes" => workspace.update_nodes(
                text(&args, "taskId")?,
                text(&args, "baseVersionId")?,
                serde_json::from_value(args.get("edits").cloned().ok_or("edits are required")?)
                    .map_err(|e| e.to_string())?,
                args.get("nodeOrder")
                    .cloned()
                    .filter(|v| !v.is_null())
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| e.to_string())?,
            ),
            "resolve_review" => workspace.resolve_review(
                text(&args, "taskId")?,
                text(&args, "baseVersionId")?,
                text(&args, "itemId")?,
                text(&args, "note")?,
            ),
            "rollback_version" => {
                workspace.rollback_version(text(&args, "taskId")?, text(&args, "versionId")?)
            }
            "revert_correction" => workspace.revert_correction(
                text(&args, "taskId")?,
                text(&args, "baseVersionId")?,
                text(&args, "correctionId")?,
            ),
            "undo_visual_repair" => workspace.undo_visual_repair(
                text(&args, "taskId")?,
                text(&args, "baseVersionId")?,
                text(&args, "repairId")?,
            ),
            "save_style" => workspace.save_style(
                serde_json::from_value(args.get("style").cloned().ok_or("style is required")?)
                    .map_err(|e| e.to_string())?,
            ),
            "save_settings" => workspace.save_settings(text(&args, "ocrLanguages")?),
            "prepare_handoff" => workspace.prepare_handoff(text(&args, "taskId")?),
            "import_result" => {
                workspace.import_result(text(&args, "taskId")?, text(&args, "json")?)
            }
            "validate_current" | "preview_current" => {
                workspace.preview_current(text(&args, "taskId")?)
            }
            "export_current" => {
                if optional(&args, "outputDir").is_some() {
                    return Err("web exports use the project-owned export directory".into());
                }
                workspace.export_current(text(&args, "taskId")?, None)
            }
            "export_diagnostics" => {
                let snapshot = workspace.snapshot(None).map_err(|e| e.to_string())?;
                let report = entwurf_core::diagnostics::build_redacted_diagnostics(
                    Some(&snapshot),
                    capabilities,
                );
                return serde_json::to_value(report).map_err(|e| e.to_string());
            }
            _ => return Err(format!("unknown workspace command: {command}")),
        }
        .map_err(|e| e.to_string())?;
        snapshot.root = root.clone();
        self.save_selection(client, &root, &snapshot)?;
        // The core has retained and hashed source bytes before committing its database.
        Self::remove_staging(&paths);
        serde_json::to_value(snapshot).map_err(|e| e.to_string())
    }
    pub fn artifact(
        &mut self,
        client: &str,
        task: &str,
        version: &str,
        kind: &str,
        index: usize,
    ) -> Result<Vec<u8>, String> {
        let root = self.selection(client)?.root.ok_or("no project is open")?;
        let workspace = self.open(&root)?;
        let snapshot = workspace.snapshot(Some(task)).map_err(|e| e.to_string())?;
        if snapshot.current_version.as_ref().map(|v| v.id.as_str()) != Some(version) {
            return Err("artifact version changed; refresh the task".into());
        }
        let path = workspace
            .artifact_path(task, kind, index)
            .map_err(|e| e.to_string())?;
        fs::read(path).map_err(|e| e.to_string())
    }
}
