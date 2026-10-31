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
}
