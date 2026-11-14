//! Human-only controls over Host authority. No CLI/configuration side effects.
use super::*;

#[derive(Clone, Default)]
pub(super) struct NativeUi {
    open: bool,
    preview: Option<Preview>,
    focus_cancel: bool,
    notice: String,
}

#[derive(Clone)]
struct Preview {
    incarnation: String,
    generation: String,
    locator: Option<PathBuf>,
    owner: Option<(String, u32)>,
}

impl Host {
    fn binding_preview(&self) -> Preview {
        Preview {
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
        if preview.incarnation != self.editor.connection_incarnation
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
        ui.label("Automatic Hermes registration is unavailable; no client configuration is changed here.");
        if !state.notice.is_empty() {
            ui.label(&state.notice);
        }
        if let Some(preview) = state.preview.clone() {
            egui::Modal::new(egui::Id::unique(("ai_binding_consent", &preview.incarnation))).show(&ctx, |ui| {
                ui.heading(if preview.owner.is_some() { "Approve this exact adapter?" } else { "Start a project-local listener?" });
                ui.label(format!("Project: {} · Scene: {}", self.editor.document_label(), self.editor.session.scene().id));
                ui.label(format!("Session: {}\nGeneration: {}", preview.incarnation, preview.generation));
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
