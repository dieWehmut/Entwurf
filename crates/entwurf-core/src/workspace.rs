//! GUI-independent durable desktop workflows. Canonical types are preserved in snapshots.
use crate::{
    error::{EntwurfError, Result},
    ingest::{ingest_project_paths, IngestOptions},
    ir::*,
    project::{AuditKind, Project, TaskVersion, VersionReason},
    prompt::{self, PromptBundle},
    storage::{self, ProjectPaths},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewPage {
    pub page: usize,
    pub svg: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handoff {
    pub envelope_json: String,
    pub markdown: String,
    pub task_id: String,
    pub revision: u32,
    pub correction_attempts: u8,
    pub status: String,
    pub last_error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredHandoff {
    version_id: String,
    bundle: PromptBundle,
    view: Handoff,
}
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceSnapshot {
    pub root: String,
    pub project: Project,
    pub selected_task_id: Option<String>,
    pub current_version: Option<TaskVersion>,
    pub preview_pages: Vec<PreviewPage>,
    pub handoff: Option<Handoff>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeEdit {
    pub id: String,
    pub title: String,
    pub body: String,
    pub latex: Option<String>,
    pub anchors: Vec<SourceAnchor>,
    pub importance: Importance,
}
impl From<&KnowledgeNode> for NodeEdit {
    fn from(n: &KnowledgeNode) -> Self {
        Self {
            id: n.id.clone(),
            title: n.title.clone(),
            body: n.body.clone(),
            latex: n.latex.clone(),
            anchors: n.anchors.clone(),
            importance: n.importance.clone(),
        }
    }
}
pub struct Workspace {
    paths: ProjectPaths,
    project: Project,
    selected: Option<String>,
    preview: Vec<PreviewPage>,
}
fn invalid(message: impl Into<String>) -> EntwurfError {
    EntwurfError::InvalidInput(message.into())
}
impl Workspace {
    pub fn create(root: impl Into<PathBuf>, name: &str) -> Result<Self> {
        if name.trim().is_empty() {
            return Err(invalid("project name is required"));
        }
        let paths = ProjectPaths::new(root);
        if paths.root.as_os_str().is_empty() {
            return Err(invalid("project directory is required"));
        }
        if paths.database_file().exists() || paths.project_file().exists() {
            return Err(invalid("project already exists; open it instead"));
        }
        let project = Project::new(name.trim());
        storage::save_project(&paths, &project)?;
        Ok(Self {
            paths,
            project,
            selected: None,
            preview: vec![],
        })
    }
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let paths = ProjectPaths::new(root);
        if paths.root.as_os_str().is_empty() {
            return Err(invalid("project directory is required"));
        }
        let project = storage::load_project(&paths)?;
        let selected = project.tasks.first().map(|t| t.id.clone());
        Ok(Self {
            paths,
            project,
            selected,
            preview: vec![],
        })
    }
    pub fn root(&self) -> &Path {
        &self.paths.root
    }
    fn current(&self, task_id: &str) -> Result<&TaskVersion> {
        let task = self
            .project
            .task(task_id)
            .ok_or_else(|| invalid("task does not belong to this project"))?;
        task.versions
            .iter()
            .find(|v| Some(&v.id) == task.current_version.as_ref())
            .ok_or_else(|| invalid("task has no current version; import sources first"))
    }
    fn baseline(&self, task_id: &str, base: &str) -> Result<KnowledgeIr> {
        let v = self.current(task_id)?;
        if v.id != base {
            return Err(invalid("this version changed; reload before editing"));
        }
        Ok(v.ir.clone())
    }
    fn commit(&mut self, project: Project, task_id: Option<&str>) -> Result<WorkspaceSnapshot> {
        storage::save_project(&self.paths, &project)?;
        self.project = project;
        self.selected = task_id.map(str::to_owned);
        self.preview.clear();
        self.snapshot(task_id)
    }
    /// Change the desktop selection without changing project knowledge.
    pub fn select_task(&mut self, task_id: Option<&str>) -> Result<WorkspaceSnapshot> {
        if let Some(id) = task_id {
            if self.project.task(id).is_none() {
                return Err(invalid("unknown task"));
            }
            if self.selected.as_deref() != Some(id) {
                self.preview.clear();
            }
            self.selected = Some(id.to_owned());
        }
        self.snapshot(task_id)
    }
    pub fn snapshot(&self, task_id: Option<&str>) -> Result<WorkspaceSnapshot> {
        let selected = task_id.or(self.selected.as_deref());
        if selected.is_some_and(|id| self.project.task(id).is_none()) {
            return Err(invalid("unknown task"));
        }
        let current_version = selected.and_then(|id| self.current(id).ok()).cloned();
        let handoff = if let Some(v) = &current_version {
            self.read_handoff(&v.ir.task_id, &v.id)?.map(|h| h.view)
        } else {
            None
        };
        Ok(WorkspaceSnapshot {
            root: self.paths.root.display().to_string(),
            project: self.project.clone(),
            selected_task_id: selected.map(str::to_owned),
            current_version,
            preview_pages: if selected == self.selected.as_deref() {
                self.preview.clone()
            } else {
                vec![]
            },
            handoff,
        })
    }
    fn save_ir(
        &mut self,
        task_id: &str,
        mut ir: KnowledgeIr,
        reason: VersionReason,
        detail: &str,
    ) -> Result<WorkspaceSnapshot> {
        ir.id = new_id();
        ir.touch();
        let mut p = self.project.clone();
        p.save_version(task_id, ir, reason)?;
        Self::state(&mut p, task_id, TaskState::IrReady)?;
        p.audit(AuditKind::VersionCreated, Some(task_id.into()), detail);
        self.commit(p, Some(task_id))
    }
    fn state(p: &mut Project, task_id: &str, next: TaskState) -> Result<()> {
        let task = p.task_mut(task_id).ok_or_else(|| invalid("unknown task"))?;
        let previous = task.state.clone();
        task.state = next.clone();
        task.updated_at = chrono::Utc::now();
        task.last_error = None;
        task.paused_from = None;
        if previous != next {
            p.audit(
                AuditKind::StateChanged,
                Some(task_id.into()),
                format!(
                    "{} -> {} (explicit desktop action)",
                    previous.as_str(),
                    next.as_str()
                ),
            );
        }
        Ok(())
    }
    pub fn create_task(
        &mut self,
        title: &str,
        chapter: Option<String>,
    ) -> Result<WorkspaceSnapshot> {
        if title.trim().is_empty() {
            return Err(invalid("task title is required"));
        }
        let mut p = self.project.clone();
        let id = p.add_task(title.trim(), chapter).id.clone();
        p.audit(AuditKind::VersionCreated, Some(id.clone()), "task created");
        self.commit(p, Some(&id))
    }
    pub fn import_sources(
        &mut self,
        task_id: &str,
        paths: &[PathBuf],
        options: &IngestOptions,
    ) -> Result<WorkspaceSnapshot> {
        let task = self
            .project
            .task(task_id)
            .ok_or_else(|| invalid("unknown task"))?;
        if paths.is_empty() {
            return Err(invalid("select at least one source"));
        }
        let mut ir = self
            .project
            .current_ir(task_id)
            .cloned()
            .unwrap_or_else(|| KnowledgeIr::empty(&self.project.id, task_id, &task.title));
        let mut options = options.clone();
        options.ocr_languages = Some(self.project.settings.ocr_languages.clone());
        let report = ingest_project_paths(&self.paths, paths, &options)?;
        let fresh = report.into_ir(&self.project.id, task_id, &task.title);
        let old_hashes: BTreeSet<_> = ir.sources.iter().map(|s| s.content_hash.clone()).collect();
        let accepted: BTreeSet<_> = fresh
            .sources
            .iter()
            .filter(|s| !old_hashes.contains(&s.content_hash))
            .map(|s| s.id.clone())
            .collect();
        let nodes: Vec<_> = fresh
            .nodes
            .into_iter()
            .filter(|n| n.anchors.iter().any(|a| accepted.contains(&a.source_id)))
            .collect();
        let node_ids: BTreeSet<_> = nodes.iter().map(|n| n.id.clone()).collect();
        let names: BTreeSet<_> = fresh
            .sources
            .iter()
            .filter(|s| accepted.contains(&s.id))
            .map(|s| s.original_name.clone())
            .collect();
        ir.sources.extend(
            fresh
                .sources
                .into_iter()
                .filter(|s| accepted.contains(&s.id)),
        );
        ir.nodes.extend(nodes);
        ir.edges.extend(
            fresh
                .edges
                .into_iter()
                .filter(|e| node_ids.contains(&e.from) && node_ids.contains(&e.to)),
        );
        ir.unverified
            .extend(fresh.unverified.into_iter().filter(|u| {
                u.node_id.as_ref().is_some_and(|id| node_ids.contains(id))
                    || u.node_id.is_none() && names.contains(&u.text)
            }));
        self.save_ir(
            task_id,
            ir,
            VersionReason::Ingested,
            "sources imported; copied originals retained",
        )
    }
    pub fn reimport_source(
        &mut self,
        task_id: &str,
        base: &str,
        source_id: &str,
        options: &IngestOptions,
    ) -> Result<WorkspaceSnapshot> {
        let mut ir = self.baseline(task_id, base)?;
        let source = ir
            .sources
            .iter()
            .find(|s| s.id == source_id)
            .cloned()
            .ok_or_else(|| invalid("unknown source"))?;
        let removed: BTreeSet<_> = ir
            .nodes
            .iter()
            .filter(|n| n.anchors.iter().any(|a| a.source_id == source_id))
            .map(|n| n.id.clone())
            .collect();
        if ir
            .nodes
            .iter()
            .any(|n| removed.contains(&n.id) && n.origin == NodeOrigin::UserEdit)
        {
            return Err(invalid(
                "reimport would overwrite manual edits; import into a new task instead",
            ));
        }
        if ir
            .sources
            .iter()
            .filter(|s| s.original_name == source.original_name)
            .count()
            > 1
        {
            return Err(invalid(
                "source names are ambiguous; reimport into a new task",
            ));
        }
        let path = PathBuf::from(&source.stored_path);
        let bytes = fs::read(&path).map_err(|e| EntwurfError::read(&path, e))?;
        if crate::ingest::hash_bytes(&bytes) != source.content_hash {
            return Err(invalid("retained original differs from imported hash"));
        }
        let mut options = options.clone();
        options.ocr_languages = Some(self.project.settings.ocr_languages.clone());
        let parsed = crate::ingest::ingest_source(source.clone(), &bytes, &path, &options)?;
        let fresh = crate::ingest::IngestReport {
            sources: vec![parsed],
            diagnostics: vec![],
        }
        .into_ir(&ir.project_id, task_id, &ir.title);
        ir.sources.retain(|s| s.id != source_id);
        ir.sources.extend(fresh.sources);
        ir.nodes.retain(|n| !removed.contains(&n.id));
        for n in &mut ir.nodes {
            if n.parent_id.as_ref().is_some_and(|id| removed.contains(id)) {
                n.parent_id = None;
            }
        }
        ir.nodes.extend(fresh.nodes);
        ir.edges
            .retain(|e| !removed.contains(&e.from) && !removed.contains(&e.to));
        ir.edges.extend(fresh.edges);
        ir.unverified.retain(|u| {
            !u.node_id.as_ref().is_some_and(|id| removed.contains(id))
                && !(u.node_id.is_none() && u.text == source.original_name)
        });
        ir.unverified.extend(fresh.unverified);
        self.save_ir(
            task_id,
            ir,
            VersionReason::Ingested,
            &format!("reimported retained source {source_id}"),
        )
    }
    pub fn import_style_images(&mut self, paths: &[PathBuf]) -> Result<WorkspaceSnapshot> {
        if paths.is_empty() {
            return Err(invalid("select at least one reference image"));
        }
        let mut style = self.project.style.clone();
        for path in paths {
            let bytes = fs::read(path).map_err(|e| EntwurfError::read(path, e))?;
            let reader = image::ImageReader::new(std::io::Cursor::new(&bytes))
                .with_guessed_format()
                .map_err(|e| invalid(e.to_string()))?;
            reader
                .into_dimensions()
                .map_err(|e| invalid(format!("invalid reference image: {e}")))?;
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| invalid("reference filename must be Unicode"))?;
            let copied =
                storage::copy_source(&self.paths, path, &crate::ingest::hash_bytes(&bytes), name)?
                    .display()
                    .to_string();
            if !style.reference_images.contains(&copied) {
                style.reference_images.push(copied);
            }
        }
        self.save_style(style)
    }
    pub fn save_settings(&mut self, ocr_languages: &str) -> Result<WorkspaceSnapshot> {
        if ocr_languages.is_empty()
            || ocr_languages.split('+').any(|part| {
                part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        {
            return Err(invalid(
                "OCR languages must be installed language codes separated by +",
            ));
        }
        let mut p = self.project.clone();
        p.settings.ocr_languages = ocr_languages.into();
        p.settings.automation_enabled = false;
        p.audit(
            AuditKind::StateChanged,
            None,
            format!("OCR language setting changed to {ocr_languages}"),
        );
        let selected = self.selected.clone();
        self.commit(p, selected.as_deref())
    }
    pub fn save_style(&mut self, style: StyleSpec) -> Result<WorkspaceSnapshot> {
        crate::contracts::validate_json(
            "style.schema.json",
            &serde_json::to_value(&style).map_err(|e| invalid(e.to_string()))?,
        )?;
        let mut p = self.project.clone();
        p.style = style;
        let currents: Vec<_> = p
            .tasks
            .iter()
            .filter_map(|t| p.current_ir(&t.id).cloned().map(|ir| (t.id.clone(), ir)))
            .collect();
        for (id, mut ir) in currents {
            ir.id = new_id();
            ir.touch();
            p.save_version(&id, ir, VersionReason::ManualSnapshot)?;
            Self::state(&mut p, &id, TaskState::IrReady)?;
        }
        p.audit(
            AuditKind::VersionCreated,
            None,
            "style changed; current outputs invalidated",
        );
        let selected = self.selected.clone();
        self.commit(p, selected.as_deref())
    }
    pub fn update_nodes(
        &mut self,
        task_id: &str,
        base: &str,
        edits: Vec<NodeEdit>,
        order: Option<Vec<String>>,
    ) -> Result<WorkspaceSnapshot> {
        let mut ir = self.baseline(task_id, base)?;
        let mut ids = BTreeSet::new();
        for edit in edits {
            if !ids.insert(edit.id.clone()) || edit.title.trim().is_empty() {
                return Err(invalid("duplicate node edit or empty title"));
            }
            let previous = ir
                .node(&edit.id)
                .cloned()
                .ok_or_else(|| invalid("unknown edited node"))?;
            for (field, before, after) in [
                (
                    CorrectionField::Title,
                    previous.title.clone(),
                    edit.title.clone(),
                ),
                (
                    CorrectionField::Body,
                    previous.body.clone(),
                    edit.body.clone(),
                ),
                (
                    CorrectionField::Latex,
                    previous.latex.clone().unwrap_or_default(),
                    edit.latex.clone().unwrap_or_default(),
                ),
                (
                    CorrectionField::Anchors,
                    serde_json::to_string(&previous.anchors).unwrap(),
                    serde_json::to_string(&edit.anchors).unwrap(),
                ),
            ] {
                if before != after {
                    ir.corrections.push(Correction {
                        id: new_id(),
                        node_id: edit.id.clone(),
                        field,
                        before,
                        after,
                        reason: "Explicit manual edit".into(),
                        anchors: edit.anchors.clone(),
                        accepted: true,
                        reverted: false,
                        created_at: chrono::Utc::now(),
                    });
                }
            }
            let node = ir.node_mut(&edit.id).unwrap();
            node.title = edit.title;
            node.body = edit.body;
            node.latex = edit.latex;
            node.anchors = edit.anchors;
            node.importance = edit.importance;
            node.origin = NodeOrigin::UserEdit;
            node.verified = false;
        }
        if let Some(order) = order {
            let all: BTreeSet<_> = ir.nodes.iter().map(|n| n.id.clone()).collect();
            if order.len() != ir.nodes.len()
                || order.iter().collect::<BTreeSet<_>>().len() != order.len()
                || order.iter().any(|id| !all.contains(id))
            {
                return Err(invalid("node order must contain every node exactly once"));
            }
            let mut nodes = std::mem::take(&mut ir.nodes);
            for id in order {
                let index = nodes.iter().position(|n| n.id == id).unwrap();
                ir.nodes.push(nodes.remove(index));
            }
        }
        self.save_ir(
            task_id,
            ir,
            VersionReason::UserEdited,
            "manual node edit/order; GPT overrides protected",
        )
    }
    pub fn rollback_version(
        &mut self,
        task_id: &str,
        version_id: &str,
    ) -> Result<WorkspaceSnapshot> {
        let task = self
            .project
            .task(task_id)
            .ok_or_else(|| invalid("unknown task"))?;
        let ir = task
            .versions
            .iter()
            .find(|v| v.id == version_id)
            .ok_or_else(|| invalid("unknown version"))?
            .ir
            .clone();
        self.save_ir(
            task_id,
            ir,
            VersionReason::ManualSnapshot,
            &format!("rollback from version {version_id}"),
        )
    }
    pub fn revert_correction(
        &mut self,
        task_id: &str,
        base: &str,
        correction_id: &str,
    ) -> Result<WorkspaceSnapshot> {
        let mut ir = self.baseline(task_id, base)?;
        prompt::revert_correction(&mut ir, correction_id)?;
        self.save_ir(
            task_id,
            ir,
            VersionReason::CorrectionReverted,
            &format!("reverted correction {correction_id}"),
        )
    }
    pub fn resolve_review(
        &mut self,
        task_id: &str,
        base: &str,
        item_id: &str,
        note: &str,
    ) -> Result<WorkspaceSnapshot> {
        if note.trim().is_empty() {
            return Err(invalid("record how this item was verified"));
        }
        let mut ir = self.baseline(task_id, base)?;
        let item = ir
            .unverified
            .iter()
            .find(|u| u.id == item_id)
            .cloned()
            .ok_or_else(|| invalid("unknown review item"))?;
        let id=item.node_id.ok_or_else(||invalid("source extraction is incomplete; reimport the source after fixing the parser/OCR dependency"))?;
        let mut evidence = ValidationReport::new();
        crate::provenance::validate_provenance(&ir, &mut evidence);
        if !evidence.is_passing() {
            return Err(invalid(
                "source/formula evidence must be repaired before review can be confirmed",
            ));
        }
        let node = ir
            .node_mut(&id)
            .ok_or_else(|| invalid("review node no longer exists"))?;
        if node.anchors.is_empty() {
            return Err(invalid("add source anchors before confirming this node"));
        }
        node.verified = true;
        node.review_note = Some(note.trim().into());
        node.origin = NodeOrigin::UserEdit;
        ir.unverified.retain(|u| u.id != item_id);
        self.save_ir(
            task_id,
            ir,
            VersionReason::UserEdited,
            &format!("review {item_id}: {}", note.trim()),
        )
    }
    fn compile_options(
        &self,
        task_id: &str,
        version: &str,
        output: Option<PathBuf>,
    ) -> crate::compiler::CompileOptions {
        let mut options = crate::compiler::CompileOptions::new(
            output
                .unwrap_or_else(|| self.paths.exports_dir())
                .join(task_id)
                .join(version),
        );
        options.layout = crate::layout::LayoutOptions::from_style(&self.project.style);
        options.validation.max_pages = self.project.style.paper.max_pages;
        options
    }
    pub fn preview_current(&mut self, task_id: &str) -> Result<WorkspaceSnapshot> {
        let v = self.current(task_id)?.clone();
        let options = self.compile_options(task_id, &v.id, None);
        let mut report = ValidationReport::new();
        crate::provenance::validate_provenance(&v.ir, &mut report);
        for item in &v.ir.unverified {
            report.push("unresolved_review", Severity::Blocking, item.reason.clone());
        }
        let (mut document, mut pages) = (None, vec![]);
        if report.is_passing() {
            match crate::compiler::prepare(&v.ir, &self.project.style, &options) {
                Ok(prepared) => {
                    report = prepared.validation;
                    document = prepared.document;
                    pages = prepared
                        .svg_pages
                        .into_iter()
                        .enumerate()
                        .map(|(i, svg)| PreviewPage { page: i + 1, svg })
                        .collect();
                }
                Err(e) => report.push("compiler_error", Severity::Blocking, e.to_string()),
            }
        }
        let passing = report.is_passing();
        let mut p = self.project.clone();
        let current = p
            .task_mut(task_id)
            .unwrap()
            .versions
            .iter_mut()
            .find(|ver| ver.id == v.id)
            .unwrap();
        current.validation = Some(report);
        current.layout = document;
        if !passing {
            current.artifacts = None;
        }
        let next = if !passing {
            TaskState::NeedsReview
        } else if current.artifacts.is_some() {
            TaskState::Exported
        } else {
            TaskState::Approved
        };
        Self::state(&mut p, task_id, next)?;
        if !passing {
            p.audit(
                AuditKind::ValidationFailed,
                Some(task_id.into()),
                "current version blocked by content/layout validation",
            );
        }
        self.commit(p, Some(task_id))?;
        self.preview = pages;
        self.snapshot(Some(task_id))
    }
    pub fn export_current(
        &mut self,
        task_id: &str,
        output: Option<PathBuf>,
    ) -> Result<WorkspaceSnapshot> {
        let checked = self.preview_current(task_id)?;
        let v = checked.current_version.as_ref().unwrap();
        if !v.validation.as_ref().is_some_and(|r| r.is_passing()) {
            return Ok(checked);
        }
        let options = self.compile_options(task_id, &v.id, output);
        let mut p = self.project.clone();
        match crate::compiler::compile(&v.ir, &p.style, &options) {
            Ok(artifacts) => {
                let paths = crate::project::ArtifactPaths {
                    svg: artifacts
                        .svg_paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect(),
                    png: artifacts
                        .png_paths
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect(),
                    pdf: artifacts.pdf_path.display().to_string(),
                    manifest: artifacts
                        .manifest_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    ir: artifacts.ir_path.display().to_string(),
                };
                p.attach_version_outputs(
                    task_id,
                    &v.id,
                    artifacts.layout_document,
                    artifacts.validation_report,
                    paths,
                )?;
                Self::state(&mut p, task_id, TaskState::Exported)?;
                p.audit(
                    AuditKind::ExportCompleted,
                    Some(task_id.into()),
                    format!(
                        "{} verified pages exported to {}",
                        artifacts.page_count,
                        options.output_dir.display()
                    ),
                );
            }
            Err(e) => {
                let current = p
                    .task_mut(task_id)
                    .unwrap()
                    .versions
                    .iter_mut()
                    .find(|ver| ver.id == v.id)
                    .unwrap();
                current.artifacts = None;
                current.validation.as_mut().unwrap().push(
                    "export_failed",
                    Severity::Blocking,
                    e.to_string(),
                );
                Self::state(&mut p, task_id, TaskState::NeedsReview)?;
                p.task_mut(task_id).unwrap().last_error = Some(e.to_string());
                p.audit(
                    AuditKind::ValidationFailed,
                    Some(task_id.into()),
                    format!("export failed: {e}"),
                );
            }
        }
        self.commit(p, Some(task_id))?;
        self.preview = checked.preview_pages;
        self.snapshot(Some(task_id))
    }
    pub fn artifact_path(&self, task_id: &str, kind: &str, index: usize) -> Result<PathBuf> {
        let v = self.current(task_id)?;
        let a = v
            .artifacts
            .as_ref()
            .ok_or_else(|| invalid("current version has no approved export"))?;
        let path = match kind {
            "svg" => a.svg.get(index),
            "png" => a.png.get(index),
            "pdf" => Some(&a.pdf),
            "manifest" => Some(&a.manifest),
            "ir" => Some(&a.ir),
            _ => None,
        }
        .ok_or_else(|| invalid("unknown artifact kind or page"))?;
        let path = PathBuf::from(path);
        let manifest_path = PathBuf::from(&a.manifest);
        let bytes = fs::read(&manifest_path).map_err(|e| EntwurfError::read(&manifest_path, e))?;
        let manifest: crate::compiler::ArtifactManifest = serde_json::from_slice(&bytes)
            .map_err(|e| invalid(format!("invalid export manifest: {e}")))?;
        let expected_ir = serde_json::to_vec_pretty(&v.ir).map_err(|e| invalid(e.to_string()))?;
        if manifest.schema_version != 1
            || manifest.project_id != self.project.id
            || manifest.task_id != task_id
            || manifest.ir_id != v.ir.id
            || manifest.ir_hash != crate::ingest::hash_bytes(&expected_ir)
        {
            return Err(invalid(
                "export belongs to another task/version or was replaced; export again",
            ));
        }
        let directory = manifest_path
            .parent()
            .ok_or_else(|| invalid("invalid export directory"))?;
        let canonical_path = path
            .canonicalize()
            .map_err(|e| EntwurfError::read(&path, e))?;
        let mut selected = kind == "manifest"
            && canonical_path
                == manifest_path
                    .canonicalize()
                    .map_err(|e| EntwurfError::read(&manifest_path, e))?;
        for file in &manifest.files {
            if Path::new(&file.path).components().count() != 1
                || !matches!(
                    Path::new(&file.path).components().next(),
                    Some(std::path::Component::Normal(_))
                )
            {
                return Err(invalid("invalid export manifest path"));
            }
            let candidate = directory.join(&file.path);
            let data = fs::read(&candidate).map_err(|e| EntwurfError::read(&candidate, e))?;
            if data.len() as u64 != file.bytes || crate::ingest::hash_bytes(&data) != file.sha256 {
                return Err(invalid(
                    "exported file changed outside Entwurf; export again",
                ));
            }
            if candidate
                .canonicalize()
                .map_err(|e| EntwurfError::read(&candidate, e))?
                == canonical_path
            {
                selected = true;
            }
        }
        if !selected {
            return Err(invalid("artifact is not in the current export manifest"));
        }
        Ok(path)
    }
    fn handoff_path(&self, task_id: &str, version: &str) -> PathBuf {
        self.paths
            .root
            .join("handoffs")
            .join(format!("{task_id}-{version}.json"))
    }
    fn read_handoff(&self, task_id: &str, version: &str) -> Result<Option<StoredHandoff>> {
        let path = self.handoff_path(task_id, version);
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path).map_err(|e| EntwurfError::read(&path, e))?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| invalid(format!("invalid persisted handoff: {e}")))
    }
    fn write_handoff(&self, h: &StoredHandoff) -> Result<()> {
        let path = self.handoff_path(&h.view.task_id, &h.version_id);
        fs::create_dir_all(path.parent().unwrap()).map_err(|e| EntwurfError::write(&path, e))?;
        storage::write_atomic(
            &path,
            &serde_json::to_vec_pretty(h).map_err(|e| invalid(e.to_string()))?,
        )
    }
    pub fn prepare_handoff(&mut self, task_id: &str) -> Result<WorkspaceSnapshot> {
        let v = self.current(task_id)?.clone();
        if self.read_handoff(task_id, &v.id)?.is_none() {
            let excerpt =
                v.ir.nodes
                    .iter()
                    .map(|n| n.body.as_str())
                    .collect::<Vec<_>>()
                    .join("\n\n");
            let mut bundle = prompt::build_prompt(&v.ir, &self.project.style, v.revision, excerpt)?;
            bundle.envelope.task_version = Some(v.id.clone());
            bundle.envelope_json = serde_json::to_string_pretty(&bundle.envelope)
                .map_err(|e| invalid(e.to_string()))?;
            bundle.markdown=format!("# Entwurf task {}\n\nReturn one ResultEnvelope JSON matching the envelope; preserve formulas and all manual edits.\n\n```json\n{}\n```\n",task_id,bundle.envelope_json);
            let view = Handoff {
                envelope_json: bundle.envelope_json.clone(),
                markdown: bundle.markdown.clone(),
                task_id: task_id.into(),
                revision: v.revision,
                correction_attempts: 0,
                status: "prepared".into(),
                last_error: None,
            };
            self.write_handoff(&StoredHandoff {
                version_id: v.id,
                bundle,
                view,
            })?;
        }
        let mut p = self.project.clone();
        Self::state(&mut p, task_id, TaskState::PromptReady)?;
        self.commit(p, Some(task_id))
    }
    pub fn import_result(&mut self, task_id: &str, json: &str) -> Result<WorkspaceSnapshot> {
        let v = self.current(task_id)?.clone();
        let mut h = self
            .read_handoff(task_id, &v.id)?
            .ok_or_else(|| invalid("prepare a prompt for the current version first"))?;
        if h.version_id != v.id
            || h.bundle.envelope.revision != v.revision
            || h.bundle.envelope.task_id != task_id
            || serde_json::to_value(&h.bundle.envelope.baseline_ir).unwrap()
                != serde_json::to_value(&v.ir).unwrap()
        {
            return Err(invalid("persisted handoff does not match current version"));
        }
        let applied: Result<_> = (|| {
            let result = prompt::parse_result_envelope(json)?;
            prompt::validate_result(&h.bundle.envelope, &result)?;
            let (ir, corrections) = prompt::apply_result(&v.ir, &result)?;
            Ok((ir, corrections, result))
        })();
        match applied {
            Ok((mut ir, corrections, result)) => {
                let result_bytes =
                    serde_json::to_vec_pretty(&result).map_err(|e| invalid(e.to_string()))?;
                let import_dir = self.paths.root.join("imports");
                fs::create_dir_all(&import_dir).map_err(|e| EntwurfError::write(&import_dir, e))?;
                let result_path =
                    import_dir.join(format!("entwurf-task-{task_id}-v{}-plan.json", v.revision));
                storage::write_atomic(&result_path, &result_bytes)?;
                ir.id = new_id();
                ir.touch();
                let mut p = self.project.clone();
                let version_id = p.save_version(task_id, ir, VersionReason::GptImported)?;
                let version = p
                    .task_mut(task_id)
                    .unwrap()
                    .versions
                    .iter_mut()
                    .find(|v| v.id == version_id)
                    .unwrap();
                version.source_prompt_hash = Some(prompt::envelope_sha256(&h.bundle.envelope)?);
                Self::state(&mut p, task_id, TaskState::IrReady)?;
                p.audit(
                    AuditKind::VersionCreated,
                    Some(task_id.into()),
                    format!(
                        "imported result {} sha256 {}",
                        result_path.display(),
                        crate::ingest::hash_bytes(&result_bytes)
                    ),
                );
                for c in corrections {
                    p.audit(
                        AuditKind::CorrectionAdded,
                        Some(task_id.into()),
                        format!("correction {}: {}", c.id, c.reason),
                    );
                }
                let snapshot = self.commit(p, Some(task_id))?;
                h.view.status = "imported".into();
                h.view.last_error = None;
                self.write_handoff(&h)?;
                Ok(snapshot)
            }
            Err(error) => {
                h.view.correction_attempts = (h.view.correction_attempts + 1).min(2);
                h.view.last_error = Some(error.to_string());
                h.view.status = if h.view.correction_attempts == 1 {
                    "correction_required"
                } else {
                    "manual_required"
                }
                .into();
                h.view.markdown = format!(
                    "{}\n\n## Result rejected\n{}\n{}\n",
                    h.bundle.markdown,
                    error,
                    if h.view.correction_attempts == 1 {
                        "Return one corrected JSON object. This is the single correction retry."
                    } else {
                        "Correction retry exhausted. Review the result manually before importing again."
                    }
                );
                self.write_handoff(&h)?;
                let mut p = self.project.clone();
                Self::state(&mut p, task_id, TaskState::NeedsReview)?;
                p.task_mut(task_id).unwrap().last_error = Some(error.to_string());
                p.audit(
                    AuditKind::Error,
                    Some(task_id.into()),
                    format!("result rejected: {error}"),
                );
                self.commit(p, Some(task_id))
            }
        }
    }
}
