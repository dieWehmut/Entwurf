use super::*;
use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Path, Query, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;

pub(super) fn router(state: AutomationState) -> Router {
    Router::new()
        .route("/api/automation/status", get(status))
        .route("/api/automation/runs", get(list).post(start))
        .route("/api/automation/runs/{id}", get(run))
        .route("/api/automation/runs/{id}/cancel", post(cancel))
        .route(
            "/api/automation/runs/{id}/resume-pre-submit",
            post(resume_pre_submit),
        )
        .route("/api/automation/runs/{id}/artifact", get(artifact))
        .route("/api/automation/runs/{id}/ack", post(acknowledge))
        .route("/api/automation/extension", get(upgrade))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), boundary))
        .with_state(state)
}
impl IntoResponse for AutomationError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            "unauthorized" => StatusCode::UNAUTHORIZED,
            "server_shutting_down" => StatusCode::SERVICE_UNAVAILABLE,
            "forbidden" => StatusCode::FORBIDDEN,
            "run_not_found" | "artifact_unavailable" => StatusCode::NOT_FOUND,
            "run_active"
            | "run_conflict"
            | "automation_unavailable"
            | "invalid_acknowledgement"
            | "run_completed" => StatusCode::CONFLICT,
            "resume_not_allowed" | "resume_limit_reached" => StatusCode::CONFLICT,
            "automation_storage_failed" | "artifact_invalid" => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::BAD_REQUEST,
        };
        (status, Json(json!({"error":self.0}))).into_response()
    }
}
#[derive(Clone)]
struct Owner(String);
fn value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}
async fn boundary(
    State(state): State<AutomationState>,
    mut request: Request,
    next: Next,
) -> Response {
    let host =
        value(request.headers(), "host").and_then(|s| s.parse::<axum::http::uri::Authority>().ok());
    if !host.is_some_and(|h| {
        matches!(h.host(), "127.0.0.1" | "localhost" | "[::1]")
            && h.port_u16().unwrap_or(80) == state.0.port
    }) {
        return AutomationError("forbidden").into_response();
    }
    let origin = value(request.headers(), "origin").map(str::to_owned);
    if request.uri().path() == "/api/automation/extension" {
        if origin.as_ref().is_some_and(|s| {
            !s.strip_prefix("chrome-extension://")
                .is_some_and(|id| id.len() == 32 && id.bytes().all(|b| (b'a'..=b'p').contains(&b)))
        }) {
            return AutomationError("forbidden").into_response();
        }
        return next.run(request).await;
    }
    let origin = match origin {
        Some(origin) => origin,
        None if value(request.headers(), "sec-fetch-site") != Some("cross-site") => {
            // Same-origin GET requests omit Origin. Derive only the verified loopback
            // HTTP origin, never the Pages or native WebView origin of another token.
            format!("http://{}", value(request.headers(), "host").unwrap())
        }
        None => return AutomationError("unauthorized").into_response(),
    };
    if request.method() == Method::OPTIONS {
        if !state.origin_known(&origin) {
            return AutomationError("unauthorized").into_response();
        }
        if value(request.headers(), "access-control-request-method")
            .is_some_and(|m| !matches!(m, "GET" | "POST"))
            || value(request.headers(), "access-control-request-headers").is_some_and(|h| {
                h.split(',').any(|s| {
                    !matches!(
                        s.trim().to_ascii_lowercase().as_str(),
                        "content-type" | "authorization"
                    )
                })
            })
        {
            return AutomationError("forbidden").into_response();
        }
        let mut response = StatusCode::NO_CONTENT.into_response();
        cors(&mut response, &origin);
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("Content-Type, Authorization"),
        );
        if value(request.headers(), "access-control-request-private-network") == Some("true") {
            response.headers_mut().insert(
                "access-control-allow-private-network",
                HeaderValue::from_static("true"),
            );
        }
        return response;
    }
    let bearer = value(request.headers(), "authorization")
        .and_then(|s| s.strip_prefix("Bearer "))
        .unwrap_or("");
    let owner = match state.authorized(bearer, &origin) {
        Ok(owner) => owner,
        Err(error) => return error.into_response(),
    };
    request.extensions_mut().insert(Owner(owner));
    let mut response = next.run(request).await;
    cors(&mut response, &origin);
    response
}
fn cors(response: &mut Response, origin: &str) {
    if let Ok(origin) = HeaderValue::from_str(origin) {
        response
            .headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
    response
        .headers_mut()
        .insert(header::VARY, HeaderValue::from_static("Origin"));
    response.headers_mut().insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("X-Entwurf-Sha256, Content-Type"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}
async fn status(State(s): State<AutomationState>) -> Result<Json<Value>> {
    Ok(Json(s.status()?))
}
#[derive(Deserialize)]
struct Filter {
    task_id: Option<String>,
}
async fn list(
    State(s): State<AutomationState>,
    Extension(owner): Extension<Owner>,
    Query(f): Query<Filter>,
) -> Result<Json<Vec<RunView>>> {
    Ok(Json(s.list(&owner.0, f.task_id.as_deref())?))
}
async fn run(
    State(s): State<AutomationState>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
) -> Result<Json<RunView>> {
    Ok(Json(s.get(&owner.0, &id)?))
}
async fn start(
    State(s): State<AutomationState>,
    Extension(owner): Extension<Owner>,
    Json(request): Json<StartRun>,
) -> Result<Response> {
    let (new, view) = s.start(&owner.0, request)?;
    Ok((
        if new {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        },
        Json(view),
    )
        .into_response())
}
async fn cancel(
    State(s): State<AutomationState>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
) -> Result<Json<RunView>> {
    Ok(Json(s.cancel(&owner.0, &id)?))
}
async fn resume_pre_submit(
    State(s): State<AutomationState>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
    Json(request): Json<ResumeRun>,
) -> Result<Response> {
    let (new, view) = s.resume_pre_submit(&owner.0, &id, request)?;
    Ok((
        if new {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        },
        Json(view),
    )
        .into_response())
}
async fn acknowledge(
    State(s): State<AutomationState>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
    Json(ack): Json<Acknowledge>,
) -> Result<Json<RunView>> {
    Ok(Json(s.acknowledge(&owner.0, &id, ack)?))
}
async fn artifact(
    State(s): State<AutomationState>,
    Extension(owner): Extension<Owner>,
    Path(id): Path<String>,
) -> Result<Response> {
    let (view, bytes) = s.artifact(&owner.0, &id)?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, view.mime_type.unwrap())
        .header("x-entwurf-sha256", view.sha256.unwrap())
        .body(Body::from(bytes))
        .unwrap())
}
async fn upgrade(State(state): State<AutomationState>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(45 * 1024 * 1024)
        .max_frame_size(45 * 1024 * 1024)
        .on_upgrade(move |socket| session(state, socket))
}
async fn send(socket: &mut WebSocket, value: Value) -> bool {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .is_ok()
}
async fn session(state: AutomationState, socket: WebSocket) {
    let mut shutdown = state.0.shutdown.subscribe();
    if *shutdown.borrow() {
        return;
    }
    tokio::select! {
        biased;
        _ = shutdown.changed() => {},
        _ = session_active(state, socket) => {},
    }
}
async fn session_active(state: AutomationState, mut socket: WebSocket) {
    let auth = match tokio::time::timeout(Duration::from_secs(10), socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) if text.len() <= 4096 => {
            serde_json::from_str::<Value>(&text).ok()
        }
        _ => None,
    };
    let id = Uuid::new_v4().to_string();
    let (sender, mut receiver) = mpsc::channel::<Value>(16);
    let accepted = (|| -> Result<Vec<String>> {
        let mut i = state.lock()?;
        if *state.0.shutdown.borrow() {
            return Err(AutomationError("server_shutting_down"));
        }
        let selected_token = if i.database.provider == "windows" {
            &i.database.native_token
        } else {
            &i.database.extension_token
        };
        if auth
            .as_ref()
            .and_then(|v| v.get("type"))
            .and_then(Value::as_str)
            != Some("authenticate")
            || !auth
                .as_ref()
                .and_then(|v| v.get("token"))
                .and_then(Value::as_str)
                .is_some_and(|t| equal_token(t, selected_token))
        {
            return Err(AutomationError("unauthorized"));
        }
        if i.peer.is_some() {
            return Err(AutomationError("extension_busy"));
        }
        i.peer = Some(Peer {
            id: id.clone(),
            sender,
            parked_runs: Default::default(),
            retryable_runs: Default::default(),
        });
        i.available = false;
        i.reason = "probing".into();
        Ok(i.database
            .runs
            .values()
            .filter(|r| active(&r.view.phase))
            .map(|r| r.view.run_id.clone())
            .collect())
    })();
    let recover = match accepted {
        Ok(ids) => ids,
        Err(error) => {
            send(&mut socket, json!({"type":"error","code":error.0})).await;
            let _ = socket.close().await;
            return;
        }
    };
    if !send(&mut socket, json!({"type":"authenticated"})).await {
        disconnect(&state, &id);
        return;
    }
    for run_id in recover {
        if !send(&mut socket, json!({"type":"status","run_id":run_id})).await {
            disconnect(&state, &id);
            return;
        }
    }
    let (mut output, mut input) = socket.split();
    loop {
        tokio::select! {
         message = receiver.recv() => match message { Some(value) => { if output.send(Message::Text(value.to_string().into())).await.is_err() { break; } }, None => break },
         incoming = tokio::time::timeout(Duration::from_secs(90), input.next()) => {
          match incoming {
           Ok(Some(Ok(Message::Text(text)))) => {
            let result = serde_json::from_str::<Value>(&text).map_err(|_| AutomationError("invalid_extension_message")).and_then(|value| apply_extension(&state, &id, value));
            if let Err(error) = result {
                if output.send(Message::Text(json!({"type":"error","code":error.0}).to_string().into())).await.is_err() { break; }
                if error.0 == "automation_storage_failed" {
                    // The companion retains captured bytes until owner import ACK.
                    // Its reconnect restore retransmits those bytes, never a prompt.
                    // Keeping this failed session alive would strand awaiting_import
                    // because ordinary ticks report status without resending bytes.
                    let _ = output.send(Message::Close(None)).await;
                    break;
                }
            }
           },
           Ok(Some(Ok(Message::Ping(bytes)))) => { if output.send(Message::Pong(bytes)).await.is_err() { break; } },
           Ok(Some(Ok(Message::Pong(_)))) => {},
           _ => break,
          }
         }
        }
    }
    disconnect(&state, &id);
}
fn disconnect(state: &AutomationState, id: &str) {
    if let Ok(mut i) = state.lock() {
        if i.peer.as_ref().is_some_and(|p| p.id == id) {
            i.peer = None;
            i.available = false;
            i.reason = "extension_disconnected".into();
            let mut database = i.database.clone();
            for r in database.runs.values_mut() {
                r.view.pre_submit_retryable = false;
                r.view.pre_submit_parked = false;
                r.pending_resume = None;
                if active(&r.view.phase) && r.view.phase != "awaiting_import" {
                    r.view.phase = "paused".into();
                    r.view.reason = Some("extension_disconnected".into());
                }
            }
            let _ = state.commit(&mut i, database);
        }
    }
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .ok_or(AutomationError("invalid_extension_message"))
}
fn reason(value: Option<&Value>) -> String {
    value
        .and_then(Value::as_str)
        .filter(|s| {
            s.len() <= 80
                && !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        })
        .unwrap_or("adapter_unavailable")
        .into()
}
fn diagnostic_name(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 80
                && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .map(str::to_owned)
}
fn apply_extension(state: &AutomationState, peer_id: &str, value: Value) -> Result<()> {
    let mut i = state.lock()?;
    let mut database = i.database.clone();
    if !i.peer.as_ref().is_some_and(|p| p.id == peer_id) {
        return Err(AutomationError("unauthorized"));
    }
    match field(&value, "type")? {
        // Browser MV3 workers exchange transport traffic independently of page
        // probes; a slow/occluded tab must not expire the worker's WebSocket.
        // This reports no readiness and cannot change durable run state.
        "keepalive" => return Ok(()),
        "probe" => {
            i.available = value.get("available") == Some(&Value::Bool(true));
            i.reason = reason(value.get("reason"));
            return Ok(());
        }
        "resume_rejected" => {
            let id = field(&value, "run_id")?;
            let request_id = field(&value, "request_id")?;
            let r = database
                .runs
                .get_mut(id)
                .ok_or(AutomationError("run_not_found"))?;
            if !active(&r.view.phase) || r.pending_resume.as_deref() != Some(request_id) {
                return Ok(());
            }
            r.pending_resume = None;
            r.view.phase = "paused".into();
            r.view.pre_submit_retryable = false;
            r.view.pre_submit_parked = false;
            r.view.reason = Some(reason(value.get("reason")));
            return state.commit(&mut i, database);
        }
        "status" => {
            let id = field(&value, "run_id")?;
            let phase = field(&value, "phase")?;
            let r = database
                .runs
                .get_mut(id)
                .ok_or(AutomationError("run_not_found"))?;
            if !active(&r.view.phase) {
                // Reconnection restores the companion's own unfinished record.
                // Reconcile a cancel or durable ACK that could not reach it while
                // disconnected, without replaying a prompt or broadcasting history.
                // A matching terminal status is confirmation, not a new request.
                if active(phase) {
                    let command = if r.view.phase == "completed" {
                        json!({"type":"artifact_accepted","run_id":id})
                    } else {
                        json!({"type":"cancel","run_id":id})
                    };
                    i.peer
                        .as_ref()
                        .unwrap()
                        .sender
                        .try_send(command)
                        .map_err(|_| AutomationError("extension_disconnected"))?;
                }
                return Ok(());
            }
            if r.view.sha256.is_some() {
                return Ok(());
            }
            if let Some(pending) = &r.pending_resume {
                // Late status from the previous attempt cannot release a newly
                // accepted request. Only this request's native receipt can do so.
                if value.get("resume_request_id").and_then(Value::as_str) != Some(pending) {
                    return Ok(());
                }
                r.pending_resume = None;
            }
            let retryable = i.database.provider == "windows"
                && phase == "paused"
                && value.get("pre_submit_retryable") == Some(&Value::Bool(true));
            let parked = i.database.provider == "windows"
                && phase == "paused"
                && value.get("pre_submit_parked") == Some(&Value::Bool(true));
            if r.view.phase == "paused" && phase != "paused" {
                // A later unsafe observation cannot reopen an old durable
                // phase, but it must revoke this peer's earlier idle proof.
                let peer = i.peer.as_mut().unwrap();
                peer.parked_runs.remove(id);
                peer.retryable_runs.remove(id);
                return Ok(());
            }
            let order = [
                "queued",
                "preparing",
                "submitting",
                "generating",
                "downloading",
            ];
            let valid = matches!(phase, "paused" | "failed" | "cancelled")
                || order
                    .iter()
                    .position(|p| *p == phase)
                    .zip(order.iter().position(|p| *p == r.view.phase))
                    .is_some_and(|(next, old)| next >= old);
            if valid {
                r.view.phase = phase.into();
                r.view.reason = value.get("detail").map(|d| reason(Some(d)));
                r.view.pre_submit_retryable = retryable;
                r.view.pre_submit_parked = parked;
                r.view.stage = diagnostic_name(value.get("stage"));
                r.view.exception_type = diagnostic_name(value.get("exception_type"));
                r.view.failure_method = diagnostic_name(value.get("failure_method"));
                r.view.exception_types = value
                    .get("exception_types")
                    .and_then(Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .take(3)
                            .filter_map(|v| diagnostic_name(Some(v)))
                            .collect()
                    });
                // A failed status write cannot keep an older peer observation
                // authorized. Only a committed report from this peer reopens it.
                let peer = i.peer.as_mut().unwrap();
                peer.parked_runs.remove(id);
                peer.retryable_runs.remove(id);
                state.commit(&mut i, database)?;
                let peer = i.peer.as_mut().unwrap();
                if parked {
                    peer.parked_runs.insert(id.into());
                }
                if retryable {
                    peer.retryable_runs.insert(id.into());
                }
            }
            return Ok(());
        }
        "artifact" => {}
        _ => return Err(AutomationError("invalid_extension_message")),
    }
    let id = field(&value, "run_id")?;
    let r = database
        .runs
        .get(id)
        .ok_or(AutomationError("run_not_found"))?;
    if r.view.task_id != field(&value, "task_id")?
        || r.view.base_version_id != field(&value, "base_version_id")?
    {
        return Err(AutomationError("artifact_identity_mismatch"));
    }
    let mime = field(&value, "mime_type")?;
    if !matches!(mime, "image/png" | "image/jpeg" | "image/webp") {
        return Err(AutomationError("artifact_invalid"));
    }
    let encoded = field(&value, "bytes_base64")?;
    if encoded.len() > MAX_ARTIFACT_BYTES.div_ceil(3) * 4 {
        return Err(AutomationError("artifact_too_large"));
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| AutomationError("artifact_invalid"))?;
    if bytes.is_empty() || bytes.len() > MAX_ARTIFACT_BYTES {
        return Err(AutomationError("artifact_too_large"));
    }
    let digest = hash(&bytes);
    if digest != field(&value, "sha256")? {
        return Err(AutomationError("artifact_hash_mismatch"));
    }
    if r.view.sha256.as_ref().is_some_and(|s| s != &digest) {
        return Err(AutomationError("artifact_conflict"));
    }
    if r.view.phase == "completed" {
        if r.view.sha256.as_ref() != Some(&digest) {
            return Err(AutomationError("artifact_conflict"));
        }
        let _ = i
            .peer
            .as_ref()
            .unwrap()
            .sender
            .try_send(json!({"type":"artifact_accepted","run_id":id}));
        return Ok(());
    }
    if matches!(
        r.view.phase.as_str(),
        "cancelled" | "failed" | "queued" | "preparing"
    ) {
        return Err(AutomationError("artifact_not_expected"));
    }
    atomic_write(
        &state.0.directory.join("images").join(format!("{id}.image")),
        &bytes,
    )?;
    let r = database.runs.get_mut(id).unwrap();
    r.view.sha256 = Some(digest);
    r.view.mime_type = Some(mime.into());
    r.view.phase = "awaiting_import".into();
    r.view.pre_submit_retryable = false;
    r.view.pre_submit_parked = false;
    r.pending_resume = None;
    r.view.reason = None;
    state.commit(&mut i, database)
}
