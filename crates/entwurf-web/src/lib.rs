//! Shared local HTTP boundary. UI clients never supply native filesystem paths.
pub mod access;
mod store;

use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use entwurf_core::ingest::IngestOptions;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tower_http::services::ServeDir;

pub const MAX_UPLOAD: usize = 32 * 1024 * 1024;

#[derive(Clone)]
pub struct ServerConfig {
    pub data_dir: PathBuf,
    pub frontend_dir: PathBuf,
    pub port: u16,
    pub pdfium_dir: Option<PathBuf>,
    pub tesseract_dir: Option<PathBuf>,
}
impl ServerConfig {
    pub fn new(data_dir: PathBuf, frontend_dir: PathBuf) -> Self {
        Self {
            data_dir,
            frontend_dir,
            port: 48123,
            pdfium_dir: None,
            tesseract_dir: None,
        }
    }
}

pub struct ServerState {
    pub config: ServerConfig,
    session_token: String,
    store: Mutex<store::Store>,
    ocr_languages: tokio::sync::OnceCell<Vec<String>>,
}
impl ServerState {
    pub fn new(config: ServerConfig) -> Result<Arc<Self>, String> {
        let store = store::Store::new(config.data_dir.clone())?;
        Ok(Arc::new(Self {
            config,
            session_token: format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            ),
            store: Mutex::new(store),
            ocr_languages: tokio::sync::OnceCell::new(),
        }))
    }
    fn ingestion(&self) -> IngestOptions {
        IngestOptions {
            pdfium_dir: self.config.pdfium_dir.clone(),
            binary_dir: self.config.tesseract_dir.clone(),
            ..IngestOptions::standard()
        }
    }
    async fn languages(&self) -> Vec<String> {
        self.ocr_languages
            .get_or_init(|| async {
                let binary = self
                    .config
                    .tesseract_dir
                    .as_ref()
                    .map(|dir| {
                        dir.join(if cfg!(windows) {
                            "tesseract.exe"
                        } else {
                            "tesseract"
                        })
                    })
                    .unwrap_or_else(|| "tesseract".into());
                let mut command = tokio::process::Command::new(binary);
                command.arg("--list-langs").kill_on_drop(true);
                #[cfg(windows)]
                command.creation_flags(0x08000000);
                tokio::time::timeout(std::time::Duration::from_secs(3), command.output())
                    .await
                    .ok()
                    .and_then(Result::ok)
                    .filter(|output| output.status.success())
                    .map(|output| {
                        String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .filter(|line| !line.starts_with("List of") && !line.is_empty())
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default()
            })
            .await
            .clone()
    }
    fn diagnostic_capabilities(&self) -> entwurf_core::diagnostics::DiagnosticCapabilities {
        entwurf_core::diagnostics::DiagnosticCapabilities {
            pdfium_available: self.config.pdfium_dir.as_ref().is_some_and(|dir| {
                ["pdfium.dll", "libpdfium.so", "libpdfium.dylib"]
                    .iter()
                    .any(|name| dir.join(name).is_file())
            }),
            tesseract_available: self
                .ocr_languages
                .get()
                .is_some_and(|languages| !languages.is_empty()),
            ..Default::default()
        }
    }
}

pub struct ApiError(pub StatusCode, pub String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
impl From<String> for ApiError {
    fn from(error: String) -> Self {
        Self(StatusCode::BAD_REQUEST, error)
    }
}
impl From<std::io::Error> for ApiError {
    fn from(error: std::io::Error) -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }
}

fn header_text<'a>(headers: &'a HeaderMap, key: &str) -> Option<&'a str> {
    headers.get(key).and_then(|value| value.to_str().ok())
}
fn client(headers: &HeaderMap) -> Result<String, ApiError> {
    Ok(
        access::component(header_text(headers, "x-entwurf-client").ok_or_else(|| {
            ApiError(
                StatusCode::BAD_REQUEST,
                "client identifier is required".into(),
            )
        })?)
        .map_err(ApiError::from)?
        .to_owned(),
    )
}

async fn protect(
    State(state): State<Arc<ServerState>>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let headers = request.headers();
    if !header_text(headers, "host")
        .is_some_and(|host| access::permitted_host(host, state.config.port))
        || !access::permitted_origin(header_text(headers, "origin"), state.config.port)
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "request origin or host is not permitted".into(),
        ));
    }
    if request.uri().path().starts_with("/api/")
        && request.uri().path() != "/api/bootstrap"
        && header_text(headers, "x-entwurf-session") != Some(state.session_token.as_str())
    {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "local session is required".into(),
        ));
    }
    let mut response = next.run(request).await;
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert(header::X_CONTENT_TYPE_OPTIONS, "nosniff".parse().unwrap());
    Ok(response)
}

pub fn router(state: Arc<ServerState>) -> Router {
    let frontend = state.config.frontend_dir.clone();
    Router::new()
        .route("/api/bootstrap", get(bootstrap))
        .route("/api/projects", get(projects))
        .route("/api/command", post(command))
        .route(
            "/api/uploads",
            post(uploads).layer(DefaultBodyLimit::max(MAX_UPLOAD * 64 + 1024 * 1024)),
        )
        .route("/api/artifacts", get(artifact))
        .fallback_service(ServeDir::new(frontend).append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD + 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), protect))
        .with_state(state)
}

async fn bootstrap(State(state): State<Arc<ServerState>>) -> Json<Value> {
    let token = state.session_token.clone();
    let languages = state.languages().await;
    Json(
        json!({"version":entwurf_core::VERSION,"platform":std::env::consts::OS,"runtime":"local-http","storage":"sqlite","session_token":token,
        "tesseract_available":!languages.is_empty(),"ocr_languages":languages,"automation_enabled":false,"bridge_status":"not_connected","bridge_detail":"请连接已登录的 ChatGPT 会话。"}),
    )
}

async fn projects(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    client(&headers)?;
    let result = tokio::task::spawn_blocking(move || {
        state
            .store
            .lock()
            .map_err(|_| "workspace lock failed".to_string())?
            .projects()
    })
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    Ok(Json(result))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandRequest {
    command: String,
    #[serde(default)]
    args: Value,
}

async fn command(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Json(request): Json<CommandRequest>,
) -> Result<Json<Value>, ApiError> {
    let client = client(&headers)?;
    if request.command == "export_diagnostics" {
        state.languages().await;
    }
    let capabilities = state.diagnostic_capabilities();
    let result = tokio::task::spawn_blocking(move || {
        state
            .store
            .lock()
            .map_err(|_| "workspace lock failed".to_string())?
            .command(
                &client,
                &request.command,
                request.args,
                &state.ingestion(),
                capabilities,
            )
    })
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    Ok(Json(result))
}

async fn uploads(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<Value>, ApiError> {
    let client = client(&headers)?;
    let mut files = Vec::new();
    let result: Result<(), ApiError> = async {
        while let Some(mut field) = multipart
            .next_field()
            .await
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?
        {
            if field.name() != Some("files") || files.len() >= 64 {
                return Err(ApiError(
                    StatusCode::BAD_REQUEST,
                    "expected up to 64 files".into(),
                ));
            }
            let name = field
                .file_name()
                .ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "filename is required".into()))?
                .to_owned();
            access::upload_name(&name)?;
            let mut bytes = Vec::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e.to_string()))?
            {
                if bytes.len().saturating_add(chunk.len()) > MAX_UPLOAD {
                    return Err(ApiError(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        "file exceeds 32 MiB".into(),
                    ));
                }
                bytes.extend_from_slice(&chunk);
            }
            if bytes.is_empty() {
                return Err(ApiError(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "file must be between 1 byte and 32 MiB".into(),
                ));
            }
            let file = state
                .store
                .lock()
                .map_err(|_| {
                    ApiError(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "workspace lock failed".into(),
                    )
                })?
                .upload(&client, &name, &bytes)?;
            files.push(file);
        }
        if files.is_empty() {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "select at least one file".into(),
            ));
        }
        Ok(())
    }
    .await;
    if let Err(error) = result {
        if let Ok(store) = state.store.lock() {
            store.discard_uploads(&client, &files);
        }
        return Err(error);
    }
    Ok(Json(Value::Array(files)))
}

#[derive(Deserialize)]
struct ArtifactQuery {
    task_id: String,
    version_id: String,
    kind: String,
    #[serde(default)]
    index: usize,
}

async fn artifact(
    State(state): State<Arc<ServerState>>,
    headers: HeaderMap,
    Query(query): Query<ArtifactQuery>,
) -> Result<Response, ApiError> {
    let client = client(&headers)?;
    let kind = query.kind.clone();
    let bytes = tokio::task::spawn_blocking(move || {
        state
            .store
            .lock()
            .map_err(|_| "workspace lock failed".to_string())?
            .artifact(
                &client,
                &query.task_id,
                &query.version_id,
                &query.kind,
                query.index,
            )
    })
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;
    let mime = match kind.as_str() {
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "pdf" => "application/pdf",
        _ => "application/json",
    };
    Ok(([(header::CONTENT_TYPE, mime)], Body::from(bytes)).into_response())
}
