use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
    Router,
};
use entwurf_automation::AutomationState;
use serde_json::{json, Value};
use tower::ServiceExt;

const ORIGIN: &str = "https://diewehmut.github.io";
async fn request(
    router: Router,
    method: &str,
    path: &str,
    token: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = router
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header("host", "127.0.0.1:48123")
                .header("origin", ORIGIN)
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let code = response.status();
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    (code, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn access_is_durable_origin_bound_and_automation_only() {
    let dir = tempfile::tempdir().unwrap();
    let state = AutomationState::new(dir.path(), 48123).unwrap();
    let token = state.issue_access("browser-owner", ORIGIN).unwrap();
    assert!(token.len() >= 32);
    assert_eq!(state.issue_access("browser-owner", ORIGIN).unwrap(), token);
    let pairing = state.pairing_info();
    drop(state);
    let state = AutomationState::new(dir.path(), 48123).unwrap();
    assert_eq!(state.issue_access("browser-owner", ORIGIN).unwrap(), token);
    assert_eq!(
        state.pairing_info().extension_token,
        pairing.extension_token
    );
    assert_eq!(
        request(
            state.router(),
            "GET",
            "/api/automation/status",
            &token,
            json!(null)
        )
        .await
        .0,
        StatusCode::OK
    );
    let response = state
        .router()
        .oneshot(
            Request::builder()
                .uri("/api/automation/status")
                .header("host", "127.0.0.1:48123")
                .header("origin", "https://evil.test")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        request(state.router(), "GET", "/api/command", &token, json!(null))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn host_and_preflight_do_not_grant_wildcard_access() {
    let dir = tempfile::tempdir().unwrap();
    let state = AutomationState::new(dir.path(), 48123).unwrap();
    state.issue_access("owner", ORIGIN).unwrap();
    for host in [
        "evil.test",
        "127.0.0.1.evil.test:48123",
        "localhost@evil.test",
    ] {
        let response = state
            .router()
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/api/automation/runs")
                    .header("host", host)
                    .header("origin", ORIGIN)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
    let response = state
        .router()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/automation/runs")
                .header("host", "127.0.0.1:48123")
                .header("origin", ORIGIN)
                .header("access-control-request-private-network", "true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(response.headers()["access-control-allow-origin"], ORIGIN);
    assert_eq!(
        response.headers()["access-control-allow-private-network"],
        "true"
    );
}

use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use sha2::{Digest, Sha256};
use std::time::Duration;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
};
type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;
struct Fixture {
    dir: tempfile::TempDir,
    state: AutomationState,
    token: String,
    port: u16,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        let state = AutomationState::new(dir.path(), port).unwrap();
        let router = state.router();
        let token = state.issue_access("owner", ORIGIN).unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            dir,
            state,
            token,
            port,
            server,
        }
    }
    async fn http(
        &self,
        method: &str,
        path: &str,
        token: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let response = self
            .state
            .router()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("host", format!("127.0.0.1:{}", self.port))
                    .header("origin", ORIGIN)
                    .header("authorization", format!("Bearer {token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    async fn socket(&self) -> Socket {
        connect_async(self.state.pairing_info().websocket_url)
            .await
            .unwrap()
            .0
    }
    async fn authenticated(&self) -> Socket {
        let mut socket = self.socket().await;
        socket
            .send(Message::Text(
                json!({"type":"authenticate","token":self.state.pairing_info().extension_token})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        assert_eq!(read(&mut socket).await["type"], "authenticated");
        socket
    }
    async fn ready(&self) -> Socket {
        let mut socket = self.authenticated().await;
        socket
            .send(Message::Text(
                json!({"type":"probe","available":true,"reason":"composer_detected"})
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
        for _ in 0..100 {
            if self
                .http("GET", "/api/automation/status", &self.token, json!(null))
                .await
                .1["available"]
                == true
            {
                return socket;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("probe did not become available")
    }
    async fn phase(&self, phase: &str) -> Value {
        for _ in 0..100 {
            let value = self
                .http(
                    "GET",
                    "/api/automation/runs/run-1",
                    &self.token,
                    json!(null),
                )
                .await
                .1;
            if value["phase"] == phase {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("run did not reach {phase}")
    }
}
async fn read(socket: &mut Socket) -> Value {
    let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    serde_json::from_str(message.to_text().unwrap()).unwrap()
}
fn start_request() -> Value {
    json!({"run_id":"run-1","task_id":"task-1","base_version_id":"version-1","prompt":"Make one fixture image."})
}
async fn generating(socket: &mut Socket) {
    socket
        .send(Message::Text(
            json!({"type":"status","run_id":"run-1","phase":"generating"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
}
fn image() -> (Vec<u8>, String, Value) {
    let bytes = vec![137, 80, 78, 71, 13, 10, 26, 10, 1, 2, 3];
    let sha = format!("{:x}", Sha256::digest(&bytes));
    let message = json!({"type":"artifact","run_id":"run-1","task_id":"task-1","base_version_id":"version-1","mime_type":"image/png","bytes_base64":STANDARD.encode(&bytes),"sha256":sha});
    (bytes, sha, message)
}

#[tokio::test]
async fn websocket_authentication_and_origin_are_required_before_any_work() {
    let f = Fixture::new().await;
    let mut socket = f.socket().await;
    socket
        .send(Message::Text(
            json!({"type":"authenticate","token":"wrong"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        read(&mut socket).await,
        json!({"type":"error","code":"unauthorized"})
    );
    assert_eq!(
        f.http("GET", "/api/automation/status", &f.token, json!(null))
            .await
            .1["connected"],
        false
    );
    let mut req = f
        .state
        .pairing_info()
        .websocket_url
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("origin", "https://evil.test".parse().unwrap());
    assert!(connect_async(req).await.is_err());
}

#[tokio::test]
async fn reconnect_and_process_restart_never_replay_a_submitted_prompt() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, start_request())
            .await
            .0,
        StatusCode::ACCEPTED
    );
    let command = read(&mut socket).await;
    assert_eq!(command["type"], "start");
    assert_eq!(command["run"]["prompt"], "Make one fixture image.");
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, start_request())
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), socket.next())
            .await
            .is_err()
    );
    generating(&mut socket).await;
    f.phase("generating").await;
    let persisted = std::fs::read_to_string(f.dir.path().join("automation.json")).unwrap();
    assert!(!persisted.contains("Make one fixture image"));
    socket.close(None).await.unwrap();
    f.phase("paused").await;
    let mut second = f.authenticated().await;
    assert_eq!(
        read(&mut second).await,
        json!({"type":"status","run_id":"run-1"})
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), second.next())
            .await
            .is_err()
    );
    second.close(None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let reopened = AutomationState::new(f.dir.path(), f.port).unwrap();
    let status = reopened
        .router()
        .oneshot(
            Request::builder()
                .uri("/api/automation/runs/run-1")
                .header("host", format!("127.0.0.1:{}", f.port))
                .header("origin", ORIGIN)
                .header("authorization", format!("Bearer {}", f.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let value: Value =
        serde_json::from_slice(&to_bytes(status.into_body(), 1024 * 1024).await.unwrap()).unwrap();
    assert_eq!(value["phase"], "paused");
    assert_eq!(value["reason"], "server_restarted");
}

#[tokio::test]
async fn original_bytes_wait_for_owner_database_ack_and_survive_restart() {
    let f = Fixture::new().await;
    let other = f.state.issue_access("another-owner", ORIGIN).unwrap();
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    generating(&mut socket).await;
    f.phase("generating").await;
    let (bytes, sha, message) = image();
    socket
        .send(Message::Text(message.to_string().into()))
        .await
        .unwrap();
    f.phase("awaiting_import").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(60), socket.next())
            .await
            .is_err(),
        "staging must not acknowledge import"
    );
    assert_eq!(
        f.http("GET", "/api/automation/runs/run-1", &other, json!(null))
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.http("GET", "/api/automation/runs", &other, json!(null))
            .await
            .1,
        json!([])
    );
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/ack",
            &other,
            json!({"sha256":sha,"image_id":"asset-1"})
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/ack",
            &f.token,
            json!({"sha256":"wrong","image_id":"asset-1"})
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let response = f
        .state
        .router()
        .oneshot(
            Request::builder()
                .uri("/api/automation/runs/run-1/artifact")
                .header("host", format!("127.0.0.1:{}", f.port))
                .header("origin", ORIGIN)
                .header("authorization", format!("Bearer {}", f.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.headers()["x-entwurf-sha256"], sha);
    assert_eq!(
        &to_bytes(response.into_body(), 1024 * 1024).await.unwrap()[..],
        bytes
    );
    socket.close(None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let reopened = AutomationState::new(f.dir.path(), f.port).unwrap();
    assert_eq!(reopened.issue_access("owner", ORIGIN).unwrap(), f.token);
    let restored_image = reopened
        .router()
        .oneshot(
            Request::builder()
                .uri("/api/automation/runs/run-1/artifact")
                .header("host", format!("127.0.0.1:{}", f.port))
                .header("origin", ORIGIN)
                .header("authorization", format!("Bearer {}", f.token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(restored_image.status(), StatusCode::OK);
    assert_eq!(restored_image.headers()["x-entwurf-sha256"], sha);
    assert_eq!(
        &to_bytes(restored_image.into_body(), 1024 * 1024)
            .await
            .unwrap()[..],
        bytes
    );
    // The live server uses the same durable artifact; acknowledgement alone completes it.
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/ack",
            &f.token,
            json!({"sha256":sha,"image_id":"asset-1"})
        )
        .await
        .1["phase"],
        "completed"
    );
    let mut reconnected = f.authenticated().await;
    reconnected
        .send(Message::Text(message.to_string().into()))
        .await
        .unwrap();
    assert_eq!(
        read(&mut reconnected).await,
        json!({"type":"artifact_accepted","run_id":"run-1"})
    );
    reconnected.close(None).await.unwrap();
}

#[tokio::test]
async fn mismatched_artifact_or_second_active_run_cannot_replace_first_run() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    let mut second = start_request();
    second["run_id"] = json!("run-2");
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, second.clone())
            .await
            .0,
        StatusCode::CONFLICT
    );
    generating(&mut socket).await;
    f.phase("generating").await;
    let (_, _, mut artifact) = image();
    artifact["task_id"] = json!("wrong-task");
    socket
        .send(Message::Text(artifact.to_string().into()))
        .await
        .unwrap();
    assert_eq!(
        read(&mut socket).await["code"],
        "artifact_identity_mismatch"
    );
    artifact["task_id"] = json!("task-1");
    artifact["sha256"] = json!("wrong");
    socket
        .send(Message::Text(artifact.to_string().into()))
        .await
        .unwrap();
    assert_eq!(read(&mut socket).await["code"], "artifact_hash_mismatch");
    assert_eq!(
        f.http(
            "GET",
            "/api/automation/runs/run-1/artifact",
            &f.token,
            json!(null)
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/cancel",
            &f.token,
            json!({})
        )
        .await
        .1["phase"],
        "cancelled"
    );
    assert_eq!(read(&mut socket).await["type"], "cancel");
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, second)
            .await
            .0,
        StatusCode::ACCEPTED
    );
    assert_eq!(read(&mut socket).await["type"], "start");
    socket.close(None).await.unwrap();
}

#[tokio::test]
async fn failed_durable_ack_never_publishes_completed_or_notifies_extension() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    generating(&mut socket).await;
    f.phase("generating").await;
    let (_, sha, message) = image();
    socket
        .send(Message::Text(message.to_string().into()))
        .await
        .unwrap();
    f.phase("awaiting_import").await;
    std::fs::remove_file(f.dir.path().join("automation.json")).unwrap();
    std::fs::create_dir(f.dir.path().join("automation.json")).unwrap();
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/ack",
            &f.token,
            json!({"sha256":sha,"image_id":"asset-1"})
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        f.http("GET", "/api/automation/runs/run-1", &f.token, json!(null))
            .await
            .1["phase"],
        "awaiting_import"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), socket.next())
            .await
            .is_err()
    );
    socket.close(None).await.unwrap();
}

#[tokio::test]
async fn missing_origin_only_derives_the_verified_local_host_capability() {
    let dir = tempfile::tempdir().unwrap();
    let state = AutomationState::new(dir.path(), 48123).unwrap();
    let local = state
        .issue_access("local", "http://127.0.0.1:48123")
        .unwrap();
    let pages = state.issue_access("pages", ORIGIN).unwrap();
    for (token, site, want) in [
        (&local, None, StatusCode::OK),
        (&pages, None, StatusCode::UNAUTHORIZED),
        (&local, Some("cross-site"), StatusCode::UNAUTHORIZED),
    ] {
        let mut builder = Request::builder()
            .uri("/api/automation/status")
            .header("host", "127.0.0.1:48123")
            .header("authorization", format!("Bearer {token}"));
        if let Some(site) = site {
            builder = builder.header("sec-fetch-site", site);
        }
        let response = state
            .router()
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), want);
    }
}

#[tokio::test]
async fn cancellation_while_disconnected_reconciles_without_replaying_start() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    assert_eq!(read(&mut socket).await["type"], "start");
    generating(&mut socket).await;
    f.phase("generating").await;
    socket.close(None).await.unwrap();
    f.phase("paused").await;
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/cancel",
            &f.token,
            json!({})
        )
        .await
        .1["phase"],
        "cancelled"
    );
    let mut reconnected = f.authenticated().await;
    // The real extension restores its unfinished local record on authentication.
    generating(&mut reconnected).await;
    assert_eq!(
        read(&mut reconnected).await,
        json!({"type":"cancel","run_id":"run-1"})
    );
    reconnected
        .send(Message::Text(
            json!({"type":"status","run_id":"run-1","phase":"cancelled"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(60), reconnected.next())
            .await
            .is_err(),
        "terminal confirmation must not loop cancel messages"
    );
    reconnected.close(None).await.unwrap();
}

#[tokio::test]
async fn durable_completion_reconciles_lost_ack_without_affecting_next_run() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    generating(&mut socket).await;
    f.phase("generating").await;
    let (_, sha, artifact) = image();
    socket
        .send(Message::Text(artifact.to_string().into()))
        .await
        .unwrap();
    f.phase("awaiting_import").await;
    socket.close(None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/ack",
            &f.token,
            json!({"sha256":sha,"image_id":"image-1"})
        )
        .await
        .1["phase"],
        "completed"
    );
    let mut reconnected = f.authenticated().await;
    let pending = json!({"type":"status","run_id":"run-1","phase":"awaiting_import"});
    reconnected
        .send(Message::Text(pending.to_string().into()))
        .await
        .unwrap();
    assert_eq!(
        read(&mut reconnected).await,
        json!({"type":"artifact_accepted","run_id":"run-1"})
    );
    reconnected
        .send(Message::Text(
            json!({"type":"status","run_id":"run-1","phase":"completed"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(60), reconnected.next())
            .await
            .is_err(),
        "ACK confirmation must not loop"
    );
    reconnected
        .send(Message::Text(
            json!({"type":"probe","available":true,"reason":"composer_detected"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    let mut next = start_request();
    next["run_id"] = json!("run-2");
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, next)
            .await
            .0,
        StatusCode::ACCEPTED
    );
    assert_eq!(read(&mut reconnected).await["run"]["run_id"], "run-2");
    reconnected
        .send(Message::Text(pending.to_string().into()))
        .await
        .unwrap();
    assert_eq!(
        read(&mut reconnected).await,
        json!({"type":"artifact_accepted","run_id":"run-1"})
    );
    assert_eq!(
        f.http("GET", "/api/automation/runs/run-2", &f.token, json!(null))
            .await
            .1["phase"],
        "queued"
    );
    reconnected.close(None).await.unwrap();
}

#[tokio::test]
async fn failed_artifact_storage_disconnects_and_reconnect_retries_bytes_not_prompt() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    assert_eq!(read(&mut socket).await["type"], "start");
    generating(&mut socket).await;
    f.phase("generating").await;
    let (_, sha, artifact) = image();
    let database = f.dir.path().join("automation.json");
    let backup = f.dir.path().join("saved-automation.json");
    std::fs::rename(&database, &backup).unwrap();
    std::fs::create_dir(&database).unwrap();
    socket
        .send(Message::Text(artifact.to_string().into()))
        .await
        .unwrap();
    assert_eq!(
        read(&mut socket).await,
        json!({"type":"error","code":"automation_storage_failed"})
    );
    let closed = tokio::time::timeout(Duration::from_secs(2), socket.next()).await;
    assert!(
        matches!(
            closed,
            Ok(None) | Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_)))
        ),
        "storage failure must close the connection so retained bytes are retried"
    );
    std::fs::remove_dir(&database).unwrap();
    std::fs::rename(&backup, &database).unwrap();
    let mut reconnected = f.authenticated().await;
    assert_eq!(
        read(&mut reconnected).await,
        json!({"type":"status","run_id":"run-1"})
    );
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, start_request())
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), reconnected.next())
            .await
            .is_err(),
        "reconnect and duplicate request must not replay start"
    );
    reconnected
        .send(Message::Text(
            json!({"type":"status","run_id":"run-1","phase":"awaiting_import"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    reconnected
        .send(Message::Text(artifact.to_string().into()))
        .await
        .unwrap();
    f.phase("awaiting_import").await;
    assert!(
        tokio::time::timeout(Duration::from_millis(60), reconnected.next())
            .await
            .is_err(),
        "restaging bytes cannot acknowledge the owner database import"
    );
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/ack",
            &f.token,
            json!({"sha256":sha,"image_id":"image-after-retry"})
        )
        .await
        .1["phase"],
        "completed"
    );
    assert_eq!(
        read(&mut reconnected).await,
        json!({"type":"artifact_accepted","run_id":"run-1"})
    );
    reconnected.close(None).await.unwrap();
}
