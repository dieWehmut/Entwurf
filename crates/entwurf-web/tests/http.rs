use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use entwurf_web::{router, ServerConfig, ServerState};
use serde_json::{json, Value};
use tower::ServiceExt;

async fn request(
    app: &axum::Router,
    method: &str,
    uri: &str,
    client: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Host", "127.0.0.1:48123")
        .header("X-Entwurf-Client", client)
        .header("Content-Type", "application/json");
    if let Some(token) = token {
        request = request.header("X-Entwurf-Session", token);
    }
    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn application(data: &std::path::Path) -> axum::Router {
    let config = ServerConfig::new(data.to_path_buf(), data.join("frontend"));
    router(ServerState::new(config).unwrap())
}

#[tokio::test]
async fn cross_origin_cannot_acquire_session_and_commands_require_token() {
    let temp = tempfile::tempdir().unwrap();
    let app = application(temp.path());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/bootstrap")
                .header("Host", "127.0.0.1:48123")
                .header("Origin", "https://untrusted.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let (status, _) = request(
        &app,
        "POST",
        "/api/command",
        "one",
        None,
        json!({"command":"create_project","args":{"root":"/projects/one","name":"First"}}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn clients_keep_separate_projects_and_reopen_real_sqlite_after_restart() {
    let temp = tempfile::tempdir().unwrap();
    let app = application(temp.path());
    let (_, capabilities) = request(&app, "GET", "/api/bootstrap", "one", None, Value::Null).await;
    assert_eq!(capabilities["storage"], "sqlite");
    let token = capabilities["session_token"].as_str().unwrap();
    for client in ["one", "two"] {
        let (status, project) = request(&app, "POST", "/api/command", client, Some(token), json!({"command":"create_project","args":{"root":format!("/projects/{client}"),"name":client}})).await;
        assert_eq!(status, StatusCode::OK, "{project}");
        assert_eq!(project["root"], format!("/projects/{client}"));
    }
    let (status, first) = request(
        &app,
        "POST",
        "/api/command",
        "one",
        Some(token),
        json!({"command":"create_task","args":{"title":"only in first"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");
    let (_, second) = request(
        &app,
        "POST",
        "/api/command",
        "two",
        Some(token),
        json!({"command":"workspace_snapshot","args":{}}),
    )
    .await;
    assert_eq!(second["project"]["tasks"].as_array().unwrap().len(), 0);
    assert!(temp.path().join("projects/one/project.sqlite3").is_file());
    drop(app);
    let restarted = application(temp.path());
    let (_, fresh) = request(
        &restarted,
        "GET",
        "/api/bootstrap",
        "one",
        None,
        Value::Null,
    )
    .await;
    let token = fresh["session_token"].as_str().unwrap();
    let (status, reopened) = request(
        &restarted,
        "POST",
        "/api/command",
        "one",
        Some(token),
        json!({"command":"workspace_snapshot","args":{}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reopened}");
    assert_eq!(reopened["project"]["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(reopened["project"]["tasks"][0]["title"], "only in first");
}

#[tokio::test]
async fn explicit_reopen_reads_external_sqlite_changes() {
    let temp = tempfile::tempdir().unwrap();
    let first = application(temp.path());
    let second = application(temp.path());
    let (_, a) = request(&first, "GET", "/api/bootstrap", "a", None, Value::Null).await;
    let (_, b) = request(&second, "GET", "/api/bootstrap", "b", None, Value::Null).await;
    let a = a["session_token"].as_str().unwrap();
    let b = b["session_token"].as_str().unwrap();
    request(
        &first,
        "POST",
        "/api/command",
        "a",
        Some(a),
        json!({"command":"create_project","args":{"root":"/projects/shared","name":"Shared"}}),
    )
    .await;
    request(
        &second,
        "POST",
        "/api/command",
        "b",
        Some(b),
        json!({"command":"load_existing_project","args":{"root":"/projects/shared"}}),
    )
    .await;
    let (status, edited) = request(
        &second,
        "POST",
        "/api/command",
        "b",
        Some(b),
        json!({"command":"create_task","args":{"title":"External change"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let (status, reopened) = request(
        &first,
        "POST",
        "/api/command",
        "a",
        Some(a),
        json!({"command":"load_existing_project","args":{"root":"/projects/shared"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reopened}");
    assert_eq!(reopened["project"]["tasks"].as_array().unwrap().len(), 1);
    let (status, saved) = request(
        &first,
        "POST",
        "/api/command",
        "a",
        Some(a),
        json!({"command":"create_task","args":{"title":"After reload"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{saved}");
}

async fn multipart(
    app: &axum::Router,
    token: &str,
    parts: &[(&str, &[u8])],
) -> (StatusCode, Value) {
    let mut body = Vec::new();
    for (name, bytes) in parts {
        body.extend_from_slice(format!("--entwurf-test\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(b"--entwurf-test--\r\n");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/uploads")
                .header("Host", "127.0.0.1:48123")
                .header("X-Entwurf-Client", "a")
                .header("X-Entwurf-Session", token)
                .header("Content-Type", "multipart/form-data; boundary=entwurf-test")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn multipart_batch_limit_is_per_file_and_failed_batch_has_no_orphans() {
    let temp = tempfile::tempdir().unwrap();
    let app = application(temp.path());
    let (_, caps) = request(&app, "GET", "/api/bootstrap", "a", None, Value::Null).await;
    let token = caps["session_token"].as_str().unwrap();
    let bytes = vec![b'x'; 17 * 1024 * 1024];
    let (status, files) =
        multipart(&app, token, &[("first.md", &bytes), ("second.md", &bytes)]).await;
    assert_eq!(status, StatusCode::OK, "{files}");
    assert_eq!(files.as_array().unwrap().len(), 2);
    let before = std::fs::read_dir(temp.path().join("uploads/a"))
        .unwrap()
        .count();
    let (status, _) = multipart(
        &app,
        token,
        &[("valid.md", b"valid"), ("../escape.md", b"invalid")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        std::fs::read_dir(temp.path().join("uploads/a"))
            .unwrap()
            .count(),
        before
    );
}

#[tokio::test]
async fn uploaded_source_is_owned_until_import_then_staging_is_removed() {
    let temp = tempfile::tempdir().unwrap();
    let app = application(temp.path());
    let (_, caps) = request(&app, "GET", "/api/bootstrap", "a", None, Value::Null).await;
    let token = caps["session_token"].as_str().unwrap();
    request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"create_project","args":{"root":"/projects/a","name":"Import"}}),
    )
    .await;
    let (_, task) = request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"create_task","args":{"title":"Public fixture"}}),
    )
    .await;
    let task_id = task["selected_task_id"].as_str().unwrap();
    let (status, files) = multipart(
        &app,
        token,
        &[(
            "probability.md",
            b"# Probability\nA probability is between zero and one.\n",
        )],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let path = files[0]["path"].as_str().unwrap();
    let staged = temp.path().join(path.trim_start_matches('/'));
    assert!(staged.exists());
    let (status, _) = request(
        &app,
        "POST",
        "/api/command",
        "b",
        Some(token),
        json!({"command":"discard_uploads","args":{"paths":[path]}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(staged.exists());
    let (status, _) = request(
        &app,
        "POST",
        "/api/command",
        "b",
        Some(token),
        json!({"command":"create_project","args":{"root":"/projects/b","name":"Other"}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = request(
        &app,
        "POST",
        "/api/command",
        "b",
        Some(token),
        json!({"command":"import_sources","args":{"taskId":task_id,"paths":[path]}}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, imported) = request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"import_sources","args":{"taskId":task_id,"paths":[path]}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{imported}");
    assert_eq!(
        imported["current_version"]["ir"]["sources"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !staged.exists(),
        "retained core copy makes upload staging unnecessary"
    );
    let (status, _) = request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"discard_uploads","args":{"paths":[path]}}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "cleanup is idempotent after successful import"
    );
    let (_, pending) = multipart(
        &app,
        token,
        &[("pending.png", b"unsupported image fixture")],
    )
    .await;
    let handle = pending[0]["path"].as_str().unwrap();
    let staged = temp.path().join(handle.trim_start_matches('/'));
    let (status, _) = request(&app, "POST", "/api/command", "a", Some(token), json!({"command":"import_generated_image","args":{"path":handle,"taskId":task_id,"baseVersionId":"stale","runId":"stale-run","promptSha256":"a".repeat(64),"expectedSha256":"b".repeat(64),"mimeType":"image/png"}})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"discard_uploads","args":{"paths":[handle]}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!staged.exists());
}

#[tokio::test]
async fn image_delivery_commits_once_and_reads_identical_bytes_after_restart() {
    use base64::Engine;
    use sha2::{Digest, Sha256};
    let temp = tempfile::tempdir().unwrap();
    let app = application(temp.path());
    let (_, caps) = request(&app, "GET", "/api/bootstrap", "a", None, Value::Null).await;
    let token = caps["session_token"].as_str().unwrap();
    request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"create_project","args":{"root":"/projects/images","name":"Images"}}),
    )
    .await;
    let (_, task) = request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"create_task","args":{"title":"Public fixture"}}),
    )
    .await;
    let task_id = task["selected_task_id"].as_str().unwrap();
    let (_, files) = multipart(
        &app,
        token,
        &[(
            "probability.md",
            b"# Probability\nProbability is between zero and one.\n",
        )],
    )
    .await;
    let (status, imported) = request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"import_sources","args":{"taskId":task_id,"paths":[files[0]["path"]]}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{imported}");
    let original_version = imported["current_version"].clone();
    let png = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAIAAAD91JpzAAAADklEQVR4nGP4DwYMEAoAU7oL9ZisIGcAAAAASUVORK5CYII=").unwrap();
    let sha = format!("{:x}", Sha256::digest(&png));
    let mut image_id = String::new();
    for _ in 0..2 {
        let (_, files) = multipart(&app, token, &[("generated.png", &png)]).await;
        let (status, delivered) = request(&app, "POST", "/api/command", "a", Some(token), json!({"command":"import_generated_image","args":{
            "taskId":task_id,"baseVersionId":original_version["id"],"runId":"run-unique","promptSha256":"a".repeat(64),
            "path":files[0]["path"],"expectedSha256":sha,"mimeType":"image/png"
        }})).await;
        assert_eq!(status, StatusCode::OK, "{delivered}");
        assert_eq!(delivered["current_version"], original_version);
        assert_eq!(
            delivered["project"]["generated_images"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        image_id = delivered["project"]["generated_images"][0]["id"]
            .as_str()
            .unwrap()
            .into();
    }
    drop(app);
    let app = application(temp.path());
    let (_, caps) = request(&app, "GET", "/api/bootstrap", "a", None, Value::Null).await;
    let token = caps["session_token"].as_str().unwrap();
    let (status, bytes) = request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(token),
        json!({"command":"generated_image_bytes","args":{"taskId":task_id,"imageId":image_id}}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{bytes}");
    assert_eq!(serde_json::from_value::<Vec<u8>>(bytes).unwrap(), png);
}

async fn automation_get(app: &axum::Router, token: &str, origin: Option<&str>) -> StatusCode {
    let mut request = Request::builder()
        .uri("/api/automation/status")
        .header("Host", "127.0.0.1:48123")
        .header("Authorization", format!("Bearer {token}"));
    if let Some(origin) = origin {
        request = request.header("Origin", origin);
    }
    app.clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap()
        .status()
}

#[tokio::test]
async fn pairing_separates_workspace_session_from_origin_bound_automation_access() {
    let temp = tempfile::tempdir().unwrap();
    let app = application(temp.path());
    let (_, caps) = request(&app, "GET", "/api/bootstrap", "a", None, Value::Null).await;
    let session = caps["session_token"].as_str().unwrap();
    assert_eq!(
        automation_get(&app, session, None).await,
        StatusCode::UNAUTHORIZED
    );
    let (status, local) = request(
        &app,
        "POST",
        "/api/command",
        "a",
        Some(session),
        json!({"command":"automation_access"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{local}");
    let local = local["token"].as_str().unwrap();
    assert_eq!(automation_get(&app, local, None).await, StatusCode::OK);
    assert_eq!(
        automation_get(&app, local, Some("https://diewehmut.github.io")).await,
        StatusCode::UNAUTHORIZED
    );
    let (status, _) = request(
        &app,
        "POST",
        "/api/automation/pair",
        "a",
        Some(session),
        json!({"origin":"https://evil.example"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, pages) = request(
        &app,
        "POST",
        "/api/automation/pair",
        "a",
        Some(session),
        json!({"origin":"https://diewehmut.github.io"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{pages}");
    let pages = pages["token"].as_str().unwrap();
    assert_eq!(
        automation_get(&app, pages, Some("https://diewehmut.github.io")).await,
        StatusCode::OK
    );
    assert_eq!(
        automation_get(&app, pages, None).await,
        StatusCode::UNAUTHORIZED
    );
    let (status, _) = request(&app, "GET", "/api/projects", "a", Some(pages), Value::Null).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, native) = request(
        &app,
        "POST",
        "/api/automation/access",
        "a",
        Some(session),
        json!({"origin":"http://tauri.localhost"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{native}");
    assert_eq!(
        automation_get(
            &app,
            native["token"].as_str().unwrap(),
            Some("http://tauri.localhost")
        )
        .await,
        StatusCode::OK
    );
}

#[tokio::test]
async fn embedded_assets_serve_the_same_app_without_exposing_filesystem_paths() {
    let temp = tempfile::tempdir().unwrap();
    let state = ServerState::new(ServerConfig::new(temp.path().into(), "".into())).unwrap();
    let app = entwurf_web::router_with_assets(
        state,
        std::sync::Arc::new(|path| {
            assert!(!path.contains("..") && !path.contains('%'));
            (path == "/").then(|| {
                (
                    b"<!doctype html><title>Shared Entwurf</title>".to_vec(),
                    "text/html".into(),
                )
            })
        }),
    );
    for (path, expected) in [
        ("/", StatusCode::OK),
        ("/%2e%2e/private", StatusCode::NOT_FOUND),
        ("/missing", StatusCode::NOT_FOUND),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .header("Host", "127.0.0.1:48123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
    }
}
