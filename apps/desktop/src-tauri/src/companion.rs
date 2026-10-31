//! The installed desktop also hosts the local WebUI and narrow browser companion.
use std::{sync::Arc, time::Duration};
use tauri::Manager;

const ADDRESS: &str = "http://127.0.0.1:48123";

#[derive(Default)]
pub struct CompanionState(tokio::sync::Mutex<()>);

pub async fn access(app: &tauri::AppHandle, origin: &str) -> Result<serde_json::Value, String> {
    let state = app.state::<CompanionState>();
    let _guard = state.0.lock().await;
    match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 48123)).await {
        Ok(listener) => {
            let mut config = entwurf_web::ServerConfig::new(
                app.path()
                    .app_data_dir()
                    .map_err(|e| e.to_string())?
                    .join("local-web"),
                std::path::PathBuf::new(),
            );
            config.pdfium_dir = Some(
                app.path()
                    .resource_dir()
                    .map_err(|e| e.to_string())?
                    .join("resources/pdfium"),
            );
            config.tesseract_dir = super::tesseract_dir();
            let server = entwurf_web::ServerState::new(config)?;
            let resolver = app.asset_resolver();
            let assets: entwurf_web::AssetProvider = Arc::new(move |path| {
                let path = if path == "/" { "/index.html" } else { path };
                resolver
                    .get(path.to_owned())
                    .map(|asset| (asset.bytes, asset.mime_type))
            });
            let router = entwurf_web::router_with_assets(server, assets);
            tauri::async_runtime::spawn(async move {
                // The process lifetime owns this loopback listener.
                let _ = axum::serve(listener, router).await;
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
        Err(error) => return Err(format!("无法启动本机伴随服务：{error}")),
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(6))
        .build()
        .map_err(|e| e.to_string())?;
    let bootstrap = client
        .get(format!("{ADDRESS}/api/bootstrap"))
        .send()
        .await
        .map_err(|_| "本机伴随服务无法连接，请重新打开 Entwurf。".to_string())?
        .error_for_status()
        .map_err(|_| "端口 48123 未返回有效的 Entwurf 服务。".to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;
    if bootstrap["runtime"] != "local-http" || bootstrap["storage"] != "sqlite" {
        return Err("端口 48123 已被其他服务使用。".into());
    }
    let token = bootstrap["session_token"]
        .as_str()
        .filter(|s| s.len() == 64)
        .ok_or("本机伴随服务缺少有效会话。")?;
    let response = client
        .post(format!("{ADDRESS}/api/automation/access"))
        .header("X-Entwurf-Session", token)
        .header("X-Entwurf-Client", "desktop")
        .json(&serde_json::json!({"origin":origin}))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status();
    let value = response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(value["error"]
            .as_str()
            .unwrap_or("伴随服务配对失败。")
            .into());
    }
    Ok(value)
}

pub fn native_origin(window: &tauri::WebviewWindow) -> Result<String, String> {
    let url = window.url().map_err(|e| e.to_string())?;
    let origin = format!(
        "{}://{}{}",
        url.scheme(),
        url.host_str().unwrap_or_default(),
        url.port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default()
    );
    if entwurf_web::native_origin(&origin) {
        Ok(origin)
    } else {
        Err("此窗口不能连接本机伴随服务。".into())
    }
}
