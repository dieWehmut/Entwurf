//! Own the native UI adapter process; credentials cross only its inherited stdin.
use serde_json::{json, Value};
use std::{
    io::Write,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    time::{Duration, Instant},
};

pub struct NativeHost {
    executable: Option<PathBuf>,
    directory: PathBuf,
    child: Option<Child>,
    input: Option<ChildStdin>,
}

impl NativeHost {
    pub fn new(executable: Option<PathBuf>, directory: PathBuf) -> Self {
        Self {
            executable,
            directory,
            child: None,
            input: None,
        }
    }
    pub fn available(&self) -> bool {
        cfg!(windows) && self.executable.as_ref().is_some_and(|p| p.is_file())
    }
    pub fn view(&self, provider: String) -> Value {
        json!({"provider":provider,"native_available":self.available()})
    }
    pub fn start(&mut self, info: entwurf_automation::PairingInfo) -> Result<(), String> {
        if !self.available() {
            return Err("native_adapter_unavailable".into());
        }
        if let Some(child) = self.child.as_mut() {
            if child
                .try_wait()
                .map_err(|_| "native_adapter_process_failed")?
                .is_none()
            {
                return Ok(());
            }
        }
        self.input.take();
        self.child.take();
        std::fs::create_dir_all(&self.directory).map_err(|_| "native_adapter_storage_failed")?;
        let directory =
            std::fs::canonicalize(&self.directory).map_err(|_| "native_adapter_storage_failed")?;
        let mut command = Command::new(self.executable.as_ref().unwrap());
        command
            .arg("--automation-stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().map_err(|_| "native_adapter_start_failed")?;
        let result = (|| -> Result<ChildStdin, String> {
            let mut input = child
                .stdin
                .take()
                .ok_or("native_adapter_bootstrap_failed")?;
            let bytes = serde_json::to_vec(
                &json!({"websocket_url":info.websocket_url,"token":info.extension_token,
                "state_directory":directory,"parent_pid":std::process::id()}),
            )
            .map_err(|_| "native_adapter_bootstrap_failed")?;
            input
                .write_all(&(bytes.len() as u32).to_le_bytes())
                .and_then(|_| input.write_all(&bytes))
                .and_then(|_| input.flush())
                .map_err(|_| "native_adapter_bootstrap_failed")?;
            Ok(input)
        })();
        match result {
            Ok(input) => {
                self.input = Some(input);
                self.child = Some(child);
                Ok(())
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                Err(error)
            }
        }
    }
    pub fn stop(&mut self) {
        // EOF is the adapter's cancellation signal; wait briefly before terminating only our child.
        self.input.take();
        if let Some(mut child) = self.child.take() {
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if !matches!(child.try_wait(), Ok(None)) {
                    break;
                }
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
    }
}
impl Drop for NativeHost {
    fn drop(&mut self) {
        self.stop();
    }
}
