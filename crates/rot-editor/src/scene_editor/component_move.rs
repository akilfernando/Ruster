//! A pointer drag changes only a disposable draw snapshot until release.
use super::ComponentWorkspace;
use crate::{manipulation::paint_gizmo, strings::Text};
use eframe::egui;
use glam::{DMat3, DQuat, DVec3};
use rot_render::{component_preview::ComponentPreview, ViewportInteraction};
use rot_scene::component_scene::ObjectNode;
use std::collections::HashMap;
use transform_gizmo_egui::{
    math::{
        DMat4 as GizmoMatrix, DQuat as GizmoRotation, DVec3 as GizmoVector,
        Transform as GizmoTransform,
    },
    prelude::*,
};

pub(super) struct MoveTool {
    pub enabled: bool,
    pub local: bool,
    pub snap: bool,
    pub snap_distance: f32,
    target: Option<Target>,
    target_dirty: bool,
    gesture: Option<Gesture>,
    gizmo: Gizmo,
    suppress_pointer: bool,
}

impl Default for MoveTool {
    fn default() -> Self {
        Self {
            enabled: false,
            local: false,
            snap: false,
            snap_distance: 0.5,
            target: None,
            target_dirty: true,
            gesture: None,
            gizmo: Gizmo::default(),
            suppress_pointer: false,
        }
    }
}

struct Target {
    proxy: GizmoTransform,
    roots: Vec<usize>,
}

struct Gesture {
    revision: u64,
    cursor_start: egui::Pos2,
    proxy: GizmoTransform,
    roots: Vec<MoveRoot>,
    preview: ComponentPreview,
}

struct MoveRoot {
    index: usize,
    parent_basis: DMat3,
    parent_inverse: DMat3,
    original: [f32; 3],
    local: [f32; 3],
    draw_rows: Vec<(usize, DVec3)>,
    lights: Vec<(String, DVec3)>,
}

impl MoveTool {
    pub fn active(&self) -> bool {
        self.gesture.is_some()
    }

    pub fn preview(&self) -> Option<&ComponentPreview> {
        self.gesture.as_ref().map(|gesture| &gesture.preview)
    }

    pub fn invalidate_target(&mut self) {
        self.target = None;
        self.target_dirty = true;
    }

    pub fn cancel(&mut self) {
        if self.gesture.take().is_some() {
            self.suppress_pointer = true;
            self.gizmo = Gizmo::default();
        }
    }
}

impl ComponentWorkspace {
    fn move_target(&self) -> Result<Option<Target>, Text> {
        if self.selection.is_empty() {
            return Ok(None);
        }
        let objects = &self.session.scene().objects;
        let mut roots = Vec::new();
        let mut included_depth = None;
        for &(index, depth) in &self.rows {
            if included_depth.is_some_and(|start| depth <= start) {
                included_depth = None;
            }
            if included_depth.is_none() && self.selection.contains(&objects[index].id) {
                included_depth = Some(depth);
                if self.view_flags.get(index).is_some_and(|flags| flags.hidden) {
                    return Err(Text::ComponentMoveHidden);
                }
                // Lock excludes pointer picking. Hierarchy selection stays editable.
                roots.push(index);
            }
        }
        let Some(&first) = roots.first() else {
            return Ok(None);
        };
        let active = roots
            .iter()
            .copied()
            .find(|&index| self.selection_anchor.as_deref() == Some(&objects[index].id))
            .unwrap_or(first);
        let mut orientation = node_rotation(&objects[active]);
        let mut ancestor = self.parents[active];
        while let Some(index) = ancestor {
            orientation = node_rotation(&objects[index]) * orientation;
            ancestor = self.parents[index];
        }
        let center = roots
            .iter()
            .map(|&index| self.preview_world[index].w_axis.truncate())
            .sum::<DVec3>()
            / roots.len() as f64;
        Ok(Some(Target {
            proxy: GizmoTransform {
                translation: GizmoVector::from_array(center.to_array()).into(),
                rotation: GizmoRotation::from_array(orientation.normalize().to_array()).into(),
                scale: GizmoVector::ONE.into(),
            },
            roots,
        }))
    }

    fn begin_move(&self, cursor_start: egui::Pos2) -> Option<Gesture> {
        let target = self.move_tool.target.as_ref()?;
        let objects = &self.session.scene().objects;
        let preview = self.preview.as_ref().ok()?;
        let mut roots: Vec<_> = target
            .roots
            .iter()
            .map(|&index| {
                let parent_basis = self.parents[index].map_or(DMat3::IDENTITY, |parent| {
                    DMat3::from_mat4(self.preview_world[parent])
                });
                let original = objects[index].transform.translation;
                MoveRoot {
                    index,
                    parent_basis,
                    parent_inverse: parent_basis.inverse(),
                    original,
                    local: original,
                    draw_rows: Vec::new(),
                    lights: Vec::new(),
                }
            })
            .collect();
        if roots.iter().any(|root| !root.parent_inverse.is_finite()) {
            return None;
        }

        let selected: HashMap<_, _> = target
            .roots
            .iter()
            .copied()
            .enumerate()
            .map(|(root, index)| (index, root))
            .collect();
        let mut moved = HashMap::new();
        let mut subtree = None;
        for &(index, depth) in &self.rows {
            if subtree.is_some_and(|(start, _)| depth <= start) {
                subtree = None;
            }
            if let Some(&root) = selected.get(&index) {
                subtree = Some((depth, root));
            }
            if let Some((_, root)) = subtree {
                let original = self.preview_world[index].w_axis.truncate();
                moved.insert(objects[index].id.as_str(), (root, original));
                roots[root]
                    .lights
                    .extend(objects[index].components.iter().filter_map(|component| {
                        preview
                            .scene()
                            .lighting
                            .light(&component.id)
                            .map(|_| (component.id.clone(), original))
                    }));
            }
        }
        for (index, object) in preview.scene().objects.iter().enumerate() {
            if let Some(&(root, original)) = moved.get(object.id.as_str()) {
                roots[root].draw_rows.push((index, original));
            }
        }
        Some(Gesture {
            revision: self.session.revision(),
            cursor_start,
            proxy: target.proxy,
            roots,
            preview: preview.clone(),
        })
    }

    pub(super) fn draw_move_tool(
        &mut self,
        ui: &egui::Ui,
        viewport: &ViewportInteraction,
        blocked: bool,
    ) -> bool {
        if !self.move_tool.enabled || self.preview.is_err() {
            return false;
        }
        if self.move_tool.target_dirty && self.move_tool.gesture.is_none() {
            self.move_tool.target_dirty = false;
            match self.move_target() {
                Ok(target) => {
                    self.move_tool.target = target;
                    if self.error.as_deref() == Some(Text::ComponentMoveHidden.plain()) {
                        self.error = None;
                    }
                }
                Err(error) => self.error = Some(error.plain().into()),
            }
        }
        let (focused, cursor, pressed, down, escape, snap_modifier) = ui.input(|input| {
            (
                input.focused,
                input.pointer.hover_pos(),
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
                input.key_pressed(egui::Key::Escape),
                input.modifiers.ctrl,
            )
        });
        let owned_before = self.move_tool.active();
        if owned_before
            && (blocked
                || !focused
                || escape
                || cursor.is_none()
                || self
                    .move_tool
                    .gesture
                    .as_ref()
                    .is_some_and(|gesture| gesture.revision != self.session.revision()))
        {
            self.move_tool.cancel();
            ui.ctx().request_repaint();
            return true;
        }
        let suppressed = self.move_tool.suppress_pointer;
        if !down && !pressed {
            self.move_tool.suppress_pointer = false;
        }
        let Some(target) = self.move_tool.target.as_ref() else {
            return owned_before || suppressed;
        };
        let proxy = self
            .move_tool
            .gesture
            .as_ref()
            .map_or(target.proxy, |gesture| gesture.proxy);
        self.move_tool.gizmo.update_config(GizmoConfig {
            view_matrix: GizmoMatrix::from_cols_array_2d(
                &viewport.view.map(|column| column.map(f64::from)),
            )
            .into(),
            projection_matrix: GizmoMatrix::from_cols_array_2d(
                &viewport
                    .gizmo_projection
                    .map(|column| column.map(f64::from)),
            )
            .into(),
            viewport: viewport.rect,
            pixels_per_point: ui.ctx().pixels_per_point(),
            modes: GizmoMode::all_translate(),
            orientation: if self.move_tool.local {
                GizmoOrientation::Local
            } else {
                GizmoOrientation::Global
            },
            snapping: self.move_tool.snap || snap_modifier,
            snap_distance: self.move_tool.snap_distance,
            ..Default::default()
        });
        let cursor = cursor.unwrap_or_default();
        let permitted = focused && !blocked && !escape && !self.move_tool.suppress_pointer;
        let hovered = permitted && viewport.response.hovered() && viewport.rect.contains(cursor);
        let update = self.move_tool.gizmo.update(
            GizmoInteraction {
                hovered,
                drag_started: hovered && pressed,
                dragging: permitted && (down || pressed || owned_before),
                cursor_pos: (cursor.x, cursor.y),
            },
            &[proxy],
        );
        if hovered && pressed && self.move_tool.gizmo.is_focused() && !owned_before {
            self.move_tool.gesture = self.begin_move(cursor);
            if self.move_tool.gesture.is_none() {
                self.move_tool.gizmo = Gizmo::default();
                self.move_tool.suppress_pointer = true;
                self.error = Some(Text::ComponentMoveOutsideRange.plain().into());
                return true;
            }
        }
        if let (Some((_, transforms)), Some(gesture)) = (update, &mut self.move_tool.gesture) {
            let displacement = if cursor == gesture.cursor_start {
                DVec3::ZERO
            } else {
                DVec3::from_array(GizmoVector::from(transforms[0].translation).to_array())
                    - DVec3::from_array(GizmoVector::from(target.proxy.translation).to_array())
            };
            if !gesture.update(displacement) {
                self.move_tool.cancel();
                self.error = Some(Text::ComponentMoveOutsideRange.plain().into());
                ui.ctx().request_repaint();
                return true;
            }
            gesture.proxy = transforms[0];
        }

        paint_gizmo(ui, viewport.rect, &self.move_tool.gizmo);
        if !down {
            if let Some(gesture) = self.move_tool.gesture.take() {
                self.move_tool.gizmo = Gizmo::default();
                self.commit_move(gesture);
                ui.ctx().request_repaint();
                return true;
            }
        }
        if self.move_tool.active() {
            ui.ctx().request_repaint();
        }
        owned_before || self.move_tool.active() || suppressed
    }

    fn commit_move(&mut self, gesture: Gesture) {
        let edits: Vec<_> = gesture
            .roots
            .into_iter()
            .filter(|root| root.local != root.original)
            .map(|root| {
                let mut object = self.session.scene().objects[root.index].clone();
                object.transform.translation = root.local;
                object
            })
            .collect();
        if edits.is_empty() || self.session.revision() != gesture.revision {
            return;
        }
        match self.session.put_objects(gesture.revision, edits) {
            Ok(_) => {
                self.error = None;
                self.refresh();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

impl Gesture {
    fn update(&mut self, delta: DVec3) -> bool {
        for root in &mut self.roots {
            let original = DVec3::from_array(root.original.map(f64::from));
            let local = original + root.parent_inverse * delta;
            if !local.is_finite() || local.abs().max_element() > 10_000.0 {
                return false;
            }
            let local = local.as_vec3();
            if local.to_array() != root.local {
                // Match the authored f32 transform at the root. Every descendant
                // shares this exact world displacement, independent of its basis.
                let world_delta = root.parent_basis * (local.as_dvec3() - original);
                if self
                    .preview
                    .set_translated_rows(&root.draw_rows, &root.lights, world_delta)
                    .is_err()
                {
                    return false;
                }
                root.local = local.to_array();
            }
        }
        true
    }
}

fn node_rotation(node: &ObjectNode) -> DQuat {
    let [x, y, z] = node.transform.rotation.map(f64::from);
    DQuat::from_rotation_z(z) * DQuat::from_rotation_y(y) * DQuat::from_rotation_x(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::component_scene::{ComponentScene, ComponentSession};
    use rot_scene::{Scene, Session, Transform};

    #[test]
    fn viewport_move_preview_uses_quantized_root_translation() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("precision.scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[0].parent = Some("large-scale".into());
        scene.objects[0].transform.translation = [4096., 0., 0.];
        scene.objects.push(ObjectNode {
            id: "large-scale".into(),
            name: "Large Scale".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                scale: [1000.; 3],
                ..Default::default()
            },
            components: Vec::new(),
        });
        let mut editor = ComponentWorkspace::new(path, ComponentSession::new(scene).unwrap());
        editor.select(Some(editor.session.scene().objects[0].id.clone()));
        editor.move_tool.target = editor.move_target().ok().flatten();
        let mut gesture = editor.begin_move(egui::Pos2::ZERO).unwrap();
        let original = gesture.preview.draw_matrices()[0].w_axis;

        // 0.2 world units under this parent rounds away in the stored
        // local position, although the world renderer could show the shift.
        assert!(gesture.update(DVec3::new(0.2, 0., 0.)));
        assert_eq!(gesture.preview.draw_matrices()[0].w_axis, original);
        assert!(gesture.update(DVec3::new(0.3, 0., 0.)));
        let moved = gesture.preview.draw_matrices()[0].w_axis;
        assert_eq!(moved.x - original.x, 0.5);
        editor.commit_move(gesture);
        assert_eq!(editor.session.revision(), 1);
        assert_eq!(
            editor.preview.as_ref().unwrap().draw_matrices()[0].w_axis,
            moved
        );
    }
}
