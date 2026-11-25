//! Human-only controls over Host authority. No CLI/configuration side effects.
use super::*;
use crate::strings::Text;

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
        let (scene_id, revision) = self.editor.component.as_ref().map_or(("", 0), |component| {
            let (_, id, revision) = component.ai_document();
            (id, revision)
        });
        SceneObservation {
            session_token: self
                .editor
                .scene_guard_token(&self.editor.connection_incarnation),
            revision: revision.to_string(),
            scene_id: scene_id.to_owned(),
            incarnation: self.editor.connection_incarnation.clone(),
            project_generation: self.editor.connection_generation.clone(),
            owner: self.pending_adapter(),
            connection_state: self.connection_state(),
        }
    }

    fn scene_observation_ui(ui: &mut egui::Ui, observation: &SceneObservation, prefix: &str) {
        ui.label(format!("{prefix}Session: {}", observation.session_token));
        ui.label(format!("{prefix}Scene version: {}", observation.revision));
        ui.label(format!(
            "{prefix}Seen from: {} · {}",
            observation.connection_state,
            observation.owner.as_ref().map_or_else(
                || "No AI app seen yet".into(),
                |(owner, pid)| format!("{owner} · process {pid}")
            )
        ));
        ui.label(Text::ReadOnlySceneIdentifiers.plain());
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
            if let Some(component) = &mut self.editor.component {
                component.release_input(&ctx);
            }
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
                        ui.heading(Text::AiConnection.plain());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .button(Text::CloseErrorDetails.plain())
                                .on_hover_text(Text::CloseAiConnection.plain())
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
            ui.heading(Text::NativeApprovePendingBrowserFor.plain());
            ui.label(format!("Browser verification code: {}", &request.id[..12]));
            ui.label(&request.context);
            if request.binding.is_some() {
                ui.label(Text::NativeCompareThisCodeWith.plain());
                ui.label(Text::NativeNativeApprovalLetsThisBrowser.plain());
                ui.label(Text::NativePendingRequestsExpireAfter.plain());
            } else {
                ui.label(Text::NativeCompareThisCodeWith.plain());
                ui.label(Text::NativeApprovalLetsThisBrowser.plain());
                ui.label(Text::NativePendingRequestsExpireAfter.plain());
            }
            if ui.button(Text::AllowBrowserOnce.plain()).clicked()
                && bridge.approve_browser(&request)
            {
                state.open = false;
            }
            if ui.button(Text::DenyPendingBrowser.plain()).clicked() {
                bridge.deny_browser();
            }
        }

        ui.add_space(4.0);
        let action = match self.connection_state() {
            "retired" => Some("Start Listening"),
            "approval_required" => Some("Review Request…"),
            _ => None,
        };
        if let Some(action) = action {
            if crate::editor_theme::primary_button(ui, action).clicked() {
                state.preview = Some(self.binding_preview());
                state.focus_cancel = true;
                state.notice.clear();
            }
        }
        if self.pending_adapter().is_some() && ui.button(Text::Disconnect.plain()).clicked() {
            state.preview = None;
            self.disconnect_adapter();
        }
        if self.editor.component.is_some() && self.connection_state() == "bound" {
            ui.checkbox(
                &mut self.editor.ai_status.paused,
                Text::PauseAiEdits.plain(),
            );
        }
        if !state.notice.is_empty() {
            ui.label(&state.notice);
        }
        if ui
            .button(Text::ConnectAnAiApp.plain())
            .on_hover_text(Text::NativeAddRotEngineTo.plain())
            .clicked()
        {
            self.editor.ai_apps.show();
        }
        self.trusted_apps_ui(ui);
        ui.add_space(8.0);
        ui.separator();
        let preferences = if let Some(component) = &mut self.editor.component {
            component.ai_workspace()
        } else {
            &mut self.editor.workspace
        };
        let mut allow = preferences.prefs.allow_ai_connections;
        if ui
            .checkbox(&mut allow, Text::AllowAiAppsToConnect.plain())
            .on_hover_text(Text::NativeWhenOffThisEditor.plain())
            .changed()
        {
            preferences.prefs.allow_ai_connections = allow;
            preferences.changed();
            preferences.flush();
        }
        // Details for troubleshooting a connection, kept out of the way.
        ui.collapsing(Text::Diagnostics.plain(), |ui| {
            ui.label(format!("Scene file: {}", self.editor.document_label()));
            ui.label(format!(
                "Scene ID: {}",
                self.native_scene_observation().scene_id
            ));
            ui.label(format!(
                "Editor session: {}",
                self.editor.connection_incarnation
            ));
            Self::scene_observation_ui(ui, &self.native_scene_observation(), "");
            if let Some(bridge) = &self.bridge {
                ui.label(format!("Listening at: {}", bridge.locator_path().display()));
            }
            if let Some((_, pid)) = self.pending_adapter() {
                ui.label(format!("AI app process: {pid}"));
            }
        });
        if let Some(preview) = state.preview.clone() {
            egui::Modal::new(egui::Id::unique((
                "ai_binding_consent",
                &preview.incarnation,
            )))
            .show(&ctx, |ui| {
                ui.heading(if preview.owner.is_some() {
                    Text::AllowThisAiApp.plain()
                } else {
                    Text::StartListeningForAiApps.plain()
                });
                ui.label(format!(
                    "It can read and change {} until it disconnects.",
                    self.editor.document_label()
                ));
                if let Some((_, pid)) = &preview.owner {
                    ui.weak(format!("AI app process {pid}"));
                }
                let (cancel, confirm) = ui
                    .horizontal_wrapped(|ui| {
                        let cancel = ui.button(Text::ComponentCancel.plain());
                        let confirm = crate::editor_theme::primary_button(
                            ui,
                            if preview.owner.is_some() {
                                "Allow"
                            } else {
                                "Start"
                            },
                        );
                        (cancel, confirm)
                    })
                    .inner;
                if state.focus_cancel {
                    cancel.request_focus();
                    state.focus_cancel = false;
                }
                let escape = ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                if cancel.clicked() || escape {
                    state.preview = None;
                } else if confirm.clicked() {
                    // Taking the frozen preview consumes consent, even on stale refusal.
                    let accepted = self.confirm_binding(state.preview.take().unwrap(), &ctx);
                    state.notice = if accepted {
                        ""
                    } else {
                        Text::NativeTheRequestChangedOr.plain()
                    }
                    .into();
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

    fn disk_bytes(host: &Host) -> Vec<u8> {
        let path = host.editor.component.as_ref().unwrap().ai_document().0;
        std::fs::read(path).unwrap()
    }

    #[test]
    fn real_widget_consent_cancels_then_approves_exact_socket_owner_and_disconnects() {
        let ctx = egui::Context::default();
        let mut host = super::super::tests::host("native-consent");
        let before = disk_bytes(&host);
        let mut state = NativeUi::default();
        click(&mut host, &ctx, &mut state, "Start Listening");
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
        click(&mut host, &ctx, &mut state, "Start Listening");
        click(&mut host, &ctx, &mut state, "Start");
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
        click(&mut host, &ctx, &mut state, "Review Request…");
        click(&mut host, &ctx, &mut state, "Cancel");
        assert!(!host.bridge.as_ref().unwrap().approved());
        click(&mut host, &ctx, &mut state, "Review Request…");
        click(&mut host, &ctx, &mut state, "Allow");
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
        click(&mut host, &ctx, &mut state, "Allow Browser Once");
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
        click(&mut host, &ctx, &mut state, "Disconnect");
        assert_eq!(host.connection_state(), "revoked_draining");
        assert!(!host.editor.connection_authority.as_ref().unwrap().live());
        drop(stream);
        super::super::tests::retire(&mut host, &ctx);
        assert!(!locator_path.exists());
        assert_eq!(before, disk_bytes(&host));
    }

    #[test]
    fn prelaunch_scene_observation_matches_tool_guards_and_paints_without_authority() {
        let ctx = egui::Context::default();
        let mut host = super::super::tests::host("scene-observation");
        let before = disk_bytes(&host);
        let observation = host.native_scene_observation();
        assert!(observation.owner.is_none());
        assert_eq!(observation.connection_state, "retired");
        assert_ne!(observation.session_token, observation.incarnation);
        let token = observation.session_token.clone();
        let inspected =
            host.editor
                .component
                .as_mut()
                .unwrap()
                .ai_command(&token, "scene_inspect", &json!({}));
        assert_eq!(
            inspected["result"]["session_token"],
            observation.session_token
        );
        assert_eq!(inspected["result"]["revision"], observation.revision);
        let mut state = NativeUi::default();
        assert!(!paint(&mut host, &ctx, &mut state, vec![])
            .iter()
            .any(|(text, _)| text.contains(&observation.session_token)));
        click(&mut host, &ctx, &mut state, "Diagnostics");
        click(&mut host, &ctx, &mut state, "Start Listening");
        let painted = paint(&mut host, &ctx, &mut state, vec![]);
        assert!(painted
            .iter()
            .any(|(text, _)| text == &format!("Session: {}", observation.session_token)));
        assert!(painted
            .iter()
            .any(|(text, _)| text == &format!("Scene version: {}", observation.revision)));
        assert!(host.bridge.is_none());
        assert!(host.editor.connection_authority.is_none());
        assert_eq!(before, disk_bytes(&host));
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
        let token = preview.observation.session_token.clone();
        let result = host.editor.component.as_mut().unwrap().ai_command(
            &token,
            "scene_create",
            &json!({
                "expected_session_token": preview.observation.session_token,
                "expected_revision": preview.observation.revision,
                "object_id": "observation-probe",
                "name": "Probe"
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
