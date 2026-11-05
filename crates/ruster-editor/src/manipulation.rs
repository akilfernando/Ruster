//! Transient translation gestures and the window-owned object clipboard.
//! Authored transactions stay in Session; no pointer frame enters undo history.
use eframe::egui;
use ruster_scene::{Object, OpaqueMaterial, Scene, Session, Transform};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use transform_gizmo_egui::{
    math::{DMat3, DMat4, DQuat, DVec3, Transform as GizmoTransform},
    prelude::*,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Translate,
    Rotate,
    Scale,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Pivot {
    Individual,
    #[default]
    Center,
    Active,
}
fn rotation(t: &Transform) -> DQuat {
    let [x, y, z] = t.rotation.map(f64::from);
    DQuat::from_rotation_z(z) * DQuat::from_rotation_y(y) * DQuat::from_rotation_x(x)
}
fn nearby_euler(q: DQuat, original: [f32; 3]) -> [f32; 3] {
    let old = Transform {
        rotation: original,
        ..Default::default()
    };
    if q.dot(rotation(&old)).abs() >= 1.0 - 1e-12 {
        return original;
    }
    let m = DMat3::from_quat(q.normalize());
    let y = (-m.x_axis.z).clamp(-1.0, 1.0).asin();
    let (x, z) = if y.cos().abs() > 1e-7 {
        (m.y_axis.z.atan2(m.z_axis.z), m.x_axis.y.atan2(m.x_axis.x))
    } else {
        let x = f64::from(original[0]);
        (x, (-m.y_axis.x).atan2(m.y_axis.y) + y.signum() * x)
    };
    let close = |v: f64, reference: f32| {
        v + std::f64::consts::TAU * ((f64::from(reference) - v) / std::f64::consts::TAU).round()
    };
    [
        [x, y, z],
        [
            x + std::f64::consts::PI,
            std::f64::consts::PI - y,
            z + std::f64::consts::PI,
        ],
    ]
    .into_iter()
    .map(|angles| std::array::from_fn(|i| close(angles[i], original[i]) as f32))
    .min_by(|a: &[f32; 3], b: &[f32; 3]| {
        let cost = |v: [f32; 3]| {
            v.into_iter()
                .zip(original)
                .map(|(a, b)| f64::from(a - b).powi(2))
                .sum::<f64>()
        };
        cost(*a).total_cmp(&cost(*b))
    })
    .unwrap_or(original)
}
fn proxy_for(
    session: &Session,
    ids: &[String],
    pivot: Pivot,
    local: bool,
) -> Result<GizmoTransform, String> {
    let mut center = DVec3::ZERO;
    let mut active = None;
    for id in ids {
        let object = session
            .scene()
            .objects
            .iter()
            .find(|o| &o.id == id)
            .ok_or("Selected object disappeared.")?;
        center += DVec3::from_array(object.transform.translation.map(f64::from));
        active = Some(&object.transform);
    }
    let active = active.ok_or("Select an object first.")?;
    center /= ids.len() as f64;
    let position = if pivot == Pivot::Active {
        DVec3::from_array(active.translation.map(f64::from))
    } else {
        center
    };
    Ok(GizmoTransform::from_scale_rotation_translation(
        DVec3::ONE,
        if local {
            rotation(active)
        } else {
            DQuat::IDENTITY
        },
        position,
    ))
}
#[derive(Clone)]
pub(super) struct Gesture {
    revision: u64,
    start: Vec<(String, Transform)>,
    candidate: Vec<(String, Transform)>,
    tool: Tool,
    pivot: Pivot,
    local: bool,
    proxy: GizmoTransform,
    current_proxy: GizmoTransform,
}
impl Gesture {
    #[cfg(test)]
    fn begin(session: &Session, ids: &[String]) -> Result<Self, String> {
        Self::configured(session, ids, Tool::Translate, Pivot::Center, false)
    }
    fn configured(
        session: &Session,
        ids: &[String],
        tool: Tool,
        pivot: Pivot,
        local: bool,
    ) -> Result<Self, String> {
        let start: Vec<_> = ids
            .iter()
            .map(|id| {
                session
                    .scene()
                    .objects
                    .iter()
                    .find(|o| &o.id == id)
                    .map(|o| (id.clone(), o.transform.clone()))
                    .ok_or_else(|| format!("Object {id} disappeared; gesture cancelled."))
            })
            .collect::<Result<_, _>>()?;
        let proxy = proxy_for(session, ids, pivot, local)?;
        Ok(Self {
            revision: session.scene().revision,
            candidate: start.clone(),
            start,
            tool,
            pivot,
            local,
            proxy,
            current_proxy: proxy,
        })
    }
    fn transform(&mut self, proxy: GizmoTransform) -> Result<(), String> {
        let p = DVec3::from(self.proxy.translation);
        let translation = DVec3::from(proxy.translation) - p;
        let q_start = DQuat::from(self.proxy.rotation).normalize();
        let q_new = DQuat::from(proxy.rotation).normalize();
        let delta = (q_new * q_start.inverse()).normalize();
        let local_delta = (q_start.inverse() * q_new).normalize();
        let scale = DVec3::from(proxy.scale);
        if !scale.is_finite() || scale.min_element() <= 0.0 {
            return Err("Scale must remain positive and finite; gesture cancelled.".into());
        }
        let individual_local_scale =
            self.tool == Tool::Scale && self.local && self.pivot == Pivot::Individual;
        if self.tool == Tool::Scale
            && !individual_local_scale
            && (scale.x - scale.y).abs().max((scale.x - scale.z).abs()) > 1e-7
        {
            return Err(
                "Group scaling is uniform; use Local + Individual origins for nonuniform scale."
                    .into(),
            );
        }
        let unchanged_rotation = delta.dot(DQuat::IDENTITY).abs() >= 1.0 - 1e-12;
        let candidate = self
            .start
            .iter()
            .map(|(id, original)| {
                let mut next = original.clone();
                let position = DVec3::from_array(original.translation.map(f64::from));
                match self.tool {
                    Tool::Translate => {
                        if translation != DVec3::ZERO {
                            next.translation =
                                (position + translation).to_array().map(|v| v as f32);
                        }
                    }
                    Tool::Rotate => {
                        if !unchanged_rotation {
                            let next_q = if self.local && self.pivot == Pivot::Individual {
                                rotation(original) * local_delta
                            } else {
                                delta * rotation(original)
                            };
                            next.rotation = nearby_euler(next_q, original.rotation);
                            if self.pivot != Pivot::Individual {
                                next.translation =
                                    (p + delta * (position - p)).to_array().map(|v| v as f32);
                            }
                        }
                    }
                    Tool::Scale => {
                        if scale != DVec3::ONE {
                            next.scale = (DVec3::from_array(original.scale.map(f64::from)) * scale)
                                .to_array()
                                .map(|v| v as f32);
                            if self.pivot != Pivot::Individual {
                                next.translation =
                                    (p + (position - p) * scale.x).to_array().map(|v| v as f32);
                            }
                        }
                    }
                }
                next.validate().map_err(|e| e.to_string())?;
                Ok((id.clone(), next))
            })
            .collect::<Result<_, String>>()?;
        self.candidate = candidate;
        self.current_proxy = proxy;
        Ok(())
    }
    fn commit(self, session: &mut Session) -> Result<u64, String> {
        session
            .edit_transforms(self.revision, &self.candidate)
            .map_err(|e| e.to_string())
    }
}

#[derive(Clone, serde::Serialize)]
struct Clipboard {
    version: u32,
    source_root: Option<PathBuf>,
    document_generation: u64,
    objects: Vec<Object>,
    materials: BTreeMap<String, OpaqueMaterial>,
}

impl Clipboard {
    fn validate(&self) -> Result<(), String> {
        if self.version != 2
            || self.objects.is_empty()
            || self.objects.len() > ruster_scene::MAX_OBJECTS
            || self.materials.len() > ruster_scene::MAX_MATERIALS
        {
            return Err(
                "Unsupported or oversized object clipboard; copy the selection again.".into(),
            );
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > 256 * 1024 {
            return Err("Copied objects/materials exceed the 256 KiB clipboard bound.".into());
        }
        let referenced: BTreeSet<_> = self
            .objects
            .iter()
            .filter_map(|o| o.material.as_ref())
            .collect();
        if referenced != self.materials.keys().collect() {
            return Err(
                "Clipboard material definitions do not match its object references; copy again."
                    .into(),
            );
        }
        Scene {
            lighting: Default::default(),
            id: "clipboard".into(),
            revision: 0,
            objects: self.objects.clone(),
            materials: self.materials.clone(),
        }
        .validate()
        .map_err(|e| e.to_string())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AngleUnit {
    #[default]
    Radians,
    Degrees,
}
impl AngleUnit {
    pub fn display(self, radians: f32) -> f32 {
        match self {
            Self::Radians => radians,
            Self::Degrees => radians.to_degrees(),
        }
    }
    pub fn radians(self, value: f32) -> f32 {
        match self {
            Self::Radians => value,
            Self::Degrees => value.to_radians(),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Radians => "Rotation (radians · recovered draft)",
            Self::Degrees => "Rotation (degrees)",
        }
    }
}
// Reserve room for a signed f32 value instead of squeezing three clipped
// fields into every panel. Wrap complete axis/field groups at narrow widths.
pub(super) fn component_group_width(ui: &egui::Ui) -> f32 {
    let available = ui.available_width().max(1.0);
    let spacing = ui.spacing().item_spacing.x;
    let columns = ((available + spacing) / (144.0 + spacing))
        .floor()
        .clamp(1.0, 3.0);
    ((available - (columns - 1.0) * spacing) / columns).max(1.0)
}
// Format only canonical syncs or an active preview, never all scene objects per idle frame.
fn precision_components<'a>(
    transforms: impl Iterator<Item = &'a Transform>,
    unit: AngleUnit,
) -> ([String; 9], [bool; 9]) {
    let mut values = transforms.map(|transform| {
        let mut values = [0.0_f32; 9];
        values[..3].copy_from_slice(&transform.translation);
        values[3..6].copy_from_slice(&transform.rotation);
        values[6..].copy_from_slice(&transform.scale);
        values
    });
    let first = values.next();
    let mut mixed = [false; 9];
    for value in values {
        for i in 0..9 {
            mixed[i] |= first.is_some_and(|first| first[i] != value[i]);
        }
    }
    let text = std::array::from_fn(|i| {
        if mixed[i] {
            String::new()
        } else {
            first
                .map(|value| {
                    if (3..6).contains(&i) {
                        unit.display(value[i]).to_string()
                    } else {
                        value[i].to_string()
                    }
                })
                .unwrap_or_default()
        }
    });
    (text, mixed)
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DraftSnapshot {
    precision: [String; 9],
    mixed: [bool; 9],
    edited: [bool; 9],
    revision: Option<u64>,
    delta: [String; 3],
    #[serde(default)]
    rotation_unit: AngleUnit,
}
pub(super) struct Manipulation {
    pub ids: Vec<String>,
    pub local: bool,
    pub tool: Tool,
    pub pivot: Pivot,
    pub snap_degrees: f32,
    pub snap_scale: f32,
    precision: [String; 9],
    mixed: [bool; 9],
    edited: [bool; 9],
    precision_revision: Option<u64>,
    precision_had_focus: bool,
    rotation_unit: AngleUnit,
    pub snap: bool,
    pub snap_distance: f32,
    pub delta: [String; 3],
    gizmo: Gizmo,
    gesture: Option<Gesture>,
    clipboard: Option<Clipboard>,
    suppress_pointer: bool,
    draft_generation: u64,
}
impl Default for Manipulation {
    fn default() -> Self {
        Self {
            ids: Vec::new(),
            local: false,
            tool: Tool::Translate,
            pivot: Pivot::Center,
            snap_degrees: 15.0,
            snap_scale: 0.1,
            precision: std::array::from_fn(|_| String::new()),
            mixed: [false; 9],
            edited: [false; 9],
            precision_revision: None,
            precision_had_focus: false,
            rotation_unit: AngleUnit::Degrees,
            snap: false,
            snap_distance: 0.5,
            delta: std::array::from_fn(|_| "0".into()),
            gizmo: Gizmo::default(),
            gesture: None,
            clipboard: None,
            suppress_pointer: false,
            draft_generation: 0,
        }
    }
}
impl Manipulation {
    pub fn draft_generation(&self) -> u64 {
        self.draft_generation
    }
    pub fn draft_changed(&mut self) {
        self.draft_generation = self.draft_generation.wrapping_add(1);
    }
    pub fn offset_pending(&self) -> bool {
        self.delta.iter().any(|text| text != "0")
    }
    pub fn snapshot_drafts(&self) -> DraftSnapshot {
        DraftSnapshot {
            precision: self.precision.clone(),
            mixed: self.mixed,
            edited: self.edited,
            revision: self.precision_revision,
            delta: self.delta.clone(),
            rotation_unit: self.rotation_unit,
        }
    }
    pub fn restore_drafts(&mut self, scene: &Scene, draft: DraftSnapshot) -> Result<(), String> {
        self.reset_precision_unit(scene, draft.rotation_unit);
        if draft
            .precision
            .iter()
            .chain(draft.delta.iter())
            .any(|text| text.chars().count() > 1024)
            || draft.mixed != self.mixed
            || draft.revision
                != draft
                    .edited
                    .iter()
                    .any(|edited| *edited)
                    .then_some(scene.revision)
            || (self.ids.is_empty() && draft.edited.iter().any(|edited| *edited))
            || draft
                .precision
                .iter()
                .enumerate()
                .any(|(i, text)| !draft.edited[i] && text != &self.precision[i])
        {
            return Err("Recovery precision drafts have invalid selection, masks, revision or text bounds; checkpoint preserved.".into());
        }
        self.precision = draft.precision;
        self.edited = draft.edited;
        self.precision_revision = draft.revision;
        self.delta = draft.delta;
        if !self.precision_pending() {
            self.reset_precision(scene);
        }
        self.draft_changed();
        Ok(())
    }
    pub fn precision_pending(&self) -> bool {
        self.edited.iter().any(|v| *v)
    }
    pub fn reset_precision(&mut self, scene: &Scene) {
        self.reset_precision_unit(scene, AngleUnit::Degrees);
    }
    fn reset_precision_unit(&mut self, scene: &Scene, unit: AngleUnit) {
        self.rotation_unit = unit;
        self.draft_changed();
        self.edited = [false; 9];
        self.precision_revision = None;
        self.precision_had_focus = false;
        (self.precision, self.mixed) = precision_components(
            self.ids
                .iter()
                .filter_map(|id| scene.objects.iter().find(|o| &o.id == id))
                .map(|object| &object.transform),
            unit,
        );
    }
    fn preview_precision(&self) -> Option<([String; 9], [bool; 9])> {
        self.gesture.as_ref().map(|gesture| {
            precision_components(
                gesture.candidate.iter().map(|(_, transform)| transform),
                AngleUnit::Degrees,
            )
        })
    }
    pub fn apply_precision(&mut self, session: &mut Session) -> Result<(), String> {
        if !self.precision_pending() {
            return Ok(());
        }
        let mut values = [0.0_f32; 9];
        for (i, text) in self
            .precision
            .iter()
            .enumerate()
            .filter(|(i, _)| self.edited[*i])
        {
            values[i] = text.trim().parse::<f32>().map_err(|_| {
                "Enter finite transform values; the draft is preserved.".to_string()
            })?;
            if !values[i].is_finite() {
                return Err("Enter finite transform values; the draft is preserved.".into());
            }
        }
        let edits: Vec<_> = self
            .ids
            .iter()
            .map(|id| {
                let mut next = session
                    .scene()
                    .objects
                    .iter()
                    .find(|o| &o.id == id)
                    .ok_or("Selected object disappeared.")?
                    .transform
                    .clone();
                for (i, value) in values.iter().copied().enumerate() {
                    if self.edited[i] {
                        if i < 3 {
                            next.translation[i] = value;
                        } else if i < 6 {
                            let axis = i - 3;
                            if value != self.rotation_unit.display(next.rotation[axis]) {
                                next.rotation[axis] = self.rotation_unit.radians(value);
                            }
                        } else {
                            next.scale[i - 6] = value;
                        }
                    }
                }
                Ok((id.clone(), next))
            })
            .collect::<Result<_, String>>()?;
        session
            .edit_transforms(
                self.precision_revision
                    .ok_or("Missing precision draft revision.")?,
                &edits,
            )
            .map_err(|e| e.to_string())?;
        self.reset_precision(session.scene());
        Ok(())
    }
    pub fn precision_ui(
        &mut self,
        ui: &mut egui::Ui,
        session: &mut Session,
        auto_commit: bool,
    ) -> Result<bool, String> {
        let mut preview = self.preview_precision();
        let previewing = preview.is_some();
        let mixed = preview
            .as_ref()
            .map(|(_, mixed)| *mixed)
            .unwrap_or(self.mixed);
        let unit = if previewing {
            AngleUnit::Degrees
        } else {
            self.rotation_unit
        };
        let enter_pressed = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none());
        let escape_pressed =
            ui.input(|i| i.key_pressed(egui::Key::Escape) && i.modifiers.is_none());
        let mut focused = false;
        let mut field_has_focus = false;
        if mixed.iter().any(|mixed| *mixed) {
            ui.label("— = mixed values").on_hover_text("Editing a component replaces only that component for all selected objects; other components keep their individual values.");
        }
        for (row, label) in ["Position", unit.label(), "Scale"].into_iter().enumerate() {
            ui.label(label);
            let group_width = component_group_width(ui);
            ui.horizontal_wrapped(|ui| {
                for (axis, label) in ["X", "Y", "Z"].into_iter().enumerate() {
                    let i = row * 3 + axis;
                    ui.allocate_ui_with_layout(egui::vec2(group_width,ui.spacing().interact_size.y),egui::Layout::left_to_right(egui::Align::Center),|ui| {
                        ui.label(label);
                        let response = ui.add_sized([ui.available_width().max(48.0),ui.spacing().interact_size.y],
                            egui::TextEdit::singleline(if let Some((text,_))=&mut preview {&mut text[i]} else {&mut self.precision[i]}).char_limit(1024).interactive(!previewing && ui.is_enabled())
                                .hint_text(if mixed[i] { "—" } else { "" }));
                        let response=response.on_hover_ui(|ui| {
                            let value=preview.as_ref().map(|(text,_)|text[i].as_str()).unwrap_or(&self.precision[i]);
                            if !value.is_empty() && value.chars().count()<=128 {ui.label(value);}
                            ui.label("Editing changes only this component for selected objects. Long input stays in the field; use the cursor to scroll through it.");
                        });
                        focused |= response.has_focus() || response.lost_focus();
                        field_has_focus |= response.has_focus();
                        if response.changed() && !previewing {
                            self.draft_changed();
                            self.edited[i] = true;
                            self.precision_revision.get_or_insert(session.scene().revision);
                        }
                    });
                }
            });
        }
        if previewing {
            ui.small("Drag preview · release to apply · Escape to cancel");
            return Ok(false);
        }
        let mut apply = focused && enter_pressed;
        apply |=
            auto_commit && self.precision_had_focus && !field_has_focus && self.precision_pending();
        self.precision_had_focus = field_has_focus;
        let mut cancel = focused && escape_pressed;
        if self.precision_pending() {
            ui.horizontal_wrapped(|ui| {
                apply |= ui
                    .add_enabled(
                        self.precision_pending(),
                        egui::Button::new("Apply transform components"),
                    )
                    .clicked();
                cancel |= ui
                    .add_enabled(
                        self.precision_pending(),
                        egui::Button::new("Revert transform components"),
                    )
                    .clicked();
            });
        }
        if cancel {
            self.reset_precision(session.scene());
            return Ok(false);
        }
        if apply {
            self.apply_precision(session)?;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn active(&self) -> bool {
        self.gesture.is_some()
    }
    pub fn cancel(&mut self) {
        self.suppress_pointer |= self.gesture.is_some();
        self.gesture = None;
        self.gizmo = Gizmo::default();
    }
    pub fn clipboard_from(&mut self, other: &mut Self) {
        self.clipboard = other.clipboard.take();
    }
    pub fn reconcile(&mut self, scene: &Scene) {
        self.ids
            .retain(|id| scene.objects.iter().any(|o| &o.id == id));
    }
    pub fn select(&mut self, id: Option<String>, additive: bool) {
        if !additive {
            self.ids.clear();
        }
        if let Some(id) = id {
            if additive && self.ids.contains(&id) {
                self.ids.retain(|old| old != &id);
            } else {
                self.ids.push(id);
            }
        }
    }
    pub fn select_only(&mut self, id: Option<String>) {
        self.select(id, false);
    }
    pub fn selected(&self) -> Option<String> {
        self.ids.last().cloned()
    }
    pub fn copy(
        &mut self,
        session: &Session,
        root: Option<&Path>,
        document_generation: u64,
    ) -> Result<(), String> {
        let objects: Vec<_> = self
            .ids
            .iter()
            .map(|id| {
                session
                    .scene()
                    .objects
                    .iter()
                    .find(|o| &o.id == id)
                    .cloned()
                    .ok_or_else(|| "Selected object disappeared; Copy was not changed.".to_string())
            })
            .collect::<Result<_, _>>()?;
        if objects.is_empty() {
            return Err("Select objects before Copy.".into());
        }
        let mut materials = BTreeMap::new();
        for id in objects.iter().filter_map(|object| object.material.as_ref()) {
            let value = session.scene().materials.get(id).ok_or_else(|| {
                format!("Object references missing material {id}; repair it before Copy.")
            })?;
            materials.insert(id.clone(), value.clone());
        }
        let clipboard = Clipboard {
            version: 2,
            source_root: root.map(Path::to_path_buf),
            document_generation,
            objects,
            materials,
        };
        clipboard.validate()?;
        self.clipboard = Some(clipboard);
        Ok(())
    }
    pub fn fresh_ids(session: &Session, count: usize) -> Result<Vec<String>, String> {
        session
            .allocate_object_ids(count)
            .map_err(|e| e.to_string())
    }
    pub fn paste(
        &mut self,
        session: &mut Session,
        root: Option<&Path>,
        document_generation: u64,
    ) -> Result<(), String> {
        let clipboard = self
            .clipboard
            .as_ref()
            .ok_or("Copy objects in this editor window before Paste.")?;
        clipboard.validate()?;
        let fresh_materials: Vec<_> = clipboard
            .materials
            .keys()
            .filter(|id| {
                clipboard.document_generation != document_generation
                    || !session.scene().materials.contains_key(*id)
            })
            .collect();
        let fresh_ids = session
            .allocate_material_ids(fresh_materials.len())
            .map_err(|e| e.to_string())?;
        let remap: BTreeMap<_, _> = fresh_materials
            .into_iter()
            .cloned()
            .zip(fresh_ids)
            .collect();
        let imported: BTreeMap<_, _> = remap
            .iter()
            .map(|(old, new)| (new.clone(), clipboard.materials[old].clone()))
            .collect();
        // Same-document pastes share live definitions, including edits made since
        // Copy. Other incarnations import snapshots even beside the same catalog.
        let external_textures = clipboard.materials.iter().any(|(id, stored)| {
            let effective = if remap.contains_key(id) {
                stored
            } else {
                &session.scene().materials[id]
            };
            effective.base_color_texture.is_some()
        });
        if (clipboard.objects.iter().any(|o| o.mesh.is_some()) || external_textures)
            && (clipboard.source_root.is_none()
                || root.is_none()
                || clipboard.source_root.as_deref() != root)
        {
            return Err("Mesh/texture references belong to another or unknown project catalog; paste beside the source project or remove those references first.".into());
        }
        let ids = Self::fresh_ids(session, clipboard.objects.len())?;
        let mut objects = clipboard.objects.clone();
        for (object, id) in objects.iter_mut().zip(&ids) {
            object.id.clone_from(id);
            if let Some(replacement) = object.material.as_ref().and_then(|old| remap.get(old)) {
                object.material = Some(replacement.clone());
            }
        }
        session
            .create_many_with_materials(session.scene().revision, objects, imported)
            .map_err(|e| e.to_string())?;
        self.ids = ids;
        Ok(())
    }
    pub fn apply_delta(&self, session: &mut Session) -> Result<(), String> {
        let mut delta = [0.0_f32; 3];
        for (axis, text) in self.delta.iter().enumerate() {
            delta[axis] = text
                .trim()
                .parse::<f32>()
                .map_err(|_| "Enter finite translation offsets.".to_string())?;
            if !delta[axis].is_finite() {
                return Err("Enter finite translation offsets.".into());
            }
        }
        let mut edits = Vec::with_capacity(self.ids.len());
        for id in &self.ids {
            let mut transform = session
                .scene()
                .objects
                .iter()
                .find(|o| &o.id == id)
                .ok_or("Selected object disappeared.")?
                .transform
                .clone();
            for (value, offset) in transform.translation.iter_mut().zip(delta) {
                *value += offset;
            }
            edits.push((id.clone(), transform));
        }
        session
            .edit_transforms(session.scene().revision, &edits)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn transient_scene(&self, scene: &Scene) -> Option<Scene> {
        self.gesture.as_ref().map(|gesture| {
            let mut candidate = scene.clone();
            for (id, transform) in &gesture.candidate {
                if let Some(object) = candidate.objects.iter_mut().find(|o| &o.id == id) {
                    object.transform.clone_from(transform);
                }
            }
            candidate
        })
    }
    /// Returns true when the overlay owns the pointer (including a release).
    pub fn draw(
        &mut self,
        ui: &mut egui::Ui,
        viewport: &ruster_render::ViewportInteraction,
        session: &mut Session,
    ) -> Result<bool, String> {
        if self.ids.is_empty() {
            self.cancel();
            return Ok(false);
        }
        let cancelled = ui.input(|i| !i.focused || i.key_pressed(egui::Key::Escape))
            || self
                .gesture
                .as_ref()
                .is_some_and(|g| g.revision != session.scene().revision);
        if cancelled && self.active() {
            self.cancel();
            ui.ctx().request_repaint();
        }
        let blocked = !ui.input(|i| i.focused) || self.suppress_pointer;
        if self.suppress_pointer
            && !ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
        {
            self.suppress_pointer = false;
        }
        let proxy = match &self.gesture {
            Some(gesture) => gesture.current_proxy,
            None => proxy_for(session, &self.ids, self.pivot, self.local)?,
        };
        let targets = [proxy];
        self.gizmo.update_config(GizmoConfig {
            view_matrix: DMat4::from_cols_array_2d(&viewport.view.map(|row| row.map(f64::from)))
                .into(),
            projection_matrix: DMat4::from_cols_array_2d(
                &viewport.gizmo_projection.map(|row| row.map(f64::from)),
            )
            .into(),
            viewport: viewport.rect,
            pixels_per_point: ui.ctx().pixels_per_point(),
            modes: match self.tool {
                Tool::Translate => GizmoMode::all_translate(),
                Tool::Rotate => GizmoMode::all_rotate(),
                Tool::Scale => {
                    if self.local && self.pivot == Pivot::Individual {
                        GizmoMode::all_scale()
                    } else {
                        GizmoMode::ScaleUniform.into()
                    }
                }
            },
            orientation: if self.local {
                GizmoOrientation::Local
            } else {
                GizmoOrientation::Global
            },
            snapping: self.snap,
            snap_distance: self.snap_distance,
            snap_angle: self.snap_degrees.to_radians(),
            snap_scale: self.snap_scale,
            ..Default::default()
        });
        let before_active = self.active();
        let (cursor, pressed, down) = ui.input(|input| {
            (
                input.pointer.hover_pos().unwrap_or_default(),
                input.pointer.button_pressed(egui::PointerButton::Primary),
                input.pointer.button_down(egui::PointerButton::Primary),
            )
        });
        let hovered = !blocked && viewport.response.hovered() && viewport.rect.contains(cursor);
        // The viewport already owns pointer routing. Registering another widget
        // at the cursor steals ordinary object clicks, even far from a handle.
        let result = self.gizmo.update(
            GizmoInteraction {
                hovered,
                drag_started: hovered && pressed,
                dragging: !blocked && (down || pressed),
                cursor_pos: (cursor.x, cursor.y),
            },
            &targets,
        );
        // Latch a real handle press even if the toolkit emits no transform yet.
        // Treat coalesced press/release as an initial update, then release below.
        // Retain that ownership through release, so a zero-motion handle click
        // never selects an object or empty space behind the handle.
        if !blocked && hovered && pressed && self.gizmo.is_focused() && self.gesture.is_none() {
            self.gesture = Some(Gesture::configured(
                session, &self.ids, self.tool, self.pivot, self.local,
            )?);
        }
        if let Some((_, values)) = result {
            if let Some(gesture) = &mut self.gesture {
                if let Err(error) = gesture.transform(values[0]) {
                    self.cancel();
                    return Err(error);
                }
            }
        }
        let draw = self.gizmo.draw();
        let vertices = draw
            .vertices
            .into_iter()
            .zip(draw.colors)
            .map(|(pos, [r, g, b, a])| egui::epaint::Vertex {
                pos: pos.into(),
                uv: egui::Pos2::ZERO,
                color: egui::Rgba::from_rgba_premultiplied(r, g, b, a).into(),
            })
            .collect();
        ui.painter().with_clip_rect(viewport.rect).add(egui::Mesh {
            indices: draw.indices,
            vertices,
            ..Default::default()
        });
        if blocked {
            return Ok(true);
        }
        if self.active() && !ui.input(|i| i.pointer.button_down(egui::PointerButton::Primary)) {
            let gesture = self.gesture.take().unwrap();
            let result = gesture.commit(session);
            self.gizmo = Gizmo::default();
            ui.ctx().request_repaint();
            result?;
            return Ok(true);
        }
        if self.active() {
            ui.ctx().request_repaint();
        }
        Ok(before_active || self.active())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect};

    fn assert_orientation(actual: [f32; 3], expected: DQuat) {
        let t = Transform {
            rotation: actual,
            ..Default::default()
        };
        assert!(
            rotation(&t).dot(expected.normalize()).abs() > 1.0 - 2e-10,
            "orientation mismatch {actual:?}"
        );
    }
    // Unique geometry risks: pivot/group vs individual-local rotation, exact
    // representable scale and equivalent Euler branches must not silently shear.
    #[test]
    fn rotation_scale_pivots_and_euler_equivalence_preserve_exact_components() {
        let mut scene = Scene::demo();
        scene.objects.remove(1);
        scene.objects[0].transform.rotation = [0.2, 0.3, 0.4];
        scene.objects[1].transform.rotation = [-0.4, 0.7, 0.2];
        let session = Session::new(scene.clone()).unwrap();
        let ids = vec!["left".into(), "right".into()];
        let delta = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
        for pivot in [Pivot::Center, Pivot::Active, Pivot::Individual] {
            let mut g = Gesture::configured(&session, &ids, Tool::Rotate, pivot, false).unwrap();
            let mut proxy = g.proxy;
            proxy.rotation = (delta * DQuat::from(proxy.rotation)).into();
            g.transform(proxy).unwrap();
            let p = DVec3::from(g.proxy.translation);
            for ((_, actual), original) in g.candidate.iter().zip(&scene.objects) {
                assert_orientation(actual.rotation, delta * rotation(&original.transform));
                assert_eq!(actual.scale, original.transform.scale);
                let old = DVec3::from_array(original.transform.translation.map(f64::from));
                let expected = if pivot == Pivot::Individual {
                    old
                } else {
                    p + delta * (old - p)
                };
                assert!(
                    (DVec3::from_array(actual.translation.map(f64::from)) - expected).length()
                        < 1e-6
                );
            }
            let mut scale = Gesture::configured(&session, &ids, Tool::Scale, pivot, false).unwrap();
            let mut proxy = scale.proxy;
            proxy.scale = DVec3::splat(2.0).into();
            scale.transform(proxy).unwrap();
            let p = DVec3::from(scale.proxy.translation);
            for ((_, actual), original) in scale.candidate.iter().zip(&scene.objects) {
                assert_eq!(actual.rotation, original.transform.rotation);
                assert_eq!(actual.scale, original.transform.scale.map(|v| v * 2.0));
                let old = DVec3::from_array(original.transform.translation.map(f64::from));
                let expected = if pivot == Pivot::Individual {
                    old
                } else {
                    p + (old - p) * 2.0
                };
                assert!(
                    (DVec3::from_array(actual.translation.map(f64::from)) - expected).length()
                        < 1e-6
                );
            }
        }
        let mut g =
            Gesture::configured(&session, &ids, Tool::Rotate, Pivot::Individual, true).unwrap();
        let local_delta = DQuat::from_rotation_x(0.8);
        let mut proxy = g.proxy;
        proxy.rotation = (DQuat::from(proxy.rotation) * local_delta).into();
        g.transform(proxy).unwrap();
        for ((_, actual), original) in g.candidate.iter().zip(&scene.objects) {
            assert_eq!(actual.translation, original.transform.translation);
            assert_orientation(actual.rotation, rotation(&original.transform) * local_delta);
        }
        let mut scale =
            Gesture::configured(&session, &ids, Tool::Scale, Pivot::Individual, true).unwrap();
        let mut proxy = scale.proxy;
        proxy.scale = DVec3::new(2.0, 3.0, 0.5).into();
        scale.transform(proxy).unwrap();
        for ((_, actual), original) in scale.candidate.iter().zip(&scene.objects) {
            assert_eq!(actual.translation, original.transform.translation);
            assert_eq!(actual.rotation, original.transform.rotation);
            assert_eq!(actual.scale, [2.0, 3.0, 0.5]);
        }
        let before = scale.candidate.clone();
        proxy.scale = DVec3::ZERO.into();
        assert!(scale.transform(proxy).is_err());
        assert_eq!(scale.candidate, before);
        let mut unsupported =
            Gesture::configured(&session, &ids, Tool::Scale, Pivot::Center, false).unwrap();
        let mut proxy = unsupported.proxy;
        proxy.scale = DVec3::new(1.0, 2.0, 1.0).into();
        assert!(unsupported.transform(proxy).is_err());
        for tool in [Tool::Rotate, Tool::Scale] {
            let mut g = Gesture::configured(&session, &ids, tool, Pivot::Center, true).unwrap();
            g.transform(g.proxy).unwrap();
            assert_eq!(g.start, g.candidate);
        }
        for y in [
            std::f32::consts::FRAC_PI_2,
            -std::f32::consts::FRAC_PI_2,
            std::f32::consts::FRAC_PI_2 + 1e-4,
            0.3,
        ] {
            let original = [6.5, y, -12.9];
            let t = Transform {
                rotation: original,
                ..Default::default()
            };
            let q = rotation(&t);
            assert_eq!(
                nearby_euler(-q, original),
                original,
                "equivalent quaternion signs preserve stored Euler bits"
            );
            let next = (DQuat::from_rotation_z(0.01) * q).normalize();
            assert_orientation(nearby_euler(next, original), next);
        }
    }

    #[test]
    fn degree_precision_preserves_untouched_bits_and_legacy_radian_drafts() {
        let mut scene = Scene::demo();
        scene.objects[0].transform.rotation = [0.2, 0.3, 0.4];
        let mut session = Session::new(scene).unwrap();
        let mut m = Manipulation {
            ids: vec!["left".into()],
            ..Default::default()
        };
        m.reset_precision(session.scene());
        assert_eq!(m.precision[3], 0.2_f32.to_degrees().to_string());
        let original = session.scene().objects[0].transform.clone();
        m.edited[3] = true;
        m.precision_revision = Some(0);
        m.apply_precision(&mut session).unwrap();
        assert_eq!(
            session.undo_len(),
            0,
            "same displayed angle is a bit-exact no-op"
        );
        m.precision[4] = "90".into();
        m.edited[4] = true;
        m.precision_revision = Some(0);
        m.apply_precision(&mut session).unwrap();
        let after = &session.scene().objects[0].transform;
        assert_eq!(
            after.rotation,
            [
                original.rotation[0],
                std::f32::consts::FRAC_PI_2,
                original.rotation[2]
            ]
        );
        assert_eq!(session.undo_len(), 1);
        m.reset_precision_unit(session.scene(), AngleUnit::Radians);
        let mut old = serde_json::to_value(m.snapshot_drafts()).unwrap();
        old.as_object_mut().unwrap().remove("rotation_unit");
        old["precision"][3] = "-".into();
        old["edited"][3] = true.into();
        old["revision"] = session.scene().revision.into();
        m.restore_drafts(session.scene(), serde_json::from_value(old).unwrap())
            .unwrap();
        assert_eq!(m.precision[3], "-");
        assert!(m.rotation_unit.label().contains("radians"));
        assert!(m.apply_precision(&mut session).is_err());
        m.precision[3] = "1".into();
        m.apply_precision(&mut session).unwrap();
        assert_eq!(session.scene().objects[0].transform.rotation[0], 1.0);
        assert_eq!(m.rotation_unit, AngleUnit::Degrees);
    }

    // Unique authoring risk: mixed fields replace only explicit components,
    // and invalid/stale precision drafts remain recoverable rather than partial.
    #[test]
    fn mixed_precision_changes_only_edited_axes_with_one_undo() {
        let mut scene = Scene::demo();
        scene.objects[0].transform.rotation = [0.2, 0.3, 0.4];
        scene.objects[2].transform.scale = [2.0, 3.0, 4.0];
        let mut session = Session::new(scene.clone()).unwrap();
        let mut m = Manipulation {
            ids: vec!["left".into(), "right".into()],
            ..Default::default()
        };
        m.reset_precision(session.scene());
        assert!(m.mixed[0]);
        assert!(m.precision[0].is_empty());
        m.precision[0] = "7".into();
        m.edited[0] = true;
        m.precision_revision = Some(0);
        m.apply_precision(&mut session).unwrap();
        assert_eq!(session.undo_len(), 1);
        assert!(!m.precision_pending());
        for id in &m.ids {
            let before = &scene
                .objects
                .iter()
                .find(|o| &o.id == id)
                .unwrap()
                .transform;
            let actual = &session
                .scene()
                .objects
                .iter()
                .find(|o| &o.id == id)
                .unwrap()
                .transform;
            assert_eq!(
                actual.translation,
                [7.0, before.translation[1], before.translation[2]]
            );
            assert_eq!(actual.rotation, before.rotation);
            assert_eq!(actual.scale, before.scale);
        }
        session.undo(1).unwrap();
        m.reset_precision(session.scene());
        m.precision[6] = "-1".into();
        m.edited[6] = true;
        m.precision_revision = Some(session.scene().revision);
        let before = session.scene().clone();
        assert!(m.apply_precision(&mut session).is_err());
        assert_eq!(session.scene(), &before);
        assert!(m.precision_pending());
        assert_eq!(m.precision[6], "-1");
        m.precision[6] = "2".into();
        session
            .rename(session.scene().revision, "left", Some("Other edit".into()))
            .unwrap();
        let before = session.scene().clone();
        assert!(m.apply_precision(&mut session).is_err());
        assert_eq!(session.scene(), &before);
        m.reset_precision(session.scene());
        assert!(!m.precision_pending());
        let ctx = egui::Context::default();
        m.precision[0] = "9".into();
        m.edited[0] = true;
        m.precision_revision = Some(session.scene().revision);
        m.precision_had_focus = true;
        let mut applied = false;
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| applied = m.precision_ui(ui, &mut session, true).unwrap(),
        );
        output.textures_delta.clear();
        assert!(applied);
        assert!(!m.precision_pending());
        m.precision[0] = "-".into();
        m.edited[0] = true;
        m.precision_revision = Some(session.scene().revision);
        m.precision_had_focus = true;
        let before = session.scene().clone();
        let mut error = false;
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| error = m.precision_ui(ui, &mut session, true).is_err(),
        );
        output.textures_delta.clear();
        assert!(error);
        assert_eq!(session.scene(), &before);
        assert_eq!(m.precision[0], "-");
        m.precision_had_focus = true;
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| assert!(!m.precision_ui(ui, &mut session, false).unwrap()),
        );
        output.textures_delta.clear();
        assert!(
            m.precision_pending(),
            "modal focus loss preserves the draft instead of committing or discarding it"
        );
    }

    // Unique boundary: copied membership/mesh origins and group precision edits
    // must stay atomic, preserve untouched components and use fresh identities.
    #[test]
    fn selection_clipboard_and_delta_are_group_transactions() {
        let mut session = Session::new(Scene::demo()).unwrap();
        let mut m = Manipulation::default();
        m.select(Some("left".into()), false);
        m.select(Some("right".into()), true);
        assert_eq!(m.ids, ["left", "right"]);
        m.copy(&session, None, 1).unwrap();
        let before = session.scene().clone();
        m.paste(&mut session, None, 1).unwrap();
        assert_eq!(session.scene().objects.len(), 5);
        assert!(m
            .ids
            .iter()
            .all(|id| !before.objects.iter().any(|o| &o.id == id)));
        assert_eq!(session.undo_len(), 1);
        let copies = m.ids.clone();
        m.delta = ["1.25".into(), "0".into(), "-2".into()];
        m.apply_delta(&mut session).unwrap();
        assert_eq!(session.undo_len(), 2);
        for (id, original) in copies.iter().zip([&before.objects[0], &before.objects[2]]) {
            let actual = session
                .scene()
                .objects
                .iter()
                .find(|o| &o.id == id)
                .unwrap();
            assert_eq!(actual.transform.rotation, original.transform.rotation);
            assert_eq!(actual.transform.scale, original.transform.scale);
            assert_eq!(
                actual.transform.translation,
                [
                    original.transform.translation[0] + 1.25,
                    original.transform.translation[1],
                    original.transform.translation[2] - 2.0
                ]
            );
        }
        let revision = session.scene().revision;
        m.delta[0] = "NaN".into();
        assert!(m.apply_delta(&mut session).is_err());
        assert_eq!(session.scene().revision, revision);
        session.undo(revision).unwrap();
        session.undo(revision + 1).unwrap();
        assert_eq!(session.scene().objects, before.objects);
        m.paste(&mut session, None, 1).unwrap();
        assert!(
            m.ids.iter().all(|id| !copies.contains(id)),
            "redo identities remain reserved"
        );
        session
            .set_mesh(session.scene().revision, &m.ids[0], Some("owned".into()))
            .unwrap();
        m.copy(&session, Some(Path::new("/first")), 1).unwrap();
        let before = session.scene().clone();
        assert!(m
            .paste(&mut session, Some(Path::new("/second")), 1)
            .is_err());
        assert_eq!(session.scene(), &before);
        m.copy(&session, None, 1).unwrap();
        assert!(
            m.paste(&mut session, None, 1).is_err(),
            "unknown mesh origins cannot bind silently"
        );
        // Material identities are scoped to a document incarnation, even when
        // both documents use the same catalog root and the same material ID.
        let mut source_scene = Scene::demo();
        source_scene.objects.truncate(2);
        for object in &mut source_scene.objects {
            object.material = Some("paint".into());
        }
        let copied = OpaqueMaterial {
            tint: [0.2, 0.4, 0.8],
            ..Default::default()
        };
        source_scene
            .materials
            .insert("paint".into(), copied.clone());
        let mut source = Session::new(source_scene).unwrap();
        let mut clipboard = Manipulation {
            ids: vec!["left".into(), "center".into()],
            ..Default::default()
        };
        let root = Some(Path::new("/shared"));
        clipboard.copy(&source, root, 10).unwrap();
        let edited = OpaqueMaterial {
            tint: [0.8, 0.4, 0.2],
            ..Default::default()
        };
        source.set_material(0, "paint", edited.clone()).unwrap();
        clipboard.paste(&mut source, root, 10).unwrap();
        assert_eq!(source.scene().materials.len(), 1);
        assert_eq!(source.scene().materials["paint"], edited);
        assert!(clipboard.ids.iter().all(|id| source
            .scene()
            .objects
            .iter()
            .any(|o| &o.id == id && o.material.as_deref() == Some("paint"))));
        let mut destination_scene = Scene::empty("another-document");
        destination_scene
            .materials
            .insert("paint".into(), OpaqueMaterial::default());
        let mut destination = Session::new(destination_scene).unwrap();
        clipboard.paste(&mut destination, root, 11).unwrap();
        let imported = destination.scene().objects[0].material.clone().unwrap();
        assert_ne!(imported, "paint");
        assert_eq!(
            destination.scene().objects[1].material.as_deref(),
            Some(imported.as_str())
        );
        assert_eq!(destination.scene().materials[&imported], copied);
        assert_eq!(
            destination.scene().materials["paint"],
            OpaqueMaterial::default()
        );
        assert_eq!(destination.undo_len(), 1);
        destination.undo(1).unwrap();
        assert!(destination.scene().objects.is_empty());
        assert!(!destination.scene().materials.contains_key(&imported));
        destination.redo(2).unwrap();
        assert_eq!(destination.scene().materials[&imported], copied);
        // Removing the original same-document definition never silently binds
        // the stored snapshot to another historical identity on the next paste.
        let owners: Vec<_> = source
            .scene()
            .objects
            .iter()
            .map(|o| o.id.clone())
            .collect();
        for id in owners {
            source
                .assign_material(source.scene().revision, &id, None)
                .unwrap();
        }
        source
            .remove_material(source.scene().revision, "paint")
            .unwrap();
        clipboard.paste(&mut source, root, 10).unwrap();
        let restored_id = source
            .scene()
            .objects
            .iter()
            .find(|o| o.id == clipboard.ids[0])
            .unwrap()
            .material
            .clone()
            .unwrap();
        assert_ne!(restored_id, "paint");
        assert_eq!(source.scene().materials[&restored_id], copied);
        let mut full_scene = Scene::empty("full");
        full_scene.objects = (0..ruster_scene::MAX_OBJECTS)
            .map(|n| Object::cube(format!("full-{n}"), None))
            .collect();
        let mut full = Session::new(full_scene).unwrap();
        let before = full.snapshot_bytes().unwrap();
        let selection = clipboard.ids.clone();
        let stored = serde_json::to_vec(clipboard.clipboard.as_ref().unwrap()).unwrap();
        assert!(clipboard.paste(&mut full, root, 20).is_err());
        assert_eq!(full.snapshot_bytes().unwrap(), before);
        assert_eq!(clipboard.ids, selection);
        assert_eq!(
            serde_json::to_vec(clipboard.clipboard.as_ref().unwrap()).unwrap(),
            stored
        );
        clipboard.clipboard.as_mut().unwrap().version = 1;
        assert!(clipboard.paste(&mut full, root, 20).is_err());
        clipboard.clipboard.as_mut().unwrap().version = 2;
        // Texture references require catalog-root confinement even without meshes.
        source
            .set_material(
                source.scene().revision,
                &restored_id,
                OpaqueMaterial {
                    base_color_texture: Some("missing-but-authored-texture".into()),
                    ..copied
                },
            )
            .unwrap();
        clipboard.copy(&source, root, 10).unwrap();
        assert!(clipboard
            .paste(&mut destination, Some(Path::new("/other")), 11)
            .is_err());
        clipboard.copy(&source, None, 10).unwrap();
        assert!(clipboard.paste(&mut destination, None, 11).is_err());
    }

    // Unique UI boundary: the real toolkit's result stream previews only until
    // release, then commits exactly one authored group; Escape/focus loss cancel.
    #[test]
    fn real_off_axis_gizmo_commits_once_and_cancels_without_history() {
        for orthographic in [false, true] {
            let rect = Rect::from_min_max(Pos2::new(173.0, 91.0), Pos2::new(973.0, 691.0));
            let yaw = 0.65_f64;
            let pitch = 0.45_f64;
            let eye = DVec3::new(
                yaw.sin() * pitch.cos(),
                pitch.sin(),
                yaw.cos() * pitch.cos(),
            ) * 8.0;
            let view = DMat4::look_at_rh(eye, DVec3::ZERO, DVec3::Y);
            let projection = if orthographic {
                DMat4::orthographic_rh_gl(-5.0, 5.0, -3.75, 3.75, 0.008, 800.0)
            } else {
                DMat4::perspective_rh_gl(45_f64.to_radians(), 800.0 / 600.0, 0.008, 800.0)
            };
            let median = DVec3::new(2.0, 1.0, -1.0);
            let scale =
                (projection * view * median.extend(1.0)).w / projection.x_axis.x / 800.0 * 2.0;
            let project = |world: DVec3| {
                let ndc = (projection * view).project_point3(world);
                Pos2::new(
                    rect.center().x + (ndc.x * 400.0) as f32,
                    rect.center().y - (ndc.y * 300.0) as f32,
                )
            };
            let start = project(median + DVec3::X * scale * 40.0);
            let end = project(median + DVec3::X * scale * 65.0);
            let mut scene = Scene::demo();
            scene.objects.truncate(2);
            scene.objects[0].transform.translation = [2.0, 1.0, 0.0];
            scene.objects[0].transform.rotation = [0.0, 0.7, 0.0];
            scene.objects[1].transform.translation = [2.0, 1.0, -2.0];
            let ctx = egui::Context::default();
            let viewport_enabled = std::cell::Cell::new(true);
            let frame = |m: &mut Manipulation,
                         session: &mut Session,
                         events: Vec<Event>,
                         focused| {
                let mut outcome = Ok(false);
                let mut viewport_clicked = false;
                let mut output = ctx.run_ui(
                    RawInput {
                        focused,
                        screen_rect: Some(Rect::from_min_max(Pos2::ZERO, Pos2::new(1200.0, 800.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        if m.active() {
                            let before = serde_json::to_value(m.snapshot_drafts()).unwrap();
                            let generation = m.draft_generation();
                            let mut inspector = ui.new_child(egui::UiBuilder::new().max_rect(
                                Rect::from_min_max(Pos2::new(990.0, 0.0), Pos2::new(1200.0, 800.0)),
                            ));
                            assert!(!m.precision_ui(&mut inspector, session, true).unwrap());
                            assert_eq!(
                                serde_json::to_value(m.snapshot_drafts()).unwrap(),
                                before,
                                "live inspector must not mutate recovery drafts"
                            );
                            assert_eq!(m.draft_generation(), generation);
                        }
                        let response = ui
                            .add_enabled_ui(viewport_enabled.get(), |ui| {
                                ui.interact(
                                    rect,
                                    ui.scope_id().with("viewport"),
                                    egui::Sense::click_and_drag(),
                                )
                            })
                            .inner;
                        viewport_clicked = response.clicked();
                        outcome = m.draw(
                            ui,
                            &ruster_render::ViewportInteraction {
                                rect,
                                response,
                                picked: None,
                                view: view.to_cols_array_2d().map(|row| row.map(|v| v as f32)),
                                projection: projection
                                    .to_cols_array_2d()
                                    .map(|row| row.map(|v| v as f32)),
                                gizmo_projection: projection
                                    .to_cols_array_2d()
                                    .map(|row| row.map(|v| v as f32)),
                            },
                            session,
                        );
                    },
                );
                output.textures_delta.clear();
                assert!(
                    !output.shapes.is_empty(),
                    "selected gizmo remains visible even when input is blocked"
                );
                (viewport_clicked, outcome.unwrap())
            };
            let mut session = Session::new(scene.clone()).unwrap();
            let mut m = Manipulation {
                local: true,
                ids: scene.objects.iter().map(|o| o.id.clone()).collect(),
                ..Default::default()
            };
            let pointer = |pos, pressed| Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            };
            // A selected gizmo must not create an invisible cursor widget that
            // steals ordinary object/empty-space clicks from the viewport.
            let off_handle = rect.min + egui::vec2(24.0, 24.0);
            for _ in 0..2 {
                frame(
                    &mut m,
                    &mut session,
                    vec![Event::PointerMoved(off_handle)],
                    true,
                );
                frame(
                    &mut m,
                    &mut session,
                    vec![Event::PointerMoved(off_handle)],
                    true,
                );
                frame(&mut m, &mut session, vec![pointer(off_handle, true)], true);
                let (clicked, owned) =
                    frame(&mut m, &mut session, vec![pointer(off_handle, false)], true);
                assert!(
                    clicked,
                    "off-handle viewport click must reach object selection"
                );
                assert!(!owned, "only a real handle may own a viewport click");
                assert!(!m.active());
                assert_eq!(session.scene(), &scene);
                assert_eq!(session.undo_len(), 0);
            }
            m.reset_precision(session.scene());
            // A zero-motion handle press owns release without touching history.
            frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
            frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
            let (_, owned) = frame(&mut m, &mut session, vec![pointer(start, true)], true);
            assert!(owned && m.active());
            let (_, owned) = frame(&mut m, &mut session, vec![pointer(start, false)], true);
            assert!(owned && !m.active());
            assert_eq!(session.undo_len(), 0);
            assert!(m.preview_precision().is_none());
            // Fast input may coalesce press and release into one egui frame.
            let (_, owned) = frame(
                &mut m,
                &mut session,
                vec![pointer(start, true), pointer(start, false)],
                true,
            );
            assert!(
                owned && !m.active(),
                "coalesced handle click must consume release"
            );
            assert_eq!(session.undo_len(), 0);
            frame(
                &mut m,
                &mut session,
                vec![Event::PointerMoved(off_handle)],
                true,
            );
            frame(
                &mut m,
                &mut session,
                vec![Event::PointerMoved(off_handle)],
                true,
            );
            let (clicked, owned) = frame(
                &mut m,
                &mut session,
                vec![pointer(off_handle, true), pointer(off_handle, false)],
                true,
            );
            assert!(
                clicked && !owned,
                "coalesced off-handle click must still select"
            );
            viewport_enabled.set(false);
            frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
            frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
            frame(&mut m, &mut session, vec![pointer(start, true)], true);
            assert!(
                !m.active(),
                "disabled viewport cannot authorize a handle press"
            );
            frame(&mut m, &mut session, vec![pointer(start, false)], true);
            viewport_enabled.set(true);
            frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
            frame(&mut m, &mut session, vec![pointer(start, true)], true);
            assert!(m.active());
            frame(&mut m, &mut session, vec![Event::PointerMoved(end)], true);
            let (display, mixed) = m.preview_precision().unwrap();
            let candidate = &m.gesture.as_ref().unwrap().candidate[0].1;
            assert_eq!(display[0], candidate.translation[0].to_string());
            assert_ne!(
                display[0], m.precision[0],
                "existing inspector fields must show candidate, not authored baseline"
            );
            assert!(
                mixed[2] && display[2].is_empty(),
                "mixed selection stays mixed during drag"
            );
            assert_eq!(session.scene(), &scene);
            assert_eq!(session.undo_len(), 0);
            frame(&mut m, &mut session, vec![pointer(end, false)], true);
            assert!(!m.active());
            assert!(m.preview_precision().is_none());
            assert_eq!(
                session.scene().objects[0].transform.translation[0].to_string(),
                display[0]
            );
            assert_eq!(session.undo_len(), 1);
            let delta = session.scene().objects[0].transform.translation[0]
                - scene.objects[0].transform.translation[0];
            assert!((delta - (scale * 25.0) as f32).abs() < 1e-3);
            assert!(
                (session.scene().objects[1].transform.translation[0]
                    - scene.objects[1].transform.translation[0]
                    - delta)
                    .abs()
                    < 1e-5
            );
            session.undo(1).unwrap();
            for cancel in [false, true] {
                frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
                frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
                frame(&mut m, &mut session, vec![pointer(start, true)], true);
                assert!(m.active());
                frame(&mut m, &mut session, vec![Event::PointerMoved(end)], true);
                let events = if cancel {
                    vec![Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }]
                } else {
                    vec![]
                };
                frame(&mut m, &mut session, events, cancel);
                assert!(!m.active());
                assert!(
                    m.preview_precision().is_none(),
                    "cancel returns inspector to authored values"
                );
                assert_eq!(session.scene().objects, scene.objects);
                assert_eq!(session.undo_len(), 0);
                frame(&mut m, &mut session, vec![pointer(end, false)], true);
            }
            let snap_end = project(median + DVec3::X * scale * 120.0);
            m.snap = true;
            m.snap_distance = 0.5;
            frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
            frame(&mut m, &mut session, vec![Event::PointerMoved(start)], true);
            frame(&mut m, &mut session, vec![pointer(start, true)], true);
            frame(
                &mut m,
                &mut session,
                vec![Event::PointerMoved(snap_end)],
                true,
            );
            frame(&mut m, &mut session, vec![pointer(snap_end, false)], true);
            let snapped = session.scene().objects[0].transform.translation[0]
                - scene.objects[0].transform.translation[0];
            assert!(snapped.abs() > 0.1);
            assert!((snapped / 0.5 - (snapped / 0.5).round()).abs() < 1e-4);
            session.undo(session.scene().revision).unwrap();
            for tool in [Tool::Rotate, Tool::Scale] {
                m.tool = tool;
                m.local = false;
                m.pivot = Pivot::Center;
                let center = project(median);
                let ring_start = center + egui::vec2(82.0, 0.0);
                let ring_end = if tool == Tool::Rotate {
                    center + egui::vec2(0.0, 82.0)
                } else {
                    center + egui::vec2(123.0, 0.0)
                };
                frame(
                    &mut m,
                    &mut session,
                    vec![Event::PointerMoved(ring_start)],
                    true,
                );
                frame(
                    &mut m,
                    &mut session,
                    vec![Event::PointerMoved(ring_start)],
                    true,
                );
                frame(&mut m, &mut session, vec![pointer(ring_start, true)], true);
                assert!(
                    m.active(),
                    "{tool:?} ring must start a real toolkit gesture"
                );
                frame(
                    &mut m,
                    &mut session,
                    vec![Event::PointerMoved(ring_end)],
                    true,
                );
                assert_eq!(
                    session.scene().objects,
                    scene.objects,
                    "tool preview must not mutate authored objects"
                );
                frame(&mut m, &mut session, vec![pointer(ring_end, false)], true);
                assert_eq!(session.undo_len(), 1);
                let actual = &session.scene().objects[0].transform;
                if tool == Tool::Rotate {
                    assert_ne!(actual.rotation, scene.objects[0].transform.rotation);
                    assert_eq!(actual.scale, scene.objects[0].transform.scale);
                } else {
                    assert_eq!(actual.rotation, scene.objects[0].transform.rotation);
                    assert!((actual.scale[0] - 1.5).abs() < 1e-4);
                }
                session.undo(session.scene().revision).unwrap();
            }
            // A stale revision cannot apply a captured candidate over another edit.
            let mut gesture = Gesture::begin(&session, &m.ids).unwrap();
            gesture.candidate[0].1.translation[0] += 1.0;
            session
                .rename(session.scene().revision, "left", Some("Changed".into()))
                .unwrap();
            assert!(gesture.commit(&mut session).is_err());
        }
    }
}
