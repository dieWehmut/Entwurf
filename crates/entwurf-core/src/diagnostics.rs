//! Opt-in local diagnostics. Construct an allowlisted summary; never serialize project data.
use crate::{ir::SourceKind, workspace::WorkspaceSnapshot};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct DiagnosticCapabilities {
    pub pdfium_available: bool,
    pub tesseract_available: bool,
    pub bridge_available: bool,
    pub chatgpt_detected: bool,
    pub automation_enabled: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct DiagnosticCounts {
    tasks: usize,
    versions: usize,
    sources: usize,
    nodes: usize,
    edges: usize,
    corrections: usize,
    findings: usize,
    audit_events: usize,
    state_changes: usize,
}

#[derive(Debug, Serialize)]
pub struct RedactedDiagnostics {
    schema_version: u32,
    application_version: &'static str,
    platform: &'static str,
    architecture: &'static str,
    test_status: &'static str,
    network_upload: bool,
    project_open: bool,
    capabilities: DiagnosticCapabilities,
    counts: DiagnosticCounts,
    source_kinds: BTreeMap<&'static str, usize>,
    task_states: BTreeMap<&'static str, usize>,
    report_codes: BTreeMap<&'static str, usize>,
}

/// Only fixed literals, booleans and aggregate counts may cross this boundary.
/// Counts cover all saved versions, so repeated sources can appear more than once.
/// Test status is explicitly unknown: export does not execute acceptance tests.
pub fn build_redacted_diagnostics(
    snapshot: Option<&WorkspaceSnapshot>,
    capabilities: DiagnosticCapabilities,
) -> RedactedDiagnostics {
    let mut out = RedactedDiagnostics {
        schema_version: 1,
        application_version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
        test_status: "not_run_in_this_export",
        network_upload: false,
        project_open: snapshot.is_some(),
        capabilities,
        counts: DiagnosticCounts::default(),
        source_kinds: BTreeMap::new(),
        task_states: BTreeMap::new(),
        report_codes: BTreeMap::new(),
    };
    let Some(snapshot) = snapshot else { return out };
    let project = &snapshot.project;
    out.counts.tasks = project.tasks.len();
    out.counts.audit_events = project.audit.len();
    out.counts.state_changes = project
        .audit
        .iter()
        .filter(|event| matches!(event.kind, crate::project::AuditKind::StateChanged))
        .count();
    for task in &project.tasks {
        *out.task_states.entry(task.state.as_str()).or_default() += 1;
        out.counts.versions += task.versions.len();
        for version in &task.versions {
            out.counts.sources += version.ir.sources.len();
            out.counts.nodes += version.ir.nodes.len();
            out.counts.edges += version.ir.edges.len();
            out.counts.corrections += version.ir.corrections.len();
            for source in &version.ir.sources {
                let kind = match source.kind {
                    SourceKind::Pdf => "pdf",
                    SourceKind::Markdown => "markdown",
                    SourceKind::Image => "image",
                    SourceKind::Latex => "latex",
                };
                *out.source_kinds.entry(kind).or_default() += 1;
            }
            if let Some(report) = &version.validation {
                out.counts.findings += report.findings.len();
                for finding in &report.findings {
                    // Codes are strings in persisted projects and therefore untrusted.
                    let code = match finding.code.as_str() {
                        "missing_critical_anchor" => "missing_critical_anchor",
                        "unresolved_review" => "unresolved_review",
                        "node_overlap" => "node_overlap",
                        "out_of_bounds" => "out_of_bounds",
                        "small_font" => "small_font",
                        "formula_mismatch" => "formula_mismatch",
                        _ => "other",
                    };
                    *out.report_codes.entry(code).or_default() += 1;
                }
            }
        }
    }
    out
}
