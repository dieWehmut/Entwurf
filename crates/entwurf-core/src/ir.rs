//! Versioned contracts shared by ingestion, GPT planning, rendering, and the UI.

// Wire declarations are generated from the canonical JSON Schemas.
include!("../../../packages/contracts/generated/ir.rs");

use chrono::Utc;
use std::collections::BTreeMap;

pub const IR_SCHEMA_VERSION: u32 = 1;
pub const ENVELOPE_SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    IR_SCHEMA_VERSION
}

impl KnowledgeIr {
    pub fn empty(
        project_id: impl Into<String>,
        task_id: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            schema_version: IR_SCHEMA_VERSION,
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.into(),
            task_id: task_id.into(),
            title: title.into(),
            subject: None,
            language: "zh-CN".to_string(),
            sources: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            corrections: Vec::new(),
            unverified: Vec::new(),
            updated_at: now,
        }
    }

    pub fn node(&self, id: &str) -> Option<&KnowledgeNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut KnowledgeNode> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    pub fn critical_nodes(&self) -> impl Iterator<Item = &KnowledgeNode> {
        self.nodes
            .iter()
            .filter(|node| node.importance == Importance::Critical)
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

impl Default for StyleSpec {
    fn default() -> Self {
        Self {
            id: "classic-note".to_string(),
            name: "Classic 高密度笔记".to_string(),
            paper: PaperSpec {
                width_mm: 420.0,
                height_mm: 297.0,
                margin_mm: 18.0,
                max_pages: 3,
            },
            colors: ColorSpec {
                background: "#fffdf7".to_string(),
                surface: "#f7f3ea".to_string(),
                text: "#202124".to_string(),
                muted: "#6b675f".to_string(),
                accent: "#6b3fc4".to_string(),
                border: "#d9d2c4".to_string(),
            },
            typography: TypeSpec {
                family: "LXGW WenKai".to_string(),
                title_pt: 30.0,
                body_pt: 10.5,
                caption_pt: 8.5,
                line_height: 1.42,
            },
            density: Density::Dense,
            reference_images: Vec::new(),
            rules: vec![
                "keep formulas legible at A3 print size".to_string(),
                "use restrained hand-drawn accents, not decorative clutter".to_string(),
            ],
        }
    }
}

impl ValidationReport {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            checked_at: Utc::now(),
            findings: Vec::new(),
            metrics: BTreeMap::new(),
        }
    }

    pub fn push(&mut self, code: &str, severity: Severity, message: impl Into<String>) {
        self.findings.push(Finding {
            id: uuid::Uuid::new_v4().to_string(),
            code: code.to_string(),
            severity,
            message: message.into(),
            node_id: None,
            evidence: None,
        });
    }

    pub fn blocking_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|item| item.severity == Severity::Blocking)
            .count()
    }

    pub fn is_passing(&self) -> bool {
        self.blocking_count() == 0
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Ingesting => "INGESTING",
            Self::IrReady => "IR_READY",
            Self::PromptReady => "PROMPT_READY",
            Self::Sending => "SENDING",
            Self::Waiting => "WAITING",
            Self::Importing => "IMPORTING",
            Self::Compiling => "COMPILING",
            Self::Verifying => "VERIFYING",
            Self::NeedsReview => "NEEDS_REVIEW",
            Self::Approved => "APPROVED",
            Self::Exported => "EXPORTED",
            Self::Paused => "PAUSED",
            Self::Failed => "FAILED",
            Self::Cancelled => "CANCELLED",
        }
    }

    pub fn can_transition_to(&self, next: &Self) -> bool {
        use TaskState::*;
        if self == next {
            return true;
        }
        matches!(
            (self, next),
            (Draft, Ingesting)
                | (Ingesting, IrReady)
                | (IrReady, PromptReady)
                | (PromptReady, Sending)
                | (Sending, Waiting)
                | (Waiting, Importing)
                | (Importing, Compiling)
                | (Compiling, Verifying)
                | (Verifying, NeedsReview)
                | (Verifying, Approved)
                | (NeedsReview, Approved)
                | (Approved, Exported)
                | (IrReady, Compiling)
                | (PromptReady, Compiling)
                | (NeedsReview, PromptReady)
                | (_, Paused)
                | (Paused, IrReady)
                | (Paused, PromptReady)
                | (Paused, Waiting)
                | (Paused, NeedsReview)
                | (_, Failed)
                | (_, Cancelled)
        )
    }
}

pub fn new_id() -> Id {
    uuid::Uuid::new_v4().to_string()
}
