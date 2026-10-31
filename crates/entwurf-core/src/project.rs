//! Local-first project state, immutable versions, audits and deterministic task
//! transitions. All time stamps are explicit UTC values.

// Wire declarations are generated from the canonical JSON Schemas.
include!("../../../packages/contracts/generated/project.rs");

use std::collections::BTreeMap;

use chrono::Utc;

use crate::compiler::{ArtifactSet, CompileOptions};
use crate::error::{EntwurfError, Result};
use crate::ir::{new_id, KnowledgeIr, StyleSpec, TaskState, ValidationReport};
use crate::layout::{layout, LayoutOptions};
use crate::model::LayoutDocument;
use crate::validate::{validate_ir, ValidationOptions};

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            automation_enabled: false,
            tesseract_available: false,
            ocr_languages: "chi_sim+eng".to_string(),
            locale: "zh-CN".to_string(),
        }
    }
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        let mut project = Self {
            schema_version: 1,
            id: new_id(),
            name: name.into(),
            created_at: now,
            updated_at: now,
            style: StyleSpec::default(),
            tasks: Vec::new(),
            audit: Vec::new(),
            settings: ProjectSettings::default(),
        };
        project.audit(AuditKind::ProjectCreated, None, "project created");
        project
    }

    pub fn audit(&mut self, kind: AuditKind, task_id: Option<String>, detail: impl Into<String>) {
        let event = AuditEvent {
            id: new_id(),
            at: Utc::now(),
            task_id,
            kind,
            detail: detail.into(),
        };
        self.audit.push(event);
        self.updated_at = Utc::now();
    }

    pub fn add_task(&mut self, title: impl Into<String>, chapter: Option<String>) -> &mut Task {
        let now = Utc::now();
        let task = Task {
            schema_version: 1,
            id: new_id(),
            title: title.into(),
            chapter,
            state: TaskState::Draft,
            created_at: now,
            updated_at: now,
            versions: Vec::new(),
            current_version: None,
            paused_from: None,
            last_error: None,
        };
        self.tasks.push(task);
        self.tasks.last_mut().expect("just pushed")
    }

    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn task_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    pub fn current_ir(&self, task_id: &str) -> Option<&KnowledgeIr> {
        let task = self.task(task_id)?;
        let version_id = task.current_version.as_deref()?;
        task.versions
            .iter()
            .find(|version| version.id == version_id)
            .map(|version| &version.ir)
    }

    pub fn save_version(
        &mut self,
        task_id: &str,
        ir: KnowledgeIr,
        reason: VersionReason,
    ) -> Result<String> {
        let task = self
            .task_mut(task_id)
            .ok_or_else(|| EntwurfError::TaskNotFound(task_id.to_string()))?;
        let revision = task
            .versions
            .iter()
            .map(|version| version.revision)
            .max()
            .unwrap_or(0)
            + 1;
        let version_id = new_id();
        task.versions.push(TaskVersion {
            id: version_id.clone(),
            revision,
            created_at: Utc::now(),
            reason: reason.clone(),
            ir,
            layout: None,
            validation: None,
            artifacts: None,
            source_prompt_hash: None,
            layout_options: None,
            visual_repairs: None,
            visual_repair_enabled: None,
        });
        task.current_version = Some(version_id.clone());
        task.updated_at = Utc::now();
        self.audit(
            match reason {
                VersionReason::CorrectionReverted => AuditKind::CorrectionReverted,
                _ => AuditKind::VersionCreated,
            },
            Some(task_id.to_string()),
            format!("saved revision {revision}"),
        );
        Ok(version_id)
    }

    pub fn attach_version_outputs(
        &mut self,
        task_id: &str,
        version_id: &str,
        layout: LayoutDocument,
        validation: ValidationReport,
        artifacts: ArtifactPaths,
    ) -> Result<()> {
        let task = self
            .task_mut(task_id)
            .ok_or_else(|| EntwurfError::TaskNotFound(task_id.to_string()))?;
        let version = task
            .versions
            .iter_mut()
            .find(|version| version.id == version_id)
            .ok_or_else(|| EntwurfError::TaskNotFound(format!("version {version_id}")))?;
        version.layout = Some(layout);
        version.validation = Some(validation);
        version.artifacts = Some(artifacts);
        task.updated_at = Utc::now();
        Ok(())
    }

    pub fn rollback_to(&mut self, task_id: &str, version_id: &str) -> Result<()> {
        let task = self
            .task_mut(task_id)
            .ok_or_else(|| EntwurfError::TaskNotFound(task_id.to_string()))?;
        let source = task
            .versions
            .iter()
            .find(|version| version.id == version_id)
            .cloned()
            .ok_or_else(|| EntwurfError::TaskNotFound(format!("version {version_id}")))?;
        let revision = task
            .versions
            .iter()
            .map(|version| version.revision)
            .max()
            .unwrap_or(0)
            + 1;
        let new_version_id = new_id();
        task.versions.push(TaskVersion {
            id: new_version_id.clone(),
            revision,
            created_at: Utc::now(),
            reason: VersionReason::ManualSnapshot,
            ir: source.ir,
            layout: source.layout,
            validation: source.validation,
            artifacts: None,
            source_prompt_hash: source.source_prompt_hash,
            layout_options: source.layout_options,
            visual_repairs: source.visual_repairs,
            visual_repair_enabled: source.visual_repair_enabled,
        });
        task.current_version = Some(new_version_id);
        task.updated_at = Utc::now();
        Ok(())
    }

    pub fn transition(&mut self, task_id: &str, next: TaskState) -> Result<()> {
        let task = self
            .task_mut(task_id)
            .ok_or_else(|| EntwurfError::TaskNotFound(task_id.to_string()))?;
        if !task.state.can_transition_to(&next) {
            return Err(EntwurfError::InvalidTransition {
                from: task.state.as_str().to_string(),
                to: next.as_str().to_string(),
            });
        }
        if next == TaskState::Paused {
            task.paused_from = Some(task.state.clone());
        } else if task.state == TaskState::Paused && task.paused_from.is_some() {
            task.paused_from = None;
        }
        let previous = task.state.clone();
        task.state = next.clone();
        task.updated_at = Utc::now();
        self.audit(
            AuditKind::StateChanged,
            Some(task_id.to_string()),
            format!("{} -> {}", previous.as_str(), next.as_str()),
        );
        Ok(())
    }

    pub fn fail(&mut self, task_id: &str, message: impl Into<String>) -> Result<()> {
        let message = message.into();
        let task = self
            .task_mut(task_id)
            .ok_or_else(|| EntwurfError::TaskNotFound(task_id.to_string()))?;
        if !task.state.can_transition_to(&TaskState::Failed) {
            return Err(EntwurfError::InvalidTransition {
                from: task.state.as_str().to_string(),
                to: TaskState::Failed.as_str().to_string(),
            });
        }
        task.state = TaskState::Failed;
        task.last_error = Some(message.clone());
        task.updated_at = Utc::now();
        self.audit(AuditKind::Error, Some(task_id.to_string()), message);
        Ok(())
    }

    pub fn cancel(&mut self, task_id: &str) -> Result<()> {
        self.transition(task_id, TaskState::Cancelled)
    }

    pub fn stats(&self) -> BTreeMap<String, usize> {
        let mut stats = BTreeMap::new();
        stats.insert("tasks".to_string(), self.tasks.len());
        stats.insert(
            "versions".to_string(),
            self.tasks.iter().map(|task| task.versions.len()).sum(),
        );
        stats.insert("audit_events".to_string(), self.audit.len());
        stats
    }

    /// Compile the current version, attach the layout/validation/artifact
    /// paths, and leave the task in APPROVED/EXPORTED only after every gate
    /// passes. A blocked compile records the validation report and fails the
    /// transition without discarding the version.
    pub fn compile_current(
        &mut self,
        task_id: &str,
        options: &CompileOptions,
    ) -> Result<ArtifactSet> {
        let task = self
            .task(task_id)
            .ok_or_else(|| EntwurfError::TaskNotFound(task_id.to_string()))?;
        let version_id = task
            .current_version
            .clone()
            .ok_or_else(|| EntwurfError::Contract("task has no current version".to_string()))?;
        let ir = task
            .versions
            .iter()
            .find(|version| version.id == version_id)
            .map(|version| version.ir.clone())
            .ok_or_else(|| EntwurfError::TaskNotFound(format!("version {version_id}")))?;
        let artifacts = crate::compiler::compile(&ir, &self.style, options)?;
        let document = layout(&ir, &self.style, &LayoutOptions::from_style(&self.style))?;
        let report = validate_ir(&ir, &ValidationOptions::default())?;
        let paths = ArtifactPaths {
            svg: artifacts
                .svg_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            png: artifacts
                .png_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
            pdf: artifacts.pdf_path.display().to_string(),
            manifest: artifacts
                .manifest_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_default(),
            ir: artifacts.ir_path.display().to_string(),
        };
        self.attach_version_outputs(task_id, &version_id, document, report, paths)?;
        if let Some(task) = self.task_mut(task_id) {
            task.state = TaskState::Exported;
            task.updated_at = Utc::now();
        }
        self.audit(
            AuditKind::ExportCompleted,
            Some(task_id.to_string()),
            format!("{} pages", artifacts.page_count),
        );
        Ok(artifacts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_rejects_skips_and_supports_pause_resume() {
        let mut project = Project::new("Test");
        let task_id = project.add_task("Chapter 1", None).id.clone();
        assert!(project.transition(&task_id, TaskState::Compiling).is_err());
        project.transition(&task_id, TaskState::Ingesting).unwrap();
        project.transition(&task_id, TaskState::Paused).unwrap();
        project.transition(&task_id, TaskState::IrReady).unwrap();
        assert_eq!(project.task(&task_id).unwrap().state, TaskState::IrReady);
    }

    #[test]
    fn versions_are_immutable_and_rollback_creates_revision() {
        let mut project = Project::new("Test");
        let task_id = project.add_task("Chapter 1", None).id.clone();
        let ir = KnowledgeIr::empty(&project.id, &task_id, "Chapter 1");
        let first = project
            .save_version(&task_id, ir.clone(), VersionReason::Ingested)
            .unwrap();
        let mut second_ir = ir;
        second_ir.title = "Edited".into();
        let second = project
            .save_version(&task_id, second_ir, VersionReason::UserEdited)
            .unwrap();
        project.rollback_to(&task_id, &first).unwrap();
        let task = project.task(&task_id).unwrap();
        assert_eq!(task.versions.len(), 3);
        assert_ne!(task.current_version.as_deref(), Some(second.as_str()));
        assert_eq!(task.versions.last().unwrap().ir.title, "Chapter 1");
    }
}
