//! One editor-local authored snapshot and a shared guarded paste transaction.
use super::ComponentWorkspace;
use eframe::egui;
use rot_scene::{
    component_scene::{CameraView, Component},
    Transform,
};
use std::collections::{HashMap, HashSet};

type PasteError = (&'static str, String);

#[derive(Clone, Copy)]
pub(super) struct Channels(u8);

impl Channels {
    const POSITION: Self = Self(1);
    const ROTATION: Self = Self(2);
    const SCALE: Self = Self(4);
    const ALL: Self = Self(7);

    pub(super) fn parse(value: &serde_json::Value) -> Option<Self> {
        let mut bits = 0;
        for channel in value.as_array()? {
            let channel = match channel.as_str()? {
                "position" => Self::POSITION,
                "rotation" => Self::ROTATION,
                "scale" => Self::SCALE,
                _ => return None,
            };
            if bits & channel.0 != 0 {
                return None;
            }
            bits |= channel.0;
        }
        (bits != 0).then_some(Self(bits))
    }

    fn merge(self, target: &mut Transform, source: &Transform) {
        if self.0 & Self::POSITION.0 != 0 {
            target.translation = source.translation;
        }
        if self.0 & Self::ROTATION.0 != 0 {
            target.rotation = source.rotation;
        }
        if self.0 & Self::SCALE.0 != 0 {
            target.scale = source.scale;
        }
    }
}

impl ComponentWorkspace {
    pub(super) fn local_transform_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Local Transform", |ui| {
            let editable = !self.runtime.in_play_mode()
                && !self.authored_edit_pending()
                && !self.ai_transition_pending();
            ui.label("Values are relative to each object's parent.");
            if ui
                .add_enabled(
                    editable && self.selected_index.is_some(),
                    egui::Button::new("Copy Local Transform"),
                )
                .clicked()
            {
                if let Some(index) = self.selected_index {
                    self.local_transform_copy =
                        Some(self.session.scene().objects[index].transform.clone());
                    self.error = None;
                }
                ui.close();
            }
            let paste_enabled =
                editable && self.local_transform_copy.is_some() && !self.selection.is_empty();
            for (label, channel) in [
                ("Paste Position", Channels::POSITION),
                ("Paste Rotation", Channels::ROTATION),
                ("Paste Scale", Channels::SCALE),
                ("Paste All", Channels::ALL),
            ] {
                if ui
                    .add_enabled(paste_enabled, egui::Button::new(label))
                    .clicked()
                {
                    let source = self
                        .local_transform_copy
                        .clone()
                        .expect("paste requires a snapshot");
                    let ids = self.target_ids(None);
                    let ids = ids.iter().map(String::as_str).collect::<Vec<_>>();
                    self.error = self
                        .paste_local_transform(self.session.revision(), &source, &ids, channel)
                        .err()
                        .map(|(_, message)| message);
                    ui.close();
                }
            }
        });
    }

    pub(super) fn paste_local_transform(
        &mut self,
        revision: u64,
        source: &Transform,
        ids: &[&str],
        channels: Channels,
    ) -> Result<usize, PasteError> {
        if revision != self.session.revision() {
            return Err(("stale_revision", "Inspect the current scene again.".into()));
        }
        if self.runtime.in_play_mode() {
            return Err((
                "play_active",
                "Stop Play before pasting a local Transform.".into(),
            ));
        }
        if self.authored_edit_pending() || self.ai_transition_pending() {
            return Err((
                "drafts_pending",
                "Apply or cancel the pending edit before pasting.".into(),
            ));
        }
        if ids.is_empty() {
            return Err((
                "invalid_arguments",
                "Select objects to receive the local Transform.".into(),
            ));
        }
        let objects = &self.session.scene().objects;
        let indices: HashMap<_, _> = objects
            .iter()
            .enumerate()
            .map(|(index, object)| (object.id.as_str(), index))
            .collect();
        let mut seen = HashSet::with_capacity(ids.len());
        let mut edits = Vec::with_capacity(ids.len());
        let mut changes = HashMap::with_capacity(ids.len());
        for &id in ids {
            let &index = indices
                .get(id)
                .ok_or_else(|| ("object_not_found", format!("Object {id} no longer exists.")))?;
            if !seen.insert(index) {
                return Err(("invalid_arguments", format!("Object {id} appears twice.")));
            }
            if self.view_flags.get(index).is_some_and(|flags| flags.locked) {
                return Err((
                    "objects_locked",
                    format!("Unlock {} before pasting.", objects[index].name),
                ));
            }
            let object = &objects[index];
            let mut transform = object.transform.clone();
            channels.merge(&mut transform, source);
            if transform != object.transform {
                let mut next = object.clone();
                next.transform = transform;
                changes.insert(index, next.transform.affine_matrix());
                edits.push(next);
            }
        }
        if edits.is_empty() {
            return Ok(0);
        }
        self.validate_local_transform_paste(&changes)?;
        let count = edits.len();
        self.session
            .put_objects(revision, edits)
            .map_err(|error| ("invalid_transform", error.to_string()))?;
        self.refresh();
        Ok(count)
    }

    fn validate_local_transform_paste(
        &self,
        changes: &HashMap<usize, glam::DMat4>,
    ) -> Result<(), PasteError> {
        let scene = self.session.scene();
        let preview = self
            .preview
            .as_ref()
            .map_err(|error| ("world_transform_unrepresentable", error.to_string()))?;
        let mut staged = HashMap::with_capacity(changes.len());
        for &(index, _) in &self.rows {
            let parent = self.parents[index];
            let staged_parent = parent.and_then(|parent| staged.get(&parent).copied());
            let changed = changes.get(&index).copied();
            if changed.is_none() && staged_parent.is_none() {
                continue;
            }
            let object = &scene.objects[index];
            let local = changed.unwrap_or_else(|| object.transform.affine_matrix());
            let world = staged_parent
                .or_else(|| parent.map(|parent| self.preview_world[parent]))
                .map_or(local, |parent| parent * local);
            let unrepresentable = || {
                (
                    "world_transform_unrepresentable",
                    format!(
                        "Pasting would put {} beyond the world transform limits.",
                        object.name
                    ),
                )
            };
            if !world.is_finite() {
                return Err(unrepresentable());
            }
            if self.view_flags.get(index).is_some_and(|flags| flags.locked) {
                return Err((
                    "objects_locked",
                    format!(
                        "Unlock {} before pasting its ancestor's local Transform.",
                        object.name
                    ),
                ));
            }
            staged.insert(index, world);

            for component in object
                .components
                .iter()
                .filter(|component| component.enabled)
            {
                match &component.kind {
                    Component::MeshRenderer { .. } => {
                        let mut current = Some(index);
                        let mut enabled = true;
                        while let Some(node) = current {
                            enabled &= scene.objects[node].enabled;
                            current = self.parents[node];
                        }
                        if enabled && !self.view_flags.get(index).is_some_and(|flags| flags.hidden)
                        {
                            rot_render::component_preview::validate_draw_world(world, &object.id)
                                .map_err(|error| {
                                ("world_transform_unrepresentable", error.to_string())
                            })?;
                        }
                    }
                    Component::Light { .. } => {
                        preview
                            .validate_light_world(&component.id, world)
                            .map_err(|error| {
                                ("world_transform_unrepresentable", error.to_string())
                            })?;
                    }
                    Component::Camera {
                        view: CameraView::Transform(_),
                    } if scene.gameplay_camera.as_ref() == Some(&component.id)
                        && preview.transform_camera().is_some()
                        && !world.w_axis.truncate().as_vec3().is_finite() =>
                    {
                        return Err(unrepresentable());
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }
}
