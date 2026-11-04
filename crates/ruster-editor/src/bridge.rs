//! Explicit opt-in, same-user local bridge. Never owns mutable editor state.
use std::io::{BufReader, Write};
use std::os::unix::{fs::PermissionsExt, net::UnixListener};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use serde_json::{json, Value};

const LIMIT: u64 = 65536;
type Pending = Arc<Mutex<Option<(Value, mpsc::SyncSender<Value>, Instant)>>>;

pub struct Bridge {
    receiver: mpsc::Receiver<Pending>,
    directory: tempfile::TempDir,
    nonce: String,
}

impl Bridge {
    pub fn start(ctx: egui::Context) -> std::io::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("ruster-mcp-")
            .rand_bytes(24)
            .tempdir()?;
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o700))?;
        let path = directory.path().join("editor.sock");
        let listener = UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let nonce = directory
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        eprintln!(
            "Ruster MCP bridge: {} (private same-user capability; opt-in)",
            path.display()
        );
        let lifetime = path.clone();
        std::thread::spawn(move || {
            while lifetime.exists() {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                let mut bytes = Vec::new();
                let deadline = Instant::now() + Duration::from_secs(2);
                let result = (|| {
                    use std::io::Read;
                    let mut reader = BufReader::new((&stream).take(LIMIT + 1));
                    loop {
                        let remaining = deadline.checked_duration_since(Instant::now())?;
                        stream.set_read_timeout(Some(remaining)).ok()?;
                        // Read one byte per call to enforce a total framing deadline.
                        let mut byte = [0u8; 1];
                        if reader.read(&mut byte).ok()? == 0 { return None; }
                        bytes.push(byte[0]);
                        if byte[0] == b'\n' || bytes.len() as u64 > LIMIT { break; }
                    }
                    if bytes.len() as u64 > LIMIT || bytes.last() != Some(&b'\n') { return None; }
                    let value: Value = serde_json::from_slice(&bytes).ok()?;
                    let (reply, result) = mpsc::sync_channel(1);
                    let pending = Arc::new(Mutex::new(Some((value, reply, deadline))));
                    sender.try_send(pending.clone()).ok()?;
                    ctx.request_repaint();
                    match result.recv_timeout(Duration::from_secs(2)) {
                        Ok(value) => Some(value),
                        Err(_) => {
                            // Serialize expiry with dispatch. Holding this lock through
                            // execution means an already committed reply wins over timeout.
                            let mut state = pending.lock().ok()?;
                            if state.take().is_none() { result.try_recv().ok() } else { None }
                        }
                    }
                })().unwrap_or_else(|| json!({"ok":false,"error":{"code":"bridge_unavailable","message":"Invalid, busy or expired bridge request; inspect before retrying."}}));
                let result = if serde_json::to_vec(&result)
                    .map_or(true, |v| v.len() as u64 >= LIMIT)
                {
                    json!({"ok":false,"error":{"code":"response_too_large","message":"Response exceeds bridge limit; use bounded inspection pages."}})
                } else {
                    result
                };
                if let Ok(mut bytes) = serde_json::to_vec(&result) {
                    if (bytes.len() as u64) < LIMIT {
                        bytes.push(b'\n');
                        let _ = stream.write_all(&bytes);
                    }
                }
            }
        });
        Ok(Self {
            receiver,
            directory,
            nonce,
        })
    }

    pub fn dispatch(&mut self, editor: &mut crate::model::Editor) -> bool {
        self.dispatch_command(|nonce, operation, arguments| {
            let token = format!("{}:{}", nonce, editor.project_generation);
            crate::commands::dispatch(editor, &token, operation, arguments)
        })
    }

    pub fn dispatch_command(&mut self, handle: impl FnOnce(&str, &str, Value) -> Value) -> bool {
        // One command per frame; never drain unbounded external work into UI.
        if let Ok(pending) = self.receiver.try_recv() {
            if let Ok(mut state) = pending.lock() {
                if let Some((request, reply, deadline)) = state.take() {
                    if Instant::now() >= deadline {
                        return false;
                    }
                    let started = Instant::now();
                    let response = match request.as_object() {
                        Some(map)
                            if map.len() == 2
                                && map.contains_key("operation")
                                && map.contains_key("arguments") =>
                        {
                            handle(
                                &self.nonce,
                                request["operation"].as_str().unwrap_or(""),
                                request["arguments"].clone(),
                            )
                        }
                        _ => {
                            json!({"ok":false,"error":{"code":"invalid_arguments","message":"Expected operation and arguments only"}})
                        }
                    };
                    let _ = reply.send(response);
                    if started.elapsed() > Duration::from_millis(50) {
                        eprintln!("MCP dispatch exceeded 50ms: {:?}", started.elapsed());
                    }
                }
            }
            return true;
        }
        false
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.directory.path().join("editor.sock"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expired_queued_edit_never_executes_even_when_editor_wins_lock() {
        let (sender, receiver) = mpsc::sync_channel(1);
        let directory = tempfile::tempdir().unwrap();
        let mut bridge = Bridge {
            receiver,
            directory,
            nonce: "test".into(),
        };
        let mut editor = crate::model::Editor::new(
            ruster_core::ProjectSession::new("expiry", ruster_core::Config::default()).unwrap(),
        );
        let (reply, result) = mpsc::sync_channel(1);
        let request = json!({"operation":"edit","arguments":{
            "expected_session_token":format!("test:{}",editor.project_generation),"expected_revision":"0",
            "config":{"initial_stock":"0","production_per_tick":"3","transfer_per_tick":"4"}}});
        sender
            .send(Arc::new(Mutex::new(Some((
                request,
                reply,
                Instant::now() - Duration::from_secs(1),
            )))))
            .unwrap();
        let before = editor.project.scenario().clone();
        assert!(!bridge.dispatch(&mut editor));
        assert_eq!(editor.project.scenario(), &before);
        assert!(result.try_recv().is_err());
    }
}
