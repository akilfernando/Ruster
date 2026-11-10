//! Shared bounded HUD preparation, optional interaction and post-viewport painting.
use crate::egui::{self, Color32, FontId, Painter, Rect, Vec2};
use ruster_scene::hud::{
    hud_button_identity, Hud, HudAnchor, HudSource, ResolvedHud, MAX_HUD_LABELS,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudPresentationMode {
    Committed,
    AuthoredPreview,
    Stale,
    Stopped,
}

impl HudPresentationMode {
    fn badge(self) -> Option<&'static str> {
        match self {
            Self::Committed => None,
            Self::AuthoredPreview => Some("HUD preview · authored defaults"),
            Self::Stale => Some("HUD · last committed · Reset required"),
            Self::Stopped => Some("HUD preview · stopped · authored defaults"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HudPaintReport {
    pub clipped: bool,
    pub panel_rect: Option<Rect>,
    /// Natural definition indices corresponding to the laid-out visible rows.
    pub label_indices: Vec<usize>,
    /// Full row rectangles, including at most one partially clipped boundary row.
    pub label_rects: Vec<Rect>,
}

fn color(components: [f64; 4]) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (components[0].clamp(0.0, 1.0) * 255.0).round() as u8,
        (components[1].clamp(0.0, 1.0) * 255.0).round() as u8,
        (components[2].clamp(0.0, 1.0) * 255.0).round() as u8,
        (components[3].clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

/// A frozen HUD layout. Its row geometry, text and diagnostics all describe
/// the same resolved frame. Preparing never allocates widgets or paints shapes.
#[derive(Clone)]
pub struct PreparedHud {
    pub report: HudPaintReport,
    clip_rect: Rect,
    content_rect: Rect,
    background: Color32,
    text_color: Color32,
    badge: Option<(egui::Pos2, Arc<egui::Galley>)>,
    rows: Vec<PreparedRow>,
    notice: Option<PreparedNotice>,
}

impl Default for PreparedHud {
    fn default() -> Self {
        Self {
            report: HudPaintReport::default(),
            clip_rect: Rect::NOTHING,
            content_rect: Rect::NOTHING,
            background: Color32::TRANSPARENT,
            text_color: Color32::WHITE,
            badge: None,
            rows: Vec::new(),
            notice: None,
        }
    }
}

#[derive(Clone)]
struct PreparedRow {
    index: usize,
    rect: Rect,
    text_position: egui::Pos2,
    galley: Arc<egui::Galley>,
    button: Option<PreparedButton>,
}

#[derive(Clone)]
struct PreparedButton {
    id: String,
    identity: String,
    available: bool,
    unavailable_reason: Option<&'static str>,
    visuals: egui::style::WidgetVisuals,
    opacity: f32,
}

#[derive(Clone)]
struct PreparedNotice {
    rect: Rect,
    position: egui::Pos2,
    galley: Arc<egui::Galley>,
}

/// Passive compatibility entry point. Existing callers create no input regions.
pub fn paint_hud(
    painter: &Painter,
    viewport: Rect,
    hud: &Hud,
    resolved: &ResolvedHud,
    mode: HudPresentationMode,
) -> HudPaintReport {
    paint_prepared_hud(
        painter,
        &prepare_hud(painter, viewport, hud, resolved, mode),
    )
}

/// Prepare within the already allocated 3D viewport. Effective geometry is clamped
/// to that viewport without changing the authored layout or creating hit areas.
/// Consumers attach their own scene/run/tick/commit identity to prepared values.
pub fn prepare_hud(
    painter: &Painter,
    viewport: Rect,
    hud: &Hud,
    resolved: &ResolvedHud,
    mode: HudPresentationMode,
) -> PreparedHud {
    if !hud.enabled || resolved.labels.is_empty() || !viewport.is_positive() {
        return PreparedHud::default();
    }
    let painter = painter.with_clip_rect(viewport.intersect(painter.clip_rect()));
    let margin = (hud.margin as f32).clamp(0.0, viewport.width().min(viewport.height()) * 0.25);
    let available = viewport.shrink(margin);
    let width = (hud.width as f32).clamp(1.0, available.width().max(1.0));
    let padding = 8.0_f32.min(width * 0.25);
    let text_width = (width - padding * 2.0).max(1.0);
    let font = FontId::proportional((hud.font_size as f32).clamp(8.0, 48.0));
    let text_color = color(hud.text_color);
    let spacing = (hud.spacing as f32).clamp(0.0, 64.0);
    let badge = mode.badge();
    let has_unavailable = resolved
        .labels
        .iter()
        .any(|label| label.diagnostic.is_some());
    let badge_galley = badge.map(|badge| {
        painter.layout(
            badge.to_owned(),
            FontId::proportional(12.0),
            Color32::LIGHT_YELLOW,
            text_width,
        )
    });
    let badge_height = badge_galley
        .as_ref()
        .map_or(0.0, |galley| galley.size().y + spacing);
    let right = matches!(hud.anchor, HudAnchor::TopRight | HudAnchor::BottomRight);
    let bottom = matches!(hud.anchor, HudAnchor::BottomLeft | HudAnchor::BottomRight);
    let x = if right {
        available.right() - width
    } else {
        available.left()
    };
    let content_height = (available.height() - padding * 2.0 - badge_height).max(0.0);
    let count = resolved.labels.len().min(MAX_HUD_LABELS);
    let mut rows = Vec::new();
    let mut used_height = 0.0;
    // Walk inward from the anchored edge. Resolving every label is independent
    // from shaping text, so rows outside this viewport need no layout work.
    for offset in 0..count {
        let gap = if offset == 0 { 0.0 } else { spacing };
        if used_height + gap >= content_height {
            break;
        }
        let index = if bottom { count - 1 - offset } else { offset };
        let definition = hud.elements.get(index).filter(|label| {
            label.id == resolved.labels[index].id
                && matches!(label.source, HudSource::Button { .. })
        });
        let button = definition.map(|definition| {
            let identity = hud_button_identity(definition).ok();
            PreparedButton {
                id: definition.id.clone(),
                available: mode == HudPresentationMode::Committed
                    && resolved.labels[index].diagnostic.is_none()
                    && identity.is_some(),
                identity: identity.unwrap_or_default(),
                unavailable_reason: resolved.labels[index]
                    .diagnostic
                    .map(|reason| reason.message())
                    .or_else(|| {
                        (mode != HudPresentationMode::Committed)
                            .then_some("Reset to initialize this button.")
                    }),
                visuals: painter.ctx().global_style().visuals.widgets.inactive,
                opacity: if mode == HudPresentationMode::Committed
                    && resolved.labels[index].diagnostic.is_none()
                {
                    1.0
                } else {
                    painter.ctx().global_style().visuals.disabled_alpha()
                },
            }
        });
        let inset = if button.is_some() { 6.0 } else { 0.0 };
        let galley = painter.layout(
            resolved.labels[index].text.clone(),
            font.clone(),
            text_color,
            (text_width - inset * 2.0).max(1.0),
        );
        let height = if button.is_some() {
            (galley.size().y + inset * 2.0)
                .max(painter.ctx().global_style().spacing.interact_size.y)
        } else {
            galley.size().y
        };
        used_height += gap;
        let row_y = if bottom {
            available.bottom() - padding - used_height - height
        } else {
            available.top() + padding + badge_height + used_height
        };
        let position = egui::pos2(x + padding, row_y);
        let size = if button.is_some() {
            Vec2::new(text_width, height)
        } else {
            galley.size()
        };
        rows.push(PreparedRow {
            index,
            rect: Rect::from_min_size(position, size),
            text_position: position + Vec2::splat(inset),
            galley,
            button,
        });
        used_height += height;
        if used_height > content_height {
            break;
        }
    }
    let clipped = rows.len() < count
        || used_height > content_height
        || padding * 2.0 + badge_height > available.height();
    let height = if clipped {
        available.height()
    } else {
        padding * 2.0 + badge_height + used_height
    };
    let y = if bottom {
        available.bottom() - height
    } else {
        available.top()
    };
    let panel_rect = Rect::from_min_size(egui::pos2(x, y), Vec2::new(width, height));
    let content_top = (y + padding + badge_height).min(panel_rect.bottom());
    let content_bottom = (panel_rect.bottom() - padding).max(content_top);
    let content_rect = Rect::from_min_max(
        egui::pos2(x + padding, content_top),
        egui::pos2(x + width - padding, content_bottom),
    );
    if bottom {
        rows.reverse();
    }
    let label_indices = rows.iter().map(|row| row.index).collect();
    let label_rects = rows.iter().map(|row| row.rect).collect();
    let notice = if clipped || has_unavailable {
        let text = if clipped {
            "HUD clipped to viewport"
        } else {
            "HUD has unavailable values"
        };
        let galley = painter.layout(
            text.into(),
            FontId::proportional(12.0),
            Color32::LIGHT_YELLOW,
            text_width,
        );
        let notice_height = galley.size().y.min(viewport.height());
        let notice_y = if bottom {
            (panel_rect.top() + padding + badge_height).min(panel_rect.bottom() - notice_height)
        } else {
            panel_rect.bottom() - notice_height - padding
        }
        .max(viewport.top());
        Some(PreparedNotice {
            rect: Rect::from_min_size(
                egui::pos2(x, notice_y),
                Vec2::new(width, notice_height + padding),
            ),
            position: egui::pos2(x + padding, notice_y),
            galley,
        })
    } else {
        None
    };
    PreparedHud {
        report: HudPaintReport {
            clipped,
            panel_rect: Some(panel_rect),
            label_indices,
            label_rects,
        },
        clip_rect: painter.clip_rect(),
        content_rect,
        background: color(hud.background),
        text_color,
        badge: badge_galley.map(|galley| (egui::pos2(x + padding, y + padding), galley)),
        rows,
        notice,
    }
}

/// Draw after the viewport's GPU callback so both the panel and controls remain
/// visible. Uses the exact prepared geometry, including interaction styling.
pub fn paint_prepared_hud(painter: &Painter, prepared: &PreparedHud) -> HudPaintReport {
    let Some(panel_rect) = prepared.report.panel_rect else {
        return prepared.report.clone();
    };
    let painter = painter.with_clip_rect(prepared.clip_rect.intersect(painter.clip_rect()));
    painter.rect_filled(panel_rect, 5.0, prepared.background);
    if let Some((position, galley)) = &prepared.badge {
        painter
            .with_clip_rect(panel_rect.intersect(painter.clip_rect()))
            .galley(*position, galley.clone(), Color32::LIGHT_YELLOW);
    }
    let content_painter =
        painter.with_clip_rect(prepared.content_rect.intersect(painter.clip_rect()));
    for row in &prepared.rows {
        if let Some(button) = &row.button {
            let visuals = button.visuals;
            let mut button_painter = content_painter.clone();
            button_painter.multiply_opacity(button.opacity);
            button_painter.rect(
                row.rect,
                visuals.corner_radius,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
            button_painter.galley_with_override_text_color(
                row.text_position,
                row.galley.clone(),
                visuals.text_color(),
            );
        } else {
            content_painter.galley(row.text_position, row.galley.clone(), prepared.text_color);
        }
    }
    // The diagnostic overlays the bounded content without changing row geometry.
    if let Some(notice) = &prepared.notice {
        painter.rect_filled(notice.rect, 0.0, Color32::from_black_alpha(240));
        painter.galley(
            notice.position,
            notice.galley.clone(),
            Color32::LIGHT_YELLOW,
        );
    }
    prepared.report.clone()
}

/// Input ownership for one viewport run. Keep this with the run owner, separate
/// from frozen presentations. A new run scope cancels the preceding gesture.
#[derive(Default)]
pub struct HudInteractionState {
    scope: Option<egui::Id>,
    gesture: Option<HudPointerGesture>,
    focused: Option<egui::Id>,
    last_input_frame: Option<u64>,
    last_pointer_owned: bool,
    last_keyboard_owned: bool,
}

struct HudPointerGesture {
    id: String,
    identity: String,
    primary: bool,
    cancelled: bool,
}

#[derive(Clone, Debug, Default)]
pub struct HudInteraction {
    /// Stable authored element ID. The run owner validates and queues its intent.
    pub activated: Option<String>,
    pub pointer_owned: bool,
    pub keyboard_owned: bool,
    /// Escape, window focus loss or a run boundary invalidated pending UI intent.
    pub cancelled: bool,
}

impl PreparedHud {
    fn button_hit_rect(&self, row: &PreparedRow, ui: &egui::Ui) -> Rect {
        row.rect
            .intersect(self.content_rect)
            .intersect(self.clip_rect)
            .intersect(ui.clip_rect())
    }
}

/// Register genuine stable click-only widgets, without drawing or allocating
/// parent layout space. Call before viewport navigation and gameplay input.
/// `enabled` gates actual availability. `pending` preserves real widget focus and
/// input ownership while suppressing additional activation until acknowledgement.
pub fn interact_prepared_hud(
    ui: &mut egui::Ui,
    prepared: &mut PreparedHud,
    run_scope: egui::Id,
    enabled: bool,
    pending: bool,
    state: &mut HudInteractionState,
) -> HudInteraction {
    let frame = ui.ctx().cumulative_frame_nr();
    let scope_changed = state.scope.is_some_and(|scope| scope != run_scope);
    if scope_changed {
        if let Some(focused) = state.focused.take() {
            ui.memory_mut(|memory| memory.surrender_focus(focused));
        }
        // Cancel the old target while swallowing its physical gesture through
        // release. A new run must never inherit an already held press.
        if let Some(gesture) = &mut state.gesture {
            gesture.cancelled = true;
        }
        state.last_input_frame = None;
    }
    state.scope = Some(run_scope);
    let fresh_input = state.last_input_frame != Some(frame);
    let mut result = HudInteraction {
        cancelled: scope_changed,
        ..Default::default()
    };
    let mut responses = Vec::new();
    for index in 0..prepared.rows.len() {
        let row = &prepared.rows[index];
        let Some(button) = &row.button else {
            continue;
        };
        let rect = prepared.button_hit_rect(row, ui);
        if !rect.is_positive() {
            continue;
        }
        let id = run_scope.with(("hud-button", &button.id));
        let available = enabled && button.available;
        let builder = egui::UiBuilder::new().id_salt(id).max_rect(rect);
        let mut child = ui.new_child(if available {
            builder
        } else {
            builder.disabled()
        });
        child.set_clip_rect(rect);
        let response = child.interact(rect, id, egui::Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, available, &row.galley.job.text)
        });
        let opacity = if response.enabled() && !pending {
            1.0
        } else {
            child.visuals().disabled_alpha()
        };
        let response = if let Some(reason) = button.unavailable_reason {
            response.on_disabled_hover_text(reason)
        } else if pending {
            response.on_hover_text("Waiting for the previous HUD action to finish.")
        } else {
            response
        };
        responses.push((index, rect, response, opacity));
    }
    // egui clears real widget focus for Escape in Focus::begin_pass, before
    // this phase runs. Retain that previous button's ownership only to consume
    // Escape. Focus stays cleared, and removed/disabled/clipped rows do not own it.
    let escape_cleared_button_focus = state.focused.is_some_and(|previous| {
        ui.memory(|memory| memory.focused().is_none())
            && responses
                .iter()
                .any(|(_, _, response, _)| response.id == previous && response.enabled())
    });
    let focused = responses
        .iter()
        .find(|(_, _, response, _)| response.enabled() && response.has_focus())
        .map(|(_, _, response, _)| response.id);
    if state.focused.is_some() && focused.is_none() {
        if let Some(previous) = state.focused {
            // A removed, disabled or clipped row must leave real keyboard focus.
            if ui.memory(|memory| memory.has_focus(previous)) {
                ui.memory_mut(|memory| memory.surrender_focus(previous));
            }
        }
    }
    state.focused = focused;
    let window_focused = ui.input(|input| input.focused);
    result.keyboard_owned = focused.is_some() && window_focused;
    let pointer_over_button = responses
        .iter()
        .any(|(_, rect, _, _)| ui.rect_contains_pointer(*rect));
    result.pointer_owned = pointer_over_button || state.gesture.is_some();
    if let Some(gesture) = &mut state.gesture {
        let target_available = responses.iter().any(|(index, _, response, _)| {
            let button = prepared.rows[*index].button.as_ref().unwrap();
            response.enabled()
                && !pending
                && button.id == gesture.id
                && button.identity == gesture.identity
        });
        gesture.cancelled |= !target_available;
    }
    if fresh_input {
        state.last_input_frame = Some(frame);
        let events = ui.input(|input| input.events.clone());
        let owns_escape =
            state.focused.is_some() || escape_cleared_button_focus || state.gesture.is_some();
        let escape = owns_escape
            && events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Escape,
                        pressed: true,
                        ..
                    }
                )
            });
        if !window_focused || escape || scope_changed {
            result.cancelled = true;
            if let Some(focused) = state.focused.take() {
                ui.memory_mut(|memory| memory.surrender_focus(focused));
            }
            if let Some(gesture) = &mut state.gesture {
                gesture.cancelled = true;
            }
            if escape {
                ui.input_mut(|input| {
                    input.events.retain(|event| {
                        !matches!(
                            event,
                            egui::Event::Key {
                                key: egui::Key::Escape,
                                ..
                            }
                        )
                    })
                });
                result.keyboard_owned = true;
            }
        } else {
            for event in &events {
                match event {
                    egui::Event::PointerButton {
                        pos,
                        button,
                        pressed: true,
                        ..
                    } => {
                        if state.gesture.is_none() {
                            if let Some((index, _, response, _)) =
                                responses.iter().find(|(_, rect, _, _)| {
                                    rect.contains(*pos)
                                        && ui.ctx().layer_id_at(*pos) == Some(ui.layer_id())
                                })
                            {
                                let definition = prepared.rows[*index].button.as_ref().unwrap();
                                state.gesture = Some(HudPointerGesture {
                                    id: definition.id.clone(),
                                    identity: definition.identity.clone(),
                                    primary: *button == egui::PointerButton::Primary,
                                    cancelled: !response.enabled() || pending,
                                });
                                if response.enabled() && *button == egui::PointerButton::Primary {
                                    response.request_focus();
                                    state.focused = Some(response.id);
                                    result.keyboard_owned = true;
                                }
                                result.pointer_owned = true;
                            }
                        }
                    }
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        ..
                    } => {
                        if let Some(gesture) = &state.gesture {
                            if gesture.primary && !gesture.cancelled {
                                if let Some((index, _, _, _)) =
                                    responses.iter().find(|(index, rect, response, _)| {
                                        let definition =
                                            prepared.rows[*index].button.as_ref().unwrap();
                                        definition.id == gesture.id
                                            && definition.identity == gesture.identity
                                            && rect.contains(*pos)
                                            && response.enabled()
                                            && !pending
                                            && response.clicked_by(egui::PointerButton::Primary)
                                    })
                                {
                                    result.activated = Some(
                                        prepared.rows[*index].button.as_ref().unwrap().id.clone(),
                                    );
                                }
                            }
                        }
                    }
                    egui::Event::PointerGone => {
                        if let Some(gesture) = &mut state.gesture {
                            gesture.cancelled = true;
                        }
                    }
                    egui::Event::Key {
                        key: egui::Key::Enter | egui::Key::Space,
                        pressed: true,
                        repeat: false,
                        ..
                    } if state.focused.is_some() && !pending => {
                        if let Some((index, _, _, _)) =
                            responses.iter().find(|(_, _, response, _)| {
                                Some(response.id) == state.focused && response.enabled()
                            })
                        {
                            result.activated.get_or_insert_with(|| {
                                prepared.rows[*index].button.as_ref().unwrap().id.clone()
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        if !ui.input(|input| input.pointer.any_down()) {
            state.gesture = None;
        }
        state.last_pointer_owned = result.pointer_owned;
        state.last_keyboard_owned = result.keyboard_owned;
    } else {
        // egui may replay a discarded pass with the same physical input.
        result.pointer_owned |= state.last_pointer_owned;
        result.keyboard_owned |= state.last_keyboard_owned;
    }
    if result.keyboard_owned {
        ui.input_mut(|input| {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Enter | egui::Key::Space,
                        ..
                    }
                )
            });
        });
    }
    for (index, _, response, opacity) in responses {
        let button = prepared.rows[index].button.as_mut().unwrap();
        button.visuals = *ui.style().interact(&response);
        button.opacity = opacity;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::hud::ResolvedHudLabel;

    fn button_source(caption: &str) -> HudSource {
        HudSource::Button {
            caption: caption.into(),
            command: ruster_scene::hud::HudCommandBinding {
                binding_id: "app".into(),
                command_id: "submit".into(),
                arguments: Default::default(),
            },
        }
    }

    struct InteractionHarness {
        context: egui::Context,
        state: HudInteractionState,
        hud: Hud,
        resolved: ResolvedHud,
        viewport: Rect,
        scope: egui::Id,
        enabled: bool,
        pending: bool,
        window_focused: bool,
        replay: bool,
        report: HudPaintReport,
    }

    impl InteractionHarness {
        fn new() -> Self {
            let mut harness = Self {
                context: egui::Context::default(),
                state: HudInteractionState::default(),
                hud: Hud {
                    enabled: true,
                    elements: vec![ruster_scene::hud::HudLabel {
                        id: "submit".into(),
                        source: button_source("Submit"),
                    }],
                    ..Hud::default()
                },
                resolved: ResolvedHud {
                    labels: vec![ResolvedHudLabel {
                        id: "submit".into(),
                        text: "Submit".into(),
                        diagnostic: None,
                    }],
                },
                viewport: Rect::from_min_size(egui::pos2(30.0, 40.0), Vec2::new(400.0, 300.0)),
                scope: egui::Id::unique("run-one"),
                enabled: true,
                pending: false,
                window_focused: true,
                replay: false,
                report: HudPaintReport::default(),
            };
            harness.frame(Vec::new());
            harness
        }

        fn frame(&mut self, events: Vec<egui::Event>) -> Vec<HudInteraction> {
            let mut interactions = Vec::new();
            let mut output = self.context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(800.0, 600.0),
                    )),
                    focused: self.window_focused,
                    events,
                    ..Default::default()
                },
                |ui| {
                    let available = ui.available_rect_before_wrap();
                    let mut prepared = prepare_hud(
                        ui.painter(),
                        self.viewport,
                        &self.hud,
                        &self.resolved,
                        HudPresentationMode::Committed,
                    );
                    interactions.push(interact_prepared_hud(
                        ui,
                        &mut prepared,
                        self.scope,
                        self.enabled,
                        self.pending,
                        &mut self.state,
                    ));
                    assert_eq!(ui.available_rect_before_wrap(), available);
                    self.report = paint_prepared_hud(ui.painter(), &prepared);
                    if self.replay && ui.ctx().current_pass_index() == 0 {
                        ui.ctx()
                            .request_discard("HUD physical input replay regression");
                    }
                },
            );
            output.textures_delta.clear();
            interactions
        }

        fn pointer(
            &mut self,
            pos: egui::Pos2,
            button: egui::PointerButton,
            pressed: bool,
        ) -> HudInteraction {
            self.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ])
            .remove(0)
        }

        fn key(&mut self, key: egui::Key, repeat: bool) -> Vec<HudInteraction> {
            self.frame(vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat,
                modifiers: egui::Modifiers::NONE,
            }])
        }
    }

    #[test]
    fn buttons_own_gestures_without_click_through_or_inheriting_pressed_targets() {
        for button in [
            egui::PointerButton::Primary,
            egui::PointerButton::Secondary,
            egui::PointerButton::Middle,
        ] {
            let mut harness = InteractionHarness::new();
            let inside = harness.report.label_rects[0].center();
            let outside = harness.viewport.right_bottom() - Vec2::splat(5.0);
            let press = harness.pointer(inside, button, true);
            assert!(press.pointer_owned);
            assert!(press.activated.is_none());
            let moved = harness
                .frame(vec![egui::Event::PointerMoved(outside)])
                .remove(0);
            assert!(moved.pointer_owned);
            harness.enabled = false;
            harness.viewport =
                Rect::from_min_size(egui::pos2(450.0, 300.0), Vec2::new(200.0, 200.0));
            assert!(harness.frame(Vec::new())[0].pointer_owned);
            let release = harness.pointer(outside, button, false);
            assert!(release.pointer_owned);
            assert!(release.activated.is_none());
            assert!(!harness.frame(Vec::new())[0].pointer_owned);
        }
        let mut harness = InteractionHarness::new();
        let inside = harness.report.label_rects[0].center();
        harness.enabled = false;
        assert!(
            harness
                .pointer(inside, egui::PointerButton::Primary, true)
                .pointer_owned
        );
        harness.enabled = true;
        assert!(harness
            .pointer(inside, egui::PointerButton::Primary, false)
            .activated
            .is_none());
        harness.pointer(inside, egui::PointerButton::Primary, true);
        harness.scope = egui::Id::unique("run-two");
        let boundary = harness.frame(Vec::new()).remove(0);
        assert!(boundary.cancelled && boundary.pointer_owned);
        assert!(harness
            .pointer(inside, egui::PointerButton::Primary, false)
            .activated
            .is_none());
    }

    #[test]
    fn button_activation_uses_real_focus_nonrepeat_keys_and_one_physical_frame() {
        let mut harness = InteractionHarness::new();
        let inside = harness.report.label_rects[0].center();
        harness.pointer(inside, egui::PointerButton::Primary, true);
        let release = harness.pointer(inside, egui::PointerButton::Primary, false);
        assert_eq!(release.activated.as_deref(), Some("submit"));
        assert!(release.keyboard_owned);
        assert_eq!(
            harness.key(egui::Key::Enter, false)[0].activated.as_deref(),
            Some("submit")
        );
        assert!(harness.key(egui::Key::Enter, true)[0].activated.is_none());
        let focused = harness.context.memory(|memory| memory.focused()).unwrap();
        harness.frame(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        harness.pending = true;
        let busy = harness.key(egui::Key::Enter, false).remove(0);
        assert!(busy.keyboard_owned && busy.activated.is_none());
        assert_eq!(
            harness.context.memory(|memory| memory.focused()),
            Some(focused)
        );
        assert!(!harness
            .context
            .input(|input| input.events.iter().any(|event| matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    ..
                }
            ))));
        harness.pending = false;
        harness.frame(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert_eq!(
            harness.key(egui::Key::Enter, false)[0].activated.as_deref(),
            Some("submit")
        );
        assert_eq!(
            harness.context.memory(|memory| memory.focused()),
            Some(focused)
        );
        harness.frame(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        harness.replay = true;
        let replay = harness.key(egui::Key::Space, false);
        assert!(replay.len() > 1);
        assert_eq!(
            replay
                .iter()
                .filter(|result| result.activated.is_some())
                .count(),
            1
        );
        assert!(harness.context.memory(|memory| memory.focused()).is_some());
        let escape = harness.key(egui::Key::Escape, false).remove(0);
        assert!(escape.cancelled && escape.keyboard_owned);
        assert!(harness.context.memory(|memory| memory.focused()).is_none());
        assert!(harness.key(egui::Key::Enter, false)[0].activated.is_none());
        harness.replay = false;
        harness.pointer(inside, egui::PointerButton::Primary, true);
        harness.window_focused = false;
        let lost = harness
            .frame(vec![egui::Event::WindowFocused(false)])
            .remove(0);
        assert!(lost.cancelled && lost.activated.is_none());
        assert!(harness.context.memory(|memory| memory.focused()).is_none());
    }

    #[test]
    fn clipped_focus_is_released_and_unavailable_buttons_still_block_input() {
        let mut harness = InteractionHarness::new();
        let inside = harness.report.label_rects[0].center();
        harness.pointer(inside, egui::PointerButton::Primary, true);
        harness.pointer(inside, egui::PointerButton::Primary, false);
        harness.viewport = Rect::from_min_size(harness.viewport.min, Vec2::new(100.0, 1.0));
        let clipped = harness.frame(Vec::new()).remove(0);
        assert!(!clipped.keyboard_owned);
        assert!(harness.context.memory(|memory| memory.focused()).is_none());
        harness.viewport = Rect::from_min_size(egui::pos2(30.0, 40.0), Vec2::new(400.0, 300.0));
        harness.resolved.labels[0].diagnostic =
            Some(ruster_scene::hud::HudDiagnostic::MissingCommand);
        harness.frame(Vec::new());
        let inside = harness.report.label_rects[0].center();
        let disabled = harness.pointer(inside, egui::PointerButton::Primary, true);
        assert!(disabled.pointer_owned && disabled.activated.is_none());
        assert!(!disabled.keyboard_owned);
        assert!(harness
            .pointer(inside, egui::PointerButton::Primary, false)
            .activated
            .is_none());
    }

    // The reference deliberately shapes the entire group. Production must
    // produce the same visible positions while shaping only the intersecting rows.
    fn complete_layout_reference(
        painter: &Painter,
        viewport: Rect,
        hud: &Hud,
        resolved: &ResolvedHud,
        mode: HudPresentationMode,
    ) -> (bool, Rect, Vec<(usize, Rect)>) {
        let margin = (hud.margin as f32).min(viewport.width().min(viewport.height()) * 0.25);
        let available = viewport.shrink(margin);
        let width = (hud.width as f32).min(available.width());
        let padding = 8.0_f32.min(width * 0.25);
        let wrap_width = width - padding * 2.0;
        let spacing = hud.spacing as f32;
        let badge_height = mode.badge().map_or(0.0, |badge| {
            painter
                .layout(
                    badge.into(),
                    FontId::proportional(12.0),
                    Color32::LIGHT_YELLOW,
                    wrap_width,
                )
                .size()
                .y
                + spacing
        });
        let sizes: Vec<_> = resolved
            .labels
            .iter()
            .enumerate()
            .map(|(index, label)| {
                let button = hud.elements.get(index).is_some_and(|element| {
                    element.id == label.id && matches!(element.source, HudSource::Button { .. })
                });
                let inset = if button { 6.0 } else { 0.0 };
                let size = painter
                    .layout(
                        label.text.clone(),
                        FontId::proportional(hud.font_size as f32),
                        color(hud.text_color),
                        (wrap_width - inset * 2.0).max(1.0),
                    )
                    .size();
                if button {
                    Vec2::new(
                        wrap_width,
                        (size.y + inset * 2.0)
                            .max(painter.ctx().global_style().spacing.interact_size.y),
                    )
                } else {
                    size
                }
            })
            .collect();
        let total_rows = sizes.iter().map(|size| size.y).sum::<f32>()
            + spacing * sizes.len().saturating_sub(1) as f32;
        let natural_height = padding * 2.0 + badge_height + total_rows;
        let clipped = natural_height > available.height();
        let height = natural_height.min(available.height());
        let right = matches!(hud.anchor, HudAnchor::TopRight | HudAnchor::BottomRight);
        let bottom = matches!(hud.anchor, HudAnchor::BottomLeft | HudAnchor::BottomRight);
        let x = if right {
            available.right() - width
        } else {
            available.left()
        };
        let y = if bottom {
            available.bottom() - height
        } else {
            available.top()
        };
        let panel = Rect::from_min_size(egui::pos2(x, y), Vec2::new(width, height));
        let content_top = y + padding + badge_height;
        let content_bottom = panel.bottom() - padding;
        let mut cursor = if bottom {
            content_bottom - total_rows
        } else {
            content_top
        };
        let mut visible = Vec::new();
        for (index, size) in sizes.into_iter().enumerate() {
            let rect = Rect::from_min_size(egui::pos2(x + padding, cursor), size);
            if rect.bottom() > content_top && rect.top() < content_bottom {
                visible.push((index, rect));
            }
            cursor += size.y + spacing;
        }
        (clipped, panel, visible)
    }

    #[test]
    fn viewport_culling_matches_complete_group_for_all_anchors_and_badges() {
        let context = egui::Context::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    Vec2::new(1000.0, 1000.0),
                )),
                ..Default::default()
            },
            |ui| {
                for (count, height, buttons) in [
                    (3, 600.0, false),
                    (32, 180.0, false),
                    (3, 600.0, true),
                    (32, 180.0, true),
                ] {
                    let resolved = ResolvedHud {
                        labels: (0..count)
                            .map(|index| ResolvedHudLabel {
                                id: format!("row-{index}"),
                                text: format!(
                                    "Row {index}\n{}",
                                    "Detailed reading ".repeat(index % 3 + 1)
                                ),
                                diagnostic: None,
                            })
                            .collect(),
                    };
                    let viewport =
                        Rect::from_min_size(egui::pos2(30.0, 40.0), Vec2::new(300.0, height));
                    for anchor in [
                        HudAnchor::TopLeft,
                        HudAnchor::TopRight,
                        HudAnchor::BottomLeft,
                        HudAnchor::BottomRight,
                    ] {
                        let hud = Hud {
                            enabled: true,
                            anchor,
                            width: 180.0,
                            elements: if buttons {
                                resolved
                                    .labels
                                    .iter()
                                    .enumerate()
                                    .map(|(index, label)| ruster_scene::hud::HudLabel {
                                        id: label.id.clone(),
                                        source: if index % 2 == 0 {
                                            button_source(&label.text)
                                        } else {
                                            HudSource::Static {
                                                text: label.text.clone(),
                                            }
                                        },
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            },
                            ..Hud::default()
                        };
                        for mode in [
                            HudPresentationMode::Committed,
                            HudPresentationMode::AuthoredPreview,
                            HudPresentationMode::Stale,
                        ] {
                            let (clipped, panel, visible) = complete_layout_reference(
                                ui.painter(),
                                viewport,
                                &hud,
                                &resolved,
                                mode,
                            );
                            let report = paint_hud(ui.painter(), viewport, &hud, &resolved, mode);
                            assert_eq!(report.clipped, clipped);
                            let actual_panel = report.panel_rect.unwrap();
                            assert!(actual_panel.min.distance(panel.min) < 0.01);
                            assert!(actual_panel.max.distance(panel.max) < 0.01);
                            assert_eq!(
                                report.label_indices,
                                visible.iter().map(|(index, _)| *index).collect::<Vec<_>>()
                            );
                            assert_eq!(report.label_rects.len(), visible.len());
                            assert!(report
                                .label_indices
                                .windows(2)
                                .all(|pair| pair[0] < pair[1]));
                            for (actual, (_, expected)) in report.label_rects.iter().zip(visible) {
                                assert!(actual.min.distance(expected.min) < 0.01);
                                assert!(actual.max.distance(expected.max) < 0.01);
                            }
                            if clipped {
                                assert!(report.label_rects.len() < count);
                                if matches!(anchor, HudAnchor::BottomLeft | HudAnchor::BottomRight)
                                {
                                    assert_eq!(report.label_indices.last(), Some(&(count - 1)));
                                } else {
                                    assert_eq!(report.label_indices.first(), Some(&0));
                                }
                            } else {
                                assert_eq!(report.label_rects.len(), count);
                            }
                        }
                    }
                }
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn anchors_wrap_and_clip_without_resizing_or_taking_input() {
        let context = egui::Context::default();
        let resolved = ResolvedHud {
            labels: vec![ResolvedHudLabel {
                id: "reading".into(),
                text: "A wrapped instrument reading with enough words to span lines".into(),
                diagnostic: None,
            }],
        };
        let viewport = Rect::from_min_size(egui::pos2(30.0, 40.0), Vec2::new(240.0, 180.0));
        let mut hud = Hud {
            enabled: true,
            width: 120.0,
            ..Hud::default()
        };
        for anchor in [
            HudAnchor::TopLeft,
            HudAnchor::TopRight,
            HudAnchor::BottomLeft,
            HudAnchor::BottomRight,
        ] {
            hud.anchor = anchor;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(800.0, 600.0),
                    )),
                    events: vec![egui::Event::PointerMoved(viewport.center())],
                    ..Default::default()
                },
                |ui| {
                    let focus = ui.ctx().memory(|memory| memory.focused());
                    let pointer_owned = ui.ctx().egui_wants_pointer_input();
                    let available = ui.available_rect_before_wrap();
                    let report = paint_hud(
                        ui.painter(),
                        viewport,
                        &hud,
                        &resolved,
                        HudPresentationMode::Committed,
                    );
                    let panel = report.panel_rect.unwrap();
                    assert!(!report.clipped);
                    assert_eq!(report.label_indices, vec![0]);
                    assert!(viewport.contains_rect(panel));
                    assert!(report.label_rects[0].height() > hud.font_size as f32 * 2.0);
                    assert_eq!(ui.available_rect_before_wrap(), available);
                    assert_eq!(ui.ctx().memory(|memory| memory.focused()), focus);
                    assert_eq!(ui.ctx().egui_wants_pointer_input(), pointer_owned);
                    let right = matches!(anchor, HudAnchor::TopRight | HudAnchor::BottomRight);
                    let bottom = matches!(anchor, HudAnchor::BottomLeft | HudAnchor::BottomRight);
                    assert_eq!(
                        if right { panel.right() } else { panel.left() },
                        if right {
                            viewport.right() - 12.0
                        } else {
                            viewport.left() + 12.0
                        }
                    );
                    assert_eq!(
                        if bottom { panel.bottom() } else { panel.top() },
                        if bottom {
                            viewport.bottom() - 12.0
                        } else {
                            viewport.top() + 12.0
                        }
                    );
                    let compact = paint_hud(
                        ui.painter(),
                        Rect::from_min_size(viewport.min, Vec2::new(70.0, 35.0)),
                        &hud,
                        &resolved,
                        HudPresentationMode::AuthoredPreview,
                    );
                    assert!(compact.clipped);
                },
            );
            output.textures_delta.clear();
        }
    }
}
