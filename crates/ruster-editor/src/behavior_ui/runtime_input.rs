//! Native ownership only. Worker leases own the map and per-tick latch.
use super::*;
use crate::behavior_worker::{InputLease, InputMeta};
use ruster_scene::input::InputKey;

#[derive(Default)]
pub(super) struct InputControl {
    lease: Option<InputLease>,
    intent: Option<InputMeta>,
    surface: Option<(egui::Id, egui::Rect)>,
    activation_frame: Option<u64>,
    last_button_frame: Option<u64>,
    last_frame: Option<u64>,
    release_pointer_gesture: bool,
}
impl RuntimeUi {
    pub(crate) fn input_owned(&self) -> bool {
        self.input_control.lease.is_some() || self.input_control.intent.is_some()
    }
    pub(crate) fn release_input(&mut self, ctx: &egui::Context) {
        if self.input_control.lease.take().is_some() || self.input_control.intent.take().is_some() {
            self.input_control.intent = None;
            if let Some(worker) = &self.worker {
                let _ = worker.revoke_input(worker.current_key());
            }
        }
        if let Some((id, _)) = self.input_control.surface {
            ctx.memory_mut(|memory| memory.surrender_focus(id));
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
        }
    }
    pub(super) fn input_control_button(&mut self, ui: &mut egui::Ui, blocked: bool) {
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
        let enabled = !blocked
            && self.ready
            && self.visible
            && self.playing
            && !self.shutdown
            && self.pending.is_none()
            && self.awaiting.is_none()
            && self.debug_snapshot().is_none()
            && self.input_control.surface.is_some();
        let owned = self.input_owned();
        if ui.add_enabled(enabled || owned, egui::Button::new(if owned || release_gesture { "Release controls (Esc)" } else { "Control runtime" }))
            .on_hover_text("Give the scene keyboard control. Escape releases it; Pause and Stop remain available.")
            .clicked() && self.input_control.last_button_frame != Some(ui.ctx().cumulative_frame_nr()) {
            self.input_control.last_button_frame = Some(ui.ctx().cumulative_frame_nr());
            if owned || release_gesture { self.release_input(ui.ctx()); return; }
            let Some(worker) = &self.worker else { return; };
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

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::input::{ActionDefinition, ActionKind};
    use std::time::{Duration, Instant};

    // This is the distinct editor boundary: its worker runs independently and
    // early outside cancellation must not turn a Release click into a new claim.
    #[test]
    fn real_control_gesture_releases_across_frames_and_survives_layout_replay() {
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
        let ctx = egui::Context::default();
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
        let mut draw = |runtime: &mut RuntimeUi, events, discard: bool| {
            time += 0.1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 800.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    runtime.input_begin(&ctx, false);
                    runtime.controls(ui, &session, 1, false);
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
            output
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
                .unwrap()
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
        runtime.shutdown();
        let until = Instant::now() + Duration::from_secs(5);
        while !runtime.finished() {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
    }
}
