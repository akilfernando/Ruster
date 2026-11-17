//! Human-only controls over Host authority. No CLI/configuration side effects.
use super::*;

#[derive(Clone, Default)]
pub(super) struct NativeUi {
    open: bool,
    preview: Option<Preview>,
    focus_cancel: bool,
    notice: String,
}

/// Local human-visible readback only. These are concurrency guards, never the
/// private Bridge capability. Observing before launch does not select an owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SceneObservation {
    pub(super) session_token: String,
    pub(super) revision: String,
    pub(super) scene_id: String,
    pub(super) incarnation: String,
    pub(super) project_generation: String,
    pub(super) owner: Option<(String, u32)>,
    pub(super) connection_state: &'static str,
}

#[derive(Clone)]
struct Preview {
    observation: SceneObservation,
    incarnation: String,
    generation: String,
    locator: Option<PathBuf>,
    owner: Option<(String, u32)>,
}

impl Host {
    /// Supported local observation, including when no listener/adapter exists.
    /// Does not dispatch a tool, start a listener, approve or claim an owner.
    pub(super) fn native_scene_observation(&self) -> SceneObservation {
        SceneObservation {
            session_token: self
                .editor
                .scene_guard_token(&self.editor.connection_incarnation),
            revision: self.editor.session.scene().revision.to_string(),
            scene_id: self.editor.session.scene().id.clone(),
            incarnation: self.editor.connection_incarnation.clone(),
            project_generation: self.editor.connection_generation.clone(),
            owner: self.pending_adapter(),
            connection_state: self.connection_state(),
        }
    }

    fn scene_observation_ui(ui: &mut egui::Ui, observation: &SceneObservation, prefix: &str) {
        ui.label(format!(
            "{prefix}Scene guard session_token: {}",
            observation.session_token
        ));
        ui.label(format!(
            "{prefix}Scene guard revision: {}",
            observation.revision
        ));
        ui.label(format!(
            "{prefix}Observation owner: {} · {}",
            observation.connection_state,
            observation.owner.as_ref().map_or_else(
                || "No adapter owner observed; prelaunch observation does not bind a future owner"
                    .into(),
                |(owner, pid)| format!("{owner} · local PID {pid}")
            )
        ));
        ui.label("Scene guards are read-only concurrency identifiers, not connection session labels or approval capabilities.");
    }

    fn binding_preview(&self) -> Preview {
        Preview {
            observation: self.native_scene_observation(),
            incarnation: self.editor.connection_incarnation.clone(),
            generation: self.editor.connection_generation.clone(),
            locator: self
                .bridge
                .as_ref()
                .map(crate::bridge::Bridge::locator_path),
            owner: self.pending_adapter(),
        }
    }

    fn confirm_binding(&mut self, preview: Preview, ctx: &egui::Context) -> bool {
        if preview.observation != self.native_scene_observation()
            || preview.incarnation != self.editor.connection_incarnation
            || preview.generation != self.editor.connection_generation
            || preview.locator
                != self
                    .bridge
                    .as_ref()
                    .map(crate::bridge::Bridge::locator_path)
            || preview.owner != self.pending_adapter()
            || self.editor.recovery_transition.is_some()
            || self.editor.close_in_progress()
        {
            return false;
        }
        match preview.owner {
            Some(owner) => {
                self.connection_state() == "approval_required" && self.approve_adapter(&owner)
            }
            None if self.bridge.is_none() => self.connect_adapter(ctx).is_ok(),
            None => false,
        }
    }

    pub(in crate::scene_editor) fn native_connection_ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let id = egui::Id::unique(("native_ai_connection", &self.editor.connection_incarnation));
        let mut state = ctx
            .data_mut(|data| data.get_temp::<NativeUi>(id))
            .unwrap_or_default();
        if std::mem::take(&mut self.editor.ai_connection_requested) {
            state.open = !state.open;
            self.editor.runtime.release_input(&ctx);
        }
        if state.open {
            let available = ui.available_size();
            let panel = if available.x >= 700.0 {
                egui::Panel::right(id.with("dock"))
                    .exact_size((available.x * 0.34).clamp(310.0, 416.0))
            } else {
                egui::Panel::top(id.with("compact_dock"))
                    .exact_size((available.y * 0.52).min(280.0))
            };
            panel
                .resizable(false)
                .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::same(14)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Rot connection");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button("Close")
                                .on_hover_text("Close Rot panel")
                                .clicked()
                            {
                                state.open = false;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.connection_status_ui(ui);
                        self.binding_controls_ui(ui, &mut state);
                    });
                });
        }
        if !state.open {
            state.preview = None;
        }
        ctx.data_mut(|data| data.insert_temp(id, state));
    }

    pub(super) fn binding_controls_ui(&mut self, ui: &mut egui::Ui, state: &mut NativeUi) {
        let ctx = ui.ctx().clone();
        if let Some((bridge, request)) = self.bridge.as_mut().and_then(|bridge| {
            let request = bridge.browser_request()?;
            Some((bridge, request))
        }) {
            ui.separator();
            ui.heading("Approve pending browser for this project?");
            ui.label(format!("Browser verification code: {}", &request.id[..12]));
            ui.label(&request.context);
            if request.binding.is_some() {
                ui.label("Compare the code with your browser. This is not verified application provenance. Approval permits a native project preview with non-authored camera navigation and shared selection only, for at most 30 minutes. Browser transforms, properties, Undo, Save and export are not granted. Direct MCP authoring retains its separate adapter approval. Reload is automatic; Disconnect AI revokes this browser. Pending requests expire after 2 minutes. Denial requires a fresh native connection.");
            } else {
                ui.label("Compare the code with your browser. This is not verified application provenance. Approval permits reading and editing this exact native project/session for at most 30 minutes. Reload is automatic; Disconnect AI revokes it. Pending requests expire after 2 minutes. Denial requires a fresh native connection.");
            }
            if ui.button("Approve this browser/project once").clicked()
                && bridge.approve_browser(&request)
            {
                state.open = false;
            }
            if ui.button("Deny pending browser").clicked() {
                bridge.deny_browser();
            }
        }

        ui.add_space(4.0);
        let enrolled = std::env::var_os("ROT_CONNECTOR_ENROLLMENT").is_some();
        if enrolled {
            ui.label("Use project_connect in the enrolled connector. No native approval is needed; browser access is optional and separate. Disconnect retains enrollment; Revoke removes persistent access.");
        }
        let action = match self.connection_state() {
            "retired" => Some("Connect AI"),
            "approval_required" => Some("Review adapter approval"),
            _ => None,
        };
        if let Some(action) = action.filter(|_| !enrolled) {
            if crate::editor_theme::primary_button(ui, action).clicked() {
                state.preview = Some(self.binding_preview());
                state.focus_cancel = true;
                state.notice.clear();
            }
        }
        if let Some((grant, registered_project)) = self.enrolled_policy().ok().flatten() {
            ui.label(registered_project);
            ui.label("Local enrollment administration only; revoked or replacement documents receive no project work authority. Save, export and execution keep their separate grants.");
            for (label, all) in [
                ("Revoke persistent access to this project", false),
                ("Revoke access to all registered projects", true),
            ] {
                if ui.button(label).clicked() {
                    self.disconnect_adapter();
                    match grant.revoke(all) { Ok(())=>state.notice="Persistent access revoked. Deliberate re-enrollment required.".into(),Err(_)=>state.notice="Revocation not persisted: a publication may be finishing or enrollment changed. Disconnected locally; retry Revoke after it settles.".into() }
                }
            }
        }
        if self.bridge.is_some() && ui.button("Disconnect AI").clicked() {
            state.preview = None;
            self.disconnect_adapter();
        }
        if !state.notice.is_empty() {
            ui.label(&state.notice);
        }
        self.trusted_apps_ui(ui);
        ui.add_space(8.0);
        ui.separator();
        ui.collapsing("Connection details", |ui| {
            ui.label(format!("Project: {}", self.editor.document_label()));
            ui.label(format!("Scene: {}", self.editor.session.scene().id));
            ui.label(format!(
                "Editor session: {}",
                self.editor.connection_incarnation
            ));
            ui.label(format!(
                "Project generation: {}",
                self.editor.connection_generation
            ));
            Self::scene_observation_ui(ui, &self.native_scene_observation(), "");
            if let Some(bridge) = &self.bridge {
                ui.label(format!(
                    "Nonsecret locator: {}",
                    bridge.locator_path().display()
                ));
            }
            if let Some((owner, pid)) = self.pending_adapter() {
                ui.label(format!("Adapter owner: {owner} · local PID {pid}"));
                ui.label(
                    "Owner identity is socket-bound, not executable provenance or client registration.",
                );
            }
            ui.weak("Registration and client reload are separate from editor binding.");
            ui.weak("Automatic Hermes registration is unavailable. This panel leaves client configuration alone.");
        });
        if let Some(preview) = state.preview.clone() {
            egui::Modal::new(egui::Id::unique(("ai_binding_consent", &preview.incarnation))).show(&ctx, |ui| {
                ui.heading(if preview.owner.is_some() { "Approve this exact adapter?" } else { "Start a project-local listener?" });
                ui.label(format!("Project: {} · Scene: {}", self.editor.document_label(), self.editor.session.scene().id));
                ui.collapsing("Approval details", |ui| {
                    ui.label(format!("Session: {}\nGeneration: {}", preview.incarnation, preview.generation));
                    Self::scene_observation_ui(ui, &preview.observation, "Frozen ");
                    ui.label(format!("Frozen scene: {}", preview.observation.scene_id));
                });
                if let Some((owner, pid)) = &preview.owner { ui.label(format!("Adapter: {owner} · local PID {pid}")); }
                ui.label("Approval grants this adapter authority over this project/session only. No automatic retargeting or reconnect.");
                ui.label("Asset catalog changes can persist immediately, before Save. Save/export destinations require separate grants.");
                let (cancel, confirm) = ui.horizontal_wrapped(|ui| {
                    let cancel = ui.button("Cancel AI connection");
                    let confirm = crate::editor_theme::primary_button(ui, "Confirm AI connection");
                    (cancel, confirm)
                }).inner;
                if state.focus_cancel { cancel.request_focus(); state.focus_cancel = false; }
                let escape = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                if cancel.clicked() || escape {
                    state.preview = None;
                } else if confirm.clicked() {
                    // Taking the frozen preview consumes consent, even on stale refusal.
                    let accepted = self.confirm_binding(state.preview.take().unwrap(), &ctx);
                    state.notice = if accepted { "Local binding action completed. Client registration/reload was not performed." } else { "Connection changed or is unavailable. Consent discarded; review again." }.into();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(
        host: &mut Host,
        ctx: &egui::Context,
        state: &mut NativeUi,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Pos2)> {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1600., 1200.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                host.connection_status_ui(ui);
                host.binding_controls_ui(ui, state);
            },
        );
        output.textures_delta.clear();
        output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => Some((
                    text.galley.job.text.clone(),
                    text.pos + text.galley.size() * 0.5,
                )),
                _ => None,
            })
            .collect()
    }

    fn click(host: &mut Host, ctx: &egui::Context, state: &mut NativeUi, label: &str) {
        let painted = paint(host, ctx, state, vec![]);
        let pos = painted
            .iter()
            .find(|(text, _)| text == label)
            .unwrap_or_else(|| panic!("missing {label}: {painted:?}"))
            .1;
        paint(
            host,
            ctx,
            state,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        paint(
            host,
            ctx,
            state,
            vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }

    #[test]
    fn local_revoke_controls_survive_revocation_and_document_change() {
        use std::os::unix::fs::PermissionsExt;
        if let Some(path) = std::env::var_os("NATIVE_ADMIN_RECOVERY") {
            let path = PathBuf::from(path);
            let ctx = egui::Context::default();
            let mut host = super::super::tests::host("registered-scene");
            let startup: crate::project_startup::Startup =
                serde_json::from_str(&std::env::var("ROT_PROJECT_STARTUP").unwrap()).unwrap();
            host.editor.session.save(&startup.scene_path).unwrap();
            let before = host.editor.session.snapshot_bytes().unwrap();
            let mut state = NativeUi::default();
            let original = crate::enrollment::Grant::load(&path, "one").unwrap();
            click(
                &mut host,
                &ctx,
                &mut state,
                "Revoke persistent access to this project",
            );
            assert!(!original.live());
            assert!(original.commit_lock().is_err());
            assert!(crate::enrollment::Grant::load(&path, "one").is_err());
            assert!(crate::enrollment::Grant::load(&path, "two").is_ok());
            assert!(host.enrolled_grant().is_err());
            assert!(host.connect_adapter(&ctx).is_err());
            assert!(paint(&mut host, &ctx, &mut state, vec![])
                .iter()
                .any(|(label, _)| label == "Revoke access to all registered projects"));
            assert_eq!(host.editor.session.snapshot_bytes().unwrap(), before);
            host.editor.session = Session::new(Scene::authored_empty("changed-document")).unwrap();
            let before = host.editor.session.snapshot_bytes().unwrap();
            let labels = paint(&mut host, &ctx, &mut state, vec![]);
            assert!(labels
                .iter()
                .any(|(label, _)| label.contains("Registered project: one")
                    && label.contains("registered.rot")));
            click(
                &mut host,
                &ctx,
                &mut state,
                "Revoke access to all registered projects",
            );
            assert!(crate::enrollment::Grant::load(&path, "two").is_err());
            assert!(host.connect_adapter(&ctx).is_err());
            assert_eq!(host.editor.session.snapshot_bytes().unwrap(), before);
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("enrollment.json");
        let exe = std::fs::read_link("/proc/self/exe").unwrap();
        std::fs::write(&path, json!({"version":1,"connector_id":"fixture","executable":exe,"binary_identity":crate::project_startup::BinaryIdentity::read(&exe).unwrap(),"secret":crate::connection::unpredictable().unwrap(),"generation":1,"projects":[{"id":"one","generation":1,"allowed":true},{"id":"two","generation":1,"allowed":true}]}).to_string()).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let startup = json!({"scene_path":root.path().join("registered.rot"),"native_binary":exe,"state_directory":"/fixture","binary_identity":{"dev":0,"ino":0,"len":0,"mtime":0,"mtime_nsec":0},"routing_identity":{"directory_dev":0,"directory_ino":0,"owner_ino":0,"startup_ino":0}});
        let result = std::process::Command::new(exe).args(["--exact", "scene_editor::connection::native::tests::local_revoke_controls_survive_revocation_and_document_change", "--nocapture"]).env("NATIVE_ADMIN_RECOVERY", &path).env("ROT_CONNECTOR_ENROLLMENT", &path).env("ROT_PROJECT_ID", "one").env("ROT_PROJECT_SCENE_ID", "registered-scene").env("ROT_PROJECT_STARTUP", startup.to_string()).output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn real_widget_consent_cancels_then_approves_exact_socket_owner_and_disconnects() {
        let ctx = egui::Context::default();
        let mut host = super::super::tests::host("native-consent");
        let before = host.editor.session.snapshot_bytes().unwrap();
        let mut state = NativeUi::default();
        click(&mut host, &ctx, &mut state, "Connect AI");
        assert!(state.preview.is_some());
        assert!(host.bridge.is_none());
        paint(
            &mut host,
            &ctx,
            &mut state,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(state.preview.is_none());
        assert!(host.bridge.is_none());
        click(&mut host, &ctx, &mut state, "Connect AI");
        click(&mut host, &ctx, &mut state, "Confirm AI connection");
        assert!(state.preview.is_none());
        let locator_path = host.bridge.as_ref().unwrap().locator_path();
        let locator: crate::connection::Locator =
            serde_json::from_slice(&std::fs::read(&locator_path).unwrap()).unwrap();
        let mut stream =
            crate::connection::connect_socket(&locator_path.parent().unwrap().join("editor.sock"))
                .unwrap();
        let owner = crate::connection::unpredictable().unwrap();
        crate::connection::send(
            &mut stream,
            &json!({"wire":crate::connection::WIRE,"hello":owner,"binding":locator.binding()}),
        )
        .unwrap();
        assert_eq!(
            crate::bridge::tests::receive(&mut stream)["state"],
            "approval_required"
        );
        click(&mut host, &ctx, &mut state, "Review adapter approval");
        click(&mut host, &ctx, &mut state, "Cancel AI connection");
        assert!(!host.bridge.as_ref().unwrap().approved());
        click(&mut host, &ctx, &mut state, "Review adapter approval");
        click(&mut host, &ctx, &mut state, "Confirm AI connection");
        assert!(state.preview.is_none());
        crate::connection::send(
            &mut stream,
            &json!({"wire":crate::connection::WIRE,"bind":owner}),
        )
        .unwrap();
        assert!(crate::bridge::tests::receive(&mut stream)["capability"].is_string());
        assert_eq!(host.connection_state(), "bound");
        let request = crate::bridge::browser::BrowserRequest {
            id: "a".repeat(64),
            context: "native-consent exact browser/project".into(),
            binding: None,
        };
        assert_eq!(
            host.bridge
                .as_mut()
                .unwrap()
                .request_browser(request.clone()),
            "pending"
        );
        assert!(!host.bridge.as_ref().unwrap().browser_authorized(&request));
        click(
            &mut host,
            &ctx,
            &mut state,
            "Approve this browser/project once",
        );
        assert!(host.bridge.as_ref().unwrap().browser_authorized(&request));
        let wrong_project = crate::bridge::browser::BrowserRequest {
            context: "different-project".into(),
            ..request.clone()
        };
        assert!(!host
            .bridge
            .as_ref()
            .unwrap()
            .browser_authorized(&wrong_project));
        click(&mut host, &ctx, &mut state, "Disconnect AI");
        assert_eq!(host.connection_state(), "revoked_draining");
        assert!(!host.editor.connection_authority.as_ref().unwrap().live());
        drop(stream);
        super::super::tests::retire(&mut host, &ctx);
        assert!(!locator_path.exists());
        assert_eq!(before, host.editor.session.snapshot_bytes().unwrap());
    }

    #[test]
    fn prelaunch_scene_observation_matches_tool_guards_and_paints_without_authority() {
        let ctx = egui::Context::default();
        let mut host = super::super::tests::host("scene-observation");
        let before = host.editor.session.snapshot_bytes().unwrap();
        let observation = host.native_scene_observation();
        assert!(observation.owner.is_none());
        assert_eq!(observation.connection_state, "retired");
        assert_ne!(observation.session_token, observation.incarnation);
        let nonce = host.editor.connection_incarnation.clone();
        let inspected = host.editor.command(&nonce, "scene_inspect", json!({}));
        assert_eq!(
            inspected["result"]["session_token"],
            observation.session_token
        );
        assert_eq!(inspected["result"]["revision"], observation.revision);
        let mut state = NativeUi::default();
        assert!(!paint(&mut host, &ctx, &mut state, vec![])
            .iter()
            .any(|(text, _)| text.contains(&observation.session_token)));
        click(&mut host, &ctx, &mut state, "Connection details");
        click(&mut host, &ctx, &mut state, "Connect AI");
        click(&mut host, &ctx, &mut state, "Approval details");
        let painted = paint(&mut host, &ctx, &mut state, vec![]);
        for prefix in ["", "Frozen "] {
            assert!(painted.iter().any(|(text, _)| text
                == &format!(
                    "{prefix}Scene guard session_token: {}",
                    observation.session_token
                )));
            assert!(painted.iter().any(|(text, _)| text
                == &format!("{prefix}Scene guard revision: {}", observation.revision)));
        }
        assert!(host.bridge.is_none());
        assert!(host.editor.connection_authority.is_none());
        assert_eq!(before, host.editor.session.snapshot_bytes().unwrap());
    }

    #[test]
    fn scene_guard_changes_discard_frozen_binding_consent() {
        let ctx = egui::Context::default();
        let mut host = super::super::tests::host("scene-stale");
        let preview = host.binding_preview();
        host.editor.generation += 1;
        assert_ne!(
            preview.observation.session_token,
            host.native_scene_observation().session_token
        );
        assert!(!host.confirm_binding(preview, &ctx));
        let preview = host.binding_preview();
        let nonce = host.editor.connection_incarnation.clone();
        let result = host.editor.command(
            &nonce,
            "scene_create",
            json!({
                "expected_session_token":preview.observation.session_token,
                "expected_revision":preview.observation.revision,
                "object_id":"observation-probe","name":"Probe"
            }),
        );
        assert_eq!(result["ok"], true, "{result}");
        assert!(!host.confirm_binding(preview, &ctx));
        assert!(host.bridge.is_none());
    }

    #[test]
    fn consumed_preview_cannot_approve_replacement_generation_or_owner() {
        let ctx = egui::Context::default();
        let mut host = super::super::tests::host("stale-consent");
        let preview = host.binding_preview();
        host.editor.connection_generation = crate::connection::unpredictable().unwrap();
        assert!(!host.confirm_binding(preview, &ctx));
        assert!(host.bridge.is_none());
        host.connect_adapter(&ctx).unwrap();
        let mut preview = host.binding_preview();
        preview.owner = Some(("not-the-socket-owner".into(), 1));
        assert!(!host.confirm_binding(preview, &ctx));
        assert!(!host.bridge.as_ref().unwrap().approved());
        super::super::tests::retire(&mut host, &ctx);
    }
}
