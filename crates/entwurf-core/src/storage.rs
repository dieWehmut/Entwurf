//! SQLite WAL persistence with portable JSON recovery and immutable IR snapshots.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::contracts;
use crate::error::{EntwurfError, Result};
use crate::ir::KnowledgeIr;
use crate::project::{Project, TaskVersion};

pub const PROJECT_FILE: &str = "project.json";
pub const DATABASE_FILE: &str = "project.sqlite3";
pub const IR_FILE: &str = "knowledge.ir.json";
const DATABASE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectPaths {
    pub root: PathBuf,
}

impl ProjectPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn project_file(&self) -> PathBuf {
        self.root.join(PROJECT_FILE)
    }
    pub fn database_file(&self) -> PathBuf {
        self.root.join(DATABASE_FILE)
    }
    pub fn sources_dir(&self) -> PathBuf {
        self.root.join("sources")
    }
    pub fn versions_dir(&self) -> PathBuf {
        self.root.join("versions")
    }
    pub fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn ensure(&self) -> Result<()> {
        for dir in [
            self.root.clone(),
            self.sources_dir(),
            self.versions_dir(),
            self.exports_dir(),
            self.logs_dir(),
        ] {
            fs::create_dir_all(&dir).map_err(|e| EntwurfError::write(&dir, e))?;
        }
        Ok(())
    }
}

fn corrupt(path: &Path, reason: impl ToString) -> EntwurfError {
    EntwurfError::CorruptState {
        path: path.to_owned(),
        reason: reason.to_string(),
    }
}
fn json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    serde_json::to_vec_pretty(value).map_err(|e| EntwurfError::Contract(e.to_string()))
}
fn component(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err(EntwurfError::InvalidInput(format!(
            "unsafe file identity: {value}"
        )));
    }
    Ok(())
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

// Serializes SQLite commit + JSON mirror across processes on Windows and Unix.
fn lock_project(paths: &ProjectPaths) -> Result<fs::File> {
    paths.ensure()?;
    let path = paths.root.join(".project.lock");
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|e| EntwurfError::write(&path, e))?;
    file.lock().map_err(|e| EntwurfError::write(&path, e))?;
    Ok(file)
}

fn connect(paths: &ProjectPaths) -> Result<Connection> {
    paths.ensure()?;
    let mut connection = Connection::open(paths.database_file())?;
    connection.busy_timeout(Duration::from_secs(5))?;
    let version: u32 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
    if version > DATABASE_VERSION {
        return Err(corrupt(
            &paths.database_file(),
            format!("unsupported database schema {version}"),
        ));
    }
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "FULL")?;
    connection.pragma_update(None, "foreign_keys", true)?;
    if version == 0 {
        let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch("CREATE TABLE project (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE tasks (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES project(id), ordinal INTEGER NOT NULL, data TEXT NOT NULL);
            CREATE TABLE versions (id TEXT PRIMARY KEY, task_id TEXT NOT NULL REFERENCES tasks(id), revision INTEGER NOT NULL, immutable TEXT NOT NULL, data TEXT NOT NULL, UNIQUE(task_id, revision));
            CREATE TABLE audit (id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES project(id), ordinal INTEGER NOT NULL, data TEXT NOT NULL);
            PRAGMA user_version = 1;")?;
        tx.commit()?;
    }
    Ok(connection)
}

fn validate_project(project: &Project) -> Result<()> {
    contracts::validate_json(
        "project.schema.json",
        &serde_json::to_value(project).map_err(|e| EntwurfError::Contract(e.to_string()))?,
    )?;
    if project.schema_version != 1 {
        return Err(EntwurfError::Contract(format!(
            "unsupported project schema {}",
            project.schema_version
        )));
    }
    component(&project.id)?;
    if project.name.trim().is_empty() {
        return Err(EntwurfError::InvalidInput("project name is empty".into()));
    }
    contracts::validate_style(&project.style)?;
    let mut task_ids = HashSet::new();
    let mut version_ids = HashSet::new();
    for task in &project.tasks {
        component(&task.id)?;
        if task.schema_version != 1 || task.title.trim().is_empty() || !task_ids.insert(&task.id) {
            return Err(EntwurfError::Contract("invalid/duplicate task".into()));
        }
        let mut revisions = HashSet::new();
        for version in &task.versions {
            component(&version.id)?;
            if version.revision == 0
                || !revisions.insert(version.revision)
                || !version_ids.insert(&version.id)
            {
                return Err(EntwurfError::Contract("invalid/duplicate version".into()));
            }
            contracts::validate_ir(&version.ir)?;
            if version.ir.project_id != project.id || version.ir.task_id != task.id {
                return Err(EntwurfError::Contract(
                    "IR does not belong to project/task".into(),
                ));
            }
            if version
                .validation
                .as_ref()
                .is_some_and(|v| v.schema_version != 1)
            {
                return Err(EntwurfError::Contract(
                    "unsupported validation schema".into(),
                ));
            }
        }
        if task
            .current_version
            .as_ref()
            .is_some_and(|id| !task.versions.iter().any(|v| &v.id == id))
        {
            return Err(EntwurfError::Contract("current version is missing".into()));
        }
    }
    let mut audit_ids = HashSet::new();
    for event in &project.audit {
        if event.id.is_empty()
            || !audit_ids.insert(&event.id)
            || event
                .task_id
                .as_ref()
                .is_some_and(|id| !task_ids.contains(id))
        {
            return Err(EntwurfError::Contract("invalid audit identity".into()));
        }
    }
    Ok(())
}

fn immutable(version: &TaskVersion) -> Result<Vec<u8>> {
    json(&(
        version.id.clone(),
        version.revision,
        version.created_at,
        &version.reason,
        &version.ir,
        &version.source_prompt_hash,
    ))
}

fn decode_project(path: &Path, bytes: &[u8]) -> Result<Project> {
    let value = serde_json::from_slice(bytes).map_err(|e| corrupt(path, e))?;
    contracts::validate_json("project.schema.json", &value).map_err(|e| corrupt(path, e))?;
    let project: Project = serde_json::from_value(value).map_err(|e| corrupt(path, e))?;
    validate_project(&project).map_err(|e| corrupt(path, e))?;
    Ok(project)
}

fn persist(connection: &mut Connection, paths: &ProjectPaths, project: &Project) -> Result<()> {
    validate_project(project)?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let previous: Option<String> = tx
        .query_row("SELECT data FROM project", [], |r| r.get(0))
        .optional()?;
    if let Some(previous) = previous {
        let old = decode_project(&paths.database_file(), previous.as_bytes())?;
        if old.id != project.id || old.created_at != project.created_at {
            return Err(EntwurfError::Contract(
                "project identity is immutable".into(),
            ));
        }
        // Audits and saved knowledge history may never disappear or be rewritten.
        if project.audit.len() < old.audit.len()
            || json(&project.audit[..old.audit.len()])? != json(&old.audit)?
        {
            return Err(EntwurfError::Contract(
                "audit history is append-only".into(),
            ));
        }
        for task in &old.tasks {
            let next = project
                .task(&task.id)
                .ok_or_else(|| EntwurfError::Contract("task history cannot be removed".into()))?;
            for version in &task.versions {
                let next_version = next
                    .versions
                    .iter()
                    .find(|v| v.id == version.id)
                    .ok_or_else(|| {
                        EntwurfError::Contract("version history cannot be removed".into())
                    })?;
                if immutable(version)? != immutable(next_version)? {
                    return Err(EntwurfError::Contract(
                        "saved version knowledge is immutable".into(),
                    ));
                }
                if let Some(history) = &version.visual_repairs {
                    let retained = next_version.visual_repairs.as_ref().is_some_and(|next| {
                        next.len() >= history.len() && &next[..history.len()] == history.as_slice()
                    });
                    if !retained {
                        return Err(EntwurfError::Contract("saved visual repair history is append-only; undo requires a new version".into()));
                    }
                }
            }
        }
    }
    let data = json(project)?;
    tx.execute("INSERT INTO project(id,data) VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data", params![project.id, String::from_utf8_lossy(&data)])?;
    for (ordinal, task) in project.tasks.iter().enumerate() {
        tx.execute("INSERT INTO tasks(id,project_id,ordinal,data) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET ordinal=excluded.ordinal,data=excluded.data", params![task.id, project.id, ordinal as i64, String::from_utf8_lossy(&json(task)?)])?;
        for version in &task.versions {
            let bytes = json(&version.ir)?;
            let path = paths
                .versions_dir()
                .join(&task.id)
                .join(&version.id)
                .join(IR_FILE);
            write_immutable(&path, &bytes)?;
            tx.execute("INSERT INTO versions(id,task_id,revision,immutable,data) VALUES (?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET data=excluded.data", params![version.id, task.id, version.revision, String::from_utf8_lossy(&immutable(version)?), String::from_utf8_lossy(&json(version)?)])?;
        }
    }
    for (ordinal, event) in project.audit.iter().enumerate() {
        tx.execute(
            "INSERT OR IGNORE INTO audit(id,project_id,ordinal,data) VALUES (?1,?2,?3,?4)",
            params![
                event.id,
                project.id,
                ordinal as i64,
                String::from_utf8_lossy(&json(event)?)
            ],
        )?;
    }
    tx.commit()?;
    // SQLite is authoritative after commit. On interruption, load repairs a stale/missing mirror.
    write_atomic(&paths.project_file(), &data)
}

fn read_project_mirror(paths: &ProjectPaths) -> Result<Option<Project>> {
    if !paths.project_file().exists() {
        return Ok(None);
    }
    let bytes =
        fs::read(paths.project_file()).map_err(|e| EntwurfError::read(paths.project_file(), e))?;
    decode_project(&paths.project_file(), &bytes).map(Some)
}

// Under the project lock, restore recoverable history whenever migrations exist
// without an authoritative project row (including interrupted initialization).
fn recover_committed_project(
    connection: &mut Connection,
    paths: &ProjectPaths,
    mirror: Option<Project>,
) -> Result<Option<Project>> {
    let data: Option<String> = connection
        .query_row("SELECT data FROM project", [], |r| r.get(0))
        .optional()?;
    if let Some(data) = data {
        return decode_project(&paths.database_file(), data.as_bytes()).map(Some);
    }
    if let Some(project) = mirror {
        persist(connection, paths, &project)?;
        return Ok(Some(project));
    }
    Ok(None)
}

pub fn save_project(paths: &ProjectPaths, project: &Project) -> Result<()> {
    validate_project(project)?;
    let _lock = lock_project(paths)?;
    // Decode before creating/migrating SQLite so corrupt mirrors fail unchanged.
    let mirror = read_project_mirror(paths)?;
    let mut connection = connect(paths)?;
    recover_committed_project(&mut connection, paths, mirror)?;
    persist(&mut connection, paths, project)
}

pub fn load_project(paths: &ProjectPaths) -> Result<Project> {
    let _lock = lock_project(paths)?;
    // Malformed snapshots are surfaced; recovery must be explicit for corrupt files.
    let mirror = read_project_mirror(paths)?;
    if !paths.database_file().exists() && mirror.is_none() {
        return Err(EntwurfError::ProjectNotFound(
            paths.root.display().to_string(),
        ));
    }
    let mut connection = connect(paths)?;
    let project = recover_committed_project(&mut connection, paths, mirror)?
        .ok_or_else(|| EntwurfError::ProjectNotFound(paths.root.display().to_string()))?;
    // Repair missing immutable files; a mismatching file is corruption, not a migration.
    for task in &project.tasks {
        for version in &task.versions {
            let path = paths
                .versions_dir()
                .join(&task.id)
                .join(&version.id)
                .join(IR_FILE);
            write_immutable(&path, &json(&version.ir)?)?;
        }
    }
    write_atomic(&paths.project_file(), &json(&project)?)?;
    Ok(project)
}

/// Rebuild a missing database from the validated portable project snapshot.
/// An existing database is never replaced implicitly (including a corrupt one).
pub fn rebuild_database(paths: &ProjectPaths) -> Result<Project> {
    let _lock = lock_project(paths)?;
    rebuild_database_locked(paths)
}

fn rebuild_database_locked(paths: &ProjectPaths) -> Result<Project> {
    if paths.database_file().exists() {
        return Err(EntwurfError::InvalidInput(
            "remove or quarantine existing database before rebuilding".into(),
        ));
    }
    let mirror = read_project_mirror(paths)?
        .ok_or_else(|| EntwurfError::ProjectNotFound(paths.root.display().to_string()))?;
    let mut connection = connect(paths)?;
    recover_committed_project(&mut connection, paths, Some(mirror))?
        .ok_or_else(|| EntwurfError::ProjectNotFound(paths.root.display().to_string()))
}

pub fn save_ir(paths: &ProjectPaths, task_id: &str, ir: &KnowledgeIr) -> Result<PathBuf> {
    component(task_id)?;
    if task_id != ir.task_id {
        return Err(EntwurfError::InvalidInput(
            "IR does not belong to snapshot task".into(),
        ));
    }
    contracts::validate_ir(ir)?;
    paths.ensure()?;
    let bytes = json(ir)?;
    let path = paths
        .versions_dir()
        .join(task_id)
        .join(hash(&bytes))
        .join(IR_FILE);
    write_immutable(&path, &bytes)?;
    Ok(path)
}

pub fn load_ir(path: &Path) -> Result<KnowledgeIr> {
    let bytes = fs::read(path).map_err(|e| EntwurfError::read(path, e))?;
    let value = serde_json::from_slice(&bytes).map_err(|e| corrupt(path, e))?;
    contracts::validate_json("knowledge-ir.schema.json", &value).map_err(|e| corrupt(path, e))?;
    serde_json::from_value(value).map_err(|e| corrupt(path, e))
}

pub fn copy_source(
    paths: &ProjectPaths,
    source: &Path,
    expected_hash: &str,
    _name: &str,
) -> Result<PathBuf> {
    let bytes = fs::read(source).map_err(|e| EntwurfError::read(source, e))?;
    let actual_hash = hash(&bytes);
    if actual_hash != expected_hash {
        return Err(EntwurfError::InvalidInput(
            "source SHA-256 does not match supplied hash".into(),
        ));
    }
    paths.ensure()?;
    let path = paths.sources_dir().join(actual_hash);
    write_immutable(&path, &bytes)?;
    Ok(path)
}

fn write_immutable(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        let existing = fs::read(path).map_err(|e| EntwurfError::read(path, e))?;
        if existing == bytes {
            return Ok(());
        }
        return Err(corrupt(path, "immutable file differs from saved knowledge"));
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| EntwurfError::write(parent, e))?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| EntwurfError::write(parent, e))?;
    temp.write_all(bytes)
        .map_err(|e| EntwurfError::write(path, e))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| EntwurfError::write(path, e))?;
    match temp.persist_noclobber(path) {
        Ok(_) => sync_parent(parent),
        Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::read(path).map_err(|e| EntwurfError::read(path, e))? == bytes {
                Ok(())
            } else {
                Err(corrupt(path, "conflicting immutable file"))
            }
        }
        Err(e) => Err(EntwurfError::write(path, e.error)),
    }
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|e| EntwurfError::write(parent, e))?;
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|e| EntwurfError::write(parent, e))?;
    temp.write_all(bytes)
        .map_err(|e| EntwurfError::write(path, e))?;
    temp.as_file()
        .sync_all()
        .map_err(|e| EntwurfError::write(path, e))?;
    // tempfile uses replacement semantics on Windows, unlike std::fs::rename.
    temp.persist(path)
        .map_err(|e| EntwurfError::write(path, e.error))?;
    sync_parent(parent)
}

fn sync_parent(parent: &Path) -> Result<()> {
    #[cfg(unix)]
    fs::File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|e| EntwurfError::write(parent, e))?;
    let _ = parent;
    Ok(())
}

pub fn write_text(path: &Path, text: &str) -> Result<()> {
    write_atomic(path, text.as_bytes())
}

/// Compatibility entry point retained for callers; applies real SQLite migrations.
pub fn migrate_placeholder(paths: &ProjectPaths) -> Result<()> {
    let _lock = lock_project(paths)?;
    connect(paths).map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Project, VersionReason};

    fn sample_project() -> Project {
        let mut project = Project::new("中文 项目");
        let task = project.add_task("Chapter", None).id.clone();
        project
            .save_version(
                &task,
                KnowledgeIr::empty(&project.id, &task, "原始公式"),
                VersionReason::Ingested,
            )
            .unwrap();
        project
    }

    #[test]
    fn reopen_uses_wal_database_and_recovers_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let project = sample_project();
        save_project(&paths, &project).unwrap();
        assert!(dir.path().join("project.sqlite3").exists());
        fs::remove_file(paths.project_file()).unwrap();
        assert_eq!(
            serde_json::to_value(load_project(&paths).unwrap()).unwrap(),
            serde_json::to_value(&project).unwrap()
        );
        assert!(paths.project_file().exists());
    }

    #[test]
    fn version_files_are_append_only_and_changes_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let mut project = sample_project();
        save_project(&paths, &project).unwrap();
        let mut ir = project.tasks[0].versions[0].ir.clone();
        let first = save_ir(&paths, &ir.task_id, &ir).unwrap();
        let original = fs::read(&first).unwrap();
        ir.title = "changed".into();
        let second = save_ir(&paths, &ir.task_id, &ir).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(first).unwrap(), original);
        project.tasks[0].versions[0].ir.title = "mutated history".into();
        assert!(save_project(&paths, &project).is_err());
        assert_eq!(
            load_project(&paths).unwrap().tasks[0].versions[0].ir.title,
            "原始公式"
        );
    }

    #[test]
    fn copied_sources_deduplicate_across_extensions_and_verify_hash() {
        use sha2::{Digest, Sha256};
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path().join("project"));
        let a = dir.path().join("a.md");
        let b = dir.path().join("b.txt");
        fs::write(&a, "same content").unwrap();
        fs::write(&b, "same content").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"same content"));
        assert_eq!(
            copy_source(&paths, &a, &hash, "a").unwrap(),
            copy_source(&paths, &b, &hash, "b").unwrap()
        );
        assert!(copy_source(&paths, &a, &"0".repeat(64), "bad").is_err());
        assert!(copy_source(&paths, &a, "../escape", "bad").is_err());
    }

    #[test]
    fn invalid_ir_schema_is_rejected_on_load_and_save() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let mut ir = KnowledgeIr::empty("p", "t", "title");
        ir.schema_version = 2;
        assert!(save_ir(&paths, "t", &ir).is_err());
        let path = dir.path().join("invalid.json");
        fs::write(&path, serde_json::to_vec(&ir).unwrap()).unwrap();
        assert!(matches!(
            load_ir(&path),
            Err(EntwurfError::CorruptState { .. })
        ));
    }

    #[test]
    fn recovery_rejects_missing_nested_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        paths.ensure().unwrap();
        let mut value = serde_json::to_value(sample_project()).unwrap();
        value["tasks"][0]["versions"][0]["ir"]
            .as_object_mut()
            .unwrap()
            .remove("schema_version");
        fs::write(paths.project_file(), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            load_project(&paths),
            Err(EntwurfError::CorruptState { .. })
        ));
        assert!(!paths.database_file().exists());
    }

    #[test]
    fn empty_migrated_database_recovers_mirror_before_rejecting_history_changes() {
        for change in ["version removal", "audit rewrite", "identity replacement"] {
            let dir = tempfile::tempdir().unwrap();
            let paths = ProjectPaths::new(dir.path());
            let original = sample_project();
            save_project(&paths, &original).unwrap();
            fs::remove_file(paths.database_file()).unwrap();
            migrate_placeholder(&paths).unwrap();
            let original_mirror = fs::read(paths.project_file()).unwrap();
            let mut edited = original.clone();
            match change {
                "version removal" => {
                    edited.tasks[0].versions.clear();
                    edited.tasks[0].current_version = None;
                }
                "audit rewrite" => edited.audit[0].detail = "rewritten".into(),
                _ => edited = Project::new("replacement"),
            }
            assert!(save_project(&paths, &edited).is_err(), "accepted {change}");
            assert_eq!(fs::read(paths.project_file()).unwrap(), original_mirror);
            let recovered = load_project(&paths).unwrap();
            assert_eq!(
                serde_json::to_value(&recovered).unwrap(),
                serde_json::to_value(&original).unwrap()
            );
            // Reopening uses the recovered authoritative row, not just the mirror.
            fs::remove_file(paths.project_file()).unwrap();
            assert_eq!(
                serde_json::to_value(load_project(&paths).unwrap()).unwrap(),
                serde_json::to_value(original).unwrap()
            );
        }
    }

    #[test]
    fn empty_migrated_database_recovers_mirror_before_appending_new_version() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let mut project = sample_project();
        save_project(&paths, &project).unwrap();
        let original_ir = serde_json::to_value(&project.tasks[0].versions[0].ir).unwrap();
        fs::remove_file(paths.database_file()).unwrap();
        migrate_placeholder(&paths).unwrap();
        let task_id = project.tasks[0].id.clone();
        let mut next_ir = project.tasks[0].versions[0].ir.clone();
        next_ir.title = "new version".into();
        project
            .save_version(&task_id, next_ir, VersionReason::UserEdited)
            .unwrap();
        save_project(&paths, &project).unwrap();
        let loaded = load_project(&paths).unwrap();
        assert_eq!(loaded.tasks[0].versions.len(), 2);
        assert_eq!(
            serde_json::to_value(&loaded.tasks[0].versions[0].ir).unwrap(),
            original_ir
        );
        assert_eq!(
            serde_json::to_value(loaded).unwrap(),
            serde_json::to_value(project).unwrap()
        );
    }

    #[test]
    fn standalone_snapshot_rejects_task_ownership_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let ir = KnowledgeIr::empty("p", "task-b", "title");
        assert!(matches!(
            save_ir(&paths, "task-a", &ir),
            Err(EntwurfError::InvalidInput(_))
        ));
        assert!(!paths.versions_dir().exists());
        let stored = save_ir(&paths, "task-b", &ir).unwrap();
        assert_eq!(load_ir(&stored).unwrap().task_id, "task-b");
    }

    #[test]
    fn saving_after_database_loss_preserves_existing_history() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let mut project = sample_project();
        save_project(&paths, &project).unwrap();
        fs::remove_file(paths.database_file()).unwrap();
        project.tasks[0].versions.clear();
        project.tasks[0].current_version = None;
        assert!(save_project(&paths, &project).is_err());
        assert_eq!(load_project(&paths).unwrap().tasks[0].versions.len(), 1);
    }

    #[test]
    fn database_rebuild_retains_exact_ir_and_relational_records() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let project = sample_project();
        save_project(&paths, &project).unwrap();
        let db = Connection::open(paths.database_file()).unwrap();
        let wal: String = db
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
            .unwrap();
        assert_eq!(wal, "wal");
        for table in ["tasks", "versions", "audit"] {
            let count: i64 = db
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert!(count > 0);
        }
        drop(db);
        fs::remove_file(paths.database_file()).unwrap();
        let rebuilt = load_project(&paths).unwrap();
        assert_eq!(
            serde_json::to_value(rebuilt).unwrap(),
            serde_json::to_value(project).unwrap()
        );
        assert!(paths.database_file().exists());
    }

    #[test]
    fn derived_outputs_can_attach_without_rewriting_ir_or_audit() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let mut project = sample_project();
        save_project(&paths, &project).unwrap();
        project.tasks[0].versions[0].validation = Some(crate::ir::ValidationReport::new());
        save_project(&paths, &project).unwrap();
        assert!(load_project(&paths).unwrap().tasks[0].versions[0]
            .validation
            .is_some());
        project.audit[0].detail = "changed history".into();
        assert!(save_project(&paths, &project).is_err());
        assert_ne!(
            load_project(&paths).unwrap().audit[0].detail,
            "changed history"
        );
    }

    #[test]
    fn future_database_version_and_corrupt_snapshots_are_not_migrated() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        save_project(&paths, &sample_project()).unwrap();
        let db = Connection::open(paths.database_file()).unwrap();
        db.pragma_update(None, "user_version", 999).unwrap();
        drop(db);
        assert!(matches!(
            load_project(&paths),
            Err(EntwurfError::CorruptState { .. })
        ));
        let db = Connection::open(paths.database_file()).unwrap();
        assert_eq!(
            db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
                .unwrap(),
            999
        );
    }

    #[test]
    fn project_round_trips_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        let mut project = Project::new("中文 项目");
        let task_id = project.add_task("Chapter", None).id.clone();
        project
            .save_version(
                &task_id,
                KnowledgeIr::empty(&project.id, &task_id, "Chapter"),
                VersionReason::Ingested,
            )
            .unwrap();
        save_project(&paths, &project).unwrap();
        let loaded = load_project(&paths).unwrap();
        assert_eq!(loaded.name, "中文 项目");
        assert_eq!(loaded.tasks[0].versions.len(), 1);
        assert!(!paths.project_file().with_extension("json.tmp").exists());
    }

    #[test]
    fn invalid_project_is_reported_as_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProjectPaths::new(dir.path());
        paths.ensure().unwrap();
        fs::write(paths.project_file(), "{not json").unwrap();
        assert!(matches!(
            load_project(&paths),
            Err(EntwurfError::CorruptState { .. })
        ));
    }
}
