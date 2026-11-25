//! Inspector panel body, moved out of scene_editor.rs (S-07d).

use super::super::*;
use crate::strings::Text;

impl SceneEditor {
    pub(in crate::scene_editor) fn inspector_contents(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(!self.asset_publishing() && !self.asset_loading(), |ui| {
            self.inspector_contents_inner(ui)
        });
    }
    pub(in crate::scene_editor) fn inspector_scroll_ui(&mut self, ui: &mut egui::Ui) {
        self.recovered_input_notice(ui);
        egui::ScrollArea::vertical().animated(false).show(ui, |ui| {
            crate::focus::scroll_focus_into_view(ui);
            self.inspector_contents(ui)
        });
    }
    fn behavior_inspector_ui(&mut self, ui: &mut egui::Ui) {
        let editable = self.behavior_editable();
        let behavior_target = self.selected.as_deref().filter(|id| {
            let scene = self.session.scene();
            scene.hierarchy.groups.contains_key(*id)
                || scene.objects.iter().any(|object| object.id == *id)
        });
        if let Some(action) = self.behavior.inspector(
            ui,
            &self.session,
            behavior_target,
            self.assets.as_ref(),
            self.generation,
            editable,
        ) {
            self.behavior_action(action);
        }
        self.runtime.state_inspector(ui);
        let command_editable =
            self.editing_available() && !self.behavior.pending() && !self.input_actions.pending();
        self.runtime
            .command_controls(ui, &self.session, command_editable);
    }
    fn inspector_contents_inner(&mut self, ui: &mut egui::Ui) {
        if self.inspecting_asset() {
            self.asset_inspector_ui(ui);
            return;
        }
        let title = if self.manipulation.ids.len() > 1 {
            format!("{} selected", self.manipulation.ids.len())
        } else {
            self.selected
                .as_ref()
                .and_then(|id| self.session.scene().target_display_name(id))
                .map(str::to_owned)
                .unwrap_or_else(|| "Inspector".into())
        };
        ui.horizontal_wrapped(|ui| {
            let width = ui.available_width();
            let title_width = if width >= 210.0 { width - 148.0 } else { width };
            ui.allocate_ui_with_layout(
                egui::vec2(title_width, ui.spacing().interact_size.y),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.set_min_width(title_width);
                    ui.add(egui::Label::new(egui::RichText::new(&title).heading()).truncate())
                        .on_hover_text(&title);
                },
            );
            ui.menu_button(Text::Actions.plain(), |ui| {
                if ui
                    .add_enabled(
                        self.selected.is_some(),
                        egui::Button::new(Text::RenameObject.plain()),
                    )
                    .clicked()
                {
                    match self.guard() {
                        Ok(()) => self.rename_pending = true,
                        Err(error) => self.error = error,
                    };
                    ui.close();
                }
                for (label, action) in [("Duplicate", "duplicate"), ("Delete", "delete")] {
                    if ui.button(label).clicked() {
                        self.object_action(action);
                        ui.close();
                    }
                }
                if ui.button(Text::LabelCopy.plain()).clicked() {
                    self.clipboard_action(false);
                    ui.close();
                }
                if ui.button(Text::LabelPaste.plain()).clicked() {
                    self.clipboard_action(true);
                    ui.close();
                }
            });
            if ui
                .add_enabled(
                    self.hud_draft.is_some() || self.hud_editable(),
                    egui::Button::new(Text::Hud.plain()),
                )
                .on_hover_text(Text::InspectorEditTheCreationsScreen.plain())
                .clicked()
            {
                self.hud_open = true;
                self.runtime.release_input(ui.ctx());
            }
        });
        if self.selected.is_none() {
            ui.label(egui::RichText::new(Text::InspectorSelectAnObjectOr.plain()).weak());
            if self.runtime.visible {
                ui.strong(Text::Components.plain());
                ui.small(crate::strings::tr(
                    ui.ctx(),
                    crate::strings::Text::EditsRestartPlay,
                ));
            }
            self.ambient_inspector_ui(ui);
            self.display_inspector_ui(ui);
            if self.manipulation.offset_pending() {
                ui.add_enabled_ui(
                    self.color_draft.is_none()
                        && self.material_draft.is_none()
                        && self.lighting_draft.is_none()
                        && self.display_draft.is_none()
                        && self.collider_draft.is_none()
                        && !self.behavior.pending()
                        && !self.runtime.command_draft_pending()
                        && !self.input_actions.pending()
                        && self.recovered_input.is_none(),
                    |ui| self.manipulation_ui(ui),
                );
            }
            self.behavior_inspector_ui(ui);
            return;
        }
        self.assembly_parent_ui(ui);
        if self.runtime.visible {
            self.runtime_transform_ui(ui);
        } else {
            ui.separator();
            ui.strong(Text::ComponentTransform.plain());
            if self.draft != self.baseline_fields {
                ui.add_enabled_ui(
                    (self.hud_draft.is_none()
                        && self.color_draft.is_none()
                        && self.material_draft.is_none()
                        && self.lighting_draft.is_none()
                        && self.display_draft.is_none()
                        && self.collider_draft.is_none())
                        && !self.behavior.pending()
                        && !self.runtime.command_draft_pending()
                        && !self.input_actions.pending()
                        && self.recovered_input.is_none()
                        && !self.manipulation.active()
                        && self.dialog.is_none()
                        && !self.close_approved
                        && !self.rename_pending
                        && !self.new_scene_pending
                        && !self.close_pending
                        && !self.workspace.settings_open
                        && !self.recovery.open
                        && self.recovery_transition.is_none(),
                    |ui| {
                        ui.label(Text::InspectorRecoveredTransformFieldsRepair.plain());
                        let mut enter = false;
                        let mut escape = false;
                        let mut changed = false;
                        if let Some(fields) = &mut self.draft {
                            for (label, values) in [
                                ("Position", &mut fields.translation),
                                (fields.rotation_unit.label(), &mut fields.rotation),
                                ("Scale", &mut fields.scale),
                            ] {
                                ui.label(label);
                                ui.horizontal_wrapped(|ui| {
                                    for (axis, text) in ["X", "Y", "Z"].into_iter().zip(values) {
                                        ui.label(axis);
                                        let response = ui.add(
                                            egui::TextEdit::singleline(text)
                                                .desired_width(70.0)
                                                .char_limit(1024)
                                                .interactive(ui.is_enabled()),
                                        );
                                        changed |= response.changed();
                                        let owned = response.has_focus()
                                            || ui.memory(|m| m.had_focus_last_frame(response.id));
                                        enter |=
                                            owned && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                        escape |=
                                            owned && ui.input(|i| i.key_pressed(egui::Key::Escape));
                                    }
                                });
                            }
                        }
                        if changed {
                            self.draft_generation = self.draft_generation.wrapping_add(1);
                        }
                        if ui
                            .button(Text::ApplyRecoveredTransformFields.plain())
                            .clicked()
                            || enter
                        {
                            self.apply();
                        }
                        if ui.button(Text::RevertTransformDrafts.plain()).clicked() || escape {
                            self.sync_draft();
                        }
                    },
                );
            }
            ui.add_enabled_ui(
                (self.hud_draft.is_none()
                    && self.color_draft.is_none()
                    && self.material_draft.is_none()
                    && self.lighting_draft.is_none()
                    && self.display_draft.is_none()
                    && self.collider_draft.is_none())
                    && self.draft == self.baseline_fields
                    && !self.behavior.pending()
                    && !self.runtime.command_draft_pending()
                    && !self.input_actions.pending()
                    && self.recovered_input.is_none()
                    && !self.manipulation.ids.is_empty(),
                |ui| {
                    let precision_pending = self.manipulation.precision_pending();
                    match self.manipulation.precision_ui(
                        ui,
                        &mut self.session,
                        self.dialog.is_none()
                            && !self.close_approved
                            && !self.rename_pending
                            && !self.new_scene_pending
                            && !self.close_pending
                            && !self.workspace.settings_open
                            && !self.recovery.open
                            && self.recovery_transition.is_none(),
                    ) {
                        Ok(true) => {
                            self.sync_draft();
                            self.error.clear();
                            self.notice = Text::TransformsApplied.plain().into();
                        }
                        Ok(false)
                            if precision_pending && !self.manipulation.precision_pending() =>
                        {
                            self.error.clear()
                        }
                        Ok(false) => {}
                        Err(error) => self.error = error,
                    }
                },
            );
            ui.add_enabled_ui(
                self.hud_draft.is_none()
                    && self.color_draft.is_none()
                    && self.material_draft.is_none()
                    && self.lighting_draft.is_none()
                    && self.display_draft.is_none()
                    && self.collider_draft.is_none()
                    && !self.behavior.pending()
                    && !self.runtime.command_draft_pending()
                    && !self.input_actions.pending()
                    && self.recovered_input.is_none(),
                |ui| self.manipulation_ui(ui),
            );
        }
        if self.runtime.visible {
            ui.separator();
            ui.strong(Text::Components.plain());
        }
        self.light_inspector_ui(ui);
        ui.separator();
        if let Some(object) = self
            .selected
            .as_ref()
            .filter(|_| self.manipulation.ids.len() > 1)
            .and_then(|id| {
                self.session
                    .scene()
                    .objects
                    .iter()
                    .find(|object| &object.id == id)
            })
        {
            ui.label(format!("Active object: {}", object.display_name()));
        }
        if self.selected.as_deref().is_some_and(|id| {
            self.session
                .scene()
                .objects
                .iter()
                .any(|object| object.id == id)
        }) {
            ui.strong(Text::Mesh.plain());
            self.mesh_reference_ui(ui);
            self.collider_inspector_ui(ui);
            self.material_inspector_ui(ui);
            self.color_inspector_ui(ui);
        }
        self.animation_inspector_ui(ui);
        self.behavior_inspector_ui(ui);
        self.ambient_inspector_ui(ui);
        self.display_inspector_ui(ui);
        ui.separator();
        egui::CollapsingHeader::new(Text::Details.plain())
            .id_salt("object_details")
            .show(ui, |ui| {
                if let Some(id) = &self.selected {
                    ui.label(format!("Stable ID: {id}"));
                }
                if self.manipulation.ids.len() > 1 {
                    ui.label(Text::InspectorTheLastSelectedObject.plain());
                }
                ui.label(format!(
                    "Loaded scene format v{}; Save writes v{}.",
                    self.session.loaded_format_version(),
                    self.session.save_format_version()
                ));
            });
    }
    pub(in crate::scene_editor) fn mesh_reference_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some((mesh, primitive)) = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == id)
            .map(|object| (object.mesh.clone(), object.primitive))
        else {
            return;
        };
        let response = egui::Frame::group(ui.style())
            .show(ui, |ui| {
                ui.label(mesh.as_deref().unwrap_or_else(|| {
                    primitive.map_or(Text::LabelAssignCube.plain(), |shape| match shape {
                        rot_scene::Primitive::Sphere => Text::PrimitiveSphere.plain(),
                        rot_scene::Primitive::Capsule => Text::PrimitiveCapsule.plain(),
                        rot_scene::Primitive::Cylinder => Text::LabelCreateCylinder.plain(),
                        rot_scene::Primitive::Plane => Text::LabelCreatePlane.plain(),
                        rot_scene::Primitive::TriangularPrism => {
                            Text::LabelCreateTriangularPrism.plain()
                        }
                    })
                }));
                if let Some(mesh) = &mesh {
                    if ui.button(Text::LocateMeshInContent.plain()).clicked() {
                        self.locate_mesh(mesh);
                    }
                }
            })
            .response
            .on_hover_text(Text::MeshDropHint.plain());
        response.context_menu(|ui| {
            if ui.button(Text::UseProceduralCube.plain()).clicked() {
                match self.assign_mesh_to(&id, None) {
                    Ok(_) => self.error.clear(),
                    Err(error) => self.error = error,
                }
                ui.close();
            }
            if let Some(mesh) = &mesh {
                if ui.button(Text::LocateMeshInContent.plain()).clicked() {
                    self.locate_mesh(mesh);
                    ui.close();
                }
            }
        });
        if response
            .dnd_hover_payload::<AssetDrag>()
            .is_some_and(|payload| {
                self.resolved.contains_key(&payload.asset_id)
                    && self.check_asset_drag(&payload).is_ok()
                    && self
                        .validate_mesh_material_compatibility(&id, &Some(payload.asset_id.clone()))
                        .is_ok()
            })
        {
            ui.painter().rect_stroke(
                response.rect,
                3.0,
                egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(payload) = response.dnd_release_payload::<AssetDrag>() {
            self.queued_drop = Some((id, (*payload).clone()));
        }
    }
}
