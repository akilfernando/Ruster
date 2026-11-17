//! Disposable startup grants, not production consent or a public command bypass.
use super::*;
pub struct Owner {
    editor: SceneEditor,
    nonce: String,
}
impl Owner {
    pub fn new(out: &std::path::Path, save_grant: bool, reopen: bool) -> Self {
        let destination = out.join("disposable-destination");
        let session = if reopen {
            Session::open(destination.join("fixture.json")).unwrap()
        } else {
            let fixture = std::env::var("SPIKE_BENCH_SCENE").ok();
            let scene = fixture.map_or_else(Scene::demo, |path| {
                serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
            });
            scene.validate().unwrap();
            Session::new(scene).unwrap()
        };
        let mut editor = SceneEditor::with_workspace(session, Workspace::detached());
        // Existing document-open worker must settle before accepting a command.
        let ctx = egui::Context::default();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while editor.asset_busy() {
            editor.recovery.poll(&ctx);
            editor.process_recovery_ack();
            editor.asset_nonvisual_frame(&ctx, true);
            assert!(
                std::time::Instant::now() < deadline,
                "fixture open timed out"
            );
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if save_grant {
            editor.save_grant = Some(persistence_commands::SaveGrant::new(&destination).unwrap());
        }
        Self {
            editor,
            nonce: format!("localhost-fixture-{}", std::process::id()),
        }
    }
    pub fn scene(&self) -> Scene {
        self.editor.session.scene().clone()
    }
    pub fn inspect(&mut self) -> Value {
        let mut result = self.editor.command(&self.nonce, "scene_inspect", json!({}));
        result["result"]["undo_len"] = json!(self.editor.session.undo_len());
        result["result"]["persistence"] =
            self.editor
                .command(&self.nonce, "scene_persistence_inspect", json!({}))["result"]
                .clone();
        result
    }
    pub fn command(&mut self, request: &Value) -> Value {
        let op = request["operation"].as_str().unwrap_or("");
        if op == "scene_save" {
            let mut args = request["arguments"].clone();
            // Browser inputs never choose a filesystem path or filename.
            if !args.is_object() || args.get("filename").is_some() {
                return json!({"ok":false,"error":{"code":"fixture_destination_denied"}});
            }
            if self.editor.session.path().is_none() {
                args["filename"] = json!("fixture.json");
            }
            let ctx = egui::Context::default();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            let (lease, mut response) = crate::bridge::disposable_startup_lease(deadline);
            self.editor
                .dispatch_save_request(&ctx, &self.nonce, &args, lease);
            loop {
                self.editor.recovery.poll(&ctx);
                self.editor.process_recovery_ack();
                self.editor.asset_nonvisual_frame(&ctx, true);
                if let Some(reply) = response() {
                    return reply;
                }
                if std::time::Instant::now() > deadline {
                    return json!({"ok":false,"error":{"code":"fixture_save_timeout"}});
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        if !matches!(op, "scene_transform" | "scene_undo") {
            return json!({"ok":false,"error":{"code":"fixture_operation_denied"}});
        }
        self.editor
            .command(&self.nonce, op, request["arguments"].clone())
    }
}
