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
        Self::with_database(None).await
    }
    async fn with_database(database: Option<Value>) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let dir = tempfile::tempdir().unwrap();
        if let Some(database) = database {
            std::fs::write(dir.path().join("automation.json"), database.to_string()).unwrap();
        }
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

// Exercise the public HTTP boundary over a real loopback socket, alongside the
// real WebSocket peer. No application UI or external account is involved.
async fn network_resume(f: &Fixture, capability: &str, body: Value) -> (u16, Value) {
    let port = f.port;
    let capability = capability.to_owned();
    tokio::task::spawn_blocking(move || {
        use std::io::{Read, Write};
        let body = body.to_string();
        let mut socket = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(socket, "POST /api/automation/runs/run-1/resume-pre-submit HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nOrigin: {ORIGIN}\r\nAuthorization: Bearer {capability}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        let mut response = String::new();
        socket.read_to_string(&mut response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, serde_json::from_str(body).unwrap_or(Value::Null))
    }).await.unwrap()
}
fn resume_request(id: &str) -> Value {
    json!({"request_id":id,"task_id":"task-1","base_version_id":"version-1","prompt":"Make one fixture image."})
}
async fn native_ready(f: &Fixture) -> Socket {
    f.state.set_provider("windows").unwrap();
    let mut socket = f.socket().await;
    socket
        .send(Message::Text(
            json!({"type":"authenticate","token":f.state.native_pairing_info().extension_token})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(read(&mut socket).await["type"], "authenticated");
    socket
        .send(Message::Text(
            json!({"type":"probe","available":true,"reason":"ready"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    for _ in 0..100 {
        if f.state.status().unwrap()["available"] == true {
            return socket;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("native peer did not become ready");
}
async fn paused_native(f: &Fixture, socket: &mut Socket, retryable: bool) {
    socket.send(Message::Text(json!({"type":"status","run_id":"run-1","phase":"paused","detail":"automation_unavailable","pre_submit_retryable":retryable}).to_string().into())).await.unwrap();
    for _ in 0..100 {
        let view = f.phase("paused").await;
        if view["pre_submit_retryable"] == retryable {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("native eligibility was not reflected");
}

#[tokio::test]
async fn explicit_resume_uses_same_identity_once_and_survives_restart_without_replay() {
    let f = Fixture::new().await;
    let mut socket = native_ready(&f).await;
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, start_request())
            .await
            .0,
        StatusCode::ACCEPTED
    );
    assert_eq!(read(&mut socket).await["type"], "start");
    paused_native(&f, &mut socket, true).await;
    let id = uuid::Uuid::new_v4().to_string();
    let request = resume_request(&id);
    let (code, view) = network_resume(&f, &f.token, request.clone()).await;
    assert_eq!(code, 202);
    assert_eq!(view["run_id"], "run-1");
    assert_eq!(view["resume_request_id"], id);
    assert_eq!(view["pre_submit_retryable"], false);
    let command = read(&mut socket).await;
    assert_eq!(command["type"], "resume_pre_submit");
    assert_eq!(command["request_id"], id);
    assert_eq!(command["run"]["prompt"], start_request()["prompt"]);
    assert_eq!(network_resume(&f, &f.token, request.clone()).await.0, 200);
    assert!(
        tokio::time::timeout(Duration::from_millis(80), socket.next())
            .await
            .is_err()
    );
    let database = std::fs::read_to_string(f.dir.path().join("automation.json")).unwrap();
    assert!(!database.contains("Make one fixture image"));
    assert!(database.contains(&id));
    let reopened = Fixture::with_database(Some(serde_json::from_str(&database).unwrap())).await;
    assert_eq!(
        network_resume(&reopened, &reopened.token, request).await.0,
        200
    );
    assert_eq!(
        network_resume(
            &reopened,
            &reopened.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        409
    );
}

#[tokio::test]
async fn resume_rejects_unknown_native_evidence_wrong_owner_identity_and_request_uuid() {
    let f = Fixture::new().await;
    let mut socket = native_ready(&f).await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    paused_native(&f, &mut socket, false).await;
    let request = resume_request(&uuid::Uuid::new_v4().to_string());
    assert_eq!(network_resume(&f, &f.token, request.clone()).await.0, 409);
    paused_native(&f, &mut socket, true).await;
    let other = f.state.issue_access("another-owner", ORIGIN).unwrap();
    assert_eq!(network_resume(&f, &other, request.clone()).await.0, 404);
    for field in ["task_id", "base_version_id", "prompt"] {
        let mut changed = request.clone();
        changed[field] = json!("different");
        assert_eq!(network_resume(&f, &f.token, changed).await.0, 409);
    }
    assert_eq!(
        network_resume(&f, &f.token, resume_request("not-a-uuid"))
            .await
            .0,
        400
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(80), socket.next())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn resume_disk_failure_preserves_eligibility_and_does_not_send_or_consume_request() {
    let f = Fixture::new().await;
    let mut socket = native_ready(&f).await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    paused_native(&f, &mut socket, true).await;
    let path = f.dir.path().join("automation.json");
    let before = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let request = resume_request(&uuid::Uuid::new_v4().to_string());
    assert_eq!(network_resume(&f, &f.token, request.clone()).await.0, 500);
    assert_eq!(f.phase("paused").await["pre_submit_retryable"], true);
    assert!(
        tokio::time::timeout(Duration::from_millis(80), socket.next())
            .await
            .is_err()
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::write(&path, before).unwrap();
    assert_eq!(network_resume(&f, &f.token, request).await.0, 202);
    assert_eq!(read(&mut socket).await["type"], "resume_pre_submit");
}

#[tokio::test]
async fn resume_is_native_only_and_missing_or_revoked_evidence_never_authorizes_it() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    socket
        .send(Message::Text(
            json!({"type":"status","run_id":"run-1","phase":"paused","pre_submit_retryable":true})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(f.phase("paused").await["pre_submit_retryable"], false);
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        409
    );
    let f = Fixture::new().await;
    let mut socket = native_ready(&f).await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    paused_native(&f, &mut socket, true).await;
    paused_native(&f, &mut socket, false).await;
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        409
    );
    paused_native(&f, &mut socket, true).await;
    f.http(
        "POST",
        "/api/automation/runs/run-1/cancel",
        &f.token,
        json!({}),
    )
    .await;
    assert_eq!(read(&mut socket).await["type"], "cancel");
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        409
    );
}

#[tokio::test]
async fn resume_ignores_stale_status_and_rejection_then_allows_a_fresh_authoritative_manual_attempt(
) {
    let f = Fixture::new().await;
    let mut socket = native_ready(&f).await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    paused_native(&f, &mut socket, true).await;
    let id = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        network_resume(&f, &f.token, resume_request(&id)).await.0,
        202
    );
    read(&mut socket).await;
    socket
        .send(Message::Text(
            json!({"type":"status","run_id":"run-1","phase":"paused","pre_submit_retryable":true})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    socket.send(Message::Text(json!({"type":"resume_rejected","run_id":"run-1","request_id":uuid::Uuid::new_v4().to_string(),"reason":"resume_not_allowed"}).to_string().into())).await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;
    assert_eq!(f.phase("queued").await["pre_submit_retryable"], false);
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        409
    );
    socket.send(Message::Text(json!({"type":"resume_rejected","run_id":"run-1","request_id":id,"reason":"resume_not_allowed"}).to_string().into())).await.unwrap();
    assert_eq!(f.phase("paused").await["pre_submit_retryable"], false);
    paused_native(&f, &mut socket, true).await;
    assert_eq!(
        network_resume(&f, &f.token, resume_request(&id)).await.0,
        200
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), socket.next())
            .await
            .is_err()
    );
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        202
    );
    assert_eq!(read(&mut socket).await["type"], "resume_pre_submit");
}

#[tokio::test]
async fn resume_limit_does_not_evict_old_receipts_or_bypass_other_active_runs() {
    let f = Fixture::new().await;
    let mut socket = native_ready(&f).await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    paused_native(&f, &mut socket, true).await;
    let mut database: Value =
        serde_json::from_slice(&std::fs::read(f.dir.path().join("automation.json")).unwrap())
            .unwrap();
    let ids: Vec<String> = (0..64).map(|_| uuid::Uuid::new_v4().to_string()).collect();
    database["runs"]["run-1"]["resume_requests"] = json!(ids);
    let f = Fixture::with_database(Some(database.clone())).await;
    let mut socket = native_ready(&f).await;
    assert_eq!(read(&mut socket).await["type"], "status");
    paused_native(&f, &mut socket, true).await;
    assert_eq!(
        network_resume(&f, &f.token, resume_request(&ids[0]))
            .await
            .0,
        200
    );
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .1["error"],
        "resume_limit_reached"
    );
    database["runs"]["run-1"]["resume_requests"] = json!([]);
    let mut other = database["runs"]["run-1"].clone();
    other["run_id"] = json!("another-run");
    database["runs"]["another-run"] = other;
    let f = Fixture::with_database(Some(database)).await;
    let mut socket = native_ready(&f).await;
    read(&mut socket).await;
    read(&mut socket).await;
    paused_native(&f, &mut socket, true).await;
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .1["error"],
        "run_active"
    );
}

#[tokio::test]
async fn resume_requires_fresh_evidence_after_disconnect_and_clears_missing_fields() {
    let f = Fixture::new().await;
    let mut socket = native_ready(&f).await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    read(&mut socket).await;
    paused_native(&f, &mut socket, true).await;
    socket
        .send(Message::Text(
            json!({"type":"status","run_id":"run-1","phase":"paused"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    for _ in 0..100 {
        if f.phase("paused").await["pre_submit_retryable"] == false {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        409
    );
    paused_native(&f, &mut socket, true).await;
    socket.close(None).await.unwrap();
    for _ in 0..100 {
        if f.state.status().unwrap()["connected"] == false {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(f.phase("paused").await["pre_submit_retryable"], false);
    let mut socket = native_ready(&f).await;
    assert_eq!(read(&mut socket).await["type"], "status");
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        409
    );
    paused_native(&f, &mut socket, true).await;
    assert_eq!(
        network_resume(
            &f,
            &f.token,
            resume_request(&uuid::Uuid::new_v4().to_string())
        )
        .await
        .0,
        202
    );
    assert_eq!(read(&mut socket).await["type"], "resume_pre_submit");
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

#[tokio::test]
async fn legacy_native_selection_migrates_once_to_locked_session_browser_mode() {
    let f = Fixture::with_database(Some(json!({
        "schema_version":1,"extension_token":"legacy-browser-token","provider":"windows","access":[],"runs":{}
    })))
    .await;
    assert_eq!(f.state.status().unwrap()["provider"], "browser");
    let disk: Value =
        serde_json::from_slice(&std::fs::read(f.dir.path().join("automation.json")).unwrap())
            .unwrap();
    assert_eq!(disk["schema_version"], 2);
    assert_eq!(disk["provider"], "browser");
    let native = disk["native_token"]
        .as_str()
        .expect("migration must persist native credential");
    assert_eq!(native.len(), 64);
    assert_ne!(native, "legacy-browser-token");
    let reopened = AutomationState::new(f.dir.path(), f.port).unwrap();
    assert_eq!(
        reopened.pairing_info().extension_token,
        "legacy-browser-token"
    );
    let second: Value =
        serde_json::from_slice(&std::fs::read(f.dir.path().join("automation.json")).unwrap())
            .unwrap();
    assert_eq!(second["native_token"], native);
    reopened.set_provider("windows").unwrap();
    let selected = AutomationState::new(f.dir.path(), f.port).unwrap();
    assert_eq!(selected.provider().unwrap(), "windows");
    assert_eq!(selected.native_pairing_info().extension_token, native);
    assert_eq!(
        selected.pairing_info().extension_token,
        "legacy-browser-token"
    );
}

#[tokio::test]
async fn transport_keepalive_preserves_readiness_and_run_state_without_commands() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    let path = f.dir.path().join("automation.json");
    let before = std::fs::read(&path).unwrap();
    let status = f.state.status().unwrap();
    socket
        .send(Message::Text(
            json!({"type":"keepalive"}).to_string().into(),
        ))
        .await
        .unwrap();
    // Ping/Pong on the same socket proves the preceding text was processed,
    // without relying on an observation sleep or another adapter message.
    socket.send(Message::Ping(vec![7].into())).await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(response, Message::Pong(vec![7].into()));
    assert_eq!(f.state.status().unwrap(), status);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    socket.close(None).await.unwrap();
}

#[tokio::test]
async fn legacy_migration_keeps_unfinished_native_runs_and_retained_images_with_their_adapter() {
    let f = Fixture::new().await;
    f.state.set_provider("windows").unwrap();
    let native = f.state.native_pairing_info().extension_token;
    let mut socket = authenticate_token(&f, &native, true).await;
    socket
        .send(Message::Text(
            json!({"type":"probe","available":true,"reason":"ready"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    socket.send(Message::Ping(vec![1].into())).await.unwrap();
    assert_eq!(
        socket.next().await.unwrap().unwrap(),
        Message::Pong(vec![1].into())
    );
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, start_request())
            .await
            .0,
        StatusCode::ACCEPTED
    );
    assert_eq!(read(&mut socket).await["type"], "start");
    generating(&mut socket).await;
    f.phase("generating").await;
    let (bytes, sha, artifact) = image();
    socket
        .send(Message::Text(artifact.to_string().into()))
        .await
        .unwrap();
    f.phase("awaiting_import").await;
    let path = f.dir.path().join("automation.json");
    let original: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for phase in ["queued", "generating", "paused", "awaiting_import"] {
        let mut legacy = original.clone();
        legacy["schema_version"] = json!(1);
        legacy["runs"]["run-1"]["phase"] = json!(phase);
        let migrated_fixture = Fixture::with_database(Some(legacy)).await;
        std::fs::write(
            migrated_fixture.dir.path().join("images/run-1.image"),
            &bytes,
        )
        .unwrap();
        let restored = &migrated_fixture.state;
        assert_eq!(restored.provider().unwrap(), "windows", "phase: {phase}");
        assert_eq!(restored.native_pairing_info().extension_token, native);
        assert_eq!(restored.issue_access("owner", ORIGIN).unwrap(), f.token);
        let migrated: Value = serde_json::from_slice(
            &std::fs::read(migrated_fixture.dir.path().join("automation.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(migrated["schema_version"], 2);
        for field in [
            "run_id",
            "task_id",
            "base_version_id",
            "prompt_sha256",
            "sha256",
            "mime_type",
            "image_id",
            "owner",
        ] {
            assert_eq!(
                migrated["runs"]["run-1"][field],
                original["runs"]["run-1"][field]
            );
        }
        assert_eq!(
            restored.set_provider("browser").unwrap_err().0,
            "run_active"
        );
        let mut peer = authenticate_token(&migrated_fixture, &native, true).await;
        assert_eq!(
            read(&mut peer).await,
            json!({"type":"status","run_id":"run-1"})
        );
        // Only completed capture is downloadable. Earlier phases still retain
        // identity and recover through their original peer without a new start.
        if phase != "awaiting_import" {
            assert_eq!(
                std::fs::read(migrated_fixture.dir.path().join("images/run-1.image")).unwrap(),
                bytes
            );
            peer.close(None).await.unwrap();
            continue;
        }
        let router = restored.router();
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/automation/runs/run-1/artifact")
                    .header("host", format!("127.0.0.1:{}", migrated_fixture.port))
                    .header("origin", ORIGIN)
                    .header("authorization", format!("Bearer {}", f.token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(response.into_body(), bytes.len())
                .await
                .unwrap()
                .as_ref(),
            bytes
        );
        assert_eq!(migrated["runs"]["run-1"]["sha256"], sha);
        let (status, ack) = migrated_fixture
            .http(
                "POST",
                "/api/automation/runs/run-1/ack",
                &f.token,
                json!({"sha256":sha,"image_id":"restored-image"}),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(ack["phase"], "completed");
        assert_eq!(
            read(&mut peer).await,
            json!({"type":"artifact_accepted","run_id":"run-1"})
        );
        restored.set_provider("browser").unwrap();
        closed_without_start(&mut peer).await;
    }
    socket.close(None).await.unwrap();
}

#[test]
fn unsupported_database_version_is_rejected_without_rewriting_it() {
    let dir = tempfile::tempdir().unwrap();
    let state = AutomationState::new(dir.path(), 48123).unwrap();
    let path = dir.path().join("automation.json");
    let current: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(current["schema_version"], 2);
    for version in [0, 3] {
        let mut invalid = current.clone();
        invalid["schema_version"] = json!(version);
        let bytes = invalid.to_string().into_bytes();
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            matches!(AutomationState::new(dir.path(), 48123), Err(e) if e.0 == "invalid_automation_state")
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
    drop(state);
}

#[tokio::test]
async fn selected_native_provider_rejects_browser_token_before_peer_claim() {
    let f = Fixture::with_database(Some(json!({
        "schema_version":2,"extension_token":"browser-token","native_token":"native-token",
        "provider":"windows","access":[],"runs":{}
    })))
    .await;
    let mut wrong = f.socket().await;
    wrong
        .send(Message::Text(
            json!({"type":"authenticate","token":"browser-token"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(
        read(&mut wrong).await,
        json!({"type":"error","code":"unauthorized"})
    );
    assert_eq!(f.state.status().unwrap()["connected"], false);
    let mut native = f.socket().await;
    native
        .send(Message::Text(
            json!({"type":"authenticate","token":"native-token"})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    assert_eq!(read(&mut native).await["type"], "authenticated");
    assert!(
        tokio::time::timeout(Duration::from_millis(60), native.next())
            .await
            .is_err(),
        "authentication must not send start"
    );
    native.close(None).await.unwrap();
}

async fn authenticate_token(f: &Fixture, token: &str, accepted: bool) -> Socket {
    let mut socket = f.socket().await;
    socket
        .send(Message::Text(
            json!({"type":"authenticate","token":token})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
    let response = read(&mut socket).await;
    if accepted {
        assert_eq!(response, json!({"type":"authenticated"}));
    } else {
        assert_eq!(response, json!({"type":"error","code":"unauthorized"}));
    }
    socket
}

async fn closed_without_start(socket: &mut Socket) {
    let closed = tokio::time::timeout(Duration::from_secs(2), socket.next()).await;
    assert!(
        matches!(
            closed,
            Ok(None) | Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_)))
        ),
        "connection must close without start: {closed:?}"
    );
}

#[tokio::test]
async fn provider_switch_disconnects_old_peer_and_gates_reconnect_with_independent_tokens() {
    let f = Fixture::new().await;
    let browser = f.state.pairing_info().extension_token;
    let native = f.state.native_pairing_info().extension_token;
    assert_ne!(browser, native);
    let mut wrong_native = authenticate_token(&f, &native, false).await;
    closed_without_start(&mut wrong_native).await;
    let mut old = f.ready().await;
    f.state.set_provider("windows").unwrap();
    assert_eq!(f.state.provider().unwrap(), "windows");
    assert_eq!(
        f.state.status().unwrap(),
        json!({"provider":"windows","connected":false,"available":false,"reason":"provider_changed"})
    );
    // New peer may connect before the old socket finishes disconnecting.
    let mut current = authenticate_token(&f, &native, true).await;
    closed_without_start(&mut old).await;
    let mut wrong_browser = authenticate_token(&f, &browser, false).await;
    closed_without_start(&mut wrong_browser).await;
    assert_eq!(
        f.state.status().unwrap()["connected"],
        true,
        "late old-peer disconnect must not clear the new peer"
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), current.next())
            .await
            .is_err()
    );
    let restored = AutomationState::new(f.dir.path(), f.port).unwrap();
    assert_eq!(restored.provider().unwrap(), "windows");
    assert_eq!(restored.native_pairing_info().extension_token, native);
    assert_eq!(restored.pairing_info().extension_token, browser);
    f.state.set_provider("browser").unwrap();
    closed_without_start(&mut current).await;
    let mut wrong_native = authenticate_token(&f, &native, false).await;
    closed_without_start(&mut wrong_native).await;
    let mut browser = f.authenticated().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(60), browser.next())
            .await
            .is_err()
    );
    browser.close(None).await.unwrap();
}

#[tokio::test]
async fn provider_change_rejects_queued_paused_and_pending_import_but_same_provider_recovers() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, start_request())
            .await
            .0,
        StatusCode::ACCEPTED
    );
    for _ in 0..2 {
        assert_eq!(f.state.set_provider("windows").unwrap_err().0, "run_active");
        f.state.set_provider("browser").unwrap();
    }
    assert_eq!(read(&mut socket).await["type"], "start");
    generating(&mut socket).await;
    f.phase("generating").await;
    socket.close(None).await.unwrap();
    f.phase("paused").await;
    assert_eq!(f.state.set_provider("windows").unwrap_err().0, "run_active");
    f.state.set_provider("browser").unwrap();
    let mut socket = f.authenticated().await;
    assert_eq!(
        read(&mut socket).await,
        json!({"type":"status","run_id":"run-1"})
    );
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, start_request())
            .await
            .0,
        StatusCode::OK
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(60), socket.next())
            .await
            .is_err(),
        "same provider recovery must not replay start"
    );
    let (_, sha, artifact) = image();
    socket
        .send(Message::Text(artifact.to_string().into()))
        .await
        .unwrap();
    f.phase("awaiting_import").await;
    assert_eq!(f.state.set_provider("windows").unwrap_err().0, "run_active");
    f.state.set_provider("browser").unwrap();
    assert_eq!(f.state.provider().unwrap(), "browser");
    assert_eq!(
        f.http(
            "POST",
            "/api/automation/runs/run-1/ack",
            &f.token,
            json!({"sha256":sha,"image_id":"retained-image"})
        )
        .await
        .1["phase"],
        "completed"
    );
    assert_eq!(read(&mut socket).await["type"], "artifact_accepted");
    f.state.set_provider("windows").unwrap();
    closed_without_start(&mut socket).await;
}

#[tokio::test]
async fn failed_provider_persistence_preserves_old_connection_and_ready_state() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    let before = f.state.status().unwrap();
    let path = f.dir.path().join("automation.json");
    let backup = f.dir.path().join("saved-automation.json");
    std::fs::rename(&path, &backup).unwrap();
    std::fs::create_dir(&path).unwrap();
    assert_eq!(
        f.state.set_provider("windows").unwrap_err().0,
        "automation_storage_failed"
    );
    assert_eq!(f.state.provider().unwrap(), "browser");
    assert_eq!(f.state.status().unwrap(), before);
    f.state.set_provider("browser").unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(60), socket.next())
            .await
            .is_err(),
        "failed switch must keep the old WebSocket"
    );
    std::fs::remove_dir(&path).unwrap();
    std::fs::rename(&backup, &path).unwrap();
    let persisted: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(persisted["provider"], "browser");
    assert_eq!(
        f.http("POST", "/api/automation/runs", &f.token, start_request())
            .await
            .0,
        StatusCode::ACCEPTED
    );
    assert_eq!(
        read(&mut socket).await["type"],
        "start",
        "old provider must remain usable after failed switch"
    );
    socket.close(None).await.unwrap();
}

#[test]
fn unsupported_provider_never_changes_persistent_state() {
    let dir = tempfile::tempdir().unwrap();
    let state = AutomationState::new(dir.path(), 48123).unwrap();
    let path = dir.path().join("automation.json");
    let before = std::fs::read(&path).unwrap();
    for provider in ["", "native", "Windows", "browser ", "http://remote"] {
        assert_eq!(
            state.set_provider(provider).unwrap_err().0,
            "invalid_provider"
        );
        assert_eq!(state.provider().unwrap(), "browser");
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
    let mut invalid: Value = serde_json::from_slice(&before).unwrap();
    invalid["provider"] = json!("unknown");
    std::fs::write(&path, invalid.to_string()).unwrap();
    assert!(
        matches!(AutomationState::new(dir.path(), 48123), Err(e) if e.0 == "invalid_automation_state")
    );
}

#[tokio::test]
async fn shutdown_closes_peer_without_changing_pending_image_or_submission_history() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    assert_eq!(read(&mut socket).await["type"], "start");
    generating(&mut socket).await;
    f.phase("generating").await;
    let (bytes, _, artifact) = image();
    socket
        .send(Message::Text(artifact.to_string().into()))
        .await
        .unwrap();
    f.phase("awaiting_import").await;
    let database = f.dir.path().join("automation.json");
    let before = std::fs::read(&database).unwrap();
    f.state.shutdown().unwrap();
    f.state.shutdown().unwrap();
    closed_without_start(&mut socket).await;
    assert_eq!(f.state.status().unwrap()["connected"], false);
    assert_eq!(f.state.status().unwrap()["available"], false);
    assert_eq!(f.state.status().unwrap()["reason"], "server_shutting_down");
    assert_eq!(std::fs::read(&database).unwrap(), before);
    assert_eq!(
        std::fs::read(f.dir.path().join("images/run-1.image")).unwrap(),
        bytes
    );
    let duplicate = f
        .http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    assert_eq!(duplicate.0, StatusCode::OK);
    assert_eq!(duplicate.1["phase"], "awaiting_import");
    let mut next = start_request();
    next["run_id"] = json!("run-after-shutdown");
    let rejected = f.http("POST", "/api/automation/runs", &f.token, next).await;
    assert_eq!(rejected.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(rejected.1["error"], "server_shutting_down");
    assert_eq!(std::fs::read(&database).unwrap(), before);
    // Shutdown is process state only; restart retains the pending import and pairing.
    let reopened = AutomationState::new(f.dir.path(), f.port).unwrap();
    assert_eq!(
        reopened.status().unwrap()["reason"],
        "extension_disconnected"
    );
    assert_eq!(
        reopened.pairing_info().extension_token,
        f.state.pairing_info().extension_token
    );
    assert_eq!(std::fs::read(&database).unwrap(), before);
}

#[tokio::test]
async fn shutdown_interrupts_unauthed_socket_and_rejects_delayed_or_new_authentication() {
    let f = Fixture::new().await;
    let mut waiting = f.socket().await;
    let mut delayed = f.socket().await;
    f.state.shutdown().unwrap();
    // No first frame arrives on this socket: stop must interrupt the 10-second auth wait.
    closed_without_start(&mut waiting).await;
    let _ = delayed
        .send(Message::Text(
            json!({"type":"authenticate","token":f.state.pairing_info().extension_token})
                .to_string()
                .into(),
        ))
        .await;
    closed_without_start(&mut delayed).await;
    if let Ok((mut later, _)) = connect_async(f.state.pairing_info().websocket_url).await {
        let _ = later
            .send(Message::Text(
                json!({"type":"authenticate","token":f.state.pairing_info().extension_token})
                    .to_string()
                    .into(),
            ))
            .await;
        closed_without_start(&mut later).await;
    }
    assert_eq!(f.state.status().unwrap()["connected"], false);
}

#[tokio::test]
async fn shutdown_is_bounded_even_when_persistent_storage_is_unwritable() {
    let f = Fixture::new().await;
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    assert_eq!(read(&mut socket).await["type"], "start");
    let database = f.dir.path().join("automation.json");
    let backup = f.dir.path().join("saved-automation.json");
    std::fs::rename(&database, &backup).unwrap();
    std::fs::create_dir(&database).unwrap();
    f.state.shutdown().unwrap();
    closed_without_start(&mut socket).await;
    assert_eq!(
        f.http("GET", "/api/automation/runs/run-1", &f.token, json!(null))
            .await
            .1["phase"],
        "queued"
    );
    std::fs::remove_dir(&database).unwrap();
    std::fs::rename(&backup, &database).unwrap();
}

#[tokio::test]
async fn cancelled_original_remains_private_hash_verified_and_read_only_after_restart() {
    let mut f = Fixture::new().await;
    let other = f.state.issue_access("another-owner", ORIGIN).unwrap();
    let mut socket = f.ready().await;
    f.http("POST", "/api/automation/runs", &f.token, start_request())
        .await;
    assert_eq!(read(&mut socket).await["type"], "start");
    generating(&mut socket).await;
    f.phase("generating").await;
    let (bytes, sha, message) = image();
    socket
        .send(Message::Text(message.to_string().into()))
        .await
        .unwrap();
    f.phase("awaiting_import").await;
    f.http(
        "POST",
        "/api/automation/runs/run-1/cancel",
        &f.token,
        json!({}),
    )
    .await;
    assert_eq!(read(&mut socket).await["type"], "cancel");
    // Late arrival must remain rejected even when its bytes match the retained original.
    socket
        .send(Message::Text(message.to_string().into()))
        .await
        .unwrap();
    assert_eq!(read(&mut socket).await["code"], "artifact_not_expected");
    for restarted in [false, true] {
        if restarted {
            socket.close(None).await.unwrap();
            f.server.abort();
            f.state = AutomationState::new(f.dir.path(), f.port).unwrap();
        }
        for (token, origin, expected) in [
            (f.token.as_str(), ORIGIN, StatusCode::OK),
            (other.as_str(), ORIGIN, StatusCode::NOT_FOUND),
            ("invalid", ORIGIN, StatusCode::UNAUTHORIZED),
            (
                f.token.as_str(),
                "https://evil.test",
                StatusCode::UNAUTHORIZED,
            ),
        ] {
            let response = f
                .state
                .router()
                .oneshot(
                    Request::builder()
                        .uri("/api/automation/runs/run-1/artifact")
                        .header("host", format!("127.0.0.1:{}", f.port))
                        .header("origin", origin)
                        .header("authorization", format!("Bearer {token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::OK {
                assert_eq!(response.headers()["x-entwurf-sha256"], sha);
                assert_eq!(response.headers()["content-type"], "image/png");
                assert_eq!(
                    &to_bytes(response.into_body(), 1024 * 1024).await.unwrap()[..],
                    bytes
                );
            }
        }
        let (_, view) = f
            .http("GET", "/api/automation/runs/run-1", &f.token, json!(null))
            .await;
        assert_eq!(view["phase"], "cancelled");
        assert_eq!(view["task_id"], "task-1");
        assert_eq!(view["base_version_id"], "version-1");
        assert_eq!(view["sha256"], sha);
        assert_eq!(
            f.http(
                "POST",
                "/api/automation/runs/run-1/ack",
                &f.token,
                json!({"sha256":sha,"image_id":"asset-1"})
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        if !restarted {
            assert!(
                tokio::time::timeout(Duration::from_millis(60), socket.next())
                    .await
                    .is_err(),
                "review must not send a provider command or ACK"
            );
        }
    }
    let path = f.dir.path().join("images/run-1.image");
    std::fs::write(&path, b"tampered original").unwrap();
    assert_eq!(
        f.http(
            "GET",
            "/api/automation/runs/run-1/artifact",
            &f.token,
            json!(null)
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
    std::fs::File::create(&path)
        .unwrap()
        .set_len(entwurf_automation::MAX_ARTIFACT_BYTES as u64 + 1)
        .unwrap();
    assert_eq!(
        f.http(
            "GET",
            "/api/automation/runs/run-1/artifact",
            &f.token,
            json!(null)
        )
        .await
        .0,
        StatusCode::INTERNAL_SERVER_ERROR
    );
}
