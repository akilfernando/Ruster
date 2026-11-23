//! Viewport editing for one exact authored Collider.
use super::{viewport_transform::PointerRelease, ComponentWorkspace};
use crate::strings::Text;
use eframe::egui;
use glam::{DMat4, DQuat, DVec3};
use rot_render::ViewportInteraction;
use rot_scene::{
    component_scene::{Component, ComponentInstance, ObjectNode},
    Collider, ColliderShape, ResolvedShape, Transform,
};

#[derive(Default)]
pub(super) struct ColliderTool {
    component: Option<String>,
    gesture: Option<Gesture>,
    release: PointerRelease,
}

impl ColliderTool {
    pub(super) fn active(&self) -> bool {
        self.gesture.is_some()
    }

    pub(super) fn mode(&self) -> Option<&str> {
        self.component.as_deref()
    }

    pub(super) fn cancel(&mut self) {
        if self.gesture.take().is_some() {
            self.release = PointerRelease::Pending;
        }
    }

    pub(super) fn exit(&mut self) {
        self.cancel();
        self.component = None;
    }
}

struct Gesture {
    revision: u64,
    row: usize,
    original: Collider,
    staged: Collider,
    world: DMat4,
    grip: Grip,
    origin: DVec3,
    axis: DVec3,
    world_per_local: f64,
    start: f64,
    cursor: egui::Pos2,
    camera: Projector,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Grip {
    Face(Face),
    Radius,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Face {
    axis: usize,
    positive: bool,
}

impl Face {
    fn sign(self) -> f64 {
        if self.positive {
            1.0
        } else {
            -1.0
        }
    }

    fn label(self) -> &'static str {
        match (self.axis, self.positive) {
            (0, false) => "−X",
            (0, true) => "+X",
            (1, false) => "−Y",
            (1, true) => "+Y",
            (2, false) => "−Z",
            _ => "+Z",
        }
    }
}

#[derive(Clone, Copy)]
struct Projector {
    matrix: DMat4,
    inverse: DMat4,
    rect: egui::Rect,
}

impl Projector {
    fn new(viewport: &ViewportInteraction) -> Self {
        let matrix = DMat4::from_cols_array_2d(&viewport.projection.map(|c| c.map(f64::from)))
            * DMat4::from_cols_array_2d(&viewport.view.map(|c| c.map(f64::from)));
        Self {
            matrix,
            inverse: matrix.inverse(),
            rect: viewport.rect,
        }
    }

    fn screen(self, world: DVec3) -> Option<(egui::Pos2, f64)> {
        let clip = self.matrix * world.extend(1.0);
        if !clip.is_finite() || clip.w <= 0.0 || !(0.0..=clip.w).contains(&clip.z) {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some((
            egui::pos2(
                self.rect.left() + self.rect.width() * (ndc.x as f32 + 1.0) * 0.5,
                self.rect.top() + self.rect.height() * (1.0 - ndc.y as f32) * 0.5,
            ),
            ndc.z,
        ))
    }

    fn axis_at(self, cursor: egui::Pos2, origin: DVec3, axis: DVec3) -> Option<f64> {
        let x = 2.0 * f64::from((cursor.x - self.rect.left()) / self.rect.width()) - 1.0;
        let y = 1.0 - 2.0 * f64::from((cursor.y - self.rect.top()) / self.rect.height());
        let near = self.inverse.project_point3(DVec3::new(x, y, 0.0));
        let far = self.inverse.project_point3(DVec3::new(x, y, 0.99));
        let ray = (far - near).try_normalize()?;
        let cosine = ray.dot(axis);
        let denominator = 1.0 - cosine * cosine;
        if denominator < 0.01 {
            return None;
        }
        let delta = near - origin;
        let value = (delta.dot(axis) - cosine * delta.dot(ray)) / denominator;
        value.is_finite().then_some(value)
    }
}

pub(super) fn collider_component(component: &ComponentInstance) -> Option<Collider> {
    let Component::Collider {
        role,
        offset,
        shape,
    } = component.kind
    else {
        return None;
    };
    Some(Collider {
        enabled: component.enabled,
        role,
        offset,
        shape,
    })
}

#[derive(Clone, Copy)]
pub(super) struct WorldBox {
    pub(super) center: DVec3,
    pub(super) rotation: DQuat,
    pub(super) half_extents: DVec3,
}

impl WorldBox {
    fn corner(self, index: usize) -> DVec3 {
        let sign = [0, 1, 2].map(|axis| if index & (1 << axis) == 0 { -1.0 } else { 1.0 });
        self.center + self.rotation * (self.half_extents * DVec3::from_array(sign))
    }

    pub(super) fn corners(self) -> [[f64; 3]; 8] {
        std::array::from_fn(|index| self.corner(index).to_array())
    }

    fn face(self, face: Face) -> DVec3 {
        self.center + self.axis(face.axis) * self.half_extents[face.axis] * face.sign()
    }

    fn axis(self, axis: usize) -> DVec3 {
        self.rotation * [DVec3::X, DVec3::Y, DVec3::Z][axis]
    }
}

#[derive(Clone, Copy)]
pub(super) enum WorldCollider {
    Box(WorldBox),
    Sphere(rot_scene::ResolvedCollider),
}

impl WorldCollider {
    fn grip(
        self,
        viewport: &ViewportInteraction,
        camera: Projector,
        point: egui::Pos2,
    ) -> Option<Grip> {
        match self {
            Self::Box(world) => face_at(world, camera, point).map(Grip::Face),
            Self::Sphere(world) => {
                let (origin, axis) = sphere_axis(world, viewport);
                let screen = camera.screen(origin)?.0;
                (screen.distance_sq(point) <= 81.0 && camera.axis_at(point, origin, axis).is_some())
                    .then_some(Grip::Radius)
            }
        }
    }

    fn axis(
        self,
        viewport: &ViewportInteraction,
        grip: Grip,
        original: Collider,
    ) -> (DVec3, DVec3, f64) {
        match (self, grip, original.shape) {
            (Self::Box(world), Grip::Face(face), ColliderShape::Box { half_extents }) => (
                world.face(face),
                world.axis(face.axis),
                world.half_extents[face.axis] / f64::from(half_extents[face.axis]),
            ),
            (Self::Sphere(world), Grip::Radius, ColliderShape::Sphere { radius }) => {
                let (origin, axis) = sphere_axis(world, viewport);
                let ResolvedShape::Sphere {
                    radius: world_radius,
                } = world.shape
                else {
                    unreachable!("sphere")
                };
                (origin, axis, world_radius / f64::from(radius))
            }
            _ => unreachable!("grip belongs to resolved collider"),
        }
    }
}

fn resolve_collider(
    object: &ObjectNode,
    world: DMat4,
    mut collider: Collider,
) -> Result<WorldCollider, Text> {
    let sphere = matches!(collider.shape, ColliderShape::Sphere { .. });
    let unavailable = if sphere {
        Text::ColliderSphereUnavailable
    } else {
        Text::ColliderWorldUnavailable
    };
    let world_pose = if object.parent.is_none() {
        object.transform.clone()
    } else {
        let axes = [
            world.x_axis.truncate(),
            world.y_axis.truncate(),
            world.z_axis.truncate(),
        ]
        .map(DVec3::normalize);
        if axes[0]
            .dot(axes[1])
            .abs()
            .max(axes[0].dot(axes[2]).abs())
            .max(axes[1].dot(axes[2]).abs())
            > 2e-6
        {
            return Err(if sphere {
                Text::ColliderSphereShear
            } else {
                Text::ColliderShear
            });
        }
        Transform::try_from_affine(world).map_err(|_| unavailable)?
    };
    if sphere
        && (world_pose.scale[0] != world_pose.scale[1]
            || world_pose.scale[0] != world_pose.scale[2])
    {
        return Err(Text::ColliderSphereScale);
    }
    // Disabled authoring still has a shape. Effective participation is reported separately.
    collider.enabled = true;
    let resolved = collider
        .resolve(&world_pose)
        .map_err(|_| unavailable)?
        .expect("enabled collider");
    Ok(match resolved.shape {
        ResolvedShape::Box { half_extents } => WorldCollider::Box(WorldBox {
            center: DVec3::from_array(resolved.center),
            rotation: DQuat::from_array(resolved.rotation),
            half_extents: DVec3::from_array(half_extents),
        }),
        ResolvedShape::Sphere { .. } => WorldCollider::Sphere(resolved),
    })
}

fn resize_radius(mut collider: Collider, delta: f64) -> Option<Collider> {
    let ColliderShape::Sphere { radius } = collider.shape else {
        return None;
    };
    collider.shape = ColliderShape::Sphere {
        radius: (f64::from(radius) + delta) as f32,
    };
    collider.validate().ok()?;
    Some(collider)
}

fn resize(original: Collider, face: Face, delta: f64) -> Option<Collider> {
    let ColliderShape::Box { mut half_extents } = original.shape else {
        return None;
    };
    let i = face.axis;
    let sign = face.sign();
    let mut next = original;
    let center = f64::from(original.offset[i]);
    let extent = f64::from(half_extents[i]);
    let fixed = center - sign * extent;
    let new_extent = extent + sign * delta * 0.5;
    let new_center = fixed + sign * new_extent;
    if !(0.001..=1000.0).contains(&new_extent) || new_center.abs() > 10_000.0 {
        return None;
    }
    next.offset[i] = new_center as f32;
    // Round the center first, then derive the extent from the actual stored
    // center. A large center's ULP must not shift the stationary face.
    half_extents[i] = (sign * (f64::from(next.offset[i]) - fixed)) as f32;
    next.shape = ColliderShape::Box { half_extents };
    next.validate().ok()?;
    let stored_extent = f64::from(half_extents[i]);
    let stored_face = f64::from(next.offset[i]) - sign * stored_extent;
    let precision = f64::from(f32::EPSILON) * stored_extent;
    ((stored_face - fixed).abs() <= precision).then_some(next)
}

#[derive(Default)]
pub(in crate::scene_editor) struct InspectorState {
    pub(in crate::scene_editor) active: Option<String>,
    pub(in crate::scene_editor) entries: Vec<(String, Option<Text>)>,
}

impl ComponentWorkspace {
    pub(super) fn collider_effective(&self, mut row: usize, enabled: bool) -> bool {
        if !enabled {
            return false;
        }
        loop {
            if !self.session.scene().objects[row].enabled {
                return false;
            }
            match self.parents[row] {
                Some(parent) => row = parent,
                None => return true,
            }
        }
    }

    pub(super) fn collider_for_viewport(
        &self,
        row: usize,
        collider: Collider,
    ) -> Result<WorldCollider, Text> {
        let object = &self.session.scene().objects[row];
        self.preview_world
            .get(row)
            .ok_or(Text::ColliderWorldUnavailable)
            .and_then(|world| resolve_collider(object, *world, collider))
    }

    fn collider_mode_reason(&self, row: usize, collider: Collider) -> Option<Text> {
        if self.view_flags.get(row).is_some_and(|flags| flags.hidden)
            || !self.collider_effective(row, collider.enabled)
        {
            return Some(Text::ColliderShowEnabled);
        }
        self.collider_for_viewport(row, collider).err()
    }

    pub(super) fn collider_inspector_state(&self) -> InspectorState {
        let Some(row) = self.selected_index else {
            return InspectorState::default();
        };
        let blocked = self.runtime.in_play_mode() || self.authored_edit_pending();
        let entries = self.session.scene().objects[row]
            .components
            .iter()
            .filter_map(|component| {
                let collider = collider_component(component)?;
                let reason = if blocked {
                    Some(Text::ColliderFinishEdit)
                } else {
                    self.collider_mode_reason(row, collider)
                };
                Some((component.id.clone(), reason))
            })
            .collect();
        InspectorState {
            active: self.collider_tool.mode().map(str::to_owned),
            entries,
        }
    }

    pub(super) fn toggle_collider_mode(&mut self, component: String) {
        if self.collider_tool.mode() == Some(component.as_str()) {
            self.collider_tool.exit();
        } else {
            self.collider_tool.component = Some(component);
            self.transform_tool.enabled = false;
            self.error = None;
        }
    }

    pub(super) fn draw_collider_tool(
        &mut self,
        ui: &egui::Ui,
        viewport: &ViewportInteraction,
        blocked: bool,
    ) -> bool {
        if self.collider_tool.mode().is_none() && self.collider_tool.release == PointerRelease::Idle
        {
            return false;
        }
        let frame = ui.ctx().cumulative_frame_nr();
        let (cursor, focused, pressed, down, released, escape) = ui.input(|input| {
            (
                input.pointer.interact_pos(),
                input.focused
                    && !input
                        .events
                        .iter()
                        .any(|event| matches!(event, egui::Event::WindowFocused(false))),
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
                input.pointer.button_released(egui::PointerButton::Primary),
                input.key_pressed(egui::Key::Escape),
            )
        });
        let owned = self.collider_tool.active();
        let suppressed = self
            .collider_tool
            .release
            .suppress(frame, down, pressed, released);
        if escape || !focused || blocked && owned {
            if owned {
                self.collider_tool.cancel();
            } else if escape {
                self.collider_tool.exit();
            }
            return owned || suppressed;
        }
        let target = self
            .selected_index
            .zip(self.collider_tool.mode())
            .and_then(|(row, id)| {
                let component = self.session.scene().objects[row]
                    .components
                    .iter()
                    .find(|component| component.id == id)?;
                Some((row, collider_component(component)?))
            });
        let Some((row, collider)) = target else {
            self.collider_tool.exit();
            return owned || suppressed;
        };
        let Ok(world) = self.collider_for_viewport(row, collider) else {
            self.collider_tool.exit();
            return owned || suppressed;
        };
        if self.view_flags.get(row).is_some_and(|flags| flags.hidden)
            || !self.collider_effective(row, collider.enabled)
        {
            self.collider_tool.exit();
            return owned || suppressed;
        }
        let camera = Projector::new(viewport);
        let hover = cursor
            .filter(|point| {
                !blocked
                    && !suppressed
                    && viewport.rect.contains(*point)
                    && viewport.response.contains_pointer()
            })
            .and_then(|point| world.grip(viewport, camera, point));
        if pressed && !owned {
            if let Some((grip, point)) = hover.zip(cursor) {
                let (origin, axis, world_per_local) = world.axis(viewport, grip, collider);
                if let Some(start) = camera.axis_at(point, origin, axis) {
                    self.collider_tool.gesture = Some(Gesture {
                        revision: self.session.revision(),
                        row,
                        original: collider,
                        staged: collider,
                        world: self.preview_world[row],
                        grip,
                        origin,
                        axis,
                        world_per_local,
                        start,
                        cursor: point,
                        camera,
                    });
                    self.error = None;
                }
            }
        }
        if let Some(gesture) = &mut self.collider_tool.gesture {
            let allowed = gesture.revision == self.session.revision()
                && gesture.row == row
                && camera.matrix == gesture.camera.matrix
                && camera.rect == gesture.camera.rect
                && gesture.world == self.preview_world[row];
            let staged = cursor.filter(|_| allowed).and_then(|point| {
                if point.distance(gesture.cursor) <= 3.0 {
                    return Some(gesture.original);
                }
                let at = gesture
                    .camera
                    .axis_at(point, gesture.origin, gesture.axis)?;
                let delta = (at - gesture.start) / gesture.world_per_local;
                match gesture.grip {
                    Grip::Face(face) => resize(gesture.original, face, delta),
                    Grip::Radius => resize_radius(gesture.original, delta),
                }
            });
            match staged {
                Some(staged) => gesture.staged = staged,
                None => {
                    let message = match gesture.grip {
                        Grip::Face(_) => Text::ColliderDragInvalid,
                        Grip::Radius => Text::ColliderRadiusDragInvalid,
                    };
                    self.collider_tool.cancel();
                    self.error = Some(message.plain().into());
                    return true;
                }
            }
        }

        let (display, active) = if let Some(gesture) = &self.collider_tool.gesture {
            (
                self.collider_for_viewport(row, gesture.staged)
                    .unwrap_or(world),
                Some(gesture.grip),
            )
        } else {
            (world, None)
        };
        match display {
            WorldCollider::Box(world) => {
                let face = |grip| match grip {
                    Grip::Face(face) => Some(face),
                    Grip::Radius => None,
                };
                paint_box(
                    ui,
                    camera,
                    world,
                    active.and_then(face),
                    hover.and_then(face),
                );
            }
            WorldCollider::Sphere(world) => paint_sphere(
                ui,
                viewport,
                camera,
                world,
                active.is_some(),
                hover.is_some(),
            ),
        }
        if !down {
            if let Some(gesture) = self.collider_tool.gesture.take() {
                self.collider_tool.release = PointerRelease::InFrame(frame);
                if gesture.staged != gesture.original {
                    self.commit_collider(gesture);
                }
                ui.ctx().request_repaint();
                return true;
            }
        }
        if self.collider_tool.active() {
            ui.ctx().request_repaint();
        }
        owned || suppressed || self.collider_tool.active()
    }

    fn commit_collider(&mut self, gesture: Gesture) {
        let mut owner = self.session.scene().objects[gesture.row].clone();
        let component = owner
            .components
            .iter_mut()
            .find(|component| Some(component.id.as_str()) == self.collider_tool.mode());
        let Some(component) = component else { return };
        if collider_component(component) != Some(gesture.original) {
            return;
        }
        let Component::Collider { offset, shape, .. } = &mut component.kind else {
            return;
        };
        *offset = gesture.staged.offset;
        *shape = gesture.staged.shape;
        match self.session.put_object(gesture.revision, owner) {
            Ok(_) => {
                self.refresh();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }
}

fn faces() -> impl Iterator<Item = Face> {
    (0..3).flat_map(|axis| [false, true].map(|positive| Face { axis, positive }))
}

fn face_at(world_box: WorldBox, projector: Projector, cursor: egui::Pos2) -> Option<Face> {
    faces()
        .filter_map(|face| {
            let origin = world_box.face(face);
            let (point, depth) = projector.screen(origin)?;
            let distance = point.distance_sq(cursor);
            let axis = world_box.axis(face.axis);
            if distance > 9.0_f32.powi(2)
                || projector.axis_at(point, origin, axis).is_none()
                || projector.axis_at(cursor, origin, axis).is_none()
            {
                return None;
            }
            Some((face, distance, depth))
        })
        .min_by(|(_, distance_a, depth_a), (_, distance_b, depth_b)| {
            distance_a
                .total_cmp(distance_b)
                .then(depth_a.total_cmp(depth_b))
        })
        .map(|(face, _, _)| face)
}

fn sphere_axis(
    world: rot_scene::ResolvedCollider,
    viewport: &ViewportInteraction,
) -> (DVec3, DVec3) {
    let ResolvedShape::Sphere { radius } = world.shape else {
        unreachable!("sphere")
    };
    let right = DVec3::from_array(
        [
            viewport.view[0][0],
            viewport.view[1][0],
            viewport.view[2][0],
        ]
        .map(f64::from),
    )
    .normalize();
    (DVec3::from_array(world.center) + radius * right, right)
}

fn paint_sphere(
    ui: &egui::Ui,
    viewport: &ViewportInteraction,
    camera: Projector,
    world: rot_scene::ResolvedCollider,
    active: bool,
    hover: bool,
) {
    let painter = ui.painter().with_clip_rect(camera.rect);
    let color = ui.visuals().selection.stroke.color;
    let highlight = ui.visuals().widgets.active.fg_stroke.color;
    let center = DVec3::from_array(world.center);
    let ResolvedShape::Sphere { radius } = world.shape else {
        unreachable!("sphere")
    };
    let (origin, right) = sphere_axis(world, viewport);
    let up = DVec3::from_array(
        [
            viewport.view[0][1],
            viewport.view[1][1],
            viewport.view[2][1],
        ]
        .map(f64::from),
    )
    .normalize();
    let depth = right.cross(up);
    for (a, b) in [(right, up), (right, depth), (up, depth)] {
        const SEGMENTS: usize = 32;
        let mut previous = camera.screen(center + radius * a);
        for index in 1..=SEGMENTS {
            let (sin, cos) = (std::f64::consts::TAU * index as f64 / SEGMENTS as f64).sin_cos();
            let next = camera.screen(center + radius * (cos * a + sin * b));
            if let (Some((from, _)), Some((to, _))) = (previous, next) {
                painter.line_segment([from, to], egui::Stroke::new(1.25, color));
            }
            previous = next;
        }
    }
    if let Some((point, _)) = camera.screen(origin) {
        let hot = active || hover;
        painter.circle_filled(
            point,
            if hot { 5.0 } else { 3.5 },
            if hot { highlight } else { color },
        );
        if hot {
            ui.ctx().set_cursor_icon(if active {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
            painter.text(
                point + egui::vec2(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                Text::ColliderRadius.plain(),
                egui::FontId::proportional(12.0),
                highlight,
            );
        }
    }
}

fn paint_box(
    ui: &egui::Ui,
    camera: Projector,
    world_box: WorldBox,
    active: Option<Face>,
    hover: Option<Face>,
) {
    let painter = ui.painter().with_clip_rect(camera.rect);
    let color = ui.visuals().selection.stroke.color;
    let highlight = ui.visuals().widgets.active.fg_stroke.color;
    let corners = world_box
        .corners()
        .map(|point| camera.screen(DVec3::from_array(point)));
    for (index, from) in corners.iter().enumerate() {
        for bit in [1, 2, 4] {
            if index & bit == 0 {
                if let (Some((from, _)), Some((to, _))) = (from, corners[index | bit]) {
                    painter.line_segment([*from, to], egui::Stroke::new(1.25, color));
                }
            }
        }
    }
    for face in faces() {
        let origin = world_box.face(face);
        let Some((point, _)) = camera.screen(origin) else {
            continue;
        };
        let hot = active.or(hover) == Some(face);
        let radius = if hot { 5.0 } else { 3.5 };
        let handle = egui::Rect::from_center_size(point, egui::Vec2::splat(radius * 2.0));
        if hot
            || camera
                .axis_at(point, origin, world_box.axis(face.axis))
                .is_some()
        {
            painter.rect_filled(handle, 1.0, if hot { highlight } else { color });
        } else {
            painter.rect_stroke(
                handle,
                1.0,
                egui::Stroke::new(1.0, color.gamma_multiply(0.45)),
                egui::StrokeKind::Inside,
            );
        }
        if hot {
            ui.ctx().set_cursor_icon(if active.is_some() {
                egui::CursorIcon::Grabbing
            } else {
                egui::CursorIcon::Grab
            });
            painter.text(
                point + egui::vec2(8.0, -8.0),
                egui::Align2::LEFT_BOTTOM,
                face.label(),
                egui::FontId::proportional(12.0),
                highlight,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::ColliderRole;

    #[test]
    fn rounded_center_retains_the_stationary_face_at_large_offsets() {
        let original = Collider {
            enabled: true,
            role: ColliderRole::Sensor,
            offset: [8192.0, 0.0, 0.0],
            shape: ColliderShape::Box {
                half_extents: [0.5; 3],
            },
        };
        for positive in [false, true] {
            let face = Face { axis: 0, positive };
            let result = resize(original, face, face.sign() * 0.0033).unwrap();
            assert_ne!(result, original);
            assert_eq!(result.role, original.role);
            let ColliderShape::Box { half_extents } = result.shape else {
                unreachable!()
            };
            let fixed =
                |center: f32, extent: f32| f64::from(center) - face.sign() * f64::from(extent);
            assert_eq!(
                fixed(original.offset[0], 0.5),
                fixed(result.offset[0], half_extents[0])
            );
        }
    }
}
