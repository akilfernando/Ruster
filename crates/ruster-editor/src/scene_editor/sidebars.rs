//! Objects and Inspector share the scene surface while owning their visible input.
use super::{sidebar_budget, SceneEditor, Side};
use eframe::egui;

#[derive(Clone, Copy)]
struct Panel {
    rect: egui::Rect,
    side: Side,
    inspector: bool,
}

pub(super) struct SceneSidebars {
    panels: [Option<Panel>; 2],
    visible_scene: egui::Rect,
    constrained: bool,
    fade_height: f32,
}

impl SceneSidebars {
    pub(super) fn new(editor: &SceneEditor, viewport: egui::Rect) -> Self {
        let prefs = &editor.workspace.prefs;
        let (constrained, hierarchy_width, inspector_width) = sidebar_budget(
            viewport.width(),
            (!editor.ai_workspace && prefs.hierarchy_visible).then_some(prefs.hierarchy_width),
            (!editor.ai_workspace && prefs.inspector_visible).then_some(prefs.inspector_width),
        );
        // Scale the decorative area down first so compact Inspectors retain
        // their fixed recovery controls and a useful scroll region.
        let decoration = ((viewport.height() - 155.0) / 65.0).clamp(0.0, 1.0);
        let exposed = decoration * 35.0;
        let fade_height = decoration * 30.0;
        let mut visible_scene = viewport;
        let mut panel = |width: f32, side: Side, inspector| {
            if width <= 0.0 || viewport.height() <= exposed {
                return None;
            }
            // Keep at least one unoccluded column even on forcibly tiny windows.
            let width = width.min((visible_scene.width() - 1.0).max(0.0));
            if width == 0.0 {
                return None;
            }
            let (left, right) = match side {
                Side::Left => {
                    let left = visible_scene.left();
                    visible_scene.min.x += width;
                    (left, visible_scene.left())
                }
                Side::Right => {
                    let right = visible_scene.right();
                    visible_scene.max.x -= width;
                    (visible_scene.right(), right)
                }
            };
            let rect = egui::Rect::from_min_max(
                egui::pos2(left, viewport.top()),
                egui::pos2(right, viewport.bottom() - exposed),
            );
            Some(Panel {
                rect,
                side,
                inspector,
            })
        };
        let panels = [
            panel(hierarchy_width, prefs.hierarchy_side, false),
            panel(inspector_width, prefs.inspector_side, true),
        ];
        Self {
            panels,
            visible_scene,
            constrained,
            fade_height,
        }
    }

    fn owns_point(&self, point: egui::Pos2) -> bool {
        self.panels
            .iter()
            .flatten()
            .any(|panel| panel.rect.contains(point))
    }

    pub(super) fn own_pointer(&self, ctx: &egui::Context) -> bool {
        ctx.pointer_interact_pos()
            .is_some_and(|pointer| self.owns_point(pointer))
    }

    pub(super) fn owns_pointer_press(&self, ctx: &egui::Context) -> bool {
        ctx.input(|input| {
            input.events.iter().any(|event| {
                matches!(event, egui::Event::PointerButton {pos, pressed: true, ..}
                    if self.owns_point(*pos))
            })
        })
    }

    /// This uninterrupted crop excludes editor controls and their translucent fill.
    /// Picking and Fit continue to use the full scene rectangle and projection.
    pub(super) fn visible_scene(&self) -> egui::Rect {
        self.visible_scene
    }

    pub(super) fn show(&self, editor: &mut SceneEditor, ui: &mut egui::Ui) {
        for panel in self.panels.iter().flatten() {
            let name = if panel.inspector {
                "overlay_inspector"
            } else {
                "overlay_objects"
            };
            let id = ui.scope_id().with(name);
            ui.interact(panel.rect, id.with("panel"), egui::Sense::click_and_drag());

            let fade_height = self.fade_height;
            let mut content = panel.rect.shrink(8.0);
            content.max.y = (content.max.y - fade_height).max(content.min.y);
            let palette = crate::editor_theme::colors(ui);
            let painter = ui.painter().with_clip_rect(panel.rect);
            paint_fill(&painter, panel.rect, fade_height, palette.surface);

            if content.is_positive() {
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(name)
                        .max_rect(content)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.set_clip_rect(content.intersect(ui.clip_rect()));
                child.set_width(content.width());
                child.set_max_height(content.height());
                if panel.inspector {
                    editor.inspector_scroll_ui(&mut child);
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("hierarchy_panel_scroll")
                        .max_height(content.height())
                        .show(&mut child, |ui| editor.hierarchy_contents(ui));
                }
            }

            if !self.constrained {
                let mut handle = panel.rect;
                match panel.side {
                    Side::Left => handle.min.x = handle.max.x - 5.0,
                    Side::Right => handle.max.x = handle.min.x + 5.0,
                }
                let resize = ui
                    .interact(handle, id.with("resize"), egui::Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
                    .on_hover_text("Drag to resize panel");
                let sign = if panel.side == Side::Left { 1.0 } else { -1.0 };
                let delta = sign * resize.drag_delta().x;
                if delta != 0.0 {
                    let (width, min, max) = if panel.inspector {
                        (&mut editor.workspace.prefs.inspector_width, 240.0, 1000.0)
                    } else {
                        (&mut editor.workspace.prefs.hierarchy_width, 120.0, 800.0)
                    };
                    // Never drag the sides past this window's central viewport budget.
                    let available = (self.visible_scene.width() - 304.0).max(0.0);
                    let next = (*width + delta.min(available)).clamp(min, max);
                    if next != *width {
                        *width = next;
                        editor.workspace.changed();
                    }
                }
            }
        }
    }
}

fn paint_fill(painter: &egui::Painter, rect: egui::Rect, height: f32, color: egui::Color32) {
    let fade_top = rect.bottom() - height;
    let top = egui::Rect::from_min_max(rect.min, egui::pos2(rect.right(), fade_top));
    painter.rect_filled(top, 0.0, color);
    let mut fade = egui::Mesh::default();
    for (position, color) in [
        (top.left_bottom(), color),
        (top.right_bottom(), color),
        (rect.right_bottom(), egui::Color32::TRANSPARENT),
        (rect.left_bottom(), egui::Color32::TRANSPARENT),
    ] {
        fade.colored_vertex(position, color);
    }
    fade.add_triangle(0, 1, 2);
    fade.add_triangle(0, 2, 3);
    painter.add(fade);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_fade_blocks_zoom_but_exposed_strip_uses_the_full_scene_camera() {
        let mut editor =
            SceneEditor::new(ruster_scene::Session::new(ruster_scene::Scene::demo()).unwrap());
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
        let viewport = editor.captures.rect.unwrap();
        let panels = SceneSidebars::new(&editor, viewport);
        let hierarchy = panels.panels[0].unwrap().rect;
        let inspector = panels.panels[1].unwrap().rect;
        let crop = editor.captures.visible_crop.unwrap();
        assert_eq!(hierarchy.left(), viewport.left());
        assert_eq!(inspector.right(), viewport.right());
        assert_eq!(crop.left(), hierarchy.right());
        assert_eq!(crop.right(), inspector.left());
        assert_eq!(viewport.bottom() - hierarchy.bottom(), 35.0);
        assert_eq!(hierarchy.bottom(), inspector.bottom());

        let point = |y| egui::pos2(hierarchy.center().x, y);
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
        let fade = point(hierarchy.bottom() - 12.0);
        frame(&mut editor, wheel(fade));
        assert_eq!(editor.navigation.camera.unwrap(), camera);

        let exposed = point(hierarchy.bottom() + 12.0);
        frame(&mut editor, vec![egui::Event::PointerMoved(exposed)]);
        frame(&mut editor, wheel(exposed));
        assert!(editor.navigation.camera.unwrap().distance < camera.distance);

        let player_view = ruster_scene::PlayerView {
            anchor: ruster_scene::PlayerAnchor::Fixed { position: [0.0; 3] },
            yaw: 0.0,
            pitch: 0.0,
            distance: 6.0,
            vertical_fov_degrees: 60.0,
        };
        editor
            .session
            .set_player_view(editor.session.scene().revision, Some(player_view))
            .unwrap();
        editor.set_player_preview(true);
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
        let expected = ruster_render::player_view_projection(view, player.aspect_ratio()).unwrap();
        assert_eq!(editor.captures.projection, Some(expected.projection));
    }
}
