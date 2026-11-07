//! Recoverable primitive component fields using the shared Scene transaction.
use super::{combo_focus::ComboFocus, SceneEditor};
use eframe::egui;
use ruster_scene::{Collider, ColliderRole, ColliderShape, Scene, Transform};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ColliderMode {
    None,
    Sphere,
    Box,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ColliderDraft {
    pub object_id: String,
    pub revision: u64,
    #[serde(deserialize_with = "deserialize_baseline")]
    pub baseline: Option<Collider>,
    pub baseline_transform: Transform,
    pub mode: ColliderMode,
    pub enabled: bool,
    pub role: ColliderRole,
    pub offset: [String; 3],
    pub radius: String,
    pub box_size: [String; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Shape,
    Offset(usize),
    Radius,
    Size(usize),
}
impl Field {
    fn id(self) -> egui::Id {
        match self {
            Self::Shape => egui::Id::unique("collider_shape_error"),
            Self::Offset(index) => egui::Id::unique(("collider_offset", index)),
            Self::Radius => egui::Id::unique("collider_radius"),
            Self::Size(index) => egui::Id::unique(("collider_size", index)),
        }
    }
}
fn bounded(text: &str) -> bool {
    text.len() <= 256 && text.chars().count() <= 64
}
fn deserialize_baseline<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Collider>, D::Error> {
    Option::<Collider>::deserialize(deserializer)
}
fn scalar(text: &str, minimum: f64, maximum: f64, label: &str) -> Result<f64, String> {
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| bounded(text) && value.is_finite() && (minimum..=maximum).contains(value))
        .ok_or_else(|| format!("{label} requires a finite number from {minimum} to {maximum}."))
}
impl ColliderDraft {
    pub(super) fn new(scene: &Scene, id: &str) -> Option<Self> {
        let object = scene.objects.iter().find(|object| object.id == id)?;
        let value = object.collider.unwrap_or_default();
        let (mode, radius, half_extents) = match value.shape {
            ColliderShape::Sphere { radius } => (ColliderMode::Sphere, radius, [0.5; 3]),
            ColliderShape::Box { half_extents } => (ColliderMode::Box, 0.5, half_extents),
        };
        Some(Self {
            object_id: id.into(),
            revision: scene.revision,
            baseline: object.collider,
            baseline_transform: object.transform.clone(),
            mode: if object.collider.is_some() {
                mode
            } else {
                ColliderMode::None
            },
            enabled: value.enabled,
            role: value.role,
            offset: value.offset.map(|value| value.to_string()),
            radius: radius.to_string(),
            box_size: half_extents.map(|value| (value * 2.0).to_string()),
        })
    }
    pub(super) fn storage_valid(&self) -> bool {
        ruster_scene::validate_id(&self.object_id).is_ok()
            && self.baseline_transform.validate().is_ok()
            && self
                .baseline
                .is_none_or(|value| value.resolve(&self.baseline_transform).is_ok())
            && self
                .offset
                .iter()
                .chain(&self.box_size)
                .chain(std::iter::once(&self.radius))
                .all(|text| bounded(text))
    }
    pub(super) fn validate_snapshot(&self, scene: &Scene, selected: Option<&str>) -> bool {
        self.storage_valid()
            && self.revision == scene.revision
            && selected == Some(self.object_id.as_str())
            && scene
                .objects
                .iter()
                .find(|object| object.id == self.object_id)
                .is_some_and(|object| {
                    object.collider == self.baseline && object.transform == self.baseline_transform
                })
    }
    fn value(&self) -> Result<Option<Collider>, (Field, String)> {
        if self.mode == ColliderMode::None {
            return Ok(None);
        }
        let mut offset = [0.0; 3];
        for (index, text) in self.offset.iter().enumerate() {
            offset[index] = scalar(text, -10000.0, 10000.0, "Offset")
                .map_err(|message| (Field::Offset(index), message))?;
        }
        let shape = match self.mode {
            ColliderMode::Sphere => {
                json!({"type":"sphere", "radius":scalar(&self.radius, 0.001, 1000.0, "Radius")
                .map_err(|message| (Field::Radius, message))?})
            }
            ColliderMode::Box => {
                let mut extents = [0.0; 3];
                for (index, text) in self.box_size.iter().enumerate() {
                    extents[index] = scalar(text, 0.002, 2000.0, "Size")
                        .map_err(|message| (Field::Size(index), message))?
                        / 2.0;
                }
                json!({"type":"box", "half_extents":extents})
            }
            ColliderMode::None => unreachable!(),
        };
        // Shared serde receives original f64 values before any f32 conversion.
        let value: Collider = serde_json::from_value(json!({"enabled":self.enabled,
            "role":self.role,"offset":offset,"shape":shape}))
        .map_err(|error| (Field::Shape, error.to_string()))?;
        value
            .resolve(&self.baseline_transform)
            .map_err(|error| (Field::Shape, error.to_string()))?;
        Ok(Some(value))
    }
}

impl SceneEditor {
    fn collider_editable(&self, saving: bool) -> bool {
        !self.asset_publishing()
            && !self.asset_loading()
            && self.color_draft.is_none()
            && self.material_draft.is_none()
            && self.lighting_draft.is_none()
            && self.display_draft.is_none()
            && !self.manipulation.active()
            && !self.manipulation.precision_pending()
            && !self.manipulation.offset_pending()
            && self.draft == self.baseline_fields
            && !self.behavior.pending()
            && !self.input_actions.pending()
            && self.dialog.is_none()
            && !self.export_dialog
            && !self.asset_dialog
            && !self.rename_pending
            && (saving || !self.new_scene_pending)
            && (saving || !self.close_pending)
            && !self.close_approved
            && !self.workspace.settings_open
            && !self.recovery.open
            && self.recovery_transition.is_none()
    }
    pub(super) fn apply_collider_draft(&mut self) -> bool {
        let Some(draft) = self.collider_draft.clone() else {
            return true;
        };
        #[cfg(feature = "inspection")]
        let observed = (
            self.asset_ops
                .timing_immediate_begin("scene_collider_set", "human"),
            self.session.scene().revision,
        );
        let result = (|| {
            if !self.collider_editable(true) {
                return Err("Finish other edits and dialogs before applying the collider.".into());
            }
            if !draft.validate_snapshot(self.session.scene(), self.selected.as_deref()) {
                return Err(
                    "Collider target or Scale changed. Cancel this draft and inspect again.".into(),
                );
            }
            let value = draft.value().map_err(|(_, message)| message)?;
            self.session
                .set_collider(draft.revision, &draft.object_id, value)
                .map_err(|error| error.to_string())?;
            Ok::<_, String>(())
        })();
        let success = match result {
            Ok(()) => {
                self.collider_draft = None;
                self.draft_generation = self.draft_generation.wrapping_add(1);
                self.sync_draft();
                self.error.clear();
                self.notice =
                    "Collider applied. Undo restores the edit. Reset updates the preview.".into();
                true
            }
            Err(error) => {
                self.error = error;
                false
            }
        };
        #[cfg(feature = "inspection")]
        self.asset_ops.timing_immediate_finished(
            observed.0,
            success,
            self.session.scene().revision != observed.1,
        );
        success
    }
    fn cancel_collider_draft(&mut self) {
        self.collider_draft = None;
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.error.clear();
        self.notice = "Collider draft cancelled; authored state unchanged.".into();
    }
    pub(super) fn collider_inspector_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let own = self
            .collider_draft
            .as_ref()
            .is_some_and(|draft| draft.object_id == id);
        let Some(mut draft) = self
            .collider_draft
            .clone()
            .filter(|_| own)
            .or_else(|| ColliderDraft::new(self.session.scene(), &id))
        else {
            return;
        };
        let editable = self.collider_editable(false) && (self.collider_draft.is_none() || own);
        ui.separator();
        ui.strong("Collider").on_hover_text(
            "Edits affect the active object only. Viewport guide shows the active collider.",
        );
        if self.manipulation.ids.len() > 1 {
            ui.label("Editing the active object's collider.");
        }
        ui.label("Primitive query shape. Scripts choose how to respond to contacts.");
        if draft.baseline.is_none() && !own {
            if ui
                .add_enabled(editable, egui::Button::new("Add collider"))
                .clicked()
            {
                draft.mode = ColliderMode::Box;
                draft.enabled = true;
                self.collider_draft = Some(draft);
                self.draft_generation = self.draft_generation.wrapping_add(1);
                ui.memory_mut(|memory| memory.request_focus(Field::Offset(0).id()));
                ui.ctx().request_repaint();
            }
            return;
        }
        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
        let validation = draft.value().err();
        let mut changed = false;
        let mut enter = false;
        let mut escape = false;
        let mut fields = Vec::new();
        let mut error_rect = None;
        let mut shape_id = None;
        let mut discrete_handled = false;
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.label("Shape");
                let focus = ComboFocus::capture(ui, "collider_shape");
                let mut selected = false;
                let before = draft.mode;
                let response = egui::ComboBox::from_id_salt("collider_shape")
                    .width(104.0)
                    .selected_text(match draft.mode {
                        ColliderMode::None => "Remove",
                        ColliderMode::Sphere => "Sphere",
                        ColliderMode::Box => "Box",
                    })
                    .show_ui(ui, |ui| {
                        for (mode, text) in [
                            (ColliderMode::Sphere, "Sphere"),
                            (ColliderMode::Box, "Box"),
                            (ColliderMode::None, "Remove"),
                        ] {
                            if ui.selectable_value(&mut draft.mode, mode, text).clicked() {
                                selected = true;
                                ui.close();
                            }
                        }
                    });
                changed |= before != draft.mode;
                discrete_handled |= selected;
                focus.restore(ui, &response.response, selected);
                shape_id = Some(response.response.id);
                fields.push(response.response);
            });
            if let Some((Field::Shape, message)) = &validation {
                error_rect = Some(ui.colored_label(ui.visuals().error_fg_color, message).rect);
            }
            if draft.mode == ColliderMode::None {
                ui.label("Apply removes the component. Undo restores it.");
            } else {
                let response = ui.checkbox(&mut draft.enabled, "Enabled");
                discrete_handled |= response.clicked();
                changed |= response.changed();
                fields.push(response);
                ui.horizontal(|ui| {
                    ui.label("Role");
                    let focus = ComboFocus::capture(ui, "collider_role");
                    let before = draft.role;
                    let mut selected = false;
                    let response = egui::ComboBox::from_id_salt("collider_role")
                        .width(104.0)
                        .selected_text(match draft.role {
                            ColliderRole::Blocker => "Blocker",
                            ColliderRole::Sensor => "Sensor",
                        })
                        .show_ui(ui, |ui| {
                            for (role, text) in [
                                (ColliderRole::Blocker, "Blocker"),
                                (ColliderRole::Sensor, "Sensor"),
                            ] {
                                if ui.selectable_value(&mut draft.role, role, text).clicked() {
                                    selected = true;
                                    ui.close();
                                }
                            }
                        });
                    changed |= before != draft.role;
                    discrete_handled |= selected;
                    focus.restore(ui, &response.response, selected);
                    fields.push(response.response);
                });
                ui.label("Offset · local scene units");
                for (index, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                    error_rect = collider_field(
                        ui,
                        label,
                        &mut draft.offset[index],
                        Field::Offset(index),
                        &validation,
                        &mut changed,
                        &mut fields,
                    )
                    .or(error_rect);
                }
                match draft.mode {
                    ColliderMode::Sphere => {
                        error_rect = collider_field(
                            ui,
                            "Radius",
                            &mut draft.radius,
                            Field::Radius,
                            &validation,
                            &mut changed,
                            &mut fields,
                        )
                        .or(error_rect);
                        ui.label(
                            "0.001–1000 local units. Enabled spheres need uniform object Scale.",
                        );
                    }
                    ColliderMode::Box => {
                        ui.label("Size · local scene units")
                            .on_hover_text("Full local size before object Scale.");
                        for (index, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                            error_rect = collider_field(
                                ui,
                                label,
                                &mut draft.box_size[index],
                                Field::Size(index),
                                &validation,
                                &mut changed,
                                &mut fields,
                            )
                            .or(error_rect);
                        }
                        ui.label("0.002–2000 each. Object Scale and rotation apply to the box.");
                    }
                    ColliderMode::None => {}
                }
            }
            for response in &fields {
                if editable
                    && !popup_at_start
                    && !egui::Popup::is_any_open(ui.ctx())
                    && (response.has_focus()
                        || ui.memory(|memory| {
                            memory.focused().is_none()
                                && (response.lost_focus()
                                    || memory.had_focus_last_frame(response.id))
                        }))
                {
                    if !discrete_handled {
                        enter |= ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                        });
                    }
                    escape |= ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                }
            }
        });
        if changed {
            self.collider_draft = Some(draft);
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        if (self.collider_draft.is_some() && own) || changed {
            ui.horizontal_wrapped(|ui| {
                let apply = ui.add_enabled(editable, egui::Button::new("Apply collider"));
                let cancel = ui.add_enabled(editable, egui::Button::new("Cancel collider"));
                for response in [&apply, &cancel] {
                    if response.gained_focus() {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                }
                if editable
                    && !popup_at_start
                    && !egui::Popup::is_any_open(ui.ctx())
                    && [&apply, &cancel].iter().any(|response| {
                        response.has_focus()
                            || ui.memory(|memory| {
                                memory.focused().is_none()
                                    && memory.had_focus_last_frame(response.id)
                            })
                    })
                {
                    escape |= ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                }
                if (apply.clicked() || enter) && !self.apply_collider_draft() {
                    if let Some((field, _)) = self
                        .collider_draft
                        .as_ref()
                        .and_then(|draft| draft.value().err())
                    {
                        if let Some(id) = if field == Field::Shape {
                            shape_id
                        } else {
                            Some(field.id())
                        } {
                            ui.memory_mut(|memory| memory.request_focus(id));
                        }
                        if let Some(rect) = error_rect {
                            ui.scroll_to_rect(rect, Some(egui::Align::Center));
                        }
                        ui.ctx().request_repaint();
                    }
                }
                if cancel.clicked() || escape {
                    self.cancel_collider_draft();
                }
            });
        }
    }
}

fn collider_field(
    ui: &mut egui::Ui,
    label: &str,
    text: &mut String,
    field: Field,
    validation: &Option<(Field, String)>,
    changed: &mut bool,
    fields: &mut Vec<egui::Response>,
) -> Option<egui::Rect> {
    let mut error_rect = None;
    let response = ui
        .horizontal(|ui| {
            ui.label(label);
            ui.add(
                egui::TextEdit::singleline(text)
                    .id(field.id())
                    .char_limit(64)
                    .desired_width(92.0),
            )
        })
        .inner;
    *changed |= response.changed();
    if let Some((_, message)) = validation.as_ref().filter(|(invalid, _)| *invalid == field) {
        let error = ui.colored_label(ui.visuals().error_fg_color, message);
        error_rect = Some(response.rect.union(error.rect));
    }
    let reveal_rect = error_rect.unwrap_or(response.rect);
    // Add requests focus before the field first exists, so that first genuine
    // focused response can be clipped without reporting gained_focus.
    if response.has_focus()
        && (response.gained_focus() || !ui.clip_rect().contains_rect(reveal_rect))
    {
        ui.scroll_to_rect(reveal_rect, Some(egui::Align::Center));
    }
    fields.push(response);
    error_rect
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stroke(key: egui::Key, modifiers: egui::Modifiers) -> Vec<egui::Event> {
        vec![
            egui::Event::ModifiersChanged(modifiers),
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            },
            egui::Event::Key {
                key,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers,
            },
            egui::Event::ModifiersChanged(egui::Modifiers::NONE),
        ]
    }
    fn visible_text_rect(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<egui::Rect> {
        fn find(shape: &egui::epaint::Shape, clip: egui::Rect, label: &str) -> Option<egui::Rect> {
            match shape {
                egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    clip.contains_rect(rect).then_some(rect)
                }
                egui::epaint::Shape::Vec(shapes) => {
                    shapes.iter().find_map(|shape| find(shape, clip, label))
                }
                _ => None,
            }
        }
        shapes
            .iter()
            .find_map(|shape| find(&shape.shape, shape.clip_rect, label))
    }
    fn visible_text(shapes: &[egui::epaint::ClippedShape], label: &str) -> bool {
        visible_text_rect(shapes, label).is_some()
    }

    // Real production inspector/scroll/theme protects field focus and clipped
    // validation feedback; shared Scene tests own primitive geometry semantics.
    #[test]
    fn collider_inspector_preserves_raw_keyboard_focus_and_one_component_transaction() {
        for size in [egui::vec2(1280.0, 800.0), egui::vec2(640.0, 480.0)] {
            let mut scene = Scene::demo();
            scene.objects[0].collider = Some(Collider {
                enabled: true,
                shape: ColliderShape::Sphere { radius: 0.25 },
                ..Default::default()
            });
            let mut editor = SceneEditor::new(ruster_scene::Session::new(scene).unwrap());
            editor.select_object(Some("left".into()), false);
            let baseline = editor.session.snapshot_bytes().unwrap();
            let preview = editor.preview.clone();
            let ctx = egui::Context::default();
            crate::editor_theme::configure(&ctx);
            let mut time = 0.0;
            let current_offset_response = std::cell::RefCell::new(None);
            let mut frame = |editor: &mut SceneEditor, events| {
                time += 0.1;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        ui.set_max_width(240.0);
                        editor.inspector_scroll_ui(ui);
                        // Read during this pass. After run_ui, egui's swapped
                        // widget maps can return the prior clipped rectangle.
                        current_offset_response.replace(ctx.read_response(Field::Offset(0).id()));
                    },
                );
                output.textures_delta.clear();
                output.shapes
            };
            frame(&mut editor, vec![]);
            frame(&mut editor, vec![]);
            let radius = Field::Radius.id();
            // egui omits fully clipped widgets from read_response. Reach the
            // field through the actual inspector scroll before querying its rect.
            for _ in 0..16 {
                let response = ctx.read_response(radius);
                if response
                    .as_ref()
                    .is_some_and(|response| response.interact_rect.contains_rect(response.rect))
                {
                    break;
                }
                let delta = response.map_or(-size.y * 0.25, |response| {
                    size.y * 0.5 - response.rect.center().y
                });
                let point = egui::pos2(100.0, size.y * 0.5);
                frame(
                    &mut editor,
                    vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::vec2(0.0, delta),
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                        },
                    ],
                );
                // Complete this finite native-style wheel gesture before using
                // a cached field rectangle for a later pointer click.
                frame(
                    &mut editor,
                    vec![egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::Vec2::ZERO,
                        modifiers: egui::Modifiers::NONE,
                        phase: egui::TouchPhase::End,
                        source: egui::MouseWheelSource::Unknown,
                    }],
                );
                frame(&mut editor, vec![]);
            }
            let mut previous = None;
            let mut stable = false;
            for _ in 0..24 {
                frame(&mut editor, vec![]);
                let current = ctx.read_response(radius).map(|response| response.rect);
                if current.is_some()
                    && current == previous
                    && ctx.input(|input| input.smooth_scroll_delta == egui::Vec2::ZERO)
                {
                    stable = true;
                    break;
                }
                previous = current;
            }
            assert!(
                stable,
                "finished scroll must leave a stationary field before clicking"
            );
            let response = ctx
                .read_response(radius)
                .expect("Radius must be reached by bounded real inspector scrolling");
            assert!(
                response.interact_rect.contains_rect(response.rect),
                "Radius must be genuinely visible before clicking"
            );
            let point = response.rect.center();
            let pointer = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(
                &mut editor,
                vec![egui::Event::PointerMoved(point), pointer(true)],
            );
            frame(&mut editor, vec![pointer(false)]);
            assert!(
                ctx.memory(|memory| memory.has_focus(radius)),
                "actual pointer click must focus Radius before typing"
            );
            let mut typing = stroke(
                egui::Key::A,
                egui::Modifiers {
                    ctrl: true,
                    command: true,
                    ..egui::Modifiers::NONE
                },
            );
            typing.push(egui::Event::Text("bad radius".into()));
            frame(&mut editor, typing);
            assert_eq!(
                editor
                    .collider_draft
                    .as_ref()
                    .map(|draft| draft.radius.as_str()),
                Some("bad radius"),
                "invalid text must produce a draft before Enter"
            );
            assert_eq!(editor.session.snapshot_bytes().unwrap(), baseline);
            frame(&mut editor, stroke(egui::Key::Enter, egui::Modifiers::NONE));
            frame(&mut editor, vec![]);
            let shapes = frame(&mut editor, vec![]);
            assert_eq!(editor.collider_draft.as_ref().unwrap().radius, "bad radius");
            assert_eq!(editor.session.snapshot_bytes().unwrap(), baseline);
            assert!(ctx.memory(|memory| memory.has_focus(radius)));
            assert!(ctx
                .read_response(radius)
                .unwrap()
                .interact_rect
                .contains_rect(ctx.read_response(radius).unwrap().rect));
            assert!(visible_text(
                &shapes,
                "Radius requires a finite number from 0.001 to 1000."
            ));
            frame(&mut editor, stroke(egui::Key::Tab, egui::Modifiers::NONE));
            frame(&mut editor, vec![]);
            assert!(!ctx.memory(|memory| memory.has_focus(radius)));
            frame(
                &mut editor,
                stroke(egui::Key::Escape, egui::Modifiers::NONE),
            );
            assert!(editor.collider_draft.is_none());
            assert_eq!(editor.session.snapshot_bytes().unwrap(), baseline);

            let mut draft = ColliderDraft::new(editor.session.scene(), "left").unwrap();
            draft.box_size = ["1.2".into(), "2".into(), "3".into()];
            draft.mode = ColliderMode::Box;
            draft.box_size[0] = "0.001999999999".into();
            assert!(matches!(draft.value(), Err((Field::Size(0), _))));
            draft.box_size[0] = "1.2".into();
            assert_eq!(
                draft.value().unwrap().unwrap().shape,
                ColliderShape::Box {
                    half_extents: [0.6, 1.0, 1.5]
                }
            );
            draft.mode = ColliderMode::Sphere;
            draft.radius = "0.3".into();
            assert_eq!(draft.box_size[0], "1.2");
            editor.collider_draft = Some(draft);
            assert!(editor.apply_collider_draft(), "{}", editor.error);
            assert_eq!(editor.session.undo_len(), 1);
            assert_eq!(editor.preview, preview);
            editor.history(false);
            assert_eq!(editor.session.scene().objects, preview.scene.objects);
            editor.history(true);
            let mut remove = ColliderDraft::new(editor.session.scene(), "left").unwrap();
            remove.mode = ColliderMode::None;
            editor.collider_draft = Some(remove);
            assert!(editor.apply_collider_draft());
            assert!(editor.session.scene().objects[0].collider.is_none());
            let removed = editor.session.snapshot_bytes().unwrap();
            let mut shapes;
            {
                let mut scroll = |editor: &mut SceneEditor, delta| {
                    let point = egui::pos2(100.0, size.y * 0.5);
                    frame(
                        editor,
                        vec![
                            egui::Event::PointerMoved(point),
                            egui::Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: egui::vec2(0.0, delta),
                                modifiers: egui::Modifiers::NONE,
                                phase: egui::TouchPhase::Move,
                                source: egui::MouseWheelSource::Unknown,
                            },
                        ],
                    );
                    frame(
                        editor,
                        vec![egui::Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: egui::Vec2::ZERO,
                            modifiers: egui::Modifiers::NONE,
                            phase: egui::TouchPhase::End,
                            source: egui::MouseWheelSource::Unknown,
                        }],
                    );
                    let mut shapes = frame(editor, vec![]);
                    for _ in 0..24 {
                        shapes = frame(editor, vec![]);
                        if ctx.input(|input| input.smooth_scroll_delta == egui::Vec2::ZERO) {
                            break;
                        }
                    }
                    shapes
                };
                // Return through the real scroll owner, then locate the actual
                // Add button. Focus is obtained only by clicking that button.
                shapes = scroll(&mut editor, 10_000.0);
                for _ in 0..16 {
                    if visible_text_rect(&shapes, "Add collider").is_some() {
                        break;
                    }
                    shapes = scroll(&mut editor, -size.y * 0.25);
                }
            }
            let mut previous = None;
            let mut stable = false;
            for _ in 0..24 {
                shapes = frame(&mut editor, vec![]);
                let current = visible_text_rect(&shapes, "Add collider");
                if current.is_some()
                    && current == previous
                    && ctx.input(|input| input.smooth_scroll_delta == egui::Vec2::ZERO)
                {
                    stable = true;
                    break;
                }
                previous = current;
            }
            assert!(
                stable,
                "Add collider must be visible and stationary before clicking"
            );
            let point = visible_text_rect(&shapes, "Add collider").unwrap().center();
            let pointer = |pressed| egui::Event::PointerButton {
                pos: point,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(
                &mut editor,
                vec![egui::Event::PointerMoved(point), pointer(true)],
            );
            frame(&mut editor, vec![pointer(false)]);
            frame(&mut editor, vec![]);
            frame(&mut editor, vec![]);
            let first = Field::Offset(0).id();
            assert!(editor.collider_draft.is_some());
            assert!(
                ctx.memory(|memory| memory.has_focus(first)),
                "Add collider must focus the actual first Offset field"
            );
            let response = current_offset_response
                .borrow()
                .clone()
                .expect("Focused first Offset must be visible after Add collider");
            assert!(
                response.interact_rect.contains_rect(response.rect),
                "Focused first Offset must be fully inside the current inspector clip: rect={:?}, interaction={:?}", response.rect, response.interact_rect
            );
            frame(
                &mut editor,
                stroke(egui::Key::Escape, egui::Modifiers::NONE),
            );
            assert!(editor.collider_draft.is_none());
            assert_eq!(editor.session.snapshot_bytes().unwrap(), removed);
            editor.history(false);
            assert!(editor.session.scene().objects[0].collider.is_some());
        }
    }
}
