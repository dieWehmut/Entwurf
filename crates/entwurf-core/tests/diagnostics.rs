use entwurf_core::diagnostics::{build_redacted_diagnostics, DiagnosticCapabilities};
use entwurf_core::{ingest::IngestOptions, workspace::Workspace};

#[test]
fn diagnostic_export_omits_private_fields_and_untrusted_report_codes() {
    let dir = tempfile::tempdir().unwrap();
    let secret = "SECRET_DIAGNOSTICS_SENTINEL";
    let source = dir.path().join("私密 用户 教材.md");
    std::fs::write(
        &source,
        format!("# {secret}\n\n{secret} token=sk-private clipboard prompt IR\n"),
    )
    .unwrap();
    let mut ws = Workspace::create(dir.path().join("私人 项目"), secret).unwrap();
    let id = ws
        .create_task(secret, Some(secret.into()))
        .unwrap()
        .selected_task_id
        .unwrap();
    ws.import_sources(&id, &[source], &IngestOptions::standard())
        .unwrap();
    let mut snap = ws.snapshot(Some(&id)).unwrap();
    snap.handoff = Some(entwurf_core::workspace::Handoff {
        envelope_json: secret.into(),
        markdown: secret.into(),
        task_id: secret.into(),
        revision: 1,
        correction_attempts: 0,
        status: secret.into(),
        last_error: Some(secret.into()),
    });
    snap.preview_pages
        .push(entwurf_core::workspace::PreviewPage {
            page: 1,
            svg: secret.into(),
        });
    snap.project.audit[0].detail = secret.into();
    snap.project.tasks[0].last_error = Some(secret.into());
    let report = entwurf_core::ValidationReport {
        schema_version: 1,
        checked_at: chrono::Utc::now(),
        findings: vec![entwurf_core::Finding {
            id: secret.into(),
            code: secret.into(),
            severity: entwurf_core::Severity::Warning,
            message: secret.into(),
            node_id: Some(secret.into()),
            evidence: Some(secret.into()),
        }],
        metrics: [(secret.into(), 42.0)].into(),
    };
    snap.project.tasks[0].versions[0].validation = Some(report);
    let output = serde_json::to_value(build_redacted_diagnostics(
        Some(&snap),
        DiagnosticCapabilities::default(),
    ))
    .unwrap();
    let text = output.to_string();
    for forbidden in [secret, "私密", "私人", "sk-private", &snap.root, &id] {
        assert!(!text.contains(forbidden), "leaked {forbidden}");
    }
    assert_eq!(output["counts"]["tasks"], 1);
    assert_eq!(output["source_kinds"]["markdown"], 1);
    assert_eq!(output["report_codes"]["other"], 1);
    assert_eq!(output["test_status"], "not_run_in_this_export");
    assert_eq!(output["network_upload"], false);
}

#[test]
fn diagnostics_without_open_project_only_reports_explicit_capabilities() {
    let caps = DiagnosticCapabilities {
        pdfium_available: true,
        ..Default::default()
    };
    let value = serde_json::to_value(build_redacted_diagnostics(None, caps)).unwrap();
    assert_eq!(value["project_open"], false);
    assert_eq!(value["capabilities"]["pdfium_available"], true);
    assert_eq!(value["counts"]["tasks"], 0);
}
