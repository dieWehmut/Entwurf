use reqwest::{Client, StatusCode};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

struct WebProcess(Child);

impl WebProcess {
    fn start(data_dir: &std::path::Path, frontend_dir: &std::path::Path) -> (Self, u16) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_entwurf-web"))
            .args([
                "--data-dir",
                data_dir.to_str().unwrap(),
                "--frontend",
                frontend_dir.to_str().unwrap(),
                "--port",
                "0",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start production WebUI binary");

        let stdout = child.stdout.take().expect("capture startup address");
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout)
                .read_line(&mut line)
                .map(|bytes| (bytes, line));
            let _ = ready_tx.send(result);
        });

        let process = Self(child);
        let (bytes, line) = ready_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("WebUI process startup address within 15 seconds")
            .expect("read WebUI process startup address");
        assert_ne!(
            bytes, 0,
            "WebUI process exited before announcing its address"
        );
        let port = line
            .trim()
            .strip_prefix("Entwurf local WebUI: http://127.0.0.1:")
            .expect("production process prints the loopback URL")
            .parse()
            .expect("production process prints its bound port");

        (process, port)
    }

    fn stop(&mut self) -> ExitStatus {
        if self
            .0
            .try_wait()
            .expect("check WebUI child status")
            .is_none()
        {
            self.0.kill().expect("stop WebUI process");
        }
        self.0.wait().expect("wait for WebUI process exit")
    }
}

impl Drop for WebProcess {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

async fn wait_for_bootstrap(client: &Client, port: u16, client_id: &str) -> Value {
    let url = format!("http://127.0.0.1:{port}/api/bootstrap");
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            if let Ok(response) = client
                .get(&url)
                .header("X-Entwurf-Client", client_id)
                .send()
                .await
            {
                if response.status() == StatusCode::OK {
                    return response.json().await.expect("decode bootstrap response");
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("WebUI process did not accept bootstrap requests on port {port}"))
}

async fn command(
    client: &Client,
    port: u16,
    client_id: &str,
    session: &str,
    command: &str,
    args: Value,
) -> (StatusCode, Value) {
    let response = client
        .post(format!("http://127.0.0.1:{port}/api/command"))
        .header("X-Entwurf-Client", client_id)
        .header("X-Entwurf-Session", session)
        .json(&json!({ "command": command, "args": args }))
        .send()
        .await
        .expect("send command to production WebUI process");
    let status = response.status();
    let body = response.json().await.expect("decode command response");
    (status, body)
}

#[tokio::test]
async fn production_server_process_restart_reopens_saved_sqlite_project() {
    let temp = tempfile::tempdir().unwrap();
    let data_dir = temp.path().join("data");
    let frontend_dir = temp.path().join("frontend");
    std::fs::create_dir_all(&frontend_dir).unwrap();

    let client = Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let client_id = "process-restart-smoke";
    let (mut first, first_port) = WebProcess::start(&data_dir, &frontend_dir);
    let first_bootstrap = wait_for_bootstrap(&client, first_port, client_id).await;
    assert_eq!(first_bootstrap["storage"], "sqlite");
    let first_session = first_bootstrap["session_token"]
        .as_str()
        .expect("first process session token");

    let (status, created) = command(
        &client,
        first_port,
        client_id,
        first_session,
        "create_project",
        json!({ "root": "/projects/process-restart", "name": "Process restart smoke" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let (status, created_task) = command(
        &client,
        first_port,
        client_id,
        first_session,
        "create_task",
        json!({ "title": "Persist across process restart" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created_task}");
    let task_id = created_task["project"]["tasks"][0]["id"]
        .as_str()
        .expect("created task id")
        .to_owned();
    assert!(data_dir
        .join("projects/process-restart/project.sqlite3")
        .is_file());

    let stopped = first.stop();
    assert!(
        !stopped.success(),
        "first WebUI process exited before the test stopped it"
    );
    let (mut second, second_port) = WebProcess::start(&data_dir, &frontend_dir);
    let second_bootstrap = wait_for_bootstrap(&client, second_port, client_id).await;
    let second_session = second_bootstrap["session_token"]
        .as_str()
        .expect("restarted process session token");
    assert_ne!(first_session, second_session);

    let (status, _) = command(
        &client,
        second_port,
        client_id,
        first_session,
        "workspace_snapshot",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, reopened) = command(
        &client,
        second_port,
        client_id,
        second_session,
        "load_existing_project",
        json!({ "root": "/projects/process-restart" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{reopened}");
    assert_eq!(reopened["project"]["tasks"][0]["id"], task_id);
    assert_eq!(
        reopened["project"]["tasks"][0]["title"],
        "Persist across process restart"
    );

    let (status, snapshot) = command(
        &client,
        second_port,
        client_id,
        second_session,
        "workspace_snapshot",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{snapshot}");
    assert_eq!(snapshot["project"]["tasks"][0]["id"], task_id);
    second.stop();
}
