//! Hierarchy panel body, moved out of scene_editor.rs (S-07d).

use super::super::*;
use crate::strings::Text;

impl SceneEditor {
    pub(in crate::scene_editor) fn hierarchy_contents(&mut self, ui: &mut egui::Ui) {
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none());
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape) && i.modifiers.is_none());
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.hierarchy_query)
                .hint_text(Text::SearchObjectsOrLights.plain())
                .char_limit(128)
                .desired_width(f32::INFINITY),
        );
        if self.hierarchy_focus {
            search.request_focus();
            search.scroll_to_me(Some(egui::Align::Min));
            self.hierarchy_focus = false;
        }
        if (search.has_focus() || search.lost_focus()) && escape {
            self.hierarchy_query.clear();
            search.surrender_focus();
        }
        self.hierarchy_search
            .update(self.session.scene(), &self.hierarchy_query);
        if (search.has_focus() || search.lost_focus()) && enter {
            if let Some(index) = self.hierarchy_search.visible.first() {
                self.queued_selection =
                    Some((Some(self.hierarchy_search.rows[*index].id.clone()), false));
                search.surrender_focus();
            } else {
                self.notice = Text::NoObjectsMatchTheSearch.plain().into();
            }
        }
        ui.horizontal_wrapped(|ui| {
            self.create_menu(ui);
            self.new_group_button(ui);
            for (icon, action) in [
                (toolbar_icons::Icon::Duplicate, Action::Duplicate),
                (toolbar_icons::Icon::Delete, Action::Delete),
            ] {
                if icon
                    .button(
                        ui,
                        self.command_enabled(action),
                        None,
                        &self.workspace.shortcut_text(action),
                    )
                    .clicked()
                {
                    self.queued_context = Some((None, action));
                }
            }
        });
        let hidden = self
            .manipulation
            .ids
            .iter()
            .filter(|id| {
                !self
                    .hierarchy_search
                    .visible
                    .iter()
                    .any(|i| &self.hierarchy_search.rows[*i].id == *id)
            })
            .count();
        let count = self.hierarchy_search.rows.len();
        let visible = self.hierarchy_search.visible.len();
        let selected_count = self.manipulation.ids.len();
        let item_label = if count == 1 { "item" } else { "items" };
        let mut summary = if visible == count {
            format!("{count} {item_label}")
        } else {
            format!("{visible} of {count} {item_label}")
        };
        if selected_count > 0 {
            summary.push_str(&format!(" · {selected_count} selected"));
        }
        if hidden > 0 {
            summary.push_str(&format!(" · {hidden} hidden"));
        }
        if visible > 0 || selected_count > 0 {
            ui.label(egui::RichText::new(summary).small().weak());
        }
        if !self.hierarchy_query.is_empty() && ui.button(Text::ClearObjectSearch.plain()).clicked()
        {
            self.hierarchy_query.clear();
            self.hierarchy_search.update(self.session.scene(), "");
        }
        if self.hierarchy_search.visible.is_empty() {
            ui.label(if self.hierarchy_query.is_empty() {
                crate::strings::tr(ui.ctx(), crate::strings::Text::EmptyHierarchy)
            } else {
                Text::NoObjectsOrLightsMatch.plain()
            });
        }
        let mut clicked = None;
        let mut context = None;
        let mut dropped = None;
        egui::ScrollArea::vertical()
            .id_salt("scene_object_scroll")
            .show(ui, |ui| {
                crate::focus::scroll_focus_into_view(ui);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.spacing_mut().interact_size.y = crate::ui_kit::tokens::ROW_HEIGHT;
                ui.spacing_mut().button_padding.y = 3.0;
                let mut lights_heading = false;
                for index in &self.hierarchy_search.visible {
                    let row = &self.hierarchy_search.rows[*index];
                    if row.light && !lights_heading && self.session.scene().hierarchy.is_empty() {
                        ui.separator();
                        ui.strong(Text::Lights.plain());
                        lights_heading = true;
                    }
                    let selected = self.manipulation.ids.contains(&row.id);
                    let response = ui
                        .add(
                            egui::Button::selectable(selected, &row.name)
                                .truncate()
                                .sense(egui::Sense::click_and_drag()),
                        )
                        .on_hover_text(&row.tooltip);
                    if selected && ui.is_rect_visible(response.rect) {
                        let start = response.rect.left_top() + egui::vec2(2.0, 5.0);
                        let end = response.rect.left_bottom() + egui::vec2(2.0, -5.0);
                        ui.painter().line_segment(
                            [start, end],
                            egui::Stroke::new(2.0, crate::editor_theme::colors(ui).accent),
                        );
                    }
                    if response.drag_started() && !row.light {
                        response.dnd_set_drag_payload(crate::behavior_ui::ObjectDrag {
                            generation: self.generation,
                            revision: self.session.scene().revision,
                            id: row.id.clone(),
                        });
                    }
                    if response.clicked() {
                        clicked = Some((
                            Some(row.id.clone()),
                            ui.input(|i| {
                                i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift
                            }),
                        ));
                    }
                    response.context_menu(|ui| {
                        ui.menu_button(Text::ComponentCreate.plain(), |ui| {
                            for action in [
                                Action::CreateEmpty,
                                Action::CreateCube,
                                Action::CreateSphere,
                                Action::CreateCapsule,
                                Action::CreateTriangularPrism,
                                Action::CreateCylinder,
                                Action::CreateDirectionalLight,
                                Action::CreateLight,
                            ] {
                                if ui.button(action.label()).clicked() {
                                    context = Some((None, action));
                                    ui.close();
                                }
                            }
                        });
                        for action in [
                            Action::Rename,
                            Action::Duplicate,
                            Action::Delete,
                            Action::Copy,
                            Action::Paste,
                            Action::Frame,
                        ] {
                            if ui
                                .add(
                                    egui::Button::new(action.label())
                                        .shortcut_text(self.workspace.shortcut_hint(action)),
                                )
                                .clicked()
                            {
                                context = Some((Some(row.id.clone()), action));
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
                                    .validate_mesh_material_compatibility(
                                        &row.id,
                                        &Some(payload.asset_id.clone()),
                                    )
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
                        dropped = Some((row.id.clone(), (*payload).clone()));
                    }
                }
            });
        if let Some(intent) = clicked {
            self.queued_selection = Some(intent);
        }
        if let Some(intent) = context {
            self.queued_context = Some(intent);
        }
        if let Some(intent) = dropped {
            self.queued_drop = Some(intent);
        }
    }
}
