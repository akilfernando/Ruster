//! Native ownership only. Worker leases own the map and per-tick latch.
use super::*;
use crate::behavior_worker::{InputLease, InputMeta, InputOrigin};
use ruster_scene::input::{ActionKind, ActionValue, InputKey};

#[derive(Default)]
pub(super) struct InputControl {
    lease: Option<InputLease>,
    intent: Option<InputMeta>,
    surface: Option<(egui::Id, egui::Rect)>,
    activation_frame: Option<u64>,
    last_button_frame: Option<u64>,
    last_frame: Option<u64>,
    release_pointer_gesture: bool,
    help_open: bool,
    help_focus: bool,
    help_activation_frame: Option<u64>,
    help_return_focus: Option<egui::Id>,
    help_close: Option<egui::Id>,
}
impl RuntimeUi {
    pub(crate) fn input_owned(&self) -> bool {
        self.input_control.lease.is_some() || self.input_control.intent.is_some()
    }
    pub(crate) fn release_input(&mut self, ctx: &egui::Context) {
        self.revoke_input_control();
        if let Some((id, _)) = self.input_control.surface {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
        }
    }
    fn revoke_input_control(&mut self) {
        if self.input_control.lease.take().is_some() || self.input_control.intent.take().is_some() {
            self.input_control.intent = None;
            if let Some(worker) = &self.worker {
                let _ = worker.revoke_input(worker.current_key());
            }
        }
    }
    // Called before editor shortcuts. Cancellation precedes publication; the
    // independent VM may finish a previously admitted immutable tick.
    pub(crate) fn input_begin(&mut self, ctx: &egui::Context, blocked: bool) {
        if !self.input_owned() {
            return;
        }
        let frame = ctx.cumulative_frame_nr();
        let outside_pointer = ctx.input(|input| {
            (input.pointer.any_pressed() || input.pointer.any_click())
                && input.pointer.interact_pos().is_none_or(|position| {
                    self.input_control
                        .surface
                        .is_none_or(|(_, rect)| !rect.contains(position))
                })
        });
        let modifier = |value: egui::Modifiers| value.ctrl || value.alt || value.mac_cmd;
        let cancel = blocked
            || !self.ready
            || !self.visible
            || self.stale
            || self.shutdown
            || !ctx.input(|input| input.focused)
            || self.debug_snapshot().is_some()
            || ctx.input(|input| {
                modifier(input.modifiers)
                    || input.events.iter().any(|event| match event {
                        egui::Event::Key { modifiers, .. }
                        | egui::Event::ModifiersChanged(modifiers) => modifier(*modifiers),
                        _ => false,
                    })
            })
            || egui::Popup::is_any_open(ctx)
            || ctx.input(|input| {
                input.pointer.button_down(egui::PointerButton::Secondary)
                    || input.pointer.button_down(egui::PointerButton::Middle)
                    || ((self.input_control.activation_frame != Some(frame))
                        && (input.pointer.any_pressed() || input.pointer.any_click())
                        && input.pointer.interact_pos().is_none_or(|position| {
                            self.input_control
                                .surface
                                .is_none_or(|(_, rect)| !rect.contains(position))
                        }))
            });
        let escape =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape {
            if outside_pointer {
                self.input_control.release_pointer_gesture = true;
            }
            self.release_input(ctx);
        } else if ctx.input(|input| input.key_pressed(egui::Key::Tab)) {
            // Keep the real widget as the traversal origin. Surrendering its
            // focus here would restart Tab navigation at the first control.
            self.revoke_input_control();
        }
    }
    pub(super) fn input_control_button(
        &mut self,
        ui: &mut egui::Ui,
        blocked: bool,
        has_actions: bool,
    ) {
        // Keep the release gesture through pointer-up, including when the
        // press and release arrive in separate UI frames. It cannot reacquire.
        if self.input_control.release_pointer_gesture
            && ui.input(|input| {
                !input.pointer.any_down()
                    && !input.pointer.any_released()
                    && !input.pointer.any_click()
            })
        {
            self.input_control.release_pointer_gesture = false;
        }
        let release_gesture = self.input_control.release_pointer_gesture;
        let enabled = has_actions
            && !blocked
            && self.ready
            && self.visible
            && self.playing
            && !self.shutdown
            && self.pending.is_none()
            && self.awaiting.is_none()
            && self.debug_snapshot().is_none()
            && self.input_control.surface.is_some();
        let owned = self.input_owned();
        let button = ui.add_enabled(enabled || owned, egui::Button::new(if owned || release_gesture { "Release controls (Esc)" } else { "Control runtime" }))
            .on_hover_text("Give the scene keyboard control. Escape releases it; Pause and Stop remain available.")
            .on_disabled_hover_text(if has_actions { "Reset, view and play the current runtime before claiming controls." } else { "Define an input action to enable scene keyboard controls." });
        if button.clicked()
            && self.input_control.last_button_frame != Some(ui.ctx().cumulative_frame_nr())
        {
            self.input_control.last_button_frame = Some(ui.ctx().cumulative_frame_nr());
            if owned || release_gesture {
                self.release_input(ui.ctx());
                return;
            }
            let Some(worker) = &self.worker else {
                return;
            };
            match worker.current_input_meta(worker.current_key()) {
                Ok(meta) => {
                    // This exact epoch also guards an explicit post-debugger
                    // Play→claim intent. A newer stop invalidates that intent.
                    self.input_control.intent = Some(meta);
                    self.input_control.activation_frame = Some(ui.ctx().cumulative_frame_nr());
                    if !meta.enabled {
                        if let Err(error) = self.queue_control(Operation::Play) {
                            self.release_input(ui.ctx());
                            self.status = error;
                        }
                    }
                }
                Err(error) => self.status = format!("Controls unavailable: {error:?}. Try again."),
            }
        }
    }

    pub(super) fn open_input_help(&mut self, ctx: &egui::Context) {
        self.release_input(ctx);
        self.input_control.help_open = true;
        self.input_control.help_focus = true;
        self.input_control.help_return_focus = None;
        self.input_control.help_close = None;
        self.input_control.help_activation_frame = Some(ctx.cumulative_frame_nr());
    }

    pub(super) fn input_help_menu_response(&mut self, response: &egui::Response) {
        if self.input_control.help_activation_frame == Some(response.ctx.cumulative_frame_nr()) {
            self.input_control.help_return_focus = Some(response.id);
        }
    }

    fn input_help_snapshot_current(&self, session: &Session, generation: u64) -> bool {
        let (Some(frame), Some(captured)) = (&self.frame, &self.captured) else {
            return false;
        };
        self.ready
            && !self.stale
            && !self.shutdown
            && self.current_run() == Some(frame.key)
            && frame.key.generation == generation
            && frame.key.authored_revision == session.scene().revision
            && captured.actions == session.authoring().actions
            && captured.bindings == session.authoring().bindings
    }

    pub(super) fn input_help_window(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        generation: u64,
    ) {
        if !self.input_control.help_open {
            return;
        }
        let current = self.input_help_snapshot_current(session, generation);
        let popup_at_start = egui::Popup::is_any_open(ctx);
        let activation =
            self.input_control.help_activation_frame == Some(ctx.cumulative_frame_nr());
        let pointer_gesture = ctx.input(|input| {
            input.pointer.any_pressed() || input.pointer.any_released() || input.pointer.any_click()
        });
        let navigation = if !activation
            && !popup_at_start
            && !pointer_gesture
            && ctx.input(|input| {
                input.focused
                    && !input.modifiers.any()
                    && !input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::WindowFocused(false)))
            })
            && self.input_control.help_close.is_some_and(|id| {
                ctx.memory(|memory| memory.focused()) == Some(id)
                    && ctx.read_response(id).is_some_and(|response| {
                        response.enabled()
                            && response.has_focus()
                            && response.interact_rect.contains_rect(response.rect)
                    })
            }) {
            ctx.input_mut(|input| {
                [
                    egui::Key::PageUp,
                    egui::Key::PageDown,
                    egui::Key::Home,
                    egui::Key::End,
                ]
                .into_iter()
                .find(|key| input.consume_key(egui::Modifiers::NONE, *key))
            })
        } else {
            None
        };
        let mut opened = true;
        let mut close = false;
        let mut focus = self.input_control.help_focus;
        egui::Window::new("Runtime input help")
            .id(egui::Id::unique("runtime_input_help"))
            .open(&mut opened).collapsible(false).default_width(520.0)
            .min_width(180.0).max_width((ctx.content_rect().width() - 32.0).clamp(180.0, 520.0))
            .max_height((ctx.content_rect().height() - 48.0).max(200.0))
            .show(ctx, |ui| {
                let hint = egui::WidgetText::from("Page Up/Down scroll · Home/End jump.")
                    .into_galley(ui, Some(egui::TextWrapMode::Wrap), ui.available_width(), egui::TextStyle::Body);
                let padding = egui::Margin::from(ui.spacing().button_padding).sum();
                let close_text = egui::WidgetText::from("Close input help")
                    .into_galley(ui, Some(egui::TextWrapMode::Wrap),
                        (ui.available_width() - padding.x).max(0.0), egui::TextStyle::Body);
                let close_height = (close_text.size().y + padding.y).max(ui.spacing().interact_size.y);
                // Reserve the measured footer in this retained Window before
                // sizing the body. Screen budget alone can clip its Close row.
                let footer_height = hint.size().y + close_height + 2.0 * ui.spacing().item_spacing.y;
                let body_height = (ui.available_height() - footer_height).max(0.0)
                    .min((ctx.content_rect().height() - 160.0).clamp(100.0, 720.0));
                egui::ScrollArea::vertical().id_salt("runtime_input_help_scroll")
                    .animated(false)
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        match navigation {
                            Some(egui::Key::PageUp) => ui.scroll_with_delta(egui::vec2(0.0, ui.clip_rect().height() * 0.8)),
                            Some(egui::Key::PageDown) => ui.scroll_with_delta(egui::vec2(0.0, -ui.clip_rect().height() * 0.8)),
                            Some(egui::Key::Home) => ui.scroll_to_cursor(Some(egui::Align::TOP)),
                            _ => {}
                        }
                        ui.heading("Scene controls");
                        if session.authoring().actions.is_empty() {
                            ui.label("No actions defined. Add actions in Input actions.");
                        }
                        ui.label("Shared keys drive every matching action. Escape releases scene control.");
                        for (id, definition) in &session.authoring().actions {
                            ui.separator();
                            ui.strong(format!("{} · {id}", definition.label.as_deref().unwrap_or(id)));
                            ui.label(action_hint(&definition.kind));
                        }
                        ui.separator();
                        ui.heading("Preview state");
                        if !current {
                            ui.label(if self.stale {
                                "Runtime snapshot stale. Reset to inspect the current scene and action map."
                            } else {
                                "Runtime snapshot unavailable for the current scene, map or run. Reset preview."
                            });
                            if let Some(frame) = &self.frame {
                                ui.label(format!("Retained preview tick {}", frame.tick));
                            }
                        } else {
                        ui.label(if self.input_owned() {
                            "Scene controls active. Escape releases ownership."
                        } else if self.playing {
                            "Preview playing with neutral input until Control runtime claims it."
                        } else {
                            "Preview paused. Ordinary Step uses a neutral input frame."
                        });
                        let frame = self.frame.as_ref().expect("current snapshot checked");
                        ui.label(format!("Committed preview tick {}", frame.tick));
                        if let Some(input) = frame.input.as_deref().filter(|input| {
                            input.receipt.tick == frame.tick
                                && input.frame.0.keys().eq(session.authoring().actions.keys())
                        }) {
                            let origin = match input.receipt.origin {
                                InputOrigin::Neutral => "neutral",
                                InputOrigin::Native => "native controls",
                                InputOrigin::Programmatic => "programmatic step",
                            };
                            ui.strong(format!("Last committed input · tick {} · {origin}", input.receipt.tick));
                            ui.label("Values describe the committed tick. Current physical-key state is separate.");
                            for (id, value) in &input.frame.0 {
                                ui.label(match value {
                                    ActionValue::Button(value) => format!("{id} · Held {} · Pressed {} · Released {}", value.held, value.pressed, value.released),
                                    ActionValue::Axis1d { value } => format!("{id} · Axis {value}"),
                                });
                            }
                        } else {
                            ui.label(if frame.tick == 0 { "Reset complete. No input tick committed." }
                                else { "Last committed input is unavailable for this snapshot." });
                        }
                        ui.separator();
                        ui.heading("Object transforms");
                        let bindings = &session.authoring().bindings;
                        let count = bindings.values().filter(|binding| binding.enabled).count();
                        if count == 0 { ui.label("No enabled behavior bindings."); }
                        if count > 16 { ui.label(format!("Showing the first 16 of {count} enabled bindings.")); }
                        for (id, binding) in bindings.iter().filter(|(_, binding)| binding.enabled).take(16) {
                            if let Some(object) = frame.scene.objects.iter().find(|object| Some(object.id.as_str()) == binding.object_id()) {
                                ui.strong(format!("{} · {id}", object.display_name()));
                                ui.label(format!("Position {:.3?}", object.transform.translation));
                                ui.label(format!("Rotation {:.3?}", object.transform.rotation));
                            } else {
                                ui.label(match binding.object_id() {
                                    Some(object)=>format!("{id} · Object {object} unavailable in this snapshot."),
                                    None=>format!("{id} · Application behavior"),
                                });
                            }
                        }
                        }
                        if navigation == Some(egui::Key::End) {
                            ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                        }
                    });
                ui.label(hint);
                let button = ui.button(close_text);
                self.input_control.help_close = Some(button.id);
                // Closing the opening menu can surrender focus later in the
                // activation pass. Hand it to Close once the popup is gone.
                let pointer_gesture = ui.input(|input| input.pointer.any_pressed()
                    || input.pointer.any_released() || input.pointer.any_click());
                if focus && !activation && !popup_at_start && !egui::Popup::is_any_open(ctx) {
                    let available = ui.memory(|memory| memory.focused().is_none_or(|id|
                        id == button.id || Some(id) == self.input_control.help_return_focus));
                    if available && !pointer_gesture && ui.input(|input| input.focused) {
                        button.request_focus();
                    }
                    focus = false;
                }
                // Escape clears ordinary focus at begin_pass. Only the real
                // Close owner from the preceding frame may bridge that clear.
                let owns = button.has_focus() || ui.memory(|memory|
                    memory.focused().is_none() && memory.had_focus_last_frame(button.id));
                if !activation && !popup_at_start && !egui::Popup::is_any_open(ctx) {
                    close = button.clicked() || (owns && !pointer_gesture && ui.input_mut(|input|
                        input.focused && input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)));
                    if close {
                        if let Some(id) = self.input_control.help_return_focus {
                            ui.memory_mut(|memory| memory.request_focus(id));
                        }
                    }
                }
            });
        self.input_control.help_focus = focus;
        self.input_control.help_open = opened && !close;
        if !self.input_control.help_open {
            self.input_control.help_close = None;
        }
    }
    pub(crate) fn input_finish(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        blocked: bool,
    ) {
        self.input_control.surface = Some((response.id, response.rect));
        if !self.input_owned() {
            return;
        }
        self.input_begin(ctx, blocked);
        if !self.input_owned() {
            return;
        }
        let frame = ctx.cumulative_frame_nr();
        if let Some(intent) = self.input_control.intent {
            let result = self
                .worker
                .as_ref()
                .ok_or_else(|| "Reset before controlling the runtime.".to_owned())
                .and_then(|worker| {
                    let meta = worker
                        .current_input_meta(intent.key)
                        .map_err(|error| format!("{error:?}"))?;
                    if meta.epoch != intent.epoch || meta.map_identity != intent.map_identity {
                        return Err("Control request expired; choose Control runtime again.".into());
                    }
                    if !meta.enabled || self.pending.is_some() || self.awaiting.is_some() {
                        return Ok(None);
                    }
                    worker
                        .claim_input(intent.key, intent.map_identity)
                        .map(Some)
                        .map_err(|error| format!("{error:?}"))
                });
            match result {
                Ok(Some(lease)) => {
                    self.input_control.intent = None;
                    self.input_control.lease = Some(lease);
                    self.input_control.activation_frame = Some(frame);
                    response.request_focus();
                }
                Ok(None) => return,
                Err(error) => {
                    self.release_input(ctx);
                    self.status = error;
                    return;
                }
            }
        }
        if self.input_control.activation_frame == Some(frame) {
            // A discarded activation pass can rebuild the same actual surface.
            response.request_focus();
        }
        if response.has_focus() {
            let arrow_navigation = ctx.input(|input| {
                !input.key_pressed(egui::Key::Tab)
                    && !input.key_pressed(egui::Key::Escape)
                    && input.events.iter().any(|event| matches!(event,
                        egui::Event::Key { key: egui::Key::ArrowLeft | egui::Key::ArrowRight | egui::Key::ArrowUp | egui::Key::ArrowDown, pressed: true, modifiers, .. } if modifiers.is_none()))
            });
            ctx.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        ..Default::default()
                    },
                );
                // The first frame after activation began with the old filter.
                // Cancel only its queued arrow navigation, never Tab/Escape.
                if arrow_navigation {
                    memory.move_focus(egui::FocusDirection::None);
                }
            });
        }
        if self.input_control.activation_frame == Some(frame) {
            self.input_control.last_frame = Some(frame);
            return;
        }
        if !response.has_focus()
            || self.pending.is_some()
            || self.awaiting.is_some()
            || !self.playing
        {
            self.release_input(ctx);
            return;
        }
        if self.input_control.last_frame == Some(frame) {
            return;
        }
        self.input_control.last_frame = Some(frame);
        let events: Vec<_> = ctx.input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed,
                        repeat: false,
                        ..
                    } => InputKey::from_logical_name(key.name()).map(|key| (key, *pressed)),
                    _ => None,
                })
                .take(257)
                .collect()
        });
        if events.len() > 256 {
            self.release_input(ctx);
            self.status = "Keyboard input overflow; choose Control runtime again.".into();
            return;
        }
        let (Some(worker), Some(lease)) = (&self.worker, self.input_control.lease) else {
            return;
        };
        // A fresh metadata read detects debug/Reset revocation even in an eventless
        // UI frame. A failed event publication must never leave held state armed.
        let meta = match worker.current_input_meta(lease.key) {
            Err(crate::behavior_worker::InputError::Busy) if events.is_empty() => return,
            result => result,
        };
        let result = meta.map_err(|error| format!("{error:?}")).and_then(|meta| {
            if meta.epoch != lease.epoch || !meta.enabled || !meta.claimed {
                return Err("Scene controls were suspended.".into());
            }
            for (key, pressed) in events {
                worker
                    .publish_key_event(lease, key, pressed)
                    .map_err(|error| format!("{error:?}"))?;
            }
            Ok(())
        });
        if let Err(error) = result {
            self.release_input(ctx);
            self.status = format!("{error} Choose Control runtime again.");
        }
    }
}

fn action_hint(kind: &ActionKind) -> String {
    fn names(keys: &[InputKey]) -> String {
        if keys.is_empty() {
            "Unbound".into()
        } else {
            keys.iter()
                .map(|key| key.logical_name())
                .collect::<Vec<_>>()
                .join(" / ")
        }
    }
    match kind {
        ActionKind::Button { keys } => format!("Button · {}", names(keys)),
        ActionKind::Axis1d { negative, positive } => {
            format!("Axis · − {} · + {}", names(negative), names(positive))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::input::{ActionDefinition, ActionKind};
    use std::time::{Duration, Instant};

    // This is the distinct editor boundary: its worker runs independently and
    // early outside cancellation must not turn a Release click into a new claim.
    #[test]
    fn real_control_gesture_releases_across_frames_and_survives_layout_replay() {
        // An action-map edit must not consume another row of the ordinary
        // toolbar and therefore resize the real remaining viewport Response.
        // This layout check needs no worker or GPU resources.
        fn remaining_viewport(
            session: &Session,
            stale: bool,
            size: egui::Vec2,
            queries: bool,
        ) -> egui::Rect {
            let ctx = egui::Context::default();
            crate::editor_theme::configure(&ctx);
            let mut runtime = RuntimeUi {
                stale,
                ..Default::default()
            };
            if queries {
                use ruster_behavior::{
                    spatial::{OverlapResult, SweepOutcome, SweepResult},
                    QueryObservation, QueryResult,
                };
                let mut snapshot = session.scene().clone();
                snapshot.objects = ["probe", "wall", "sensor"]
                    .map(|id| ruster_scene::Object::cube(id, Some("n".repeat(128))))
                    .into();
                runtime.visible = true;
                runtime.frame = Some(Frame {
                    commit_revision: 0,
                    last_step_commit_revision: None,
                    commit_revision_required: false,
                    command_schema_hash: None,
                    authoring: std::sync::Arc::new(Default::default()),
                    state: std::sync::Arc::new(ruster_behavior::RuntimeState::default()),
                    key: RunKey::default(),
                    tick: 1,
                    scene: Arc::new(snapshot),
                    input: None,
                    query_observations: Some(Arc::from([
                        QueryObservation {
                            binding_id: "motion".into(),
                            snapshot_tick: 0,
                            scene_revision: 0,
                            query: QueryResult::Sweep {
                                probe_id: "probe".into(),
                                delta: [1.0, 0.0, 0.0],
                                result: SweepResult {
                                    outcome: SweepOutcome::Blocked,
                                    suggested_translation: Some([0.0; 3]),
                                    fraction: 0.0,
                                    target_id: Some("wall".into()),
                                    point: None,
                                    normal: None,
                                    clearance: 0.0001,
                                },
                            },
                        },
                        QueryObservation {
                            binding_id: "motion".into(),
                            snapshot_tick: 0,
                            scene_revision: 0,
                            query: QueryResult::Overlap {
                                result: OverlapResult {
                                    probe_id: "probe".into(),
                                    sensor_id: "sensor".into(),
                                    position: [0.0; 3],
                                    overlapping: true,
                                    signed_distance: Some(0.0),
                                },
                            },
                        },
                    ])),
                });
            }
            let mut viewport = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| {
                    runtime.controls(ui, session, 1, false, "Ctrl+Shift+P");
                    let (_, response) =
                        ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
                    viewport = Some(response.rect);
                },
            );
            output.textures_delta.clear();
            viewport.unwrap()
        }
        for size in [egui::vec2(1000., 800.), egui::vec2(640., 480.)] {
            let mut layout = Session::new(ruster_scene::Scene::empty("input-layout")).unwrap();
            let empty = remaining_viewport(&layout, false, size, false);
            layout
                .set_action(0, "home", Some(ActionDefinition::default()))
                .unwrap();
            let first = remaining_viewport(&layout, false, size, false);
            let stale = remaining_viewport(&layout, true, size, false);
            layout.set_action(1, "home", None).unwrap();
            let last = remaining_viewport(&layout, false, size, false);
            assert_eq!(
                remaining_viewport(&layout, false, size, true),
                empty,
                "long movement/occupancy feedback preserves the viewport footprint"
            );
            assert_eq!(
                remaining_viewport(&layout, true, size, true),
                empty,
                "stale query feedback preserves the viewport footprint"
            );
            assert_eq!(
                first, empty,
                "first action must preserve the viewport footprint"
            );
            assert_eq!(
                stale, first,
                "Reset-required status stays within the existing Control row"
            );
            assert_eq!(
                last, empty,
                "last action removal must preserve the viewport footprint"
            );
        }

        let mut session = Session::new(ruster_scene::Scene::empty("input-focus")).unwrap();
        session
            .set_action(
                0,
                "home",
                Some(ActionDefinition {
                    label: None,
                    kind: ActionKind::Button {
                        keys: vec![InputKey::Space],
                    },
                }),
            )
            .unwrap();
        for id in ["fine", "raise", "lower"] {
            session
                .set_action(
                    session.scene().revision,
                    id,
                    Some(ActionDefinition::default()),
                )
                .unwrap();
        }
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let mut runtime = RuntimeUi::default();
        let key = runtime.reset(&ctx, &session, 1).unwrap();
        let wait = |runtime: &mut RuntimeUi, playing| {
            let until = Instant::now() + Duration::from_secs(5);
            while !runtime.ready
                || runtime.playing != playing
                || runtime.awaiting.is_some()
                || runtime.pending.is_some()
            {
                assert!(
                    Instant::now() < until,
                    "runtime acknowledgement deadline: {}",
                    runtime.status
                );
                runtime.poll(&ctx);
                std::thread::yield_now();
            }
        };
        wait(&mut runtime, false);
        runtime.pending = Some(Request::Play(key));
        runtime.flush();
        wait(&mut runtime, true);
        let mut text = String::new();
        let mut time = 0.;
        let width = std::cell::Cell::new(1000.0);
        let height = std::cell::Cell::new(800.0);
        let rendered = std::cell::RefCell::new(Vec::new());
        let controls_blocked = std::cell::Cell::new(false);
        let reset_admissions = std::cell::RefCell::new(Vec::new());
        let mut draw = |runtime: &mut RuntimeUi, events, discard: bool| {
            time += 0.1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width.get(), height.get()),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::Panel::right("input-test-inspector")
                        .default_size(100.)
                        .resizable(true)
                        .show(ui, |ui| {
                            ui.button("Inspector control").clicked();
                        });
                    runtime.input_begin(&ctx, false);
                    if let Some(key) =
                        runtime.controls(ui, &session, 1, controls_blocked.get(), "Ctrl+Shift+P")
                    {
                        reset_admissions.borrow_mut().push(key);
                    }
                    ui.text_edit_singleline(&mut text);
                    let (_, surface) = ui
                        .allocate_exact_size(egui::vec2(900., 500.), egui::Sense::click_and_drag());
                    runtime.input_finish(&ctx, &surface, false);
                    if discard && ctx.current_pass_index() == 0 {
                        ctx.request_discard("control activation replay");
                    }
                },
            );
            output.textures_delta.clear();
            let button = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text)
                        if matches!(
                            text.galley.job.text.as_str(),
                            "Control runtime" | "Release controls (Esc)"
                        ) =>
                    {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center())
                    }
                    _ => None,
                })
                .unwrap();
            *rendered.borrow_mut() = output.shapes;
            button
        };
        let mut button = egui::Pos2::ZERO;
        for _ in 0..3 {
            button = draw(&mut runtime, vec![], false);
        }
        let pointer = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        draw(
            &mut runtime,
            vec![
                egui::Event::PointerMoved(button),
                pointer(button, true),
                pointer(button, false),
            ],
            true,
        );
        assert!(
            runtime.input_owned(),
            "discarded activation cannot toggle ownership off"
        );
        assert!(
            runtime
                .worker
                .as_ref()
                .unwrap()
                .current_input_meta(key)
                .unwrap()
                .claimed
        );
        for arrow in [egui::Key::ArrowRight, egui::Key::ArrowUp] {
            draw(
                &mut runtime,
                vec![egui::Event::Key {
                    key: arrow,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                false,
            );
            assert!(
                runtime.input_owned(),
                "gameplay arrows must not navigate to the actual inspector splitter"
            );
            draw(
                &mut runtime,
                vec![egui::Event::Key {
                    key: arrow,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                false,
            );
        }
        for modifiers in [egui::Modifiers::NONE, egui::Modifiers::SHIFT] {
            button = draw(
                &mut runtime,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                false,
            );
            assert!(
                !runtime.input_owned(),
                "Tab must release gameplay ownership"
            );
            draw(
                &mut runtime,
                vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers,
                }],
                false,
            );
            assert!(
                ctx.memory(|memory| memory.focused()).is_some(),
                "Tab retains ordinary widget traversal after the next pass"
            );
            draw(
                &mut runtime,
                vec![
                    egui::Event::PointerMoved(button),
                    pointer(button, true),
                    pointer(button, false),
                ],
                true,
            );
            assert!(runtime.input_owned());
        }
        button = draw(&mut runtime, vec![], false);
        draw(
            &mut runtime,
            vec![egui::Event::PointerMoved(button), pointer(button, true)],
            false,
        );
        assert!(!runtime.input_owned(), "outside press releases immediately");
        draw(&mut runtime, vec![pointer(button, false)], true);
        assert!(
            !runtime.input_owned(),
            "release gesture cannot reacquire on pointer-up or discarded pass"
        );
        assert!(
            !runtime
                .worker
                .as_ref()
                .unwrap()
                .current_input_meta(key)
                .unwrap()
                .claimed
        );
        // The actual floating help must consume Escape after egui clears
        // focused() at begin_pass, including the compact window footprint.
        fn label_point(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
            fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.rect.center().to_vec2())
                    }
                    egui::epaint::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| find(shape, label))
                    }
                    _ => None,
                }
            }
            shapes
                .iter()
                .find_map(|shape| find(&shape.shape, label))
                .unwrap_or_else(|| panic!("visible input help control {label:?}"))
        }
        fn visible_label(shapes: &[egui::epaint::ClippedShape], label: &str) -> bool {
            fn find(shape: &egui::epaint::Shape, clip: egui::Rect, label: &str) -> bool {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        clip.contains_rect(text.galley.rect.translate(text.pos.to_vec2()))
                    }
                    egui::epaint::Shape::Vec(shapes) => {
                        shapes.iter().any(|shape| find(shape, clip, label))
                    }
                    _ => false,
                }
            }
            shapes
                .iter()
                .any(|shape| find(&shape.shape, shape.clip_rect, label))
        }
        for size in [egui::vec2(1000.0, 800.0), egui::vec2(640.0, 480.0)] {
            width.set(size.x);
            height.set(size.y);
            for _ in 0..3 {
                draw(&mut runtime, vec![], false);
            }
            for close_key in [egui::Key::Escape, egui::Key::Enter] {
                let menu = label_point(&rendered.borrow(), "Runtime");
                draw(&mut runtime, vec![egui::Event::PointerMoved(menu)], false);
                draw(&mut runtime, vec![pointer(menu, true)], false);
                draw(&mut runtime, vec![pointer(menu, false)], false);
                draw(&mut runtime, vec![], false);
                let item = label_point(&rendered.borrow(), "Runtime input help…");
                draw(&mut runtime, vec![egui::Event::PointerMoved(item)], false);
                draw(&mut runtime, vec![pointer(item, true)], false);
                draw(&mut runtime, vec![pointer(item, false)], false);
                draw(&mut runtime, vec![], false);
                let close = label_point(&rendered.borrow(), "Close input help");
                let focused = ctx.memory(|memory| memory.focused()).and_then(|id| ctx.read_response(id))
                    .expect("opening the actual Runtime menu help must hand keyboard focus to Close after the popup settles");
                assert!(
                    focused.has_focus() && focused.rect.contains(close),
                    "genuine opening focuses the visible Close button"
                );
                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains(close));
                let close_id = focused.id;
                for navigation in [
                    egui::Key::Home,
                    egui::Key::End,
                    egui::Key::PageUp,
                    egui::Key::PageDown,
                    egui::Key::Home,
                ] {
                    draw(
                        &mut runtime,
                        [true, false]
                            .into_iter()
                            .map(|pressed| egui::Event::Key {
                                key: navigation,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers: egui::Modifiers::NONE,
                            })
                            .collect(),
                        false,
                    );
                    draw(&mut runtime, vec![], false);
                    let owner = ctx.read_response(close_id).unwrap();
                    assert!(
                        owner.enabled()
                            && owner.has_focus()
                            && owner.interact_rect.contains_rect(owner.rect),
                        "help scrolling retains the actual visible Close owner at {size:?}"
                    );
                    assert!(visible_label(
                        &rendered.borrow(),
                        "Page Up/Down scroll · Home/End jump."
                    ));
                    if navigation == egui::Key::Home {
                        assert!(visible_label(&rendered.borrow(), "Scene controls"));
                    }
                    if navigation == egui::Key::End {
                        assert!(visible_label(&rendered.borrow(), "Object transforms"));
                    }
                    if navigation == egui::Key::PageDown && size.y == 480.0 {
                        assert!(
                            !visible_label(&rendered.borrow(), "Scene controls"),
                            "Page Down moves actual compact help content"
                        );
                    }
                }
                draw(
                    &mut runtime,
                    vec![egui::Event::Key {
                        key: close_key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    false,
                );
                assert!(
                    !runtime.input_control.help_open,
                    "{close_key:?} closes actual owned help at {size:?}"
                );
                draw(
                    &mut runtime,
                    vec![egui::Event::Key {
                        key: close_key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    false,
                );
            }
        }
        // Exercise the actual toolbar/worker boundary. The blocked argument
        // models component-level asset retirement without a GPU fixture. Real
        // acknowledgements are polled separately, while disabled frames are
        // rendered first so fast worker completion cannot hide focus loss.
        let authored = session.snapshot_bytes().unwrap();
        let key_event = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let focused_control = |label: &str| {
            assert!(visible_label(&rendered.borrow(), label));
            let point = label_point(&rendered.borrow(), label);
            let response = ctx
                .memory(|memory| memory.focused())
                .and_then(|id| ctx.read_response(id))
                .unwrap_or_else(|| panic!("{label} must retain actual widget focus"));
            assert!(
                response.enabled() && response.has_focus() && response.rect.contains(point),
                "{label} must own its enabled, visible Response after acknowledgement"
            );
            assert!(response.interact_rect.contains_rect(response.rect));
            response.id
        };
        runtime.focus_controls = true;
        draw(&mut runtime, vec![], false);
        let reset_id = focused_control("Reset");
        draw(&mut runtime, vec![key_event(egui::Key::Enter, true)], true);
        assert_eq!(reset_admissions.borrow().len(), 1);
        let reset_key = runtime.worker.as_ref().unwrap().current_key();
        controls_blocked.set(true);
        draw(
            &mut runtime,
            vec![key_event(egui::Key::Enter, false)],
            false,
        );
        for _ in 0..2 {
            draw(&mut runtime, vec![], false);
        }
        wait(&mut runtime, false);
        assert_eq!(runtime.frame.as_ref().unwrap().tick, 0);
        controls_blocked.set(false);
        for _ in 0..2 {
            draw(&mut runtime, vec![], false);
        }
        assert_eq!(
            focused_control("Reset"),
            reset_id,
            "Reset completion must restore its genuine activation owner after blocking"
        );
        assert_eq!(
            reset_admissions.borrow().len(),
            1,
            "Enter release/idle cannot admit another Reset"
        );
        assert_eq!(runtime.worker.as_ref().unwrap().current_key(), reset_key);

        draw(&mut runtime, vec![key_event(egui::Key::Tab, true)], false);
        draw(&mut runtime, vec![key_event(egui::Key::Tab, false)], false);
        let play_id = focused_control("Play");
        for playing in [true, false] {
            draw(&mut runtime, vec![key_event(egui::Key::Enter, true)], false);
            assert!(runtime.awaiting.is_some() || runtime.pending.is_some());
            controls_blocked.set(true);
            draw(
                &mut runtime,
                vec![key_event(egui::Key::Enter, false)],
                false,
            );
            for _ in 0..2 {
                draw(&mut runtime, vec![], false);
            }
            wait(&mut runtime, playing);
            controls_blocked.set(false);
            for _ in 0..2 {
                draw(&mut runtime, vec![], false);
            }
            assert_eq!(
                focused_control(if playing { "Pause" } else { "Play" }),
                play_id,
                "Play/Pause acknowledgement must preserve the activated button identity"
            );
            assert_eq!(runtime.worker.as_ref().unwrap().current_key(), reset_key);
            assert_eq!(
                runtime.playing, playing,
                "Enter release cannot toggle again"
            );
            assert_eq!(reset_admissions.borrow().len(), 1);
        }
        draw(&mut runtime, vec![key_event(egui::Key::Tab, true)], false);
        draw(&mut runtime, vec![key_event(egui::Key::Tab, false)], false);
        let step_id = focused_control("Step");
        let tick = runtime.frame.as_ref().unwrap().tick;
        draw(&mut runtime, vec![key_event(egui::Key::Enter, true)], false);
        controls_blocked.set(true);
        draw(
            &mut runtime,
            vec![key_event(egui::Key::Enter, false)],
            false,
        );
        draw(&mut runtime, vec![], false);
        wait(&mut runtime, false);
        controls_blocked.set(false);
        for _ in 0..2 {
            draw(&mut runtime, vec![], false);
        }
        assert_eq!(focused_control("Step"), step_id);
        assert_eq!(
            runtime.frame.as_ref().unwrap().tick,
            tick + 1,
            "one genuine Enter admits exactly one paused Step"
        );
        assert_eq!(runtime.worker.as_ref().unwrap().current_key(), reset_key);
        draw(&mut runtime, vec![key_event(egui::Key::Tab, true)], false);
        draw(&mut runtime, vec![key_event(egui::Key::Tab, false)], false);
        let stop_id = focused_control("Stop");
        draw(&mut runtime, vec![key_event(egui::Key::Enter, true)], false);
        controls_blocked.set(true);
        draw(
            &mut runtime,
            vec![key_event(egui::Key::Enter, false)],
            false,
        );
        let until = Instant::now() + Duration::from_secs(5);
        while runtime.ready || runtime.pending.is_some() || runtime.awaiting.is_some() {
            assert!(
                Instant::now() < until,
                "Stop acknowledgement deadline: {}",
                runtime.status
            );
            runtime.poll(&ctx);
            std::thread::yield_now();
        }
        let stop_key = runtime.worker.as_ref().unwrap().current_key();
        controls_blocked.set(false);
        for _ in 0..2 {
            draw(&mut runtime, vec![], false);
        }
        assert_eq!(focused_control("Stop"), stop_id);
        assert_eq!(runtime.worker.as_ref().unwrap().current_key(), stop_key);
        assert_eq!(runtime.frame.as_ref().unwrap().tick, tick + 1);
        assert!(!runtime.playing);
        assert_eq!(session.snapshot_bytes().unwrap(), authored);

        // Genuine Tab traversal to the Inspector cancels the outstanding
        // continuity request. Completion must not reclaim the old focus.
        runtime.focus_controls = true;
        draw(&mut runtime, vec![], false);
        focused_control("Reset");
        draw(&mut runtime, vec![key_event(egui::Key::Enter, true)], false);
        controls_blocked.set(true);
        draw(
            &mut runtime,
            vec![key_event(egui::Key::Enter, false)],
            false,
        );
        draw(&mut runtime, vec![key_event(egui::Key::Tab, true)], false);
        draw(&mut runtime, vec![key_event(egui::Key::Tab, false)], false);
        let inspector_id = focused_control("Inspector control");
        wait(&mut runtime, false);
        controls_blocked.set(false);
        for _ in 0..2 {
            draw(&mut runtime, vec![], false);
        }
        assert_eq!(
            focused_control("Inspector control"),
            inspector_id,
            "a completed operation cannot restore stale focus over a genuine new owner"
        );
        assert_eq!(reset_admissions.borrow().len(), 2);
        assert_eq!(session.snapshot_bytes().unwrap(), authored);
        runtime.shutdown();
        let until = Instant::now() + Duration::from_secs(5);
        while !runtime.finished() {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
    }
}
