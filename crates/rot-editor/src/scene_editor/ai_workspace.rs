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
        // A horizontal row sizes to its controls. `with_layout` alone takes the
        // panel's previous full height; adding the tools below then grows that
        // remembered height every frame until the viewport is pushed off-screen.
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Full editor").clicked() {
                    self.set_ai_workspace(false, &ctx);
                }
                #[cfg(unix)]
                crate::branding::connection_indicator(ui, self.ai_connection_label);
                if ui
                    .button("Undo")
                    .on_hover_text(self.workspace.shortcut_text(Action::Undo))
                    .clicked()
                {
                    self.queued_action = Some(Action::Undo);
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
                    if let Some(key) = self.runtime.controls_with_sounds(
                        ui,
                        &self.session,
                        self.generation,
                        blocked,
                        &shortcut,
                        self.assets.as_ref(),
                    ) {
                        self.capture_reset_assets(key);
                    }
                });
                ui.add(
                    egui::Label::new(format!(
                        "{}{}",
                        self.document_label(),
                        if self.session.is_dirty() {
                            " •"
                        } else if self.session.path().is_none() {
                            " · Not saved"
                        } else {
                            " · Saved"
                        }
                    ))
                    .truncate(),
                )
                .on_hover_text(self.document_label());
            });
        });
        if wide {
            self.ai_tools(ui);
        }
    }
    fn ai_tools(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(self.guard().is_ok() && !self.runtime.visible, |ui| {
            ui.horizontal_wrapped(|ui| {
                self.manipulation_mode_buttons(ui);
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
    fn compact_find_shortcuts_open_drawers_without_changing_full_editor_preferences() {
        let mut editor = SceneEditor::new(Session::new(Scene::empty("find-test")).unwrap());
        editor.ai_workspace = true;
        editor.workspace.prefs.hierarchy_visible = false;
        editor.workspace.prefs.content_visible = false;
        editor.queued_action = Some(Action::FindObjects);
        editor.process_queued_action();
        assert!(editor.ai_drawer == Some(Drawer::Objects));
        assert!(!editor.workspace.prefs.hierarchy_visible);
        editor.queued_action = Some(Action::FindContent);
        editor.process_queued_action();
        assert!(editor.ai_drawer == Some(Drawer::Content));
        assert!(!editor.workspace.prefs.content_visible);
    }

    #[test]
    fn workspace_switch_preserves_session_history_selection_camera_and_visible_drafts() {
        let ctx = egui::Context::default();
        let mut editor = SceneEditor::new(Session::new(Scene::empty("mode-test")).unwrap());
        let observed = editor.command("host", "scene_inspect", json!({}));
        assert_eq!(editor.command("host","scene_create",json!({"expected_session_token":observed["result"]["session_token"],"expected_revision":"0","object_id":"cube","name":"Cube"}))["ok"],true);
        editor.select_object(Some("cube".into()), false);
        editor.navigation.camera = Some(rot_render::CameraState {
            target: [1., 2., 3.],
            yaw: 0.4,
            pitch: 0.3,
            distance: 7.,
            orthographic: true,
        });
        let bytes = editor.session.snapshot_bytes().unwrap();
        let selected = editor.manipulation.ids.clone();
        let camera = editor.navigation.camera;
        assert!(editor.set_ai_workspace(true, &ctx));
        assert!(editor.set_ai_workspace(false, &ctx));
        assert_eq!(editor.session.snapshot_bytes().unwrap(), bytes);
        assert_eq!(editor.manipulation.ids, selected);
        assert_eq!(editor.navigation.camera, camera);
        assert!(editor.set_ai_workspace(true, &ctx));
        editor.draft.as_mut().unwrap().translation[0] = "unfinished -".into();
        let draft = editor.draft.clone();
        assert!(!editor.set_ai_workspace(false, &ctx));
        assert!(editor.ai_workspace);
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 850.),
                    )),
                    ..Default::default()
                },
                |ui| editor.ui(ui),
            );
            output.textures_delta.clear();
        }
        assert!(editor.ai_drawer == Some(Drawer::Selection));
        let drawer = editor.ai_drawer_rect.unwrap();
        assert!(ctx.content_rect().contains_rect(drawer));
        assert!(editor.draft == draft);
        assert_eq!(editor.session.snapshot_bytes().unwrap(), bytes);
        let result = editor.capture_command("host", "viewport_capture_start", &json!({}));
        assert_eq!(result.unwrap_err().0, "viewport_occluded");
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(ctx.content_rect()),
                events: vec![egui::Event::PointerMoved(drawer.center())],
                ..Default::default()
            },
            |ui| editor.ui(ui),
        );
        output.textures_delta.clear();
        assert!(
            !editor.navigation.enabled,
            "drawer blocks underlying native navigation/picking"
        );
    }

    #[test]
    fn production_host_ai_header_stays_at_top_and_leaves_visible_viewport() {
        use eframe::App as _;
        for (size, native_scale, zoom) in [
            (egui::vec2(1280., 805.), 1., 1.),
            (egui::vec2(1280., 805.), 1.25, 1.25),
            (egui::vec2(600., 850.), 1., 1.),
            (egui::vec2(800., 650.), 1., 1.),
            (egui::vec2(600., 850.), 1., 1.25),
        ] {
            let ctx = egui::Context::default();
            crate::editor_theme::configure(&ctx);
            ctx.set_zoom_factor(zoom);
            let project = tempfile::tempdir().unwrap();
            let path = project.path().join("demo.rot");
            Scene::demo().save(&path).unwrap();
            let mut editor = SceneEditor::test_loaded(Session::open(&path).unwrap());
            assert_eq!(editor.session.scene().objects.len(), 3);
            assert_eq!(editor.session.scene().revision, 0);
            assert!(!editor.session.is_dirty());
            let snapshot = editor.session.snapshot_bytes().unwrap();
            editor.ai_workspace = true;
            editor.workspace.welcome = false;
            editor.workspace.prefs.scale = zoom;
            editor.scale_applied = zoom;
            let mut host = Host {
                editor,
                input: Default::default(),
                #[cfg(unix)]
                bridge: None,
                #[cfg(unix)]
                workspace: None,
                #[cfg(unix)]
                workspace_enabled: false,
            };
            let mut frame = eframe::Frame::_new_kittest();
            let mut previous = None;
            for index in 0..40 {
                let mut raw = egui::RawInput {
                    // Native backends report screen geometry in egui points,
                    // not physical pixels, at the current native DPI and zoom.
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        size / (native_scale * zoom),
                    )),
                    ..Default::default()
                };
                raw.viewports
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .native_pixels_per_point = Some(native_scale);
                if index == 0 {
                    // Apply egui's initial zoom transition before measuring the
                    // native geometry (its transition pass ignores screen_rect).
                    ctx.run_ui(raw.clone(), |_| {})
                        .drop_without_applying_deltas();
                }
                host.raw_input_hook(&ctx, &mut raw);
                let mut output = ctx.run_ui(raw, |ui| host.ui(ui, &mut frame));
                output.textures_delta.clear();
                let header = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        let egui::epaint::Shape::Text(text) = &shape.shape else {
                            return None;
                        };
                        (text.galley.job.text == "Full editor")
                            .then(|| text.galley.rect.translate(text.pos.to_vec2()))
                    })
                    .expect("production Host paints workspace controls");
                let viewport = host.editor.captures.rect.expect("real viewport allocation");
                if index == 0 || index == 39 {
                    eprintln!("Host frame={index} size={size:?} native_scale={native_scale} zoom={zoom} content={:?} header={header:?} viewport={viewport:?}", ctx.content_rect());
                }
                // This proves the real scene renderer is hooked to the visible
                // allocation; CPU composition does not prove GPU pixels.
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::epaint::Shape::Callback(callback)
                        if callback.rect == viewport && shape.clip_rect.intersect(viewport).area() >= viewport.area() * 0.99)
                }), "scene callback must target the unclipped visible viewport");
                if let Some(previous) = previous {
                    assert_eq!(viewport, previous, "layout must not grow across frames");
                }
                previous = Some(viewport);
                assert!(
                    header.bottom() < ctx.content_rect().top() + 70.,
                    "header must be bounded at top, not vertically centered: {header:?}"
                );
                assert!(
                    ctx.content_rect().contains_rect(viewport),
                    "viewport must be on screen: {viewport:?}"
                );
                assert!(
                    viewport.height() > ctx.content_rect().height() * 0.8,
                    "viewport must dominate native window: {viewport:?}"
                );
                assert!(viewport.width() > ctx.content_rect().width() * 0.95);
            }
            assert_eq!(host.editor.session.snapshot_bytes().unwrap(), snapshot);
            assert!(!host.editor.session.is_dirty());
        }
    }

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
        for (size, scale) in [
            (egui::vec2(600., 850.), 1.),
            (egui::vec2(650., 900.), 1.),
            (egui::vec2(800., 650.), 1.),
            (egui::vec2(640., 600.), 1.),
            (egui::vec2(1280., 800.), 1.),
            (egui::vec2(600., 850.), 1.25),
        ] {
            editor.workspace.prefs.scale = scale;
            editor.ai_connection_label = "Approval required (manual)";
            for _ in 0..3 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| editor.ui(ui),
                );
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| {
                        let egui::epaint::Shape::Text(text) = &shape.shape else {
                            return None;
                        };
                        Some((
                            text.galley.job.text.as_str(),
                            text.galley.rect.translate(text.pos.to_vec2()),
                        ))
                    })
                    .collect();
                output.textures_delta.clear();
                for label in ["Undo", "Full editor"] {
                    let rect = labels
                        .iter()
                        .find(|(text, _)| *text == label)
                        .unwrap_or_else(|| panic!("missing {label}"))
                        .1;
                    assert!(
                        ctx.content_rect().contains_rect(rect),
                        "{label} clipped at {size:?} zoom {scale}: {rect:?}"
                    );
                }
                output.textures_delta.clear();
            }
            let viewport = editor.captures.rect.expect("real viewport allocation");
            if size.x == 800. && scale == 1. {
                assert!(viewport.area() >= size.x * size.y * 0.8, "{viewport:?}");
            }
            assert!(viewport.width() > ctx.content_rect().width() * 0.95);
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
