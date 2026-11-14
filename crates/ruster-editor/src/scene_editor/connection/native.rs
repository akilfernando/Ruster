//! Human-only controls over Host authority. No CLI/configuration side effects.
use super::*;

#[derive(Clone, Default)]
pub(super) struct NativeUi {
    open: bool,
    preview: Option<Preview>,
    focus_cancel: bool,
    notice: String,
    #[cfg(target_os = "linux")]
    registration: RegistrationUi,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Default)]
struct RegistrationUi {
    profile: String,
    manifest: String,
    digest: String,
    remove: bool,
    preview: Option<RegistrationPreview>,
    pending: Option<std::sync::Arc<RegistrationPreparation>>,
}

#[cfg(target_os = "linux")]
struct RegistrationPreparation {
    result: std::sync::Mutex<std::sync::mpsc::Receiver<Result<RegistrationPreview, String>>>,
    cancelled: std::sync::atomic::AtomicBool,
}

#[cfg(target_os = "linux")]
impl RegistrationPreparation {
    fn start(
        work: impl FnOnce() -> Result<RegistrationPreview, String> + Send + 'static,
    ) -> Result<std::sync::Arc<Self>, String> {
        let (send, result) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("hermes-consent-preflight".into())
            .spawn(move || {
                let _ = send.send(work());
            })
            .map_err(|_| "Cannot start bounded registration preflight.")?;
        Ok(std::sync::Arc::new(Self {
            result: std::sync::Mutex::new(result),
            cancelled: std::sync::atomic::AtomicBool::new(false),
        }))
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone)]
struct RegistrationPreview {
    consent: super::consent::RegistrationConsent,
    observation: SceneObservation,
    locator: PathBuf,
    inputs: (String, String, String, bool),
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
    #[cfg(target_os = "linux")]
    fn registration_controls_ui(&mut self, ui: &mut egui::Ui, state: &mut NativeUi) {
        ui.separator();
        ui.heading("Hermes registration · production held");
        ui.label("Explicit named profile (never default)");
        ui.text_edit_singleline(&mut state.registration.profile);
        ui.label("Qualified companion manifest · absolute path");
        ui.text_edit_singleline(&mut state.registration.manifest);
        ui.label("Manifest SHA256 from independent qualification receipt");
        ui.text_edit_singleline(&mut state.registration.digest);
        ui.checkbox(
            &mut state.registration.remove,
            "Remove this exact registration",
        );
        ui.label("No profile discovery, creation, default selection or runtime installation. A digest pins your selected qualification receipt; it is not host attestation.");
        if ui.button("Preview Hermes registration").clicked() {
            state.registration.preview = None;
            match self.registration_preview(&state.registration) {
                Ok(preview) => {
                    state.registration.preview = Some(preview);
                    state.focus_cancel = true;
                    state.notice.clear();
                }
                Err(error) => state.notice = error,
            }
        }
        if let Some(preview) = state.registration.preview.clone() {
            egui::Modal::new(egui::Id::unique("hermes_registration_consent")).show(ui.ctx(), |ui| {
                ui.heading("Review exact Hermes configuration change");
                let invocation = preview.consent.invocation();
                let provenance = invocation.provenance().expect("bound preview");
                ui.label(format!("Named profile: {}", provenance.profile()));
                ui.label(format!("Exact config target: {}", provenance.context()["config"]["path"].as_str().unwrap_or("unavailable")));
                ui.label(format!("Launcher: {}", provenance.launcher().display()));
                ui.label(format!("Matched adapter: {}", provenance.adapter().display()));
                ui.label(format!("Nonsecret locator: {}", preview.locator.display()));
                ui.label(format!("Frozen argv: {:?}", invocation.args()));
                // Show only the intended fields, not arbitrary raw old-entry secrets.
                let request: Value = serde_json::from_str(invocation.stdin()).expect("prepared request");
                ui.label(format!("Change: {} · exact old-entry conditional check (raw values withheld)", if request["entry"].is_null() { "remove" } else if request["expected"]["present"] == true { "replace/reuse" } else { "create" }));
                ui.label(format!("Desired entry: {}", request["entry"]));
                ui.label(format!("Project: {} · Session: {} · Generation: {}", self.editor.document_label(), preview.observation.incarnation, preview.observation.project_generation));
                Self::scene_observation_ui(ui, &preview.observation, "Frozen ");
                ui.label("Registration is NOT project approval. The actual client must establish its own socket owner; approve that same adapter separately for this project/session only. No automatic retarget or reconnect.");
                ui.label("Asset catalog changes can persist immediately, before Save. Save/export destinations need separate grants.");
                ui.label("Manual client restart required after registration/removal; shared gateway is never restarted here. Disconnect revokes editor authority; it does not remove configuration.");
                ui.label("Production held: qualified installed strict API and owner review are still required. Confirm currently refuses before launch; it cannot enable the candidate executor.");
                let cancel = ui.button("Cancel Hermes registration");
                if state.focus_cancel { cancel.request_focus(); state.focus_cancel = false; }
                let escape = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                if cancel.clicked() || escape {
                    if let Some(mut preview) = state.registration.preview.take() { preview.consent.cancel(); }
                    state.notice = "Registration cancelled; no request dispatched".into();
                } else if ui.button("Confirm Hermes registration").clicked() {
                    let preview = state.registration.preview.take().expect("pending preview");
                    state.notice = self.confirm_registration(preview, &state.registration);
                }
            });
        }
    }

    #[cfg(target_os = "linux")]
    fn registration_preview(&self, inputs: &RegistrationUi) -> Result<RegistrationPreview, String> {
        if inputs.profile.is_empty()
            || inputs.profile == "default"
            || inputs.manifest.is_empty()
            || inputs.digest.is_empty()
        {
            return Err("Select an explicit non-default named profile and qualified companion manifest/digest; no defaults were selected.".into());
        }
        let locator = self
            .bridge
            .as_ref()
            .filter(|bridge| bridge.authority().live())
            .map(crate::bridge::Bridge::locator_path)
            .ok_or(
                "Start the project-local listener before preview; no registration was attempted.",
            )?;
        let consent = super::capsule::Provenance::from_companion_manifest(
            std::path::Path::new(&inputs.manifest),
            &inputs.digest,
            &inputs.profile,
            &locator,
            inputs.remove,
        )?;
        Ok(RegistrationPreview {
            consent,
            observation: self.native_scene_observation(),
            locator,
            inputs: (
                inputs.profile.clone(),
                inputs.manifest.clone(),
                inputs.digest.clone(),
                inputs.remove,
            ),
        })
    }

    #[cfg(target_os = "linux")]
    fn confirm_registration(
        &self,
        preview: RegistrationPreview,
        inputs: &RegistrationUi,
    ) -> String {
        let unchanged = preview.inputs
            == (
                inputs.profile.clone(),
                inputs.manifest.clone(),
                inputs.digest.clone(),
                inputs.remove,
            )
            && preview.observation == self.native_scene_observation()
            && self.bridge.as_ref().is_some_and(|bridge| {
                bridge.authority().live() && bridge.locator_path() == preview.locator
            })
            && self.editor.recovery_transition.is_none()
            && !self.editor.close_in_progress();
        if !unchanged
            || preview
                .consent
                .invocation()
                .provenance()
                .is_none_or(|p| p.revalidate().is_err())
        {
            return "Registration context changed; consent discarded. No process launched; review again.".into();
        }
        match super::worker::Worker::start(preview.consent) {
            Err(error) => error,
            Ok(_) => "Registration worker pending; editor binding separate.".into(),
        }
    }

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
        if ui.button("AI connection…").clicked() {
            state.open = true;
        }
        if state.open {
            let mut open = true;
            egui::Window::new("AI connection")
                .id(id.with("window"))
                .open(&mut open)
                .show(&ctx, |ui| {
                    self.connection_status_ui(ui);
                    self.binding_controls_ui(ui, &mut state);
                });
            state.open = open;
            if !open {
                state.preview = None;
            }
        }
        ctx.data_mut(|data| data.insert_temp(id, state));
    }

    pub(super) fn binding_controls_ui(&mut self, ui: &mut egui::Ui, state: &mut NativeUi) {
        let ctx = ui.ctx().clone();
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
        let action = match self.connection_state() {
            "retired" => Some("Connect AI"),
            "approval_required" => Some("Review adapter approval"),
            _ => None,
        };
        if let Some(action) = action {
            if ui.button(action).clicked() {
                state.preview = Some(self.binding_preview());
                state.focus_cancel = true;
                state.notice.clear();
            }
        }
        if self.bridge.is_some() && ui.button("Disconnect AI").clicked() {
            state.preview = None;
            self.disconnect_adapter();
        }
        #[cfg(target_os = "linux")]
        self.registration_controls_ui(ui, state);
        #[cfg(not(target_os = "linux"))]
        ui.label("Automatic Hermes registration is unavailable on this platform.");
        if !state.notice.is_empty() {
            ui.label(&state.notice);
        }
        if let Some(preview) = state.preview.clone() {
            egui::Modal::new(egui::Id::unique(("ai_binding_consent", &preview.incarnation))).show(&ctx, |ui| {
                ui.heading(if preview.owner.is_some() { "Approve this exact adapter?" } else { "Start a project-local listener?" });
                ui.label(format!("Project: {} · Scene: {}", self.editor.document_label(), self.editor.session.scene().id));
                ui.label(format!("Session: {}\nGeneration: {}", preview.incarnation, preview.generation));
                Self::scene_observation_ui(ui, &preview.observation, "Frozen ");
                ui.label(format!("Frozen scene: {}", preview.observation.scene_id));
                if let Some((owner, pid)) = &preview.owner { ui.label(format!("Adapter: {owner} · local PID {pid}")); }
                ui.label("Approval grants this adapter authority over this project/session only. No automatic retargeting or reconnect.");
                ui.label("Asset catalog changes can persist immediately, before Save. Save/export destinations require separate grants.");
                let cancel = ui.button("Cancel AI connection");
                if state.focus_cancel { cancel.request_focus(); state.focus_cancel = false; }
                let escape = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                if cancel.clicked() || escape {
                    state.preview = None;
                } else if ui.button("Confirm AI connection").clicked() {
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
    use std::os::unix::net::UnixStream;

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
    #[cfg(target_os = "linux")]
    fn registration_requires_explicit_qualified_inputs_without_selecting_default() {
        let ctx = egui::Context::default();
        let mut host = super::super::tests::host("registration-inputs");
        let before = host.editor.session.snapshot_bytes().unwrap();
        let mut state = NativeUi::default();
        click(&mut host, &ctx, &mut state, "Preview Hermes registration");
        assert!(state.notice.contains("explicit"), "{}", state.notice);
        assert!(host.bridge.is_none());
        assert_eq!(before, host.editor.session.snapshot_bytes().unwrap());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn qualified_manifest_native_preview_cancels_rejects_changes_and_keeps_production_held() {
        use sha2::{Digest, Sha256};
        let dir = tempfile::tempdir().unwrap();
        let spec = super::super::capsule::tests::fixture(dir.path());
        let manifest_path = dir.path().join("companion.json");
        let manifest = json!({"version":1,
            "editor_sha256":format!("{:x}",Sha256::digest(std::fs::read(std::env::current_exe().unwrap()).unwrap())),
            "inspection":{"ok":true,"context":spec.context,"present":false,"entry":null},
            "launch":spec});
        let bytes = serde_json::to_vec(&manifest).unwrap();
        std::fs::write(&manifest_path, &bytes).unwrap();
        let ctx = egui::Context::default();
        let mut host = super::super::tests::host("qualified-preview");
        let before = host.editor.session.snapshot_bytes().unwrap();
        let mut state = NativeUi::default();
        state.registration.profile = "named".into();
        state.registration.manifest = manifest_path.to_str().unwrap().into();
        state.registration.digest = format!("{:x}", Sha256::digest(&bytes));
        click(&mut host, &ctx, &mut state, "Preview Hermes registration");
        assert!(
            state.registration.preview.is_none(),
            "listener must be explicit"
        );
        assert!(host.bridge.is_none());
        click(&mut host, &ctx, &mut state, "Connect AI");
        click(&mut host, &ctx, &mut state, "Confirm AI connection");
        let locator = host.bridge.as_ref().unwrap().locator_path();
        let config = dir.path().join("profiles/named/config.yaml");
        let original = std::fs::read(&config).unwrap();
        let started = std::time::Instant::now();
        click(&mut host, &ctx, &mut state, "Preview Hermes registration");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(2),
            "qualification IO must not block the editor thread"
        );
        assert!(state.registration.preview.is_some(), "{}", state.notice);
        let painted = paint(&mut host, &ctx, &mut state, vec![]);
        for label in ["Named profile: named", "Asset catalog changes can persist immediately, before Save. Save/export destinations need separate grants."] {
            assert!(painted.iter().any(|(text, _)| text == label), "missing {label}");
        }
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
        assert!(state.registration.preview.is_none());
        assert!(state.notice.contains("cancelled"));
        click(&mut host, &ctx, &mut state, "Preview Hermes registration");
        state.registration.profile = "other".into();
        click(&mut host, &ctx, &mut state, "Confirm Hermes registration");
        assert!(state.notice.contains("context changed"));
        state.registration.profile = "named".into();
        click(&mut host, &ctx, &mut state, "Preview Hermes registration");
        click(&mut host, &ctx, &mut state, "Confirm Hermes registration");
        assert!(
            state.notice.contains("No process launched"),
            "{}",
            state.notice
        );
        assert!(state.registration.preview.is_none());
        click(&mut host, &ctx, &mut state, "Preview Hermes registration");
        std::fs::write(&manifest_path, b"changed").unwrap();
        click(&mut host, &ctx, &mut state, "Confirm Hermes registration");
        assert!(state.notice.contains("context changed"));
        click(&mut host, &ctx, &mut state, "Preview Hermes registration");
        assert!(state.registration.preview.is_none());
        assert_eq!(std::fs::read(&config).unwrap(), original);
        assert_eq!(before, host.editor.session.snapshot_bytes().unwrap());
        click(&mut host, &ctx, &mut state, "Disconnect AI");
        super::super::tests::retire(&mut host, &ctx);
        assert!(!locator.exists());
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
            UnixStream::connect(locator_path.parent().unwrap().join("editor.sock")).unwrap();
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
        click(&mut host, &ctx, &mut state, "Connect AI");
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
