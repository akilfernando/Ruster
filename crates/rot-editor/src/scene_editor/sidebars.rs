//! Objects and Inspector share the scene surface while owning their visible input.
#[cfg(test)]
use super::SceneEditor;
use eframe::egui;

/// Cover the outside of each small corner arc with the surrounding chrome.
/// The renderer, picking and capture still agree on the rectangular viewport.
pub(super) fn round_viewport(ui: &egui::Ui, rect: egui::Rect) {
    let radius = (rect.width().min(rect.height()) * 0.5).clamp(0.0, 7.0);
    if radius <= 0.0 {
        return;
    }
    let color = crate::editor_theme::colors(ui).surface;
    let mut mask = egui::Mesh::default();
    for (corner, inward) in [
        (rect.left_top(), egui::vec2(1.0, 1.0)),
        (rect.right_top(), egui::vec2(-1.0, 1.0)),
        (rect.right_bottom(), egui::vec2(-1.0, -1.0)),
        (rect.left_bottom(), egui::vec2(1.0, -1.0)),
    ] {
        let start = mask.vertices.len() as u32;
        mask.colored_vertex(corner, color);
        for step in 0..=8 {
            let angle = std::f32::consts::FRAC_PI_2 * step as f32 / 8.0;
            let offset = egui::vec2(
                inward.x * (1.0 - angle.cos()),
                inward.y * (1.0 - angle.sin()),
            );
            mask.colored_vertex(corner + offset * radius, color);
        }
        for step in 0..8 {
            mask.add_triangle(start, start + 1 + step, start + 2 + step);
        }
    }
    ui.painter().add(mask);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S-06: a script error reaches the Console with its file and line, the
    /// status bar shows it on one line, and the creator's notice toasts.
    #[test]
    fn script_errors_reach_the_console_with_file_and_line() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let frame = |editor: &mut SceneEditor| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| editor.ui(ui),
            );
            output.textures_delta.clear();
            output.shapes
        };
        frame(&mut editor);
        editor.runtime.diagnostic = Some(rot_behavior::Diagnostic {
            code: rot_behavior::DiagnosticCode::Runtime,
            source_id: Some("mover".into()),
            source_hash: None,
            binding_id: None,
            object_id: None,
            phase: rot_behavior::Phase::Update,
            message: "attempt to index a nil value".into(),
            line: Some(7),
        });
        frame(&mut editor);
        let latest = editor.console.latest().expect("logged");
        assert_eq!(latest.source, "Script");
        assert_eq!(latest.location, Some(("mover".to_owned(), 7)));
        assert_eq!(editor.console.count(crate::console::Level::Error), 1);
        // Next frame the status bar shows it on one line at 960 px.
        let shapes = frame(&mut editor);
        let lines: Vec<_> = shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text)
                    if text.galley.job.text == "attempt to index a nil value" =>
                {
                    Some(text.galley.rows.len())
                }
                _ => None,
            })
            .collect();
        assert_eq!(lines, vec![1], "one status line");
        assert_eq!(
            editor.console.count(crate::console::Level::Error),
            1,
            "once"
        );
    }

    /// S-02: dragging the Position X letter scrubs the value, and the whole
    /// drag is one Undo step.
    #[test]
    fn dragging_an_axis_letter_scrubs_and_undoes_in_one_step() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        editor.select_object(Some("left".into()), false);
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let mut time = 0.;
        let mut shapes = Vec::new();
        let mut frame = |editor: &mut SceneEditor, events| {
            time += 0.1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| editor.ui(ui),
            );
            output.textures_delta.clear();
            output.shapes
        };
        frame(&mut editor, vec![]);
        shapes.extend(frame(&mut editor, vec![]));
        // The Position row's X is the "X" in the Inspector on the Position
        // label's row (the viewport's orientation gizmo also paints an X).
        let centers = |label: &str| {
            shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) if text.galley.job.text == label => {
                        Some(egui::Rect::from_min_size(text.pos, text.galley.size()).center())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let row = centers("Position")
            .into_iter()
            .find(|point| point.x > 640.0)
            .expect("Position label");
        let letter = centers("X")
            .into_iter()
            .filter(|point| point.x > 640.0 && (point.y - row.y).abs() < 8.0)
            .min_by(|a, b| a.x.total_cmp(&b.x))
            .expect("Position X letter");
        let before = editor
            .session
            .scene()
            .target_transform("left")
            .unwrap()
            .translation[0];
        let revision = editor.session.scene().revision;
        let button = |pressed| egui::Event::PointerButton {
            pos: letter,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(letter), button(true)],
        );
        for step in 1..=10 {
            let pos = letter + egui::vec2(10.0 * step as f32, 0.0);
            frame(&mut editor, vec![egui::Event::PointerMoved(pos)]);
        }
        frame(
            &mut editor,
            vec![egui::Event::PointerButton {
                pos: letter + egui::vec2(100.0, 0.0),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(&mut editor, vec![]);
        let after = editor
            .session
            .scene()
            .target_transform("left")
            .unwrap()
            .translation[0];
        assert!(after > before + 0.5, "scrubbed from {before} to {after}");
        assert_eq!(editor.session.scene().revision, revision + 1, "one commit");
        editor
            .session
            .undo(editor.session.scene().revision)
            .unwrap();
        let undone = editor
            .session
            .scene()
            .target_transform("left")
            .unwrap()
            .translation[0];
        assert_eq!(undone, before, "one Undo restores the value");
    }

    /// S-01: Tab must walk into the Hierarchy and Inspector and never drop
    /// focus on a surface such as the viewport (which used to reset to File).
    #[test]
    fn tab_reaches_both_side_panels_without_losing_focus() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        editor.select_object(Some("left".into()), false);
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let mut time = 0.;
        let mut frame = |editor: &mut SceneEditor, events| {
            time += 0.1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| editor.ui(ui),
            );
            output.textures_delta.clear();
        };
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![]);
        let dock = editor.dock.as_ref().unwrap();
        let panels = [
            super::super::panels::pane_rect(dock, super::super::panels::Pane::Hierarchy).unwrap(),
            super::super::panels::pane_rect(dock, super::super::panels::Pane::Inspector).unwrap(),
        ];
        let mut inside = [false, false];
        let mut seen = Vec::new();
        for _ in 0..90 {
            frame(&mut editor, vec![tab.clone()]);
            frame(&mut editor, vec![]);
            let focused = ctx.memory(|memory| memory.focused());
            let focused = focused.expect("Tab never leaves focus on nothing");
            if seen.contains(&focused) {
                break;
            }
            seen.push(focused);
            let center = ctx.read_response(focused).unwrap().rect.center();
            for (index, panel) in panels.iter().enumerate() {
                inside[index] |= panel.contains(center);
            }
        }
        assert_eq!(inside, [true, true], "Tab reached both side panels");
    }

    #[test]
    fn full_height_panels_block_zoom_while_center_uses_the_scene_camera() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let mut time = 0.;
        let mut frame = |editor: &mut SceneEditor, events| {
            time += 0.1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| editor.ui(ui),
            );
            output.textures_delta.clear();
        };
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![]);
        // S-07a: docked panels sit beside the view; nothing covers it.
        let viewport = editor.captures.rect.unwrap();
        let dock = editor.dock.as_ref().unwrap();
        let hierarchy =
            super::super::panels::pane_rect(dock, super::super::panels::Pane::Hierarchy).unwrap();
        let inspector =
            super::super::panels::pane_rect(dock, super::super::panels::Pane::Inspector).unwrap();
        let crop = editor.captures.visible_crop.unwrap();
        assert_eq!(crop, viewport, "the whole view is visible");
        assert!(hierarchy.right() <= viewport.left());
        assert!(inspector.left() >= viewport.right());

        let wheel = |pos| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, 72.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let camera = editor.navigation.camera.unwrap();
        let panel_bottom = egui::pos2(hierarchy.center().x, hierarchy.bottom() - 12.0);
        frame(&mut editor, wheel(panel_bottom));
        assert_eq!(editor.navigation.camera.unwrap(), camera);

        let viewport_bottom = egui::pos2(crop.center().x, crop.bottom() - 12.0);
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(viewport_bottom)],
        );
        frame(&mut editor, wheel(viewport_bottom));
        assert!(editor.navigation.camera.unwrap().distance < camera.distance);

        let player_view = rot_scene::PlayerView {
            anchor: rot_scene::PlayerAnchor::Fixed { position: [0.0; 3] },
            yaw: 0.0,
            pitch: 0.0,
            distance: 6.0,
            vertical_fov_degrees: 60.0,
        };
        editor
            .session
            .set_player_view(editor.session.scene().revision, Some(player_view))
            .unwrap();
        // The Game tab shows the player view in the same place.
        let dock = editor.dock.as_mut().unwrap();
        let game = super::super::panels::tile_of(dock, super::super::panels::Pane::Game).unwrap();
        dock.make_active(|id, _| id == game);
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![]);
        let player = editor.captures.rect.unwrap();
        assert!(editor.navigation.player_view);
        assert_eq!(player.left(), crop.left());
        assert_eq!(player.right(), crop.right());
        assert_eq!(editor.captures.visible_crop, Some(player));
        let view = editor
            .session
            .scene()
            .resolve_player_view()
            .unwrap()
            .unwrap();
        let expected = rot_render::player_view_projection(view, player.aspect_ratio()).unwrap();
        assert_eq!(editor.captures.projection, Some(expected.projection));
    }
}
