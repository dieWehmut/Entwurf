//! Read-only capability probe. Each probe owns one authenticated sidecar session.
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct ProbeReport {
    pub status: &'static str,
    pub detail: &'static str,
    pub automation_enabled: bool,
    pub capabilities: Option<serde_json::Value>,
}
impl ProbeReport {
    fn unavailable(status: &'static str) -> Self {
        Self {
            status,
            detail: "Read-only ChatGPT discovery is unavailable. Use manual handoff.",
            automation_enabled: false,
            capabilities: None,
        }
    }
}

pub async fn probe(executable: PathBuf) -> ProbeReport {
    #[cfg(windows)]
    {
        if !executable.is_file() {
            return ProbeReport::unavailable("missing");
        }
        // UI reload/refresh cannot accumulate processes or concurrent COM probes.
        static PROBE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let Ok(_guard) = PROBE.try_lock() else {
            return ProbeReport::unavailable("busy");
        };
        match native::run(&executable).await {
            Ok(capabilities) => ProbeReport {
                status: "ready_manual",
                detail: "Read-only discovery completed. Automatic ChatGPT operation is unavailable; use manual handoff.",
                // This parent implements no mutation API, even if a future sidecar claims open gates.
                automation_enabled: false,
                capabilities: Some(capabilities),
            },
            Err(code) => ProbeReport::unavailable(code),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = executable;
        ProbeReport::unavailable("unsupported_platform")
    }
}

#[cfg(windows)]
mod native {
    use serde_json::{json, Value};
    use std::{path::Path, process::Stdio, time::Duration};
    use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
        net::windows::named_pipe::{ClientOptions, NamedPipeClient},
        process::{Child, Command},
        time::{sleep, timeout},
    };

    pub(super) const MAX_FRAME: usize = 1_048_576;
    type Result<T> = std::result::Result<T, &'static str>;

    pub(super) fn command(executable: &Path) -> Command {
        let mut command = Command::new(executable);
        command
            .arg("--bootstrap-stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .kill_on_drop(true);
        command
    }
    pub(super) async fn run(executable: &Path) -> Result<Value> {
        let mut child = command(executable).spawn().map_err(|_| "launch_failed")?;
        session(&mut child, Duration::from_secs(15)).await
    }
    pub(super) async fn session(child: &mut Child, budget: Duration) -> Result<Value> {
        let outcome = timeout(budget, negotiate(child))
            .await
            .map_err(|_| "timed_out")
            .and_then(|r| r);
        // Closing bootstrap and RPC handles cancels the sidecar on every path.
        child.stdin.take();
        let cleanup = finish(child).await;
        // Keep the original safe error code on failure; never return peer stderr.
        match outcome {
            Ok(report) => cleanup.map(|_| report),
            Err(code) => Err(code),
        }
    }
    pub(super) async fn finish(child: &mut Child) -> Result<()> {
        match timeout(Duration::from_secs(2), child.wait()).await {
            Ok(Ok(status)) if status.success() => Ok(()),
            Ok(_) => Err("sidecar_stopped"),
            Err(_) => {
                timeout(Duration::from_secs(2), child.kill())
                    .await
                    .map_err(|_| "cleanup_failed")?
                    .map_err(|_| "cleanup_failed")?;
                Ok(())
            }
        }
    }
    pub(super) fn secrets() -> Result<(String, String)> {
        let mut bytes = [0u8; 48];
        getrandom::fill(&mut bytes).map_err(|_| "random_unavailable")?;
        let hex = |slice: &[u8]| slice.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let result = (
            format!("entwurf-bridge-{}", hex(&bytes[..16])),
            hex(&bytes[16..]),
        );
        bytes.fill(0);
        Ok(result)
    }
    pub(super) async fn negotiate(child: &mut Child) -> Result<Value> {
        let (pipe_name, token) = secrets()?;
        let mut stdin = child.stdin.take().ok_or("bootstrap_failed")?;
        write_frame(
            &mut stdin,
            &json!({"pipe_name":pipe_name,"token":token,"artifact_directories":[]}),
        )
        .await?;
        drop(stdin); // A single bootstrap frame; never an interactive stdin protocol.
        let mut pipe = connect(&pipe_name, child).await?;
        let attached = call(&mut pipe, 1, "attach", json!({"token":token})).await?;
        if attached.get("attached") != Some(&Value::Bool(true)) {
            return Err("authentication_failed");
        }
        let report = call(&mut pipe, 2, "detect_app", json!({})).await?;
        validate_capabilities(report)
    }
    pub(super) async fn connect(name: &str, child: &mut Child) -> Result<NamedPipeClient> {
        loop {
            // SECURITY_IDENTIFICATION | SECURITY_SQOS_PRESENT prevents the server
            // from impersonating the desktop parent; the server enforces CurrentUserOnly.
            match ClientOptions::new()
                .security_qos_flags(0x00110000)
                .open(format!(r"\\.\pipe\{name}"))
            {
                Ok(pipe) => return Ok(pipe),
                Err(error) if matches!(error.raw_os_error(), Some(2 | 231)) => {}
                Err(_) => return Err("connect_failed"),
            }
            if child.try_wait().map_err(|_| "launch_failed")?.is_some() {
                return Err("sidecar_stopped");
            }
            sleep(Duration::from_millis(25)).await;
        }
    }
    pub(super) async fn write_frame<W: AsyncWrite + Unpin>(
        stream: &mut W,
        value: &Value,
    ) -> Result<()> {
        let mut bytes = serde_json::to_vec(value).map_err(|_| "protocol_error")?;
        if bytes.is_empty() || bytes.len() > MAX_FRAME {
            return Err("protocol_error");
        }
        let result = async {
            stream
                .write_all(&(bytes.len() as u32).to_le_bytes())
                .await
                .map_err(|_| "transport_error")?;
            stream
                .write_all(&bytes)
                .await
                .map_err(|_| "transport_error")?;
            stream.flush().await.map_err(|_| "transport_error")
        }
        .await;
        bytes.fill(0);
        result
    }
    pub(super) async fn read_frame<R: AsyncRead + Unpin>(stream: &mut R) -> Result<Value> {
        let mut header = [0; 4];
        stream
            .read_exact(&mut header)
            .await
            .map_err(|_| "transport_error")?;
        let length = u32::from_le_bytes(header) as usize;
        if length == 0 || length > MAX_FRAME {
            return Err("protocol_error");
        }
        let mut bytes = vec![0; length];
        stream
            .read_exact(&mut bytes)
            .await
            .map_err(|_| "transport_error")?;
        serde_json::from_slice(&bytes).map_err(|_| "protocol_error")
    }
    pub(super) fn response(value: Value, expected_id: u64) -> Result<Value> {
        if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
            || value.get("id").and_then(Value::as_u64) != Some(expected_id)
            || value.get("error").is_some()
            || !value.get("result").is_some_and(Value::is_object)
        {
            return Err("protocol_error");
        }
        Ok(value["result"].clone())
    }
    pub(super) async fn call(
        pipe: &mut NamedPipeClient,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value> {
        write_frame(
            pipe,
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .await?;
        response(read_frame(pipe).await?, id)
    }
    pub(super) fn validate_capabilities(value: Value) -> Result<Value> {
        for key in [
            "app_found",
            "web_view_accessibility",
            "foreground",
            "automation_enabled",
        ] {
            if !value.get(key).is_some_and(Value::is_boolean) {
                return Err("protocol_error");
            }
        }
        let Some(gates) = value.get("gates").and_then(Value::as_object) else {
            return Err("protocol_error");
        };
        let mut enabled = true;
        for name in [
            "discover_window",
            "reliable_input",
            "reliable_completion",
            "reliable_capture",
        ] {
            let Some(passed) = gates
                .get(name)
                .and_then(|g| g.get("passed"))
                .and_then(Value::as_bool)
            else {
                return Err("protocol_error");
            };
            enabled &= passed;
        }
        if value["automation_enabled"] != enabled {
            return Err("protocol_error");
        }
        Ok(value)
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[tokio::test]
    async fn missing_binary_falls_back_without_enabling_automation() {
        let report = probe(PathBuf::from(r"C:\entwurf-nonexistent\Entwurf.Bridge.exe")).await;
        assert_eq!(report.status, "missing");
        assert!(!report.automation_enabled);
        assert!(report.capabilities.is_none());
    }
    #[tokio::test]
    #[ignore = "requires prepared self-contained Windows sidecar; set ENTWURF_BRIDGE_TEST_EXE"]
    async fn actual_self_contained_sidecar_reports_capabilities() {
        let path = PathBuf::from(
            std::env::var_os("ENTWURF_BRIDGE_TEST_EXE").expect("test executable path"),
        );
        let report = probe(path).await;
        assert_eq!(report.status, "ready_manual", "{report:?}");
        assert!(!report.automation_enabled);
        let capabilities = report.capabilities.unwrap();
        assert_eq!(capabilities["automation_enabled"], false);
        for key in ["reliable_input", "reliable_completion", "reliable_capture"] {
            assert_eq!(capabilities["gates"][key]["passed"], false);
        }
        eprintln!("READ_ONLY_PARENT_DISCOVERY={capabilities}");
    }
    #[test]
    fn wrong_ids_and_error_responses_are_rejected_without_returning_peer_text() {
        use serde_json::json;
        for value in [
            json!({"jsonrpc":"2.0","id":3,"result":{"attached":true}}),
            json!({"jsonrpc":"2.0","id":"2","result":{"attached":true}}),
            json!({"jsonrpc":"2.0","id":2,"error":{"message":"private peer text"}}),
            json!({"jsonrpc":"2.0","id":2,"result":{},"error":null}),
        ] {
            assert_eq!(native::response(value, 2), Err("protocol_error"));
        }
    }
    #[tokio::test]
    async fn zero_oversized_truncated_and_non_json_frames_are_rejected() {
        for bytes in [
            0u32.to_le_bytes().to_vec(),
            1_048_577u32.to_le_bytes().to_vec(),
            vec![3, 0, 0, 0, b'{'],
            vec![1, 0, 0, 0, 0xff],
        ] {
            assert!(native::read_frame(&mut bytes.as_slice()).await.is_err());
        }
    }
    #[tokio::test]
    async fn partial_frame_read_is_cancellable_by_absolute_deadline() {
        use tokio::io::AsyncWriteExt;
        let (mut writer, mut reader) = tokio::io::duplex(32);
        writer.write_all(&[3, 0, 0, 0, b'{']).await.unwrap();
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(25),
            native::read_frame(&mut reader)
        )
        .await
        .is_err());
    }
    #[test]
    fn incomplete_or_inconsistent_gate_claims_are_rejected() {
        use serde_json::json;
        assert!(native::validate_capabilities(json!({"automation_enabled":true})).is_err());
        let mut report = json!({"app_found":false,"web_view_accessibility":false,"foreground":false,"automation_enabled":true,
            "gates":{"discover_window":{"passed":false},"reliable_input":{"passed":false},"reliable_completion":{"passed":false},"reliable_capture":{"passed":false}}});
        assert!(native::validate_capabilities(report.clone()).is_err());
        report["automation_enabled"] = json!(false);
        report["gates"]
            .as_object_mut()
            .unwrap()
            .remove("reliable_input");
        assert!(native::validate_capabilities(report).is_err());
    }
    #[tokio::test]
    #[ignore = "requires prepared self-contained Windows sidecar; set ENTWURF_BRIDGE_TEST_EXE"]
    async fn actual_parent_handshake_keeps_secrets_off_stdio_and_reaps_child() {
        use serde_json::json;
        use std::{process::Stdio, time::Duration};
        use tokio::io::AsyncReadExt;
        let path = PathBuf::from(
            std::env::var_os("ENTWURF_BRIDGE_TEST_EXE").expect("test executable path"),
        );
        let mut command = native::command(&path);
        assert_eq!(
            command.as_std().get_args().collect::<Vec<_>>(),
            vec!["--bootstrap-stdin"]
        );
        assert_eq!(command.as_std().get_envs().count(), 0);
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let (name, token) = native::secrets().unwrap();
        let exchange = async {
            let mut stdin = child.stdin.take().unwrap();
            native::write_frame(
                &mut stdin,
                &json!({"pipe_name":name,"token":token,"artifact_directories":[]}),
            )
            .await
            .unwrap();
            drop(stdin);
            let mut pipe = native::connect(&name, &mut child).await.unwrap();
            assert_eq!(
                native::call(&mut pipe, 1, "attach", json!({"token":token}))
                    .await
                    .unwrap()["attached"],
                true
            );
            let report = native::call(&mut pipe, 2, "detect_app", json!({}))
                .await
                .unwrap();
            assert!(!report.to_string().contains(&token));
            assert_eq!(report["automation_enabled"], false);
            assert!(native::call(&mut pipe, 3, "attach", json!({"token":token}))
                .await
                .is_err());
        };
        tokio::time::timeout(Duration::from_secs(15), exchange)
            .await
            .unwrap();
        native::finish(&mut child).await.unwrap();
        assert!(child.try_wait().unwrap().is_some());
        let mut stdout = String::new();
        let mut stderr = String::new();
        child
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .await
            .unwrap();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .await
            .unwrap();
        assert!(stdout.is_empty());
        assert!(stderr.is_empty());
    }
    #[tokio::test]
    #[ignore = "requires prepared self-contained Windows sidecar; set ENTWURF_BRIDGE_TEST_EXE"]
    async fn timed_out_parent_session_reaps_actual_child() {
        let path = PathBuf::from(
            std::env::var_os("ENTWURF_BRIDGE_TEST_EXE").expect("test executable path"),
        );
        let mut child = native::command(&path).spawn().unwrap();
        let result = native::session(&mut child, std::time::Duration::ZERO).await;
        assert_eq!(result, Err("timed_out"));
        assert!(child.try_wait().unwrap().is_some());
    }
}
