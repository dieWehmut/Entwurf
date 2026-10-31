use entwurf_web::{router, ServerConfig, ServerState};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = ServerConfig::new(
        PathBuf::from("entwurf-data"),
        PathBuf::from("apps/desktop/dist"),
    );
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--data-dir" => {
                config.data_dir = args.next().ok_or("--data-dir needs a directory")?.into()
            }
            "--frontend" => {
                config.frontend_dir = args.next().ok_or("--frontend needs a directory")?.into()
            }
            "--port" => config.port = args.next().ok_or("--port needs a number")?.parse()?,
            "--pdfium-dir" => {
                config.pdfium_dir =
                    Some(args.next().ok_or("--pdfium-dir needs a directory")?.into())
            }
            "--tesseract-dir" => {
                config.tesseract_dir = Some(
                    args.next()
                        .ok_or("--tesseract-dir needs a directory")?
                        .into(),
                )
            }
            "--native-adapter" => {
                config.native_adapter = Some(
                    args.next()
                        .ok_or("--native-adapter needs the packaged Entwurf.Bridge.exe path")?
                        .into(),
                );
            }
            "--help" | "-h" => {
                println!("entwurf-web [--data-dir DIR] [--frontend DIST] [--port 48123] [--pdfium-dir DIR] [--tesseract-dir DIR] [--native-adapter EXE]");
                return Ok(());
            }
            _ => return Err(format!("unknown argument: {argument}").into()),
        }
    }
    let listener =
        tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, config.port)).await?;
    config.port = listener.local_addr()?.port();
    let state = ServerState::new(config.clone()).map_err(std::io::Error::other)?;
    println!("Entwurf local WebUI: http://127.0.0.1:{}", config.port);
    axum::serve(listener, router(state.clone()))
        .with_graceful_shutdown(async move {
            let _ = tokio::signal::ctrl_c().await;
            state.shutdown().await;
        })
        .await?;
    Ok(())
}
