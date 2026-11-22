//! A pointer drag changes only a disposable draw snapshot until release.
use super::ComponentWorkspace;
use crate::{
    manipulation::{nearby_euler, paint_gizmo, Pivot, Tool},
    strings::Text,
};
use eframe::egui;
use glam::{DMat3, DMat4, DQuat, DVec3, EulerRot};
use rot_render::{component_preview::ComponentPreview, ViewportInteraction};
use rot_scene::Transform;
use std::collections::HashMap;
use transform_gizmo_egui::{
    math::{
        DMat4 as GizmoMatrix, DQuat as GizmoRotation, DVec3 as GizmoVector,
        Transform as GizmoTransform,
    },
    prelude::*,
};

pub(super) struct TransformTool {
    pub enabled: bool,
    pub tool: Tool,
    pub pivot: Pivot,
    pub local: bool,
    pub snap: bool,
    pub snap_distance: f32,
    pub snap_degrees: f32,
    pub snap_scale: f32,
    target: Option<Target>,
    target_dirty: bool,
    gesture: Option<Gesture>,
    gizmo: Gizmo,
    pointer_release: PointerRelease,
}

impl Default for TransformTool {
    fn default() -> Self {
        Self {
            enabled: false,
            tool: Tool::Translate,
            pivot: Pivot::Center,
            local: false,
            snap: false,
            snap_distance: 0.5,
            snap_degrees: 15.0,
            snap_scale: 0.1,
            target: None,
            target_dirty: true,
            gesture: None,
            gizmo: Gizmo::default(),
            pointer_release: PointerRelease::Idle,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PointerRelease {
    Idle,
    Pending,
    InFrame(u64),
}

impl PointerRelease {
    fn suppress(&mut self, frame: u64, down: bool, pressed: bool, released: bool) -> bool {
        *self = match *self {
            Self::Pending if released || !down && !pressed => Self::InFrame(frame),
            Self::InFrame(release) if release != frame => Self::Idle,
            state => state,
        };
        *self != Self::Idle
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
    start: GizmoTransform,
    settings: Settings,
    roots: Vec<TransformRoot>,
    preview: ComponentPreview,
}

struct TransformRoot {
    index: usize,
    parent_basis: DMat3,
    parent_inverse: DMat3,
    parent_world: Option<DMat4>,
    origin: DVec3,
    original: Transform,
    local: Transform,
    candidate: Transform,
    nodes: Vec<PreviewNode>,
    draw_rows: Vec<(usize, DVec3)>,
    lights: Vec<(String, DVec3)>,
}

#[derive(Clone, Copy)]
struct Settings {
    tool: Tool,
    pivot: Pivot,
    local: bool,
    snap: bool,
    distance: f32,
    angle: f32,
    scale: f32,
}

struct PreviewNode {
    parent: Option<usize>,
    authored_local: DMat4,
    world: DMat4,
    draws: Vec<usize>,
    lights: Vec<String>,
}

#[derive(Clone, Copy)]
enum Delta {
    Move(DVec3),
    Rotate { world: DQuat, view_axis: bool },
    Scale(DVec3),
}

impl TransformTool {
    fn settings(&self) -> Settings {
        Settings {
            tool: self.tool,
            pivot: self.pivot,
            local: self.local,
            snap: self.snap,
            distance: self.snap_distance,
            angle: self.snap_degrees.to_radians(),
            scale: self.snap_scale,
        }
    }

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
            self.pointer_release = PointerRelease::Pending;
            self.gizmo = Gizmo::default();
        }
    }
}

impl ComponentWorkspace {
    fn transform_target(&self) -> Result<Option<Target>, Text> {
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
                    return Err(Text::ComponentTransformHidden);
                }
                // Lock excludes pointer picking. Hierarchy selection stays editable.
                roots.push(index);
            }
        }
        let Some(&first) = roots.first() else {
            return Ok(None);
        };
        let mut anchor = self
            .selection_anchor
            .as_deref()
            .and_then(|id| objects.iter().position(|object| object.id == id));
        let mut active = first;
        while let Some(index) = anchor {
            if roots.contains(&index) {
                active = index;
                break;
            }
            anchor = self.parents[index];
        }
        let mut orientation = rotation(&objects[active].transform);
        let mut ancestor = self.parents[active];
        while let Some(index) = ancestor {
            orientation = rotation(&objects[index].transform) * orientation;
            ancestor = self.parents[index];
        }
        let center = if self.transform_tool.pivot == Pivot::Center {
            roots
                .iter()
                .map(|&index| self.preview_world[index].w_axis.truncate())
                .sum::<DVec3>()
                / roots.len() as f64
        } else {
            self.preview_world[active].w_axis.truncate()
        };
        Ok(Some(Target {
            proxy: GizmoTransform {
                translation: GizmoVector::from_array(center.to_array()).into(),
                rotation: GizmoRotation::from_array(orientation.normalize().to_array()).into(),
                scale: GizmoVector::ONE.into(),
            },
            roots,
        }))
    }

    fn begin_transform(&self, cursor_start: egui::Pos2) -> Option<Gesture> {
        let target = self.transform_tool.target.as_ref()?;
        let objects = &self.session.scene().objects;
        let preview = self.preview.as_ref().ok()?;
        let mut roots: Vec<_> = target
            .roots
            .iter()
            .map(|&index| {
                let parent_world = self.parents[index].map(|parent| self.preview_world[parent]);
                let parent_basis = parent_world.map_or(DMat3::IDENTITY, DMat3::from_mat4);
                let original = objects[index].transform.clone();
                TransformRoot {
                    index,
                    parent_basis,
                    parent_inverse: parent_basis.inverse(),
                    parent_world,
                    origin: self.preview_world[index].w_axis.truncate(),
                    local: original.clone(),
                    candidate: original.clone(),
                    original,
                    nodes: Vec::new(),
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
        let moving = self.transform_tool.tool == Tool::Translate;
        let mut moved = HashMap::new();
        let mut positions = vec![0; objects.len()];
        let mut subtree = None;
        for &(index, depth) in &self.rows {
            if subtree.is_some_and(|(start, _)| depth <= start) {
                subtree = None;
            }
            if let Some(&root) = selected.get(&index) {
                subtree = Some((depth, root));
            }
            if let Some((_, root)) = subtree {
                let owner = &objects[index];
                let world = self.preview_world[index];
                let lights = owner.components.iter().filter_map(|component| {
                    preview
                        .scene()
                        .lighting
                        .light(&component.id)
                        .map(|_| component.id.clone())
                });
                let selected = &mut roots[root];
                let position = selected.nodes.len();
                moved.insert(owner.id.as_str(), (root, position, world.w_axis.truncate()));
                if moving {
                    selected
                        .lights
                        .extend(lights.map(|id| (id, world.w_axis.truncate())));
                } else {
                    let parent = if index == selected.index {
                        None
                    } else {
                        self.parents[index].map(|parent| positions[parent])
                    };
                    positions[index] = position;
                    selected.nodes.push(PreviewNode {
                        parent,
                        authored_local: owner.transform.affine_matrix(),
                        world,
                        draws: Vec::new(),
                        lights: lights.collect(),
                    });
                }
            }
        }
        for (row, object) in preview.scene().objects.iter().enumerate() {
            if let Some(&(root, node, original)) = moved.get(object.id.as_str()) {
                if moving {
                    roots[root].draw_rows.push((row, original));
                } else {
                    roots[root].nodes[node].draws.push(row);
                }
            }
        }
        Some(Gesture {
            revision: self.session.revision(),
            cursor_start,
            proxy: target.proxy,
            start: target.proxy,
            settings: self.transform_tool.settings(),
            roots,
            preview: preview.clone(),
        })
    }

    pub(super) fn draw_transform_tool(
        &mut self,
        ui: &egui::Ui,
        viewport: &ViewportInteraction,
        blocked: bool,
    ) -> bool {
        let inactive = !self.transform_tool.enabled || self.preview.is_err();
        if inactive && self.transform_tool.pointer_release == PointerRelease::Idle {
            return false;
        }
        let (focused, cursor, pressed, down, released, escape, snap_modifier) = ui.input(|input| {
            (
                input.focused,
                input.pointer.hover_pos(),
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.key_pressed(egui::Key::Escape),
                input.modifiers.ctrl,
            )
        });
        let suppressed = self.transform_tool.pointer_release != PointerRelease::Idle
            && self.transform_tool.pointer_release.suppress(
                ui.ctx().cumulative_frame_nr(),
                down,
                pressed,
                released,
            );
        if inactive {
            return suppressed;
        }
        if self.transform_tool.target_dirty && self.transform_tool.gesture.is_none() {
            self.transform_tool.target_dirty = false;
            match self.transform_target() {
                Ok(target) => {
                    self.transform_tool.target = target;
                    if self.error.as_deref() == Some(Text::ComponentTransformHidden.plain()) {
                        self.error = None;
                    }
                }
                Err(error) => self.error = Some(error.plain().into()),
            }
        }
        let owned_before = self.transform_tool.active();
        if owned_before
            && (blocked
                || !focused
                || escape
                || cursor.is_none()
                || self
                    .transform_tool
                    .gesture
                    .as_ref()
                    .is_some_and(|gesture| gesture.revision != self.session.revision()))
        {
            self.transform_tool.cancel();
            ui.ctx().request_repaint();
            return true;
        }
        let Some(target) = self.transform_tool.target.as_ref() else {
            return owned_before || suppressed;
        };
        let proxy = self
            .transform_tool
            .gesture
            .as_ref()
            .map_or(target.proxy, |gesture| gesture.proxy);
        let settings = self.transform_tool.gesture.as_ref().map_or_else(
            || self.transform_tool.settings(),
            |gesture| gesture.settings,
        );
        self.transform_tool.gizmo.update_config(GizmoConfig {
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
            modes: match settings.tool {
                Tool::Translate => GizmoMode::all_translate(),
                Tool::Rotate => GizmoMode::all_rotate(),
                Tool::Scale => GizmoMode::all_scale(),
            },
            orientation: if settings.local {
                GizmoOrientation::Local
            } else {
                GizmoOrientation::Global
            },
            snapping: settings.snap || snap_modifier,
            snap_distance: settings.distance,
            snap_angle: settings.angle,
            snap_scale: settings.scale,
            signed_scale: true,
            ..Default::default()
        });
        let cursor = cursor.unwrap_or_default();
        let permitted = focused && !blocked && !escape && !suppressed;
        let hovered = permitted && viewport.response.hovered() && viewport.rect.contains(cursor);
        let update = self.transform_tool.gizmo.update(
            GizmoInteraction {
                hovered,
                drag_started: hovered && pressed,
                dragging: permitted && (down || pressed || owned_before),
                cursor_pos: (cursor.x, cursor.y),
            },
            &[proxy],
        );
        if hovered && pressed && self.transform_tool.gizmo.is_focused() && !owned_before {
            self.transform_tool.gesture = self.begin_transform(cursor);
            if self.transform_tool.gesture.is_none() {
                self.transform_tool.gizmo = Gizmo::default();
                self.transform_tool.pointer_release = PointerRelease::Pending;
                self.error = Some(Text::ComponentMoveOutsideRange.plain().into());
                return true;
            }
        }
        if let (Some((result, transforms)), Some(gesture)) =
            (update, &mut self.transform_tool.gesture)
        {
            let delta = gesture.delta(result, transforms[0], cursor);
            if !gesture.stage(delta) {
                self.transform_tool.cancel();
                self.error = Some(
                    match delta {
                        Delta::Move(_) => Text::ComponentMoveOutsideRange,
                        Delta::Rotate { .. } => Text::ComponentRotateUnsupported,
                        Delta::Scale(factors) if factors.min_element() <= 0.0 => {
                            Text::ComponentScalePositive
                        }
                        Delta::Scale(_) => Text::ComponentScaleUnsupported,
                    }
                    .plain()
                    .into(),
                );
                ui.ctx().request_repaint();
                return true;
            }
            gesture.proxy = transforms[0];
        }

        paint_gizmo(ui, viewport.rect, &self.transform_tool.gizmo);
        if !down {
            if let Some(gesture) = self.transform_tool.gesture.take() {
                self.transform_tool.gizmo = Gizmo::default();
                self.transform_tool.pointer_release =
                    PointerRelease::InFrame(ui.ctx().cumulative_frame_nr());
                self.commit_transform(gesture);
                ui.ctx().request_repaint();
                return true;
            }
        }
        if self.transform_tool.active() {
            ui.ctx().request_repaint();
        }
        owned_before || self.transform_tool.active() || suppressed
    }

    fn commit_transform(&mut self, gesture: Gesture) {
        let edits: Vec<_> = gesture
            .roots
            .into_iter()
            .filter(|root| root.local != root.original)
            .map(|root| {
                let mut object = self.session.scene().objects[root.index].clone();
                object.transform = root.local;
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
    fn delta(&self, result: GizmoResult, proxy: GizmoTransform, cursor: egui::Pos2) -> Delta {
        let reset = cursor == self.cursor_start;
        match result {
            GizmoResult::Translation { .. } => Delta::Move(if reset {
                DVec3::ZERO
            } else {
                position(proxy) - position(self.start)
            }),
            GizmoResult::Rotation {
                axis,
                total,
                is_view_axis,
                ..
            } => {
                let mut axis = DVec3::new(axis.x, axis.y, axis.z);
                if self.settings.local && !is_view_axis {
                    axis = basis(self.start) * axis;
                }
                let turns = (total / std::f64::consts::TAU).round();
                // Snap increments come from a f32 angle. A snapped revolution
                // can differ from an exact turn by that input precision.
                let full_turn = turns != 0.0
                    && (total - turns * std::f64::consts::TAU).abs()
                        <= 4.0 * f64::from(f32::EPSILON) * turns.abs();
                Delta::Rotate {
                    world: if reset || full_turn {
                        DQuat::IDENTITY
                    } else {
                        DQuat::from_axis_angle(axis.normalize(), -total)
                    },
                    view_axis: is_view_axis,
                }
            }
            GizmoResult::Scale { total } => Delta::Scale(if reset {
                DVec3::ONE
            } else {
                DVec3::new(total.x, total.y, total.z)
            }),
            GizmoResult::Arcball { total, .. } => Delta::Rotate {
                world: DQuat::from_xyzw(total.v.x, total.v.y, total.v.z, total.s),
                view_axis: true,
            },
        }
    }

    fn stage(&mut self, delta: Delta) -> bool {
        let start = self.start;
        let mut settings = self.settings;
        if self.roots.len() == 1 {
            settings.pivot = Pivot::Individual;
        }
        // Solve every root before applying any preview updates. The caller
        // drops the transient snapshot if a candidate exceeds render precision.
        for root in &mut self.roots {
            let Some(candidate) = root.candidate(delta, settings, start) else {
                return false;
            };
            root.candidate = candidate;
        }
        self.roots
            .iter_mut()
            .all(|root| root.candidate == root.local || root.stage(&mut self.preview, delta))
    }
}

impl TransformRoot {
    fn candidate(
        &self,
        delta: Delta,
        settings: Settings,
        start: GizmoTransform,
    ) -> Option<Transform> {
        let mut candidate = self.original.clone();
        let local_pos = DVec3::from_array(candidate.translation.map(f64::from));
        let pivot = if settings.pivot == Pivot::Individual {
            self.origin
        } else {
            position(start)
        };
        let frame = if settings.local {
            basis(start)
        } else {
            DQuat::IDENTITY
        };
        let local_individual = settings.local && settings.pivot == Pivot::Individual;
        let linear = match delta {
            Delta::Move(displacement) => {
                candidate.translation = self.stored_translation(local_pos, displacement)?;
                return Some(candidate);
            }
            Delta::Rotate {
                world: delta,
                view_axis,
            } => {
                if !delta.is_finite() {
                    return None;
                }
                if delta.xyz().length_squared() <= 1e-24 {
                    return Some(candidate);
                }
                if local_individual && !view_axis {
                    let local = frame.conjugate() * delta * frame;
                    let next = (rotation(&candidate) * local).normalize();
                    candidate.rotation = authored_rotation(next, &candidate)?;
                    candidate.validate().ok()?;
                    return Some(candidate);
                }
                DMat3::from_quat(delta)
            }
            Delta::Scale(factors) => {
                if !factors.is_finite() || factors.min_element() <= 0.0 {
                    return None;
                }
                if (factors - DVec3::ONE).abs().max_element() <= 1e-12 {
                    return Some(candidate);
                }
                let uniform = factors.x == factors.y && factors.y == factors.z;
                if uniform || local_individual {
                    candidate.scale = (DVec3::from_array(candidate.scale.map(f64::from)) * factors)
                        .as_vec3()
                        .to_array();
                    if !local_individual {
                        let displacement = (self.origin - pivot) * (factors.x - 1.0);
                        candidate.translation = self.stored_translation(local_pos, displacement)?;
                    }
                    candidate.validate().ok()?;
                    return Some(candidate);
                }
                let frame = DMat3::from_quat(frame);
                frame * DMat3::from_diagonal(factors) * frame.transpose()
            }
        };

        // Equivalent to inverse(parent) · delta_about_pivot · start_world.
        // Solve the displacement separately to avoid subtracting large absolute
        // coordinates twice and preserve an unchanged local position exactly.
        let offset = self.origin - pivot;
        let displacement = linear * offset - offset;
        let original = DMat3::from_mat4(candidate.affine_matrix());
        let world_axes = linear * (self.parent_basis * original);
        let axes = self.parent_inverse * world_axes;
        let lengths = DVec3::new(
            axes.x_axis.length(),
            axes.y_axis.length(),
            axes.z_axis.length(),
        );
        if !lengths.is_finite() || lengths.min_element() <= 0.0 {
            return None;
        }
        let normalized = axes * DMat3::from_diagonal(lengths.recip());
        if normalized.determinant() <= 0.0
            || normalized.x_axis.dot(normalized.y_axis).abs() > 4e-6
            || normalized.x_axis.dot(normalized.z_axis).abs() > 4e-6
            || normalized.y_axis.dot(normalized.z_axis).abs() > 4e-6
        {
            return None;
        }

        candidate.translation = self.stored_translation(local_pos, displacement)?;
        candidate.rotation =
            authored_rotation(DQuat::from_mat3(&normalized).normalize(), &candidate)?;
        candidate.scale = lengths.as_vec3().to_array();
        candidate.validate().ok()?;
        let stored = candidate.affine_matrix();
        let stored_axes = DMat3::from_mat4(stored);
        if !close_basis(stored_axes, axes)
            || !close_basis(self.parent_basis * stored_axes, world_axes)
        {
            return None;
        }
        Some(candidate)
    }

    fn stored_translation(&self, local: DVec3, displacement: DVec3) -> Option<[f32; 3]> {
        let requested = local + self.parent_inverse * displacement;
        if !requested.is_finite() || requested.abs().max_element() > 10_000.0 {
            return None;
        }
        let stored = requested.as_vec3().as_dvec3();
        let world = self
            .parent_world
            .map_or(stored, |parent| parent.transform_point3(stored));
        if !close_vector(stored, requested) || !close_vector(world, self.origin + displacement) {
            return None;
        }
        Some(stored.as_vec3().to_array())
    }

    fn stage(&mut self, preview: &mut ComponentPreview, delta: Delta) -> bool {
        let candidate = &self.candidate;
        if matches!(delta, Delta::Move(_)) {
            let original = DVec3::from_array(self.original.translation.map(f64::from));
            let local = DVec3::from_array(candidate.translation.map(f64::from));
            let displacement = self.parent_basis * (local - original);
            if preview
                .set_translated_rows(&self.draw_rows, &self.lights, displacement)
                .is_err()
            {
                return false;
            }
        } else {
            let local = candidate.affine_matrix();
            let world = self.parent_world.map_or(local, |parent| parent * local);
            for index in 0..self.nodes.len() {
                let node = &self.nodes[index];
                let world = node.parent.map_or(world, |parent| {
                    self.nodes[parent].world * node.authored_local
                });
                if !world.is_finite() {
                    return false;
                }
                self.nodes[index].world = world;
            }
            let draws = self
                .nodes
                .iter()
                .flat_map(|node| node.draws.iter().map(|&row| (row, node.world)));
            let lights = self
                .nodes
                .iter()
                .flat_map(|node| node.lights.iter().map(|id| (id.as_str(), node.world)));
            if preview.set_transformed_rows(draws, lights).is_err() {
                return false;
            }
        }
        self.local.clone_from(candidate);
        true
    }
}

fn close_vector(stored: DVec3, requested: DVec3) -> bool {
    requested.is_finite()
        && (stored - requested)
            .abs()
            .cmple(requested.abs().max(DVec3::ONE) * 2e-6)
            .all()
}

fn close_basis(stored: DMat3, requested: DMat3) -> bool {
    close_vector(stored.x_axis, requested.x_axis)
        && close_vector(stored.y_axis, requested.y_axis)
        && close_vector(stored.z_axis, requested.z_axis)
}

fn position(proxy: GizmoTransform) -> DVec3 {
    DVec3::from_array(GizmoVector::from(proxy.translation).to_array())
}

fn basis(proxy: GizmoTransform) -> DQuat {
    DQuat::from_array(GizmoRotation::from(proxy.rotation).to_array())
}

fn rotation(transform: &Transform) -> DQuat {
    euler_rotation(transform.rotation)
}

fn authored_rotation(next: DQuat, original: &Transform) -> Option<[f32; 3]> {
    let nearby = nearby_euler(
        GizmoRotation::from_array(next.to_array()),
        original.rotation,
    )
    .map(|value| value as f32);
    let same = |rotation| (euler_rotation(rotation) * next.conjugate()).xyz().length() <= 1e-6;
    if nearby.iter().all(|value| value.abs() <= 10_000.0) && same(nearby) {
        return Some(nearby);
    }
    let (z, y, x) = next.to_euler(EulerRot::ZYX);
    let canonical = [x as f32, y as f32, z as f32];
    same(canonical).then_some(canonical)
}

fn euler_rotation(rotation: [f32; 3]) -> DQuat {
    let [x, y, z] = rotation.map(f64::from);
    DQuat::from_euler(EulerRot::ZYX, z, y, x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::component_scene::{ComponentScene, ComponentSession, ObjectNode};
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
        editor.transform_tool.target = editor.transform_target().ok().flatten();
        let mut gesture = editor.begin_transform(egui::Pos2::ZERO).unwrap();
        let original = gesture.preview.draw_matrices()[0].w_axis;

        // 0.2 world units under this parent rounds away in the stored
        // local position, although the world renderer could show the shift.
        assert!(gesture.stage(Delta::Move(DVec3::new(0.2, 0., 0.))));
        assert_eq!(gesture.preview.draw_matrices()[0].w_axis, original);
        assert!(gesture.stage(Delta::Move(DVec3::new(0.3, 0., 0.))));
        let moved = gesture.preview.draw_matrices()[0].w_axis;
        assert_eq!(moved.x - original.x, 0.5);
        editor.commit_transform(gesture);
        assert_eq!(editor.session.revision(), 1);
        assert_eq!(
            editor.preview.as_ref().unwrap().draw_matrices()[0].w_axis,
            moved
        );
    }

    #[test]
    fn viewport_scale_solves_parent_space_and_rejects_mixed_shear_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("world-space.scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[0].transform.rotation = [0., 0., std::f32::consts::FRAC_PI_2];
        scene.objects[1].parent = Some("wrapper".into());
        scene.objects[1].transform.rotation = [0., 0., 0.4];
        scene.objects[2].parent = Some(scene.objects[1].id.clone());
        scene.objects.push(ObjectNode {
            id: "wrapper".into(),
            name: "Wrapper".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                rotation: [0.2, 0.3, 0.1],
                scale: [2.0, 0.75, 1.0],
                ..Default::default()
            },
            components: Vec::new(),
        });
        let original_world = scene.world_matrices().unwrap();
        assert!(Transform::try_from_affine(original_world[1]).is_err());
        let mut editor =
            ComponentWorkspace::new(path, ComponentSession::new(scene.clone()).unwrap());
        editor.transform_tool.tool = Tool::Scale;
        editor.transform_tool.pivot = Pivot::Active;
        editor.selection = scene.objects[..3]
            .iter()
            .map(|object| object.id.clone())
            .collect();
        editor.selection_anchor = Some(scene.objects[2].id.clone());
        editor.transform_tool.target = editor.transform_target().unwrap();
        let mut gesture = editor.begin_transform(egui::Pos2::ZERO).unwrap();

        assert_eq!(position(gesture.start), original_world[1].w_axis.truncate());
        assert_eq!(gesture.roots.len(), 2);
        let world_x = Delta::Scale(DVec3::new(1.6, 1., 1.));
        let valid_root = gesture.roots[0]
            .candidate(world_x, gesture.settings, gesture.start)
            .unwrap();
        assert_eq!(valid_root.rotation, scene.objects[0].transform.rotation);
        assert_eq!(valid_root.scale[0], scene.objects[0].transform.scale[0]);
        assert!(!gesture.stage(world_x));
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(
            gesture.preview.draw_matrices(),
            editor.preview.as_ref().unwrap().draw_matrices()
        );
        assert!(gesture.roots.iter().all(|root| root.local == root.original));

        assert!(gesture.stage(Delta::Scale(DVec3::splat(1.4))));
        let pivot = original_world[1].w_axis.truncate();
        let shared = DMat4::from_translation(pivot)
            * DMat4::from_scale(DVec3::splat(1.4))
            * DMat4::from_translation(-pivot);
        let staged = gesture.preview.draw_matrices().to_vec();
        for (draw, before) in staged.iter().zip(original_world.iter()) {
            assert!(draw.abs_diff_eq((shared * before).as_mat4(), 2e-6));
        }
        editor.commit_transform(gesture);
        assert_eq!(editor.session.revision(), 1);
        assert_eq!(editor.preview.as_ref().unwrap().draw_matrices(), staged);
        let committed = editor.session.scene();
        assert_eq!(
            committed.objects[1].transform.rotation,
            scene.objects[1].transform.rotation
        );
        assert_eq!(committed.objects[2], scene.objects[2]);
    }

    #[test]
    fn viewport_local_axis_rotation_preserves_sheared_child_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("local-space.scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let parent = scene.objects[0].id.clone();
        scene.objects[0].transform.scale = [2., 0.5, 1.5];
        scene.objects[1].parent = Some(parent);
        scene.objects[1].transform.rotation = [0.2, 0.5, 0.3];
        scene.objects[1].transform.scale = [0.8, 1.2, 2.4];
        scene.objects[2].parent = Some(scene.objects[1].id.clone());
        scene.objects[2].transform.translation = [0., 9999., 0.];
        assert!(Transform::try_from_affine(scene.world_matrices().unwrap()[1]).is_err());
        let mut editor =
            ComponentWorkspace::new(path, ComponentSession::new(scene.clone()).unwrap());
        editor.select(Some(scene.objects[1].id.clone()));
        editor.transform_tool.tool = Tool::Rotate;
        editor.transform_tool.local = true;
        assert_eq!(editor.transform_tool.pivot, Pivot::Center);
        editor.transform_tool.target = editor.transform_target().unwrap();
        let mut gesture = editor.begin_transform(egui::Pos2::ZERO).unwrap();
        let axis = basis(gesture.start) * DVec3::X;
        let delta = |angle| Delta::Rotate {
            world: DQuat::from_axis_angle(axis, angle),
            view_axis: false,
        };
        assert!(gesture.stage(delta(0.6)));
        let before = &scene.objects[1].transform;
        let expected = rotation(before) * DQuat::from_rotation_x(0.6);
        let after = &gesture.roots[0].local;
        assert!(rotation(after).abs_diff_eq(expected, 1e-6));
        assert_eq!(after.translation, before.translation);
        assert_eq!(after.scale, before.scale);

        // A long lever arm magnifies ordinary f32 Euler quantization. Draw
        // this descendant from the pose we can actually save.
        let mut authored = scene.clone();
        authored.objects[1].transform.clone_from(after);
        assert_eq!(
            gesture.preview.draw_matrices()[2],
            authored.world_matrices().unwrap()[2].as_mat4()
        );

        assert!(gesture.stage(delta(std::f64::consts::TAU)));
        assert_eq!(gesture.roots[0].local, *before);
        editor.commit_transform(gesture);
        assert_eq!(editor.session.revision(), 0);
        assert_eq!(editor.session.scene(), &scene);
    }

    #[test]
    fn viewport_transform_checks_roundtrip_per_world_axis() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("precision.scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[0].parent = Some("wrapper".into());
        scene.objects[0].transform.translation = [0., 9999., 0.];
        scene.objects[1].transform.translation = [9000., 0.0008, 0.];
        scene.objects.push(ObjectNode {
            id: "wrapper".into(),
            name: "Wrapper".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [9000., -9999., 0.],
                ..Default::default()
            },
            components: Vec::new(),
        });
        let mut editor =
            ComponentWorkspace::new(path, ComponentSession::new(scene.clone()).unwrap());
        editor.selection = scene.objects[..2]
            .iter()
            .map(|object| object.id.clone())
            .collect();
        editor.transform_tool.target = editor.transform_target().unwrap();
        editor.transform_tool.tool = Tool::Scale;
        let mut gesture = editor.begin_transform(egui::Pos2::ZERO).unwrap();

        // The shared center is (9000, 0.0004, 0). A 2x scale asks the
        // child for world y = -0.0004, which local y = 9999 cannot store.
        assert!(!gesture.stage(Delta::Scale(DVec3::splat(2.))));
        assert!(gesture.roots.iter().all(|root| root.local == root.original));
        editor.commit_transform(gesture);
        editor.transform_tool.tool = Tool::Translate;
        let mut gesture = editor.begin_transform(egui::Pos2::ZERO).unwrap();
        assert!(!gesture.stage(Delta::Move(DVec3::new(0., -0.0004, 0.))));
        assert!(gesture.roots.iter().all(|root| root.local == root.original));
        editor.commit_transform(gesture);
        assert_eq!(editor.session.revision(), 0);
        assert_eq!(editor.session.scene(), &scene);
    }
}
