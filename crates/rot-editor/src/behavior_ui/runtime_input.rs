//! Native ownership only. Worker leases own the map and per-tick latch.
use super::*;
use crate::behavior_worker::{InputLease, InputMeta, InputOrigin};
use crate::strings::Text;
use rot_scene::input::{ActionKind, ActionValue, InputKey};

fn focus_lost(focused: bool, events: &[egui::Event]) -> bool {
    !focused
        || events
            .iter()
            .any(|event| matches!(event, egui::Event::WindowFocused(false)))
}

#[derive(Default)]
pub(super) struct InputControl {
    lease: Option<InputLease>,
    native: rot_app::NativeActionInput,
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
    pub(crate) fn input_raw_focus(&mut self, ctx: &egui::Context, raw: &egui::RawInput) {
        if focus_lost(raw.focused, &raw.events) {
            self.release_input(ctx);
            self.cancel_control_focus();
        }
    }

    pub(crate) fn release_input(&mut self, ctx: &egui::Context) {
        self.revoke_input_control();
        if let Some((id, _)) = self.input_control.surface {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
        }
    }
    fn revoke_input_control(&mut self) {
        self.input_control.native.clear();
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
                && !self.input_control.native.preserves_release(&input.events)
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
            || ctx.input(|input| focus_lost(input.focused, &input.events))
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
                    || (self.input_control.activation_frame != Some(frame) && outside_pointer)
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
    /// Clicking the running Game view gives it the keyboard; Escape gives it
    /// back. `clicked` is the view's click this frame.
    pub(crate) fn input_control_click(
        &mut self,
        ctx: &egui::Context,
        blocked: bool,
        has_actions: bool,
        clicked: bool,
    ) {
        let ui = ctx;
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
        if clicked
            && (enabled || owned)
            && !ctx.input(|input| focus_lost(input.focused, &input.events))
            && self.input_control.last_button_frame != Some(ui.cumulative_frame_nr())
        {
            self.input_control.last_button_frame = Some(ui.cumulative_frame_nr());
            if owned || release_gesture {
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
                    self.input_control.activation_frame = Some(ui.cumulative_frame_nr());
                    if !meta.enabled {
                        if let Err(error) = self.queue_control(Operation::Play) {
                            self.release_input(ui);
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

    fn input_help_snapshot_current(&self, source: RuntimeSource<'_>, generation: u64) -> bool {
        let (Some(frame), Some(captured)) = (&self.frame, &self.captured) else {
            return false;
        };
        self.ready
            && !self.stale
            && !self.shutdown
            && self.current_run() == Some(frame.key)
            && frame.key.generation == generation
            && frame.key.authored_revision == source.revision()
            && (self
                .transition_source
                .as_ref()
                .is_some_and(|id| id != &frame.scene.id)
                || (captured.actions == *source.actions() && source.bindings_match(captured)))
    }

    pub(super) fn input_help_window(
        &mut self,
        ctx: &egui::Context,
        source: RuntimeSource<'_>,
        generation: u64,
    ) {
        if !self.input_control.help_open {
            return;
        }
        let current = self.input_help_snapshot_current(source, generation);
        let captured = current.then(|| self.captured.clone().expect("snapshot requires map"));
        let actions = captured
            .as_ref()
            .map_or_else(|| source.actions(), |view| &view.actions);
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
        egui::Window::new(Text::RuntimeInputHelp.plain())
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
                        ui.heading(Text::SceneControls.plain());
                        if actions.is_empty() {
                            ui.label(Text::RuntimeInputNoActionsDefinedAdd.plain());
                        }
                        ui.label(Text::RuntimeInputSharedKeysDriveEvery.plain());
                        for (id, definition) in actions {
                            ui.separator();
                            ui.strong(format!("{} · {id}", definition.label.as_deref().unwrap_or(id)));
                            ui.weak(action_hint(&definition.kind));
                        }
                        ui.separator();
                        ui.heading(Text::PreviewState.plain());
                        if !current {
                            ui.label(if self.stale {
                                Text::GameControlsPreviousPlay.plain()
                            } else {
                                Text::GameControlsNoPlay.plain()
                            });
                            if let Some(frame) = &self.frame {
                                ui.label(format!("Retained preview tick {}", frame.tick));
                            }
                        } else {
                        ui.label(if self.input_owned() {
                            Text::GameControlsActive.plain()
                        } else if self.playing {
                            Text::GameControlsClickGame.plain()
                        } else {
                            Text::GameControlsPaused.plain()
                        });
                        let frame = self.frame.as_ref().expect("current snapshot checked");
                        ui.label(format!("Committed preview tick {}", frame.tick));
                        if let Some(input) = frame.input.as_deref().filter(|input| {
                            input.receipt.tick == frame.tick
                                && input.frame.0.keys().eq(actions.keys())
                        }) {
                            let origin = match input.receipt.origin {
                                InputOrigin::Neutral => "neutral",
                                InputOrigin::Native => "native controls",
                                InputOrigin::Programmatic => "programmatic step",
                            };
                            ui.strong(format!("Last committed input · tick {} · {origin}", input.receipt.tick));
                            ui.weak(Text::RuntimeInputTheseValuesDescribeInput.plain());
                            for (id, value) in &input.frame.0 {
                                ui.label(match value {
                                    ActionValue::Button(value) => format!("{id} · Held {} · Pressed {} · Released {}", value.held, value.pressed, value.released),
                                    ActionValue::Axis1d { value } => format!("{id} · Axis {value}"),
                                });
                            }
                        } else {
                            ui.label(if frame.tick == 0 { Text::RuntimeInputNoInputHasBeen.plain() }
                                else { Text::RuntimeInputInputIsUnavailableFor.plain() });
                        }
                        ui.separator();
                        ui.heading(if frame.scene.hierarchy.is_empty() {
                            Text::ObjectTransforms.plain()
                        } else {
                            Text::GroupTransforms.plain()
                        });
                        let bindings = &frame.authoring.bindings;
                        let count = bindings.values().filter(|binding| binding.enabled).count();
                        if count == 0 { ui.label(Text::NoScriptsAreEnabled.plain()); }
                        if count > 16 { ui.label(format!("Showing 16 of {count} enabled script instances.")); }
                        for (id, binding) in bindings.iter().filter(|(_, binding)| binding.enabled).take(16) {
                            match &binding.target {
                                rot_scene::BindingTarget::Object(owner) => {
                                    if let Some(object) = frame.scene.objects.iter().find(|object| object.id == *owner) {
                                        ui.strong(format!("{} · {id}", object.display_name()));
                                        ui.label(format!("Position {:.3?}", object.transform.translation));
                                        ui.label(format!("Rotation {:.3?}", object.transform.rotation));
                                    } else {
                                        ui.label(format!("{id} · Object {owner} unavailable in this snapshot."));
                                    }
                                }
                                rot_scene::BindingTarget::Group(owner) => {
                                    if let Some(group) = frame.scene.hierarchy.groups.get(owner) {
                                        ui.strong(format!("{} · {id}", group.name.as_deref().unwrap_or(owner)));
                                        ui.label(format!("Group-local rotation (radians) {:.3?}", group.transform.rotation));
                                    } else {
                                        ui.label(format!("{id} · Group {owner} unavailable in this snapshot."));
                                    }
                                }
                                rot_scene::BindingTarget::Application => {
                                    ui.label(format!("{id} · Application script"));
                                }
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
        primary_content: Option<egui::Rect>,
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
                        return Err("Control request expired; click the game view again.".into());
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
        let events = ctx.input(|input| {
            self.input_control
                .native
                .events(&input.events, primary_content)
        });
        if events.len() > 256 {
            self.release_input(ctx);
            self.status = Text::RuntimeInputKeyboardInputOverflowClick.plain().into();
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
            let mut lease = lease;
            lease.map_identity = meta.map_identity;
            for (key, pressed) in events {
                let mut result = worker.publish_physical_event(lease, key, pressed);
                if result == Err(crate::behavior_worker::InputError::StaleMap) {
                    let current = worker
                        .current_input_meta(lease.key)
                        .map_err(|error| format!("{error:?}"))?;
                    if current.epoch == lease.epoch && current.enabled && current.claimed {
                        lease.map_identity = current.map_identity;
                        result = worker.publish_physical_event(lease, key, pressed);
                    }
                }
                result.map_err(|error| format!("{error:?}"))?;
            }
            self.input_control.lease = Some(lease);
            Ok(())
        });
        if let Err(error) = result {
            self.release_input(ctx);
            self.status = format!("{error} Click the game view again.");
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
        ActionKind::Button {
            keys,
            primary_mouse,
        } => match (keys.is_empty(), primary_mouse) {
            (true, true) => "Button · Primary Mouse".into(),
            (false, true) => format!("Button · {} / Primary Mouse", names(keys)),
            (_, false) => format!("Button · {}", names(keys)),
        },
        ActionKind::Axis1d { negative, positive } => {
            format!("Axis · − {} · + {}", names(negative), names(positive))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::input::{ActionDefinition, ActionKind};
    use std::time::{Duration, Instant};

    #[test]
    fn component_game_focus_loss_revokes_keys_even_when_final_focus_is_true() {
        use rot_scene::component_scene::{ComponentScene, ComponentSession};

        let legacy = Session::new(rot_scene::Scene::empty("focus")).unwrap();
        let mut scene =
            ComponentSession::new(ComponentScene::from_legacy(&legacy).unwrap()).unwrap();
        scene
            .set_action(
                0,
                "move",
                Some(ActionDefinition {
                    label: None,
                    kind: ActionKind::Axis1d {
                        negative: vec![InputKey::A],
                        positive: vec![InputKey::D],
                    },
                }),
            )
            .unwrap();
        let ctx = egui::Context::default();
        let mut runtime = RuntimeUi::default();
        let key = runtime
            .reset_source(&ctx, RuntimeSource::Components(&scene, None), 1, None)
            .unwrap();
        let wait = |runtime: &mut RuntimeUi, ready: fn(&RuntimeUi) -> bool| {
            let deadline = Instant::now() + Duration::from_secs(5);
            while !ready(runtime) {
                assert!(Instant::now() < deadline, "runtime: {}", runtime.status);
                runtime.poll(&ctx);
                std::thread::yield_now();
            }
        };
        wait(&mut runtime, |runtime| runtime.ready);
        let (tick_reached, resume_tick) =
            runtime.worker.as_ref().unwrap().gate_input_tick_for_test();
        runtime.pending = Some(Request::Play(key));
        runtime.flush();
        wait(&mut runtime, |runtime| runtime.playing);
        tick_reached.recv_timeout(Duration::from_secs(5)).unwrap();

        for raw_hook in [true, false] {
            let worker = runtime.worker.as_ref().unwrap();
            let meta = worker.current_input_meta(key).unwrap();
            let lease = worker.claim_input(key, meta.map_identity).unwrap();
            worker.publish_key_event(lease, InputKey::A, true).unwrap();
            runtime.input_control.lease = Some(lease);
            let raw = egui::RawInput {
                focused: true,
                events: vec![
                    egui::Event::WindowFocused(false),
                    egui::Event::WindowFocused(true),
                ],
                ..Default::default()
            };
            if raw_hook {
                runtime.input_raw_focus(&ctx, &raw);
            } else {
                ctx.run_ui(raw, |_| {
                    assert!(ctx.input(|input| input.focused));
                    runtime.input_begin(&ctx, false);
                })
                .textures_delta
                .clear();
            }
            assert!(!runtime.input_owned());
            let worker = runtime.worker.as_ref().unwrap();
            let revoked = worker.current_input_meta(key).unwrap();
            assert!(!revoked.claimed && revoked.epoch > lease.epoch);
            assert_eq!(
                worker.publish_key_event(lease, InputKey::A, false),
                Err(crate::behavior_worker::InputError::StaleLease)
            );
        }
        resume_tick.send(()).unwrap();
        wait(&mut runtime, |runtime| {
            runtime.frame.as_ref().is_some_and(|frame| frame.tick > 0)
        });
        let input = runtime.frame.as_ref().unwrap().input.as_ref().unwrap();
        assert_eq!(input.receipt.origin, InputOrigin::Neutral);
        assert_eq!(input.frame.0["move"], ActionValue::Axis1d { value: 0 });
    }

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
                use rot_behavior::{
                    spatial::{OverlapResult, SweepOutcome, SweepResult},
                    QueryObservation, QueryResult,
                };
                let mut snapshot = session.scene().clone();
                snapshot.objects = ["probe", "wall", "sensor"]
                    .map(|id| rot_scene::Object::cube(id, Some("n".repeat(128))))
                    .into();
                runtime.visible = true;
                runtime.frame = Some(Frame {
                    commit_revision: 0,
                    last_step_commit_revision: None,
                    commit_revision_required: false,
                    draw_frame: None,
                    static_preview: None,
                    resolved_transform_camera: None,
                    hidden_mesh_renderers: None,
                    command_schema_hash: None,
                    authoring: std::sync::Arc::new(Default::default()),
                    state: std::sync::Arc::new(rot_behavior::RuntimeState::default()),
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
            let mut layout = Session::new(rot_scene::Scene::empty("input-layout")).unwrap();
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

        let mut session = Session::new(rot_scene::Scene::empty("input-focus")).unwrap();
        session
            .set_action(
                0,
                "home",
                Some(ActionDefinition {
                    label: None,
                    kind: ActionKind::Button {
                        keys: vec![InputKey::Space],
                        primary_mouse: false,
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
        // The focus/gesture contract must not compete with tick admission's
        // intentionally fail-closed input mutex. Keep Playing real, but park
        // the first tick before it acquires that mutex, using the worker gate.
        let (tick_reached, resume_tick) =
            runtime.worker.as_ref().unwrap().gate_input_tick_for_test();
        runtime.pending = Some(Request::Play(key));
        runtime.flush();
        wait(&mut runtime, true);
        tick_reached.recv_timeout(Duration::from_secs(5)).unwrap();
        let mut text = String::new();
        let mut time = 0.;
        let width = std::cell::Cell::new(1000.0);
        let height = std::cell::Cell::new(800.0);
        ctx.enable_accesskit();
        let rendered = std::cell::RefCell::new(Vec::new());
        let accessible_buttons = std::cell::RefCell::new(std::collections::BTreeMap::new());
        let controls_blocked = std::cell::Cell::new(false);
        let reset_admissions = std::cell::RefCell::new(Vec::new());
        let surface_focused_before_finish = std::cell::Cell::new(false);
        let surface_center = std::cell::Cell::new(egui::Pos2::ZERO);
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
                    surface_focused_before_finish.set(surface.has_focus());
                    surface_center.set(surface.rect.center());
                    // S-05: clicking the running game view claims the keyboard.
                    runtime.input_control_click(&ctx, false, true, surface.clicked());
                    runtime.input_finish(&ctx, &surface, false, None);
                    if discard && ctx.current_pass_index() == 0 {
                        ctx.request_discard("control activation replay");
                    }
                },
            );
            output.textures_delta.clear();
            // The click target is the game view itself (S-05).
            let button = surface_center.get();
            *accessible_buttons.borrow_mut() = output
                .platform_output
                .accesskit_update
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter(|(_, node)| node.role() == egui::accesskit::Role::Button)
                .filter_map(|(_, node)| Some((node.label()?.to_owned(), node.bounds()?)))
                .collect();
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
            // Capture UI-owned state only: a new worker read here can race the
            // publication under investigation and cannot explain its result.
            let focus_before = ctx.memory(|memory| memory.focused());
            let lease_before = runtime.input_control.lease;
            let status_before = runtime.status.clone();
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
                "gameplay arrow ownership lost: arrow={arrow:?} status_before={status_before:?} status={:?} focus_before={focus_before:?} focus_after={:?} surface={:?} lease_before={lease_before:?} lease_after={:?} intent={:?} playing={} pending={:?} awaiting={:?} ready={} visible={} stale={} shutdown={} activation_frame={:?} last_frame={:?} frame={}",
                runtime.status,
                ctx.memory(|memory| memory.focused()),
                runtime.input_control.surface,
                runtime.input_control.lease,
                runtime.input_control.intent,
                runtime.playing,
                runtime.pending.is_some(),
                runtime.awaiting,
                runtime.ready,
                runtime.visible,
                runtime.stale,
                runtime.shutdown,
                runtime.input_control.activation_frame,
                runtime.input_control.last_frame,
                ctx.cumulative_frame_nr(),
            );
            assert!(
                surface_focused_before_finish.get(),
                "{arrow:?} retains the genuine viewport Response before publication"
            );
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                runtime.input_control.surface.map(|(id, _)| id),
                "{arrow:?} must not focus the inspector splitter"
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
            assert!(
                runtime.input_owned(),
                "{arrow:?} key-up lost ownership: {}",
                runtime.status
            );
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                runtime.input_control.surface.map(|(id, _)| id)
            );
        }
        // A deliberately contended key-up is NOT a focus-navigation failure.
        // Assert the real pre-finish Response still owns focus, then require
        // fail-closed cancellation and rejection of the old lease. No retry.
        let space = |pressed| egui::Event::Key {
            key: egui::Key::Space,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        draw(&mut runtime, vec![space(true)], false);
        assert!(runtime.input_owned());
        let old_lease = runtime.input_control.lease.unwrap();
        let (resume_slot, slot_holder) = runtime
            .worker
            .as_ref()
            .unwrap()
            .contend_input_slot_for_test();
        draw(&mut runtime, vec![space(false)], false);
        assert!(
            surface_focused_before_finish.get(),
            "contention starts with the genuine viewport focus owner"
        );
        assert!(!runtime.input_owned(), "a busy key-up must fail closed");
        assert_eq!(runtime.status, "Busy Click the game view again.");
        assert_eq!(ctx.memory(|memory| memory.focused()), None);
        resume_slot.send(()).unwrap();
        slot_holder.join().unwrap();
        let worker = runtime.worker.as_ref().unwrap();
        assert!(!worker.current_input_meta(key).unwrap().claimed);
        assert_eq!(
            worker.publish_key_event(old_lease, InputKey::Space, false),
            Err(crate::behavior_worker::InputError::StaleLease)
        );
        // A fresh explicit ordinary activation is allowed; never replay a failed
        // input mutation or silently restore the revoked lease.
        button = draw(&mut runtime, vec![], false);
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
        // A press outside the game view (here the toolbar row) releases.
        let outside = egui::pos2(4.0, 4.0);
        draw(
            &mut runtime,
            vec![egui::Event::PointerMoved(outside), pointer(outside, true)],
            false,
        );
        assert!(!runtime.input_owned(), "outside press releases immediately");
        draw(&mut runtime, vec![pointer(outside, false)], true);
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
        resume_tick.send(()).unwrap();
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
                        // Allow a pixel of subpixel rounding at the clip edge.
                        clip.expand(1.0)
                            .contains_rect(text.galley.rect.translate(text.pos.to_vec2()))
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
                let menu = label_point(&rendered.borrow(), "…");
                draw(&mut runtime, vec![egui::Event::PointerMoved(menu)], false);
                draw(&mut runtime, vec![pointer(menu, true)], false);
                draw(&mut runtime, vec![pointer(menu, false)], false);
                draw(&mut runtime, vec![], false);
                let item = label_point(&rendered.borrow(), "Game Controls…");
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
                        assert!(visible_label(&rendered.borrow(), "Object Transforms"));
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
            // Transport icons preserve real named button bounds, not painted
            // text glyphs. Keep the same enabled/focus/clipping ownership checks.
            let bounds = *accessible_buttons.borrow().get(label).unwrap_or_else(|| {
                panic!(
                    "no accessible button {label}: {:?}",
                    accessible_buttons.borrow().keys().collect::<Vec<_>>()
                )
            });
            let rect = egui::Rect::from_min_max(
                egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
            );
            assert!(ctx.content_rect().contains_rect(rect));
            let point = rect.center();
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
        // S-05: there is no Reset button. The focus shortcut lands on Play,
        // which toggles the already-current run without another reset.
        if runtime.playing {
            let current = runtime.worker.as_ref().unwrap().current_key();
            runtime.pending = Some(Request::Pause(current));
            runtime.flush();
            wait(&mut runtime, false);
        }
        runtime.focus_controls = true;
        for _ in 0..2 {
            draw(&mut runtime, vec![], false);
        }
        let play_id = focused_control("Play");
        let reset_key = runtime.worker.as_ref().unwrap().current_key();
        let admitted = reset_admissions.borrow().len();
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
            assert_eq!(reset_admissions.borrow().len(), admitted);
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
        // After Stop, Play resets from the scene and then plays (S-05).
        runtime.focus_controls = true;
        for _ in 0..2 {
            draw(&mut runtime, vec![], false);
        }
        focused_control("Play");
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
        wait(&mut runtime, true);
        controls_blocked.set(false);
        for _ in 0..2 {
            draw(&mut runtime, vec![], false);
        }
        assert_eq!(
            focused_control("Inspector control"),
            inspector_id,
            "a completed operation cannot restore stale focus over a genuine new owner"
        );
        assert_eq!(reset_admissions.borrow().len(), admitted + 1);
        assert_eq!(session.snapshot_bytes().unwrap(), authored);
        runtime.shutdown();
        let until = Instant::now() + Duration::from_secs(5);
        while !runtime.finished() {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
    }
}
