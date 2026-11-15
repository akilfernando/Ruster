//! Restricted companion owned by one explicit ordinary Host connection.
//! Blocking spawn/framing/retirement never runs on the editor thread.
use super::*;
use std::io::{self, Read, Write};
use std::net::Shutdown;
use std::os::unix::{
    fs::PermissionsExt,
    net::{UnixListener, UnixStream},
};
use std::process::{Child, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};

#[derive(Default)]
struct Framing {
    bytes: Vec<u8>,
    started: Option<Instant>,
}
impl Framing {
    fn poll(&mut self, socket: &mut UnixStream) -> io::Result<Option<Value>> {
        socket.set_read_timeout(Some(crate::connection::POLL))?;
        if self
            .started
            .is_some_and(|start| start.elapsed() >= Duration::from_secs(2))
        {
            return Err(io::Error::other("absolute frame deadline"));
        }
        // One complete frame only; no pipelining or bytes belonging to a next request.
        let wanted = if self.bytes.len() < 4 {
            4 - self.bytes.len()
        } else {
            u32::from_be_bytes(self.bytes[..4].try_into().unwrap()) as usize + 4 - self.bytes.len()
        };
        let mut chunk = [0; 4096];
        let size = wanted.min(chunk.len());
        match socket.read(&mut chunk[..size]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "workspace EOF",
                ))
            }
            Ok(n) => {
                self.started.get_or_insert_with(Instant::now);
                self.bytes.extend_from_slice(&chunk[..n]);
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
                ) =>
            {
                return Ok(None)
            }
            Err(e) => return Err(e),
        }
        if self.bytes.len() < 4 {
            return Ok(None);
        }
        let size = u32::from_be_bytes(self.bytes[..4].try_into().unwrap()) as usize;
        if size == 0 || size > 4096 {
            return Err(io::Error::other("workspace frame length"));
        }
        if self.bytes.len() != size + 4 {
            return Ok(None);
        }
        let value = crate::strict_json::parse(&self.bytes[4..]).map_err(io::Error::other)?;
        self.bytes.clear();
        self.started = None;
        Ok(Some(value))
    }
}
fn send(socket: &mut UnixStream, value: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(value)?;
    if body.is_empty() || body.len() > 65536 {
        return Err(io::Error::other("workspace reply length"));
    }
    let bytes = [
        (body.len() as u32).to_be_bytes().as_slice(),
        body.as_slice(),
    ]
    .concat();
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut remaining = bytes.as_slice();
    while !remaining.is_empty() {
        socket.set_write_timeout(Some(
            deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| io::Error::other("absolute write deadline"))?,
        ))?;
        let n = socket.write(remaining)?;
        if n == 0 {
            return Err(io::Error::other("workspace EOF"));
        }
        remaining = &remaining[n..];
    }
    Ok(())
}

pub(super) struct WorkspaceAttachment {
    identity: Value,
    attachment: String,
    origin: Option<String>,
    peer_pid: Option<u32>,
    events: mpsc::Receiver<Value>,
    replies: mpsc::SyncSender<Value>,
    live: Arc<AtomicBool>,
    control: Arc<Mutex<Option<UnixStream>>>,
    browser: Option<crate::bridge::browser::BrowserRequest>,
    capture: Option<String>,
}
struct ChildOwner(Child);
impl Drop for ChildOwner {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
impl WorkspaceAttachment {
    fn identity(editor: &SceneEditor, bridge: &crate::bridge::Bridge) -> Option<Value> {
        let path = editor.session.path()?.canonicalize().ok()?;
        let locator: crate::connection::Locator =
            serde_json::from_slice(&std::fs::read(bridge.locator_path()).ok()?).ok()?;
        Some(
            json!({"binding":locator.binding(),"native_pid":std::process::id(),"native_uid":rustix::process::getuid().as_raw(),"project":path,"scene_id":editor.session.scene().id,"session_token":editor.scene_guard_token(&editor.connection_incarnation)}),
        )
    }
    pub(super) fn start(
        editor: &SceneEditor,
        bridge: &crate::bridge::Bridge,
        ctx: &egui::Context,
    ) -> io::Result<Option<Self>> {
        let Some(identity) = Self::identity(editor, bridge) else {
            return Ok(None);
        };
        let attachment = crate::connection::unpredictable()?;
        let challenge = crate::connection::unpredictable()?;
        let (sender, events) = mpsc::sync_channel(1);
        let (replies, receiver) = mpsc::sync_channel(1);
        let live = Arc::new(AtomicBool::new(true));
        let control = Arc::new(Mutex::new(None));
        let worker_live = live.clone();
        let worker_control = control.clone();
        let worker_identity = identity.clone();
        let worker_attachment = attachment.clone();
        let ctx = ctx.clone();
        let authority = bridge.authority();
        std::thread::Builder::new().name("workspace-attachment".into()).spawn(move || {
            let run = || -> io::Result<()> {
                let root = tempfile::Builder::new().prefix("ruster-workspace-").tempdir()?;
                std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700))?;
                let path = root.path().join("workspace.sock");
                let listener = UnixListener::bind(&path)?;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
                listener.set_nonblocking(true)?;
                let child = Command::new("python3").args(["-I", "-u", "-c", include_str!("workspace_server.py")])
                    .stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::inherit()).spawn()?;
                let mut child = ChildOwner(child);
                let config = json!({"socket":path,"challenge":challenge,"native_pid":std::process::id(),"native_uid":rustix::process::getuid().as_raw(),"binding":worker_identity,"attachment":worker_attachment,"html":include_str!("workspace.html")});
                child.0.stdin.take().ok_or_else(||io::Error::other("child stdin"))?.write_all(format!("{config}\n").as_bytes())?;
                let deadline = Instant::now() + Duration::from_secs(3);
                let mut stream = loop {
                    if !worker_live.load(Ordering::Acquire) || !authority.live() || child.0.try_wait()?.is_some() || Instant::now() >= deadline { return Err(io::Error::other("workspace startup retired")); }
                    match listener.accept() {
                        Ok((stream,_)) => {
                            if crate::connection::peer(&stream)? != (rustix::process::getuid().as_raw(), child.0.id()) { return Err(io::Error::other("workspace peer mismatch")); }
                            break stream;
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(crate::connection::POLL),
                        Err(e) => return Err(e),
                    }
                };
                drop(listener);
                *worker_control.lock().map_err(|_|io::Error::other("control lock"))? = Some(stream.try_clone()?);
                let mut framing = Framing::default();
                let receive = |stream: &mut UnixStream, framing: &mut Framing| -> io::Result<Value> {
                    let deadline = Instant::now()+Duration::from_secs(3);
                    loop {
                        if !worker_live.load(Ordering::Acquire) || !authority.live() || Instant::now() >= deadline { return Err(io::Error::other("workspace handshake retired")); }
                        if let Some(value) = framing.poll(stream)? { return Ok(value); }
                    }
                };
                if receive(&mut stream,&mut framing)? != json!({"challenge":challenge,"binding":worker_identity,"attachment":worker_attachment}) { return Err(io::Error::other("workspace challenge mismatch")); }
                send(&mut stream,&json!({"authenticated":true}))?;
                let ready = receive(&mut stream,&mut framing)?;
                let origin = ready["origin"].as_str().filter(|s| valid_origin(s)).ok_or_else(||io::Error::other("workspace origin"))?;
                if ready["attachment"] != worker_attachment { return Err(io::Error::other("workspace attachment mismatch")); }
                sender.try_send(json!({"ready":origin,"peer_pid":child.0.id()})).map_err(io::Error::other)?;
                ctx.request_repaint();
                let reply = receiver.recv_timeout(Duration::from_secs(3)).map_err(io::Error::other)?;
                if reply != json!({"published":true}) { return Err(io::Error::other("workspace publication refused")); }
                send(&mut stream,&reply)?;
                loop {
                    if !worker_live.load(Ordering::Acquire) || !authority.live() || child.0.try_wait()?.is_some() { break; }
                    let Some(request) = framing.poll(&mut stream)? else { continue; };
                    if request.as_object().is_none_or(|m|m.len()!=4) || !request["op"].is_string() || !request["args"].is_object()
                        || !request["browser"].as_str().is_some_and(|s|s.len()==64 && s.bytes().all(|b|b.is_ascii_hexdigit())) || request["origin"] != origin
                        || serde_json::to_vec(&request)?.len()>4096 { return Err(io::Error::other("workspace request schema")); }
                    sender.try_send(request).map_err(io::Error::other)?;
                    ctx.request_repaint();
                    let deadline = Instant::now()+Duration::from_secs(8);
                    let reply = loop {
                        if !worker_live.load(Ordering::Acquire) || !authority.live() || Instant::now() >= deadline { return Err(io::Error::other("workspace request retired")); }
                        match receiver.recv_timeout(crate::connection::POLL) {
                            Ok(value) => break value,
                            Err(mpsc::RecvTimeoutError::Timeout) => (),
                            Err(e) => return Err(io::Error::other(e)),
                        }
                    };
                    send(&mut stream,&reply)?;
                }
                Ok(())
            };
            if let Err(error) = run() { eprintln!("Workspace attachment retired: {error}"); }
            worker_live.store(false,Ordering::Release);
            if let Ok(mut control) = worker_control.lock() { control.take(); }
            ctx.request_repaint();
        })?;
        Ok(Some(Self {
            identity,
            attachment,
            origin: None,
            peer_pid: None,
            events,
            replies,
            live,
            control,
            browser: None,
            capture: None,
        }))
    }
    pub(super) fn stop(&self) {
        self.live.store(false, Ordering::Release);
        if let Ok(control) = self.control.lock() {
            if let Some(stream) = &*control {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
        let _ = self.replies.try_send(json!({"retired":true}));
    }
    fn descriptor(&self) -> Value {
        if !self.live.load(Ordering::Acquire) {
            return failure("workspace_unavailable");
        }
        self.origin.as_ref().map_or_else(||failure("workspace_starting"), |origin|json!({"ok":true,"result":{"url":format!("{origin}/"),"binding":self.identity["binding"],"scene_id":self.identity["scene_id"],"session_token":self.identity["session_token"],"attachment":self.attachment,"native_pid":self.identity["native_pid"],"peer_pid":self.peer_pid,"status":"native_browser_approval_required"}}))
    }
}
impl Drop for WorkspaceAttachment {
    fn drop(&mut self) {
        self.stop();
    }
}
fn failure(code: &str) -> Value {
    json!({"ok":false,"error":{"code":code,"message":code}})
}
fn valid_origin(origin: &str) -> bool {
    origin
        .strip_prefix("http://127.0.0.1:")
        .is_some_and(|port| {
            !port.is_empty()
                && port.bytes().all(|b| b.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|n| n != 0)
        })
}

impl Host {
    pub(super) fn workspace_descriptor(&self) -> Value {
        self.workspace.as_ref().map_or_else(
            || failure("workspace_unavailable"),
            WorkspaceAttachment::descriptor,
        )
    }
    pub(super) fn stop_workspace(&mut self) {
        if let Some(workspace) = self.workspace.take() {
            workspace.stop();
            if let Some(id) = &workspace.capture {
                self.editor.captures.cancel_owned(id);
            }
        }
    }
    pub(super) fn service_workspace(&mut self, ctx: &egui::Context) {
        let Some(workspace) = &mut self.workspace else {
            return;
        };
        let Some(bridge) = &mut self.bridge else {
            self.stop_workspace();
            return;
        };
        if !workspace.live.load(Ordering::Acquire)
            || !bridge.authority().live()
            || WorkspaceAttachment::identity(&self.editor, bridge).as_ref()
                != Some(&workspace.identity)
        {
            bridge.deny_browser();
            self.stop_workspace();
            return;
        }
        if workspace
            .browser
            .as_ref()
            .is_some_and(|r| !bridge.browser_authorized(r))
        {
            if let Some(id) = workspace.capture.take() {
                self.editor.captures.cancel_owned(&id);
            }
        }
        let Ok(request) = workspace.events.try_recv() else {
            return;
        };
        if let Some(origin) = request["ready"].as_str() {
            workspace.origin = Some(origin.into());
            workspace.peer_pid = request["peer_pid"].as_u64().map(|n| n as u32);
            if workspace
                .replies
                .try_send(json!({"published":true}))
                .is_err()
            {
                workspace.stop();
            }
            return;
        }
        let browser = crate::bridge::browser::BrowserRequest {
            id: request["browser"].as_str().unwrap_or("").into(),
            context: format!(
                "{} · {} · {}",
                self.editor.document_label(),
                self.editor.session.scene().id,
                workspace.origin.as_deref().unwrap_or("")
            ),
            binding: Some(crate::bridge::browser::BrowserBinding {
                native_incarnation: workspace.identity["binding"]["incarnation"]
                    .as_str()
                    .unwrap()
                    .into(),
                native_generation: workspace.identity["binding"]["generation"]
                    .as_str()
                    .unwrap()
                    .into(),
                native_connection: workspace.identity["binding"]["connection"]
                    .as_str()
                    .unwrap()
                    .into(),
                native_pid: std::process::id(),
                native_uid: rustix::process::getuid().as_raw(),
                project: workspace.identity["project"].as_str().unwrap().into(),
                scene_id: workspace.identity["scene_id"].as_str().unwrap().into(),
                session_token: workspace.identity["session_token"].as_str().unwrap().into(),
                attachment: workspace.attachment.clone(),
                peer_pid: workspace.peer_pid.unwrap_or(0),
                origin: workspace.origin.clone().unwrap_or_default(),
            }),
        };
        let op = request["op"].as_str().unwrap_or("");
        let response = if op == "browser_pair" {
            let state = bridge.request_browser(browser.clone());
            json!({"state":state,"code":&browser.id[..browser.id.len().min(12)]})
        } else if !bridge.browser_authorized(&browser) {
            json!({"live":false,"response":failure("browser_denied")})
        } else if op == "status" {
            let observed = self.editor.command(
                &self.editor.connection_incarnation.clone(),
                "viewport_inspect",
                json!({}),
            );
            let v = &observed["result"];
            json!({"live":true,"session_token":v["session_token"],"revision":v["revision"],"view_revision":v["view_revision"],"draft_reserved":false})
        } else if matches!(
            op,
            "viewport_camera" | "viewport_fit" | "viewport_pick" | "scene_select"
        ) {
            if let Some(lease) = bridge.authority().authorize_publication() {
                let result = workspace_control(
                    &mut self.editor,
                    op,
                    &request["args"],
                    workspace.capture.as_deref(),
                );
                lease.complete(result.clone());
                result
            } else {
                failure("revoked")
            }
        } else if !matches!(
            op,
            "scene_inspect"
                | "viewport_inspect"
                | "viewport_capture_start"
                | "viewport_capture_inspect"
        ) {
            failure("forbidden_operation")
        } else if op == "viewport_capture_inspect"
            && request["args"]["capture_id"].as_str() != workspace.capture.as_deref()
        {
            failure("capture_not_owned")
        } else if let Some(lease) = bridge.authority().authorize_publication() {
            let result = self.editor.command(
                &self.editor.connection_incarnation.clone(),
                op,
                request["args"].clone(),
            );
            if op == "viewport_capture_start" && result["ok"] == true {
                workspace.capture = result["result"]["capture_id"].as_str().map(str::to_owned);
                workspace.browser = Some(browser);
            }
            lease.complete(result.clone());
            result
        } else {
            failure("revoked")
        };
        if workspace.replies.try_send(response).is_err() {
            workspace.stop();
        }
        ctx.request_repaint();
    }
}

fn workspace_control(
    editor: &mut SceneEditor,
    op: &str,
    args: &Value,
    owned_capture: Option<&str>,
) -> Value {
    let nonce = editor.connection_incarnation.clone();
    if matches!(op, "viewport_camera" | "scene_select") {
        return editor.command(&nonce, op, args.clone());
    }
    let fields: &[&str] = if op == "viewport_fit" {
        &[
            "expected_session_token",
            "expected_revision",
            "expected_view_revision",
        ]
    } else {
        &[
            "expected_session_token",
            "expected_revision",
            "expected_view_revision",
            "capture_id",
            "image_width",
            "image_height",
            "x",
            "y",
        ]
    };
    if args.as_object().is_none_or(|map| {
        map.len() != fields.len() || fields.iter().any(|key| !map.contains_key(*key))
    }) {
        return failure("invalid_arguments");
    }
    // Validate all native session/revision/view and pending-draft guards before
    // resolving coordinates or replacing selection. No synthetic native input.
    let guarded = json!({"expected_session_token":args["expected_session_token"],"expected_revision":args["expected_revision"],"expected_view_revision":args["expected_view_revision"],"object_ids":editor.manipulation.ids});
    let admitted = editor.command(&nonce, "scene_select", guarded.clone());
    if admitted["ok"] != true {
        return admitted;
    }
    if editor.runtime.visible {
        return failure("authored_view_required");
    }
    let Some(rect) = editor.captures.rect else {
        return failure("viewport_unavailable");
    };
    let Some(camera) = editor.navigation.camera else {
        return failure("viewport_unavailable");
    };
    if op == "viewport_fit" {
        let Some(state) = ruster_render::frame_scene(
            editor.session.scene(),
            &editor.resolved,
            camera,
            rect.aspect_ratio(),
        ) else {
            return failure("fit_out_of_range");
        };
        let mut next = guarded;
        next.as_object_mut().unwrap().remove("object_ids");
        next["camera"] = json!({"target":state.target,"yaw":state.yaw,"pitch":state.pitch,"distance":state.distance,"orthographic":state.orthographic});
        return editor.command(&nonce, "viewport_camera", next);
    }
    if args["capture_id"].as_str() != owned_capture || owned_capture.is_none() {
        return failure("stale_capture");
    }
    let captured = editor.command(
        &nonce,
        "viewport_capture_inspect",
        json!({"capture_id":args["capture_id"]}),
    );
    if captured["ok"] != true {
        return captured;
    }
    let capture = &captured["result"];
    if capture["status"] != "ready"
        || args["image_width"] != capture["width"]
        || args["image_height"] != capture["height"]
    {
        return failure("stale_capture");
    }
    let coordinate = |key: &str, size: &str| {
        args[key]
            .as_f64()
            .filter(|v| v.is_finite() && *v >= 0. && *v < capture[size].as_f64().unwrap_or(0.))
    };
    let (Some(x), Some(y)) = (coordinate("x", "width"), coordinate("y", "height")) else {
        return failure("invalid_coordinates");
    };
    let ppp = capture["pixels_per_point"].as_f64().unwrap_or(0.);
    if ppp <= 0. {
        return failure("stale_capture");
    }
    let source = &capture["source_pixel_rect"];
    let position = egui::pos2(
        ((source["x"].as_f64().unwrap_or(0.)
            + x * capture["source_width"].as_f64().unwrap_or(0.)
                / capture["width"].as_f64().unwrap_or(1.))
            / ppp) as f32,
        ((source["y"].as_f64().unwrap_or(0.)
            + y * capture["source_height"].as_f64().unwrap_or(0.)
                / capture["height"].as_f64().unwrap_or(1.))
            / ppp) as f32,
    );
    let picked = ruster_render::pick_at(
        editor.session.scene(),
        &editor.resolved,
        camera,
        rect,
        position,
    );
    let mut next = guarded;
    next["object_ids"] = json!(picked.into_iter().collect::<Vec<_>>());
    editor.command(&nonce, "scene_select", next)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restricted_length_frames_reject_partial_expiry_duplicates_and_unsafe_origins() {
        for origin in ["http://127.0.0.1:1", "http://127.0.0.1:65535"] {
            assert!(valid_origin(origin));
        }
        for origin in [
            "http://localhost:80",
            "http://127.0.0.1:0",
            "http://127.0.0.1:65536",
            "http://127.0.0.1:80/",
            "http://127.0.0.1:80?secret=x",
            "https://127.0.0.1:80",
        ] {
            assert!(!valid_origin(origin));
        }
        let (mut writer, mut reader) = UnixStream::pair().unwrap();
        let mut frame = Framing::default();
        assert!(frame.poll(&mut reader).unwrap().is_none());
        assert!(
            frame.started.is_none(),
            "healthy idle has no partial-frame deadline"
        );
        let body = br#"{"op":"status","args":{}}"#;
        let bytes = [(body.len() as u32).to_be_bytes().as_slice(), body].concat();
        for byte in &bytes {
            writer.write_all(&[*byte]).unwrap();
            let result = frame.poll(&mut reader).unwrap();
            if frame.bytes.is_empty() {
                assert_eq!(result, Some(json!({"op":"status","args":{}})));
            } else {
                assert!(result.is_none());
            }
        }
        writer.write_all(&[0]).unwrap();
        frame.poll(&mut reader).unwrap();
        frame.started = Some(Instant::now() - Duration::from_secs(3));
        assert!(frame.poll(&mut reader).is_err());
        for size in [0u32, 4097, u32::MAX] {
            let (mut w, mut r) = UnixStream::pair().unwrap();
            w.write_all(&size.to_be_bytes()).unwrap();
            assert!(Framing::default().poll(&mut r).is_err());
        }
        let (mut w, mut r) = UnixStream::pair().unwrap();
        let body = br#"{"op":"status","op":"scene_inspect"}"#;
        w.write_all(&(body.len() as u32).to_be_bytes()).unwrap();
        w.write_all(body).unwrap();
        let mut frame = Framing::default();
        assert!(frame.poll(&mut r).unwrap().is_none());
        assert!(frame.poll(&mut r).is_err());
    }
}
