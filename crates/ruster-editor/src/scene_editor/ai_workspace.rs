//! Window-local presentation, never document or connection authority.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Drawer {
    Objects,
    Selection,
    Content,
}

impl SceneEditor {
    pub(super) fn set_ai_workspace(&mut self, ai: bool, ctx: &egui::Context) -> bool {
        if self.ai_workspace == ai {
            return true;
        }
        if self.guard().is_err() || self.manipulation.offset_pending() {
            self.notice = "Finish or cancel the pending draft, gesture or dialog before changing workspace; nothing discarded.".into();
            return false;
        }
        self.runtime.release_input(ctx);
        self.navigation.enabled = false;
        self.ai_release_navigation = true;
        self.ai_workspace = ai;
        self.ai_drawer_rect = None;
        true
    }
    pub(super) fn ai_header(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let wide = ui.available_width() >= 900.;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Full editor").clicked() {
                self.set_ai_workspace(false, &ctx);
            }
            if ui
                .button(format!("AI: {}", self.ai_connection_label))
                .clicked()
            {
                self.ai_connection_requested = true;
            }
            for action in [Action::Undo, Action::Save] {
                if ui
                    .button(action.name())
                    .on_hover_text(self.workspace.shortcut_text(action))
                    .clicked()
                {
                    self.queued_action = Some(action);
                }
            }
            ui.menu_button("…", |ui| {
                if !wide {
                    self.ai_tools(ui);
                }
                self.scene_menus(ui);
                if ui.button("Content").clicked() && !self.drafts() {
                    self.ai_drawer = Some(Drawer::Content);
                    ui.close();
                }
                self.tool_options_ui(ui);
                let shortcut = self.workspace.shortcut_text(Action::FocusRuntime);
                let blocked = self.guard().is_err();
                if let Some(key) =
                    self.runtime
                        .controls(ui, &self.session, self.generation, blocked, &shortcut)
                {
                    self.capture_reset_assets(key);
                }
            });
            ui.add(
                egui::Label::new(format!(
                    "{}{}",
                    self.document_label(),
                    if self.session.is_dirty() {
                        " •"
                    } else {
                        " · Saved"
                    }
                ))
                .truncate(),
            )
            .on_hover_text(self.document_label());
        });
        if wide {
            self.ai_tools(ui);
        }
    }
    fn ai_tools(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            use crate::manipulation::Tool;
            ui.selectable_value(&mut self.manipulation.tool, Tool::Translate, "Move");
            ui.selectable_value(&mut self.manipulation.tool, Tool::Rotate, "Rotate");
            ui.add_enabled_ui(
                self.manipulation.scale_allowed(self.session.scene()),
                |ui| {
                    ui.selectable_value(&mut self.manipulation.tool, Tool::Scale, "Scale");
                },
            );
            if ui.button("Frame").clicked() {
                self.queued_action = Some(Action::Frame);
            }
            if ui.button("Projection").clicked() {
                self.queued_action = Some(Action::Projection);
            }
            for (label, drawer) in [
                ("Objects", Drawer::Objects),
                ("Selection", Drawer::Selection),
            ] {
                if ui
                    .selectable_label(self.ai_drawer == Some(drawer), label)
                    .clicked()
                    && !self.drafts()
                {
                    self.ai_drawer = if self.ai_drawer == Some(drawer) {
                        None
                    } else {
                        Some(drawer)
                    };
                }
            }
        });
    }
    pub(super) fn ai_drawer_ui(&mut self, ctx: &egui::Context) {
        if self.drafts() {
            self.ai_drawer = Some(Drawer::Selection);
        }
        let Some(drawer) = self.ai_drawer else {
            self.ai_drawer_rect = None;
            return;
        };
        let mut open = true;
        let output = egui::Window::new(match drawer {
            Drawer::Objects => "Objects",
            Drawer::Selection => "Selection",
            Drawer::Content => "Content",
        })
        .id(egui::Id::unique("ai_workspace_drawer"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-8., 80.))
        .fixed_size(egui::vec2(
            300f32.min(ctx.content_rect().width() - 32.),
            (ctx.content_rect().height() - 130.).max(200.),
        ))
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| match drawer {
                Drawer::Objects => self.hierarchy_contents(ui),
                Drawer::Selection => self.inspector_scroll_ui(ui),
                Drawer::Content => self.browser_contents(ui),
            });
        });
        self.ai_drawer_rect = output.map(|o| o.response.rect);
        if !open && !self.drafts() {
            self.ai_drawer = None;
            self.ai_drawer_rect = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ai_layout_keeps_eighty_percent_viewport_without_overwriting_full_preferences() {
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        editor.ai_workspace = true;
        editor.workspace.welcome = false;
        let prefs = (
            editor.workspace.prefs.hierarchy_visible,
            editor.workspace.prefs.inspector_visible,
            editor.workspace.prefs.content_visible,
            editor.workspace.prefs.hierarchy_width,
            editor.workspace.prefs.inspector_width,
        );
        for size in [egui::vec2(800., 650.), egui::vec2(640., 600.)] {
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| editor.ui(ui),
                );
                output.textures_delta.clear();
            }
            let viewport = editor.captures.rect.expect("real viewport allocation");
            if size.x == 800. {
                assert!(viewport.area() >= size.x * size.y * 0.8, "{viewport:?}");
            }
            assert!(viewport.width() > size.x * 0.95);
            assert_eq!(
                (
                    editor.workspace.prefs.hierarchy_visible,
                    editor.workspace.prefs.inspector_visible,
                    editor.workspace.prefs.content_visible,
                    editor.workspace.prefs.hierarchy_width,
                    editor.workspace.prefs.inspector_width
                ),
                prefs
            );
        }
    }
}
