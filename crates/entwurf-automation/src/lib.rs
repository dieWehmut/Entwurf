//! Shared origin-bound automation coordination. No workspace or filesystem RPC.
mod router;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};
use tokio::sync::mpsc;
use uuid::Uuid;
pub const MAX_ARTIFACT_BYTES: usize = 32 * 1024 * 1024;
type Result<T> = std::result::Result<T, AutomationError>;
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AutomationError(pub &'static str);
#[derive(Clone)]
pub struct AutomationState(Arc<Shared>);
struct Shared {
    directory: PathBuf,
    port: u16,
    inner: Mutex<Inner>,
}
struct Inner {
    database: Database,
    peer: Option<Peer>,
    available: bool,
    reason: String,
}
struct Peer {
    id: String,
    sender: mpsc::Sender<Value>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Database {
    schema_version: u32,
    extension_token: String,
    #[serde(default = "token")]
    native_token: String,
    #[serde(default = "default_provider")]
    provider: String,
    access: Vec<Access>,
    runs: BTreeMap<String, StoredRun>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Access {
    owner: String,
    origin: String,
    token: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct StoredRun {
    #[serde(flatten)]
    view: RunView,
    owner: String,
    image_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunView {
    pub run_id: String,
    pub task_id: String,
    pub base_version_id: String,
    pub prompt_sha256: String,
    pub phase: String,
    pub reason: Option<String>,
    pub sha256: Option<String>,
    pub mime_type: Option<String>,
}
#[derive(Serialize)]
pub struct PairingInfo {
    pub extension_token: String,
    pub websocket_url: String,
}
#[derive(Deserialize)]
pub(crate) struct StartRun {
    run_id: String,
    task_id: String,
    base_version_id: String,
    prompt: String,
}
#[derive(Deserialize)]
pub(crate) struct Acknowledge {
    sha256: String,
    image_id: String,
}
fn default_provider() -> String {
    "browser".into()
}
fn valid_provider(provider: &str) -> bool {
    matches!(provider, "browser" | "windows")
}
fn token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn active(phase: &str) -> bool {
    !matches!(phase, "completed" | "failed" | "cancelled")
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn equal_token(left: &str, right: &str) -> bool {
    let a = Sha256::digest(left.as_bytes());
    let b = Sha256::digest(right.as_bytes());
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let temporary = path.with_extension(format!("{}.tmp", Uuid::new_v4().simple()));
    let result = (|| -> std::io::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result.map_err(|_| AutomationError("automation_storage_failed"))
}
impl AutomationState {
    pub fn new(directory: impl AsRef<Path>, port: u16) -> Result<Self> {
        if port == 0 {
            return Err(AutomationError("invalid_port"));
        }
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(directory.join("images"))
            .map_err(|_| AutomationError("automation_storage_failed"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| AutomationError("automation_storage_failed"))?;
        }
        let database = match fs::read(directory.join("automation.json")) {
            Ok(bytes) => serde_json::from_slice::<Database>(&bytes)
                .map_err(|_| AutomationError("invalid_automation_state"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Database {
                schema_version: 1,
                extension_token: token(),
                native_token: token(),
                provider: default_provider(),
                access: vec![],
                runs: BTreeMap::new(),
            },
            Err(_) => return Err(AutomationError("automation_storage_failed")),
        };
        if database.schema_version != 1 || !valid_provider(&database.provider) {
            return Err(AutomationError("invalid_automation_state"));
        }
        let state = Self(Arc::new(Shared {
            directory,
            port,
            inner: Mutex::new(Inner {
                database,
                peer: None,
                available: false,
                reason: "extension_disconnected".into(),
            }),
        }));
        {
            let mut inner = state.lock()?;
            for run in inner.database.runs.values_mut() {
                if active(&run.view.phase) && run.view.phase != "awaiting_import" {
                    run.view.phase = "paused".into();
                    run.view.reason = Some("server_restarted".into());
                }
            }
            state.save(&inner.database)?;
        }
        Ok(state)
    }
    pub fn router(&self) -> axum::Router {
        router::router(self.clone())
    }
    pub fn provider(&self) -> Result<String> {
        Ok(self.lock()?.database.provider.clone())
    }
    pub fn set_provider(&self, provider: &str) -> Result<()> {
        if !valid_provider(provider) {
            return Err(AutomationError("invalid_provider"));
        }
        let mut inner = self.lock()?;
        if inner.database.provider == provider {
            return Ok(());
        }
        if inner
            .database
            .runs
            .values()
            .any(|run| active(&run.view.phase))
        {
            return Err(AutomationError("run_active"));
        }
        let mut database = inner.database.clone();
        database.provider = provider.into();
        // Keep authentication and starts excluded until the durable selection and
        // peer removal agree. A failed write must preserve the previous session.
        self.commit(&mut inner, database)?;
        inner.peer = None; // Drops the only sender, waking the old socket to exit.
        inner.available = false;
        inner.reason = "provider_changed".into();
        Ok(())
    }
    pub fn issue_access(&self, owner: &str, allowed_origin: &str) -> Result<String> {
        if !valid_id(owner) {
            return Err(AutomationError("invalid_owner"));
        }
        let origin =
            url::Url::parse(allowed_origin).map_err(|_| AutomationError("invalid_origin"))?;
        if !matches!(origin.scheme(), "http" | "https" | "tauri")
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || !matches!(origin.path(), "" | "/")
            || allowed_origin.ends_with('/')
        {
            return Err(AutomationError("invalid_origin"));
        }
        let mut inner = self.lock()?;
        if let Some(access) = inner
            .database
            .access
            .iter()
            .find(|a| a.owner == owner && a.origin == allowed_origin)
        {
            return Ok(access.token.clone());
        }
        if inner.database.access.len() >= 1024 {
            return Err(AutomationError("pairing_limit"));
        }
        let capability = token();
        let mut database = inner.database.clone();
        database.access.push(Access {
            owner: owner.into(),
            origin: allowed_origin.into(),
            token: capability.clone(),
        });
        self.commit(&mut inner, database)?;
        Ok(capability)
    }
    pub fn pairing_info(&self) -> PairingInfo {
        let inner = self
            .0
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        PairingInfo {
            extension_token: inner.database.extension_token.clone(),
            websocket_url: format!("ws://127.0.0.1:{}/api/automation/extension", self.0.port),
        }
    }
    pub fn native_pairing_info(&self) -> PairingInfo {
        let inner = self
            .0
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        PairingInfo {
            extension_token: inner.database.native_token.clone(),
            websocket_url: format!("ws://127.0.0.1:{}/api/automation/extension", self.0.port),
        }
    }
    fn lock(&self) -> Result<MutexGuard<'_, Inner>> {
        self.0
            .inner
            .lock()
            .map_err(|_| AutomationError("automation_storage_failed"))
    }
    fn save(&self, database: &Database) -> Result<()> {
        atomic_write(
            &self.0.directory.join("automation.json"),
            &serde_json::to_vec(database)
                .map_err(|_| AutomationError("automation_storage_failed"))?,
        )
    }
    fn commit(&self, inner: &mut Inner, database: Database) -> Result<()> {
        self.save(&database)?;
        inner.database = database;
        Ok(())
    }
    fn authorized(&self, capability: &str, origin: &str) -> Result<String> {
        self.lock()?
            .database
            .access
            .iter()
            .find(|a| a.origin == origin && equal_token(&a.token, capability))
            .map(|a| a.owner.clone())
            .ok_or(AutomationError("unauthorized"))
    }
    fn origin_known(&self, origin: &str) -> bool {
        self.lock()
            .is_ok_and(|i| i.database.access.iter().any(|a| a.origin == origin))
    }
    pub fn status(&self) -> Result<Value> {
        let i = self.lock()?;
        Ok(
            json!({"connected":i.peer.is_some(),"available":i.peer.is_some() && i.available,"reason":i.reason,"provider":i.database.provider}),
        )
    }
    fn list(&self, owner: &str, task: Option<&str>) -> Result<Vec<RunView>> {
        Ok(self
            .lock()?
            .database
            .runs
            .values()
            .filter(|r| r.owner == owner && task.is_none_or(|t| r.view.task_id == t))
            .map(|r| r.view.clone())
            .collect())
    }
    fn get(&self, owner: &str, id: &str) -> Result<RunView> {
        self.lock()?
            .database
            .runs
            .get(id)
            .filter(|r| r.owner == owner)
            .map(|r| r.view.clone())
            .ok_or(AutomationError("run_not_found"))
    }
    fn start(&self, owner: &str, request: StartRun) -> Result<(bool, RunView)> {
        if [&request.run_id, &request.task_id, &request.base_version_id]
            .iter()
            .any(|v| !valid_id(v))
            || request.prompt.trim().is_empty()
            || request.prompt.len() > 256 * 1024
        {
            return Err(AutomationError("invalid_run"));
        }
        let prompt_sha256 = hash(request.prompt.as_bytes());
        let mut i = self.lock()?;
        if let Some(old) = i.database.runs.get(&request.run_id) {
            if old.owner != owner {
                return Err(AutomationError("run_not_found"));
            }
            if old.view.task_id != request.task_id
                || old.view.base_version_id != request.base_version_id
                || old.view.prompt_sha256 != prompt_sha256
            {
                return Err(AutomationError("run_conflict"));
            }
            return Ok((false, old.view.clone()));
        }
        if i.database.runs.values().any(|r| active(&r.view.phase)) {
            return Err(AutomationError("run_active"));
        }
        if !i.available || i.peer.is_none() {
            return Err(AutomationError("automation_unavailable"));
        }
        let mut view = RunView {
            run_id: request.run_id,
            task_id: request.task_id,
            base_version_id: request.base_version_id,
            prompt_sha256,
            phase: "queued".into(),
            reason: None,
            sha256: None,
            mime_type: None,
        };
        let mut database = i.database.clone();
        database.runs.insert(
            view.run_id.clone(),
            StoredRun {
                view: view.clone(),
                owner: owner.into(),
                image_id: None,
            },
        );
        self.commit(&mut i, database)?;
        let start = json!({"type":"start","run":{"run_id":view.run_id,"task_id":view.task_id,"base_version_id":view.base_version_id,"prompt":request.prompt,"prompt_sha256":view.prompt_sha256}});
        if i.peer.as_ref().unwrap().sender.try_send(start).is_err() {
            view.phase = "paused".into();
            view.reason = Some("extension_disconnected".into());
            let mut database = i.database.clone();
            database.runs.get_mut(&view.run_id).unwrap().view = view.clone();
            self.commit(&mut i, database)?;
        }
        Ok((true, view))
    }
    fn cancel(&self, owner: &str, id: &str) -> Result<RunView> {
        let mut i = self.lock()?;
        let mut database = i.database.clone();
        let r = database
            .runs
            .get_mut(id)
            .filter(|r| r.owner == owner)
            .ok_or(AutomationError("run_not_found"))?;
        if r.view.phase == "completed" {
            return Err(AutomationError("run_completed"));
        }
        r.view.phase = "cancelled".into();
        r.view.reason = Some("external_generation_not_stopped".into());
        let view = r.view.clone();
        self.commit(&mut i, database)?;
        if let Some(p) = &i.peer {
            let _ = p.sender.try_send(json!({"type":"cancel","run_id":id}));
        }
        Ok(view)
    }
    fn artifact(&self, owner: &str, id: &str) -> Result<(RunView, Vec<u8>)> {
        let view = self.get(owner, id)?;
        let expected = view
            .sha256
            .as_deref()
            .ok_or(AutomationError("artifact_unavailable"))?;
        if !matches!(view.phase.as_str(), "awaiting_import" | "completed") {
            return Err(AutomationError("artifact_unavailable"));
        }
        let path = self.0.directory.join("images").join(format!("{id}.image"));
        if fs::metadata(&path)
            .map_err(|_| AutomationError("artifact_unavailable"))?
            .len()
            > MAX_ARTIFACT_BYTES as u64
        {
            return Err(AutomationError("artifact_invalid"));
        }
        let bytes = fs::read(path).map_err(|_| AutomationError("artifact_unavailable"))?;
        if hash(&bytes) != expected {
            return Err(AutomationError("artifact_invalid"));
        }
        Ok((view, bytes))
    }
    fn acknowledge(&self, owner: &str, id: &str, ack: Acknowledge) -> Result<RunView> {
        let mut i = self.lock()?;
        let mut database = i.database.clone();
        let r = database
            .runs
            .get_mut(id)
            .filter(|r| r.owner == owner)
            .ok_or(AutomationError("run_not_found"))?;
        if !valid_id(&ack.image_id)
            || r.view.sha256.as_deref() != Some(ack.sha256.as_str())
            || !matches!(r.view.phase.as_str(), "awaiting_import" | "completed")
            || r.image_id.as_ref().is_some_and(|s| s != &ack.image_id)
        {
            return Err(AutomationError("invalid_acknowledgement"));
        }
        r.image_id = Some(ack.image_id);
        r.view.phase = "completed".into();
        r.view.reason = None;
        let view = r.view.clone();
        self.commit(&mut i, database)?;
        if let Some(p) = &i.peer {
            let _ = p
                .sender
                .try_send(json!({"type":"artifact_accepted","run_id":id}));
        }
        Ok(view)
    }
}
