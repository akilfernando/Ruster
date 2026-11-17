//! The one question asked of a creator when a new AI app connects:
//! Allow Always, Allow Once or Deny. It floats over the editor and blocks
//! nothing else.
use super::*;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

/// Clicks on a new prompt are ignored this long, so a click meant for the
/// editor cannot answer a prompt that just appeared under the pointer.
const SETTLE: Duration = Duration::from_millis(500);

/// Editor-side state of AI app trust.
pub(in crate::scene_editor) struct TrustUi {
    /// The trusted-apps file; tests point it at a temporary folder.
    pub(in crate::scene_editor) store: PathBuf,
    cache: Option<(Option<SystemTime>, crate::trust::Store)>,
    /// AI apps the creator denied in this session; they are not asked again.
    denied: Vec<String>,
    /// The connection the prompt is showing for, and since when.
    shown: Option<((String, u32), Instant)>,
    /// The connection that already received its save and build grants.
    granted: Option<(String, u32)>,
    /// One line for the Rot connection panel, such as a failed write.
    pub(in crate::scene_editor) notice: String,
}
impl Default for TrustUi {
    fn default() -> Self {
        Self {
            store: if cfg!(test) {
                PathBuf::from("/nonexistent/rot/ai-apps.json")
            } else {
                crate::trust::path()
            },
            cache: None,
            denied: Vec::new(),
            shown: None,
            granted: None,
            notice: String::new(),
        }
    }
}
impl TrustUi {
    /// The store, read again only when the file changed.
    fn current(&mut self) -> &crate::trust::Store {
        let modified = crate::trust::Store::modified(&self.store);
        if self
            .cache
            .as_ref()
            .is_none_or(|(seen, _)| *seen != modified)
        {
            self.cache = Some((modified, crate::trust::Store::load(&self.store)));
        }
        &self.cache.as_ref().unwrap().1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::scene_editor) enum Answer {
    Always,
    Once,
    Deny,
}

impl Host {
    /// The AI app waiting for an answer, if any: (name, token, owner).
    fn waiting_app(&self) -> Option<(String, String, (String, u32))> {
        let bridge = self.bridge.as_ref()?;
        if bridge.approved() || !bridge.authority().live() {
            return None;
        }
        let (name, token) = bridge.client()?;
        Some((name, token, bridge.pending_owner()?))
    }

    pub(in crate::scene_editor) fn ai_prompt_ui(&mut self, ctx: &egui::Context) {
        let Some((name, token, owner)) = self.waiting_app() else {
            self.editor.ai_trust.shown = None;
            return;
        };
        if self.editor.ai_trust.denied.contains(&name) {
            // Denied earlier in this session: refuse without asking again.
            self.disconnect_adapter();
            return;
        }
        let since = match &self.editor.ai_trust.shown {
            Some((shown, since)) if *shown == owner => *since,
            _ => {
                let now = Instant::now();
                self.editor.ai_trust.shown = Some((owner.clone(), now));
                now
            }
        };
        let settled = since.elapsed() >= SETTLE;
        if !settled {
            ctx.request_repaint_after(SETTLE - since.elapsed());
        }
        let store = self.editor.ai_trust.current();
        let new_key = store.known(&name) && !store.allowed(&name, &token);
        let app = crate::trust::display_name(&name);
        let mut answer = None;
        egui::Window::new("ai_prompt")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 96.0))
            .fixed_size(egui::vec2(340.0, 0.0))
            .show(ctx, |ui| {
                if new_key {
                    ui.strong(format!("{app} presents a new key"));
                    ui.label(format!(
                        "{app} was allowed before with a different key. Allow it only if you just set it up again."
                    ));
                } else {
                    ui.strong(format!("Allow {app} to edit this project?"));
                    ui.label(format!(
                        "{app} can read and change {} and save it.",
                        self.editor.document_label()
                    ));
                }
                if app != name {
                    ui.weak(format!("Reported name: {name}"));
                }
                ui.add_space(8.0);
                ui.add_enabled_ui(settled, |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if crate::editor_theme::primary_button(ui, "Allow Always")
                            .on_hover_text("Trust this app from now on. Revoke in Rot connection.")
                            .clicked()
                        {
                            answer = Some(Answer::Always);
                        }
                        if ui
                            .button("Allow Once")
                            .on_hover_text("Trust it until it disconnects")
                            .clicked()
                        {
                            answer = Some(Answer::Once);
                        }
                        if ui.button("Deny").clicked() {
                            answer = Some(Answer::Deny);
                        }
                    });
                });
            });
        if let Some(answer) = answer {
            self.answer_ai_prompt(answer);
        }
    }

    /// Apply the creator's answer to the waiting AI app.
    pub(in crate::scene_editor) fn answer_ai_prompt(&mut self, answer: Answer) {
        let Some((name, token, owner)) = self.waiting_app() else {
            return;
        };
        let app = crate::trust::display_name(&name);
        match answer {
            Answer::Always => {
                match crate::trust::Store::allow(&self.editor.ai_trust.store, &name, &token) {
                    Ok(()) => {
                        if let Some(bridge) = &self.bridge {
                            bridge.remember_client();
                        }
                    }
                    Err(error) => {
                        self.editor.ai_trust.notice = format!(
                            "Could not remember {app} ({error}). It is allowed for this connection only."
                        );
                    }
                }
                self.approve_adapter(&owner);
            }
            Answer::Once => {
                self.approve_adapter(&owner);
            }
            Answer::Deny => {
                self.editor.ai_trust.denied.push(name);
                self.disconnect_adapter();
            }
        }
    }

    /// An app the creator allowed may save the open scene inside its folder
    /// and, in a Rot project folder, build into `<project>/Builds`. Done once
    /// per approved connection.
    pub(in crate::scene_editor) fn grant_trusted_app(&mut self) {
        let Some(owner) = self
            .bridge
            .as_ref()
            .filter(|b| b.approved() && b.client().is_some())
            .and_then(|b| b.pending_owner())
        else {
            return;
        };
        if self.editor.ai_trust.granted.as_ref() == Some(&owner) {
            return;
        }
        self.editor.ai_trust.granted = Some(owner);
        let Some(project) = self
            .editor
            .session
            .path()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
        else {
            return;
        };
        if self.editor.save_grant.is_none() {
            match persistence_commands::SaveGrant::new(&project) {
                Ok(grant) => self.editor.save_grant = Some(grant),
                Err(error) => {
                    self.editor.ai_trust.notice = format!("AI saving is unavailable: {error}")
                }
            }
        }
        // Only a Rot project gets a Builds folder, never a loose scene's
        // folder such as Downloads.
        let rot_project = ["scene.json", ".rot-assets.json"]
            .iter()
            .any(|marker| project.join(marker).is_file());
        if rot_project && !self.editor.creation_export.has_agent_destination() {
            if let Err(error) = self
                .editor
                .creation_export
                .grant_agent_destination(&project)
            {
                self.editor.ai_trust.notice = format!("AI builds are unavailable: {error}");
            }
        }
    }

    /// Trusted apps with a Revoke button each, for the Rot connection panel.
    pub(super) fn trusted_apps_ui(&mut self, ui: &mut egui::Ui) {
        if !self.editor.ai_trust.notice.is_empty() {
            ui.add_space(4.0);
            ui.colored_label(ui.visuals().warn_fg_color, &self.editor.ai_trust.notice);
        }
        let trusted: Vec<_> = self
            .editor
            .ai_trust
            .current()
            .apps
            .iter()
            .filter(|app| app.allowed)
            .map(|app| app.name.clone())
            .collect();
        if trusted.is_empty() {
            return;
        }
        ui.add_space(8.0);
        ui.strong("Trusted AI apps");
        for name in trusted {
            ui.horizontal(|ui| {
                ui.label(crate::trust::display_name(&name))
                    .on_hover_text(format!("Reported name: {name}"));
                if ui
                    .button("Revoke")
                    .on_hover_text("Ask again the next time this app connects")
                    .clicked()
                {
                    self.revoke_app(&name);
                }
            });
        }
    }

    /// Forget an app and disconnect it if it is connected here.
    pub(in crate::scene_editor) fn revoke_app(&mut self, name: &str) {
        if let Err(error) = crate::trust::Store::revoke(&self.editor.ai_trust.store, name) {
            self.editor.ai_trust.notice = format!(
                "Could not revoke {} ({error}). It may still connect without asking.",
                crate::trust::display_name(name)
            );
        }
        if self
            .bridge
            .as_ref()
            .and_then(|b| b.client())
            .is_some_and(|(connected, _)| connected == name)
        {
            self.disconnect_adapter();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A host listening like the running editor, with a private trust store.
    fn listening(root: &Path, scene: &Path) -> Host {
        let mut host = super::super::tests::host("trust");
        host.editor.session.save(scene.to_path_buf()).unwrap();
        host.editor.ai_trust.store = root.join("config/rot/ai-apps.json");
        host.bridge = Some(
            crate::bridge::Bridge::start_discoverable(
                egui::Context::default(),
                host.editor.connection_incarnation.clone(),
                host.editor.connection_generation.clone(),
                None,
                &root.join("run"),
                Some(scene),
                Some(host.editor.ai_trust.store.clone()),
            )
            .unwrap(),
        );
        host
    }

    /// Connect as an AI app and make one call, serving the editor meanwhile.
    /// Returns the client too: dropping it disconnects the app.
    fn call(
        host: &mut Host,
        token: &str,
        operation: &str,
        arguments: Value,
    ) -> (crate::connection::Client, Value) {
        let mut client =
            crate::connection::Client::open(host.bridge.as_ref().unwrap().locator_path()).unwrap();
        client.identify(json!({"name":"claude-code","token":token}));
        let operation = operation.to_owned();
        let worker = std::thread::spawn(move || {
            let reply = client.call(&operation, &arguments);
            (client, reply)
        });
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !worker.is_finished() {
            host.service_bridge(&ctx);
            host.input = Default::default();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        worker.join().unwrap()
    }

    fn token(host: &Host) -> String {
        crate::trust::Store::token_for(&host.editor.ai_trust.store, "claude-code").unwrap()
    }

    #[test]
    fn allow_always_remembers_and_saves_without_environment_variables() {
        let root = tempfile::tempdir().unwrap();
        let scene = root.path().join("Project").join("scene.json");
        std::fs::create_dir_all(scene.parent().unwrap()).unwrap();
        let mut host = listening(root.path(), &scene);
        let token = token(&host);
        let (_app, reply) = call(&mut host, &token, "scene_inspect", json!({}));
        assert_eq!(reply["error"]["code"], "approval_required");
        host.answer_ai_prompt(Answer::Always);
        assert!(host.bridge.as_ref().unwrap().approved());
        assert!(
            crate::trust::Store::load(&host.editor.ai_trust.store).allowed("claude-code", &token)
        );

        host.grant_trusted_app();
        assert!(
            host.editor.save_grant.is_some(),
            "save with no ROT_MCP_SAVE_DIR"
        );
        assert!(scene.parent().unwrap().join("Builds").is_dir());
        let state = host.editor.persistence_command(
            &host
                .editor
                .scene_guard_token(&host.editor.connection_incarnation),
            "scene_persistence_inspect",
            &json!({}),
        );
        assert!(state.is_ok(), "{state:?}");
    }

    #[test]
    fn allow_once_is_not_remembered_and_deny_is_not_asked_again() {
        let root = tempfile::tempdir().unwrap();
        let scene = root.path().join("Loose").join("level.scene.json");
        std::fs::create_dir_all(scene.parent().unwrap()).unwrap();
        let mut host = listening(root.path(), &scene);
        let token = token(&host);
        let _app = call(&mut host, &token, "scene_inspect", json!({}));
        host.answer_ai_prompt(Answer::Once);
        assert!(host.bridge.as_ref().unwrap().approved());
        assert!(
            !crate::trust::Store::load(&host.editor.ai_trust.store).allowed("claude-code", &token)
        );
        host.grant_trusted_app();
        assert!(host.editor.save_grant.is_some());
        assert!(
            !scene.parent().unwrap().join("Builds").exists(),
            "a loose scene's folder gets no Builds folder"
        );

        host.disconnect_adapter();
        host.answer_ai_prompt(Answer::Deny); // nothing waiting: no effect
        drop(host);
        let mut host = listening(root.path(), &scene.with_file_name("other.scene.json"));
        let _app = call(&mut host, &token, "scene_inspect", json!({}));
        host.answer_ai_prompt(Answer::Deny);
        assert!(!host.bridge.as_ref().unwrap().authority().live());
        assert!(host
            .editor
            .ai_trust
            .denied
            .contains(&"claude-code".to_owned()));
    }

    #[test]
    fn revoke_disconnects_and_a_failed_revoke_is_shown() {
        let root = tempfile::tempdir().unwrap();
        let scene = root.path().join("Project").join("scene.json");
        std::fs::create_dir_all(scene.parent().unwrap()).unwrap();
        let mut host = listening(root.path(), &scene);
        let token = token(&host);
        let _app = call(&mut host, &token, "scene_inspect", json!({}));
        host.answer_ai_prompt(Answer::Always);
        host.revoke_app("claude-code");
        assert!(!host.bridge.as_ref().unwrap().authority().live());
        assert!(!crate::trust::Store::load(&host.editor.ai_trust.store).known("claude-code"));
        assert!(host.editor.ai_trust.notice.is_empty());

        let store = host.editor.ai_trust.store.clone();
        std::fs::set_permissions(&store, std::fs::Permissions::from_mode(0o644)).unwrap();
        host.revoke_app("claude-code");
        assert!(host.editor.ai_trust.notice.starts_with("Could not revoke"));
    }

    // A revoke written by another editor ends an Allow Always connection here.
    #[test]
    fn revoke_from_elsewhere_ends_a_remembered_connection() {
        let root = tempfile::tempdir().unwrap();
        let scene = root.path().join("Project").join("scene.json");
        std::fs::create_dir_all(scene.parent().unwrap()).unwrap();
        let mut host = listening(root.path(), &scene);
        let token = token(&host);
        crate::trust::Store::allow(&host.editor.ai_trust.store, "claude-code", &token).unwrap();
        let mut client =
            crate::connection::Client::open(host.bridge.as_ref().unwrap().locator_path()).unwrap();
        client.identify(json!({"name":"claude-code","token":token}));
        let ctx = egui::Context::default();
        let mut serve = |host: &mut Host, client: crate::connection::Client| {
            let worker = std::thread::spawn(move || {
                let mut client = client;
                let reply = client.call("scene_inspect", &json!({}));
                (client, reply)
            });
            let deadline = Instant::now() + Duration::from_secs(5);
            while !worker.is_finished() {
                host.service_bridge(&ctx);
                host.input = Default::default();
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            worker.join().unwrap()
        };
        let (client, reply) = serve(&mut host, client);
        assert_eq!(reply["ok"], true, "{reply}");
        std::thread::sleep(Duration::from_millis(20));
        crate::trust::Store::revoke(&host.editor.ai_trust.store, "claude-code").unwrap();
        let (_, reply) = serve(&mut host, client);
        assert_ne!(reply["ok"], true);
        assert!(!host.bridge.as_ref().unwrap().authority().live());
    }

    #[test]
    fn builds_link_leading_out_of_the_project_is_refused() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("Project");
        let outside = root.path().join("outside");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::os::unix::fs::symlink("../outside", project.join("Builds")).unwrap();
        let mut export = crate::creation_export::Controller::disabled();
        assert!(export.grant_agent_destination(&project).is_err());
        assert!(!export.has_agent_destination());
        std::fs::remove_file(project.join("Builds")).unwrap();
        assert!(export.grant_agent_destination(&project).is_ok());
        assert_eq!(
            export.output_directory(),
            Some(project.canonicalize().unwrap().join("Builds").as_path())
        );
    }
}
