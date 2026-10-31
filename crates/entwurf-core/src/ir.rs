//! Versioned contracts shared by ingestion, GPT planning, rendering, and the UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const IR_SCHEMA_VERSION: u32 = 1;
pub const ENVELOPE_SCHEMA_VERSION: u32 = 1;

fn default_schema_version() -> u32 {
    IR_SCHEMA_VERSION
}

/// A stable project identity. IDs are opaque strings so imported data keeps its
/// identity across versions.
pub type Id = String;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Pdf,
    Markdown,
    Image,
    Latex,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceFile {
    pub id: Id,
    pub original_name: String,
    pub kind: SourceKind,
    pub content_hash: String,
    pub stored_path: String,
    pub byte_len: u64,
    pub imported_at: DateTime<Utc>,
    pub notes: Vec<String>,
}

/// Every output fact must point to a source or to an explicit user/GPT
/// correction. Page numbers are one-based.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceAnchor {
    pub source_id: Id,
    pub page: Option<u32>,
    pub line: Option<u32>,
    pub char_start: Option<u32>,
    pub char_end: Option<u32>,
    pub quote_hash: Option<String>,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Chapter,
    Section,
    Concept,
    Definition,
    Formula,
    Theorem,
    Example,
    Warning,
    Note,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Importance {
    Critical,
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeOrigin {
    Source,
    GptCandidate,
    UserEdit,
    GptCorrection,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub id: Id,
    pub kind: NodeKind,
    pub title: String,
    pub body: String,
    pub latex: Option<String>,
    pub anchors: Vec<SourceAnchor>,
    pub importance: Importance,
    pub origin: NodeOrigin,
    pub parent_id: Option<Id>,
    pub tags: Vec<String>,
    pub verified: bool,
    pub review_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: Id,
    pub from: Id,
    pub to: Id,
    #[serde(rename = "type")]
    pub edge_type: EdgeKind,
    pub label: Option<String>,
    pub anchors: Vec<SourceAnchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Contains,
    DependsOn,
    Related,
    ExampleOf,
    Contradicts,
}

/// An automatic correction is still an auditable operation: the exact original
/// fact, replacement, reason and provenance are retained.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Correction {
    pub id: Id,
    pub node_id: Id,
    pub field: CorrectionField,
    pub before: String,
    pub after: String,
    pub reason: String,
    pub anchors: Vec<SourceAnchor>,
    pub accepted: bool,
    pub reverted: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CorrectionField {
    Title,
    Body,
    Latex,
    Anchors,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeIr {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub id: Id,
    pub project_id: Id,
    pub task_id: Id,
    pub title: String,
    pub subject: Option<String>,
    pub language: String,
    pub sources: Vec<SourceFile>,
    pub nodes: Vec<KnowledgeNode>,
    pub edges: Vec<GraphEdge>,
    pub corrections: Vec<Correction>,
    pub unverified: Vec<UnverifiedItem>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnverifiedItem {
    pub id: Id,
    pub node_id: Option<Id>,
    pub kind: UnverifiedKind,
    pub text: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UnverifiedKind {
    Formula,
    MissingAnchor,
    Ocr,
    Contradiction,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleSpec {
    pub id: Id,
    pub name: String,
    pub paper: PaperSpec,
    pub colors: ColorSpec,
    pub typography: TypeSpec,
    pub density: Density,
    pub reference_images: Vec<String>,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperSpec {
    pub width_mm: f32,
    pub height_mm: f32,
    pub margin_mm: f32,
    pub max_pages: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorSpec {
    pub background: String,
    pub surface: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
    pub border: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeSpec {
    pub family: String,
    pub title_pt: f32,
    pub body_pt: f32,
    pub caption_pt: f32,
    pub line_height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    Relaxed,
    Balanced,
    Dense,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskEnvelope {
    pub schema_version: u32,
    #[serde(default)]
    pub task_version: Option<String>,
    pub task_id: Id,
    pub project_id: Id,
    pub revision: u32,
    pub title: String,
    pub locale: String,
    pub source_excerpt: String,
    pub baseline_ir: KnowledgeIr,
    pub style: StyleSpec,
    pub requested_artifact: RequestedArtifact,
    pub response_schema: String,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestedArtifact {
    pub kind: ArtifactKind,
    pub pages: u16,
    pub render_mode: RenderMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    KnowledgeSheet,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RenderMode {
    NativeSvg,
    PlanPlusImage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultEnvelope {
    pub schema_version: u32,
    pub task_id: Id,
    pub revision: u32,
    pub ir_patch: IrPatch,
    pub visual_plan: VisualPlan,
    pub corrections: Vec<CorrectionProposal>,
    pub image_path: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IrPatch {
    pub add_nodes: Vec<KnowledgeNode>,
    pub update_nodes: Vec<NodePatch>,
    pub add_edges: Vec<GraphEdge>,
    pub remove_node_ids: Vec<Id>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePatch {
    pub id: Id,
    pub title: Option<String>,
    pub body: Option<String>,
    pub latex: Option<String>,
    pub importance: Option<Importance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualPlan {
    pub page_count: u16,
    pub groups: Vec<VisualGroup>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisualGroup {
    pub id: Id,
    pub title: String,
    pub node_ids: Vec<Id>,
    pub page: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionProposal {
    pub node_id: Id,
    pub field: CorrectionField,
    pub before: String,
    pub after: String,
    pub reason: String,
    pub anchors: Vec<SourceAnchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub checked_at: DateTime<Utc>,
    pub findings: Vec<Finding>,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Blocking,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub id: Id,
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub node_id: Option<Id>,
    pub evidence: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskState {
    Draft,
    Ingesting,
    IrReady,
    PromptReady,
    Sending,
    Waiting,
    Importing,
    Compiling,
    Verifying,
    NeedsReview,
    Approved,
    Exported,
    Paused,
    Failed,
    Cancelled,
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
