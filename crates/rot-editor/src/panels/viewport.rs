//! Viewport panel body, moved out of scene_editor.rs (S-07d).

use super::super::*;

impl SceneEditor {
    /// The Scene or Game view: everything inside its dock tab (S-07a).
    pub(in crate::scene_editor) fn viewport_pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        about_open: bool,
    ) {
        let ctx = ui.ctx().clone();
        if self.workspace.welcome
            && self.session.path().is_none()
            && self.session.scene().revision == 0
            && self.session.scene().objects.is_empty()
            && !self.runtime.visible
            && !self.asset_loading()
        {
            self.scene_welcome(ui);
        }
        if self.asset_loading() {
            self.runtime.cancel_control_focus();
            self.captures.rect = None;
            self.captures.visible_crop = None;
            self.captures.receipt = None;
            ui.heading(crate::strings::tr(
                ui.ctx(),
                crate::strings::Text::LoadingContent,
            ));
            ui.label("The scene and your saved edits are retained while content loads.");
            // Stop remains available for an older retained runtime while
            // Reset and playback edits wait for content authority.
            let shortcut = self.workspace.shortcut_text(Action::FocusRuntime);
            let _ = self.runtime.controls_with_sounds(
                ui,
                &self.session,
                self.generation,
                true,
                &shortcut,
                self.assets.as_ref(),
            );
            return;
        }
        if !self.player_preview_available() {
            self.set_player_preview(false);
        }
        let document_blocked =
            about_open || self.guard().is_err() || self.manipulation.offset_pending();
        if document_blocked || self.input_actions_open {
            self.runtime.cancel_control_focus();
        }
        let blocked = document_blocked || self.asset_busy();
        self.runtime.observe(
            self.generation,
            self.session.scene().revision,
            self.assets.as_ref().map(|store| store.revision()),
            self.shader_revision,
        );
        self.asset_mark_callback_owners();
        let transient = if !self.runtime.visible && !self.navigation.player_view {
            self.manipulation.transient_scene(self.session.scene())
        } else {
            None
        };
        let player_view = self.navigation.player_view;
        let selected: Vec<_> = if player_view {
            Vec::new()
        } else {
            self.manipulation.ids.iter().map(String::as_str).collect()
        };
        // At a compact height, the View menu retains projection and
        // presets while the recovered Inspector draft needs the extra row.
        self.navigation.show_controls =
            !(self.recovered_input.is_some() && ui.available_height() < 215.0);
        self.navigation.selection_feedback = true;
        self.navigation.show_editor_grid = !player_view;
        self.navigation.focusable =
            self.runtime.input_owned() || (self.runtime.visible && self.runtime.is_playing());
        self.navigation.enabled = !player_view
            && !self.dock_dragging
            && !about_open
            && !self.runtime.input_owned()
            && !self.export_dialog
            && !self.close_approved
            && !self.manipulation.active()
            && !self.workspace.settings_open
            && !self.recovery.open
            && self.recovery_transition.is_none()
            && !self.asset_dialog
            && self.dialog.is_none()
            && !self.rename_pending
            && !self.new_scene_pending
            && !self.close_pending;
        self.navigation.keyboard_enabled =
            self.navigation.enabled && !ctx.egui_wants_keyboard_input() && !self.drafts();
        self.navigation.movement_speed = self.workspace.prefs.movement_speed;
        let authored_view = transient.as_ref().unwrap_or(self.session.scene());
        let shown_frame = self.runtime.displayed_frame().cloned();
        let mut mixed_fit_error = None;
        let mut mixed_fit_succeeded = false;
        if !player_view
            && self.navigation.frame_selected
            && self
                .manipulation
                .ids
                .iter()
                .any(|id| authored_view.lighting.light(id).is_some())
        {
            // The shared runtime renderer frames geometry. Editor light
            // anchors participate only in the authored view, never its runtime.
            self.navigation.frame_selected = false;
            if !self.runtime.visible && self.navigation.enabled {
                if let Some(camera) = self.navigation.camera.and_then(|camera| {
                    lighting_viewport::frame_selection(
                        authored_view,
                        &self.manipulation.ids,
                        &self.resolved,
                        camera,
                        ui.available_width() / ui.available_height().max(1.0),
                    )
                }) {
                    self.navigation.camera = Some(camera);
                    self.navigation.clear_fit_error = true;
                    mixed_fit_succeeded = true;
                } else {
                    mixed_fit_error = Some(rot_render::FIT_ERROR);
                }
            } else if self.runtime.visible {
                self.notice = "Switch to the Scene view to frame selected lights.".into();
            }
        }
        if let Some(error) = mixed_fit_error {
            self.error = error.into();
            ui.colored_label(ui.visuals().warn_fg_color, error);
        }
        let shown_scene = shown_frame
            .as_ref()
            .map_or(authored_view, |frame| frame.scene.as_ref());
        // The view fills its dock tab; no panel covers it (S-07a).
        self.navigation.overlay_origin = None;
        let mut surface =
            rot_render::allocate_viewport(ui, shown_scene, &selected, &mut self.navigation);
        let hud_rect = surface.rect;
        let authoring = shown_frame
            .as_ref()
            .map_or(self.session.authoring(), |frame| frame.authoring.as_ref());
        let scope = shown_frame.as_ref().map_or_else(
            || {
                egui::Id::unique((
                    "hud-preview",
                    self.generation,
                    self.session.scene().revision,
                ))
            },
            |frame| {
                egui::Id::unique((
                    "hud-runtime",
                    frame.key.generation,
                    frame.key.reset_epoch,
                    frame.key.authored_revision,
                ))
            },
        );
        let (hud_owns, hud_error) = hud_interaction::interact(
            ui,
            hud_interaction::HudViewport {
                rect: hud_rect,
                frame: shown_frame.as_ref(),
                authoring,
                scope,
            },
            &mut self.hud_cache,
            &mut self.hud_controls,
            &mut self.runtime,
            blocked,
        );
        if let Some(error) = hud_error {
            self.error = error;
        }
        surface.input_owned = hud_owns;
        let viewport = rot_render::render_viewport_with_materials(
            ui,
            shown_scene,
            &selected,
            if self.runtime.visible {
                &self.preview_assets
            } else {
                &self.resolved
            },
            if self.runtime.visible {
                &self.preview_textures
            } else {
                &self.resolved_textures
            },
            &mut self.navigation,
            surface,
        );
        // The wheel changes fly speed while the right button is held.
        self.workspace
            .set_movement_speed(self.navigation.movement_speed);
        if let Some(error) = viewport.fit_error.filter(|_| viewport.fit_attempted) {
            self.error = error.into();
        } else if (mixed_fit_succeeded || viewport.fit_attempted)
            && mixed_fit_error.is_none()
            && self.error == rot_render::FIT_ERROR
        {
            self.error.clear();
        }
        // Clicking the running Game view gives it the keyboard.
        let has_actions = !self.session.authoring().actions.is_empty();
        let playing_view = self.runtime.visible && self.runtime.is_playing();
        self.runtime.input_control_click(
            &ctx,
            blocked || hud_owns,
            has_actions,
            playing_view && viewport.response.clicked(),
        );
        self.runtime
            .input_finish(&ctx, &viewport.response, blocked || hud_owns);
        self.captures.rect = Some(viewport.rect);
        self.captures.visible_crop = Some(hud_rect);
        self.captures.view = Some(viewport.view);
        self.captures.projection = Some(viewport.projection);
        self.captures.receipt = Some(viewport.receipt.clone());
        self.view_state.input_active = ui.input(|input| input.pointer.any_down());
        let may_draw = !player_view
            && !about_open
            && !self.asset_publishing()
            && !self.asset_loading()
            && !self.runtime.visible
            && !self.drafts()
            && self.dialog.is_none()
            && !self.asset_dialog
            && !self.close_pending
            && !self.close_approved
            && !self.new_scene_pending
            && !self.rename_pending
            && !self.workspace.settings_open
            && !self.recovery.open
            && self.recovery_transition.is_none();
        let can_pick = self.navigation.enabled && !self.manipulation.active() && !hud_owns;
        if !player_view && !self.runtime.visible {
            let active = self.selected.as_slice();
            collider_viewport::paint(ui, &viewport, authored_view, active);
        } else if !player_view && !self.runtime.stale {
            if let Some(frame) = self
                .runtime
                .displayed_frame()
                .filter(|frame| Some(frame.key) == self.runtime.current_run())
            {
                if let Some(observations) = &frame.query_observations {
                    collider_viewport::paint_contacts(
                        ui,
                        &viewport,
                        frame.scene.as_ref(),
                        frame.tick,
                        observations,
                    );
                }
            }
        }
        // Passive xray icons precede transform handles in paint order;
        // handles retain first refusal on a click, then icons, then surfaces.
        let light_pick = if !player_view && !self.runtime.visible {
            lighting_viewport::paint_and_pick(
                ui,
                &viewport,
                authored_view,
                &self.manipulation.ids,
                can_pick,
            )
        } else {
            None
        };
        let overlay_owns = if may_draw {
            let before = self.session.scene().revision;
            let result = self.manipulation.draw_with_pointer_block(
                ui,
                &viewport,
                &mut self.session,
                hud_owns,
            );
            if before != self.session.scene().revision {
                self.sync_draft();
                self.notice = "Transform committed; Undo restores the whole selection.".into();
            }
            match result {
                Ok(owns) => owns,
                Err(error) => {
                    self.error = error;
                    true
                }
            }
        } else {
            false
        };
        if viewport.response.clicked() && !overlay_owns && can_pick {
            let id = light_pick.clone().or(viewport.picked.clone()).filter(|id| {
                lighting_viewport::selectable_target(
                    self.session.scene(),
                    id,
                    !self.runtime.visible,
                )
            });
            let additive =
                ui.input(|i| i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift);
            self.queued_selection = Some((id, additive));
        }
        // Double-clicking an object selects and frames it, like F. The
        // first click already shows the transform handles under the
        // pointer, so the second click may land on them.
        if viewport.response.double_clicked() && !hud_owns && !self.runtime.visible && !player_view
        {
            if let Some(id) = light_pick
                .or(viewport.picked)
                .filter(|id| lighting_viewport::selectable_target(self.session.scene(), id, true))
            {
                self.queued_selection = Some((Some(id), false));
                self.navigation.frame_selected = true;
                ui.ctx().request_repaint();
            }
        }
        if let Some(ids) = viewport.box_selection.filter(|_| !overlay_owns && can_pick) {
            let additive =
                ui.input(|i| i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift);
            self.queued_box_selection = Some((ids, additive));
        }
        self.paint_hud_overlay(ui, hud_rect, shown_frame.as_ref());
        sidebars::round_viewport(ui, hud_rect);
        self.runtime.paint_play_border(ui, hud_rect);
    }
}
