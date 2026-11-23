//! Scene-only sampling of one exact authored Animator and accepted clip.
use super::ComponentWorkspace;
use crate::{strings::Text, ui_kit};
use eframe::egui;
use glam::{DMat4, DQuat, EulerRot};
use rot_render::component_preview::ComponentPreview;
use rot_scene::{component_scene::Component, Clip, LightKind, Transform};
use std::collections::{HashMap, HashSet};

type Error = (&'static str, String);

pub(super) struct AnimationPreview {
    revision: u64,
    asset_revision: u64,
    pub(super) owner_id: String,
    pub(super) component_id: String,
    clip_id: String,
    #[cfg_attr(not(unix), allow(dead_code))]
    model_hash: String,
    clip: Clip,
    channels: Vec<(usize, usize)>,
    has_spots: bool,
    nodes: Vec<Node>,
    pub(super) projection: ComponentPreview,
    pub(super) seconds: f32,
}

struct Node {
    parent: Option<usize>,
    parent_world: DMat4,
    parent_rotation: DQuat,
    authored: Transform,
    local: Transform,
    world: DMat4,
    rotation: DQuat,
    draws: Vec<usize>,
    lights: Vec<(String, bool)>,
}

fn euler(transform: &Transform) -> DQuat {
    let [x, y, z] = transform.rotation.map(f64::from);
    DQuat::from_euler(EulerRot::ZYX, z, y, x)
}

impl AnimationPreview {
    fn prepare(
        editor: &ComponentWorkspace,
        owner_id: &str,
        component_id: &str,
    ) -> Result<Self, Error> {
        let scene = editor.session.scene();
        let (owner_index, owner) = scene
            .objects
            .iter()
            .enumerate()
            .find(|(_, object)| object.id == owner_id)
            .ok_or((
                "object_not_found",
                "Choose an existing Animator owner.".into(),
            ))?;
        let component = owner
            .components
            .iter()
            .find(|component| component.id == component_id)
            .ok_or((
                "component_not_found",
                "Choose an exact Animator component.".into(),
            ))?;
        let Component::Animator { binding, .. } = &component.kind else {
            return Err(("invalid_arguments", "Choose an Animator component.".into()));
        };
        if !component.enabled || !owner.enabled {
            return Err((
                "animation_disabled",
                "Enable the Animator and its owner to preview.".into(),
            ));
        }

        let mut ancestor = editor.parents[owner_index];
        while let Some(index) = ancestor {
            if !scene.objects[index].enabled {
                return Err((
                    "animation_disabled",
                    "Enable the Animator's ancestors to preview.".into(),
                ));
            }
            ancestor = editor.parents[index];
        }

        let store = editor
            .assets
            .as_ref()
            .ok_or(("assets_loading", "Wait for project assets.".into()))?;
        let (model, _) = store.clip_owner(&binding.clip).ok_or((
            "clip_unavailable",
            Text::AnimatorClipUnavailable.plain().into(),
        ))?;
        let bytes = store.clip(&binding.clip).ok_or((
            "clip_unavailable",
            Text::AnimatorClipUnavailable.plain().into(),
        ))?;
        let clip = Clip::decode(bytes).map_err(|error| ("clip_unavailable", error.to_string()))?;
        let base = editor
            .preview
            .as_ref()
            .map_err(|error| ("preview_unavailable", error.to_string()))?;

        let tracked = clip
            .tracks()
            .iter()
            .map(|track| {
                binding
                    .targets
                    .get(&track.node)
                    .map(String::as_str)
                    .ok_or(("invalid_clip", Text::AnimatorMissingTargets.plain().into()))
            })
            .collect::<Result<HashSet<_>, _>>()?;
        let mut nodes = Vec::<Node>::new();
        let mut indexed = HashMap::new();
        for &(index, _) in &editor.rows {
            let object = &scene.objects[index];
            let parent_index = editor.parents[index];
            let parent = parent_index
                .and_then(|parent| indexed.get(scene.objects[parent].id.as_str()).copied());
            if parent.is_none() && !tracked.contains(object.id.as_str()) {
                continue;
            }
            indexed.insert(object.id.as_str(), nodes.len());
            let parent_world = if parent.is_none() {
                parent_index.map_or(DMat4::IDENTITY, |index| editor.preview_world[index])
            } else {
                DMat4::IDENTITY
            };
            let lights = object
                .components
                .iter()
                .filter_map(|component| {
                    let Component::Light { kind, .. } = &component.kind else {
                        return None;
                    };
                    base.scene()
                        .lighting
                        .light(&component.id)
                        .map(|_| (component.id.clone(), matches!(kind, LightKind::Spot { .. })))
                })
                .collect();
            nodes.push(Node {
                parent,
                parent_world,
                parent_rotation: DQuat::IDENTITY,
                authored: object.transform.clone(),
                local: object.transform.clone(),
                world: DMat4::IDENTITY,
                rotation: DQuat::IDENTITY,
                draws: Vec::new(),
                lights,
            });
        }
        for (row, draw) in base.scene().objects.iter().enumerate() {
            if let Some(&index) = indexed.get(draw.id.as_str()) {
                nodes[index].draws.push(row);
            }
        }
        let has_spots = nodes
            .iter()
            .any(|node| node.lights.iter().any(|(_, spot)| *spot));
        if has_spots {
            for &(index, _) in &editor.rows {
                if let Some(&node) = indexed.get(scene.objects[index].id.as_str()) {
                    if nodes[node].parent.is_none() {
                        nodes[node].parent_rotation = editor.parents[index]
                            .map_or(DQuat::IDENTITY, |parent| {
                                scene.world_rotation(parent, &editor.parents)
                            });
                    }
                }
            }
        }
        let channels = clip
            .tracks()
            .iter()
            .enumerate()
            .map(|(channel, track)| {
                indexed
                    .get(binding.targets[&track.node].as_str())
                    .copied()
                    .map(|node| (channel, node))
                    .ok_or(("invalid_clip", Text::AnimatorMissingTargets.plain().into()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            revision: editor.session.revision(),
            asset_revision: store.revision(),
            owner_id: owner_id.into(),
            component_id: component_id.into(),
            clip_id: binding.clip.clone(),
            model_hash: model.hash.clone(),
            clip,
            channels,
            has_spots,
            nodes,
            projection: base.clone(),
            seconds: f32::NAN,
        })
    }

    fn matches(&self, editor: &ComponentWorkspace) -> bool {
        self.revision == editor.session.revision()
            && editor
                .assets
                .as_ref()
                .is_some_and(|store| store.revision() == self.asset_revision)
    }

    fn seek(&mut self, seconds: f64) -> Result<bool, Error> {
        if !(0.0..=f64::from(self.clip.duration())).contains(&seconds) {
            return Err((
                "invalid_arguments",
                "Enter a time from zero through the clip duration.".into(),
            ));
        }
        let seconds = seconds as f32;
        if self.seconds == seconds {
            return Ok(false);
        }
        for node in &mut self.nodes {
            node.local.clone_from(&node.authored);
        }
        for &(channel, node) in &self.channels {
            self.clip
                .sample_channel(channel, seconds, &mut self.nodes[node].local)
                .map_err(|error| ("invalid_clip", error.to_string()))?;
        }
        for index in 0..self.nodes.len() {
            let node = &self.nodes[index];
            let (parent_world, parent_rotation) =
                node.parent
                    .map_or((node.parent_world, node.parent_rotation), |parent| {
                        let parent = &self.nodes[parent];
                        (parent.world, parent.rotation)
                    });
            let world = parent_world * node.local.affine_matrix();
            if !world.is_finite() {
                return Err((
                    "preview_unavailable",
                    "The sampled world transform exceeds renderer precision.".into(),
                ));
            }
            let rotation = if self.has_spots {
                (parent_rotation * euler(&node.local)).normalize()
            } else {
                DQuat::IDENTITY
            };
            let node = &mut self.nodes[index];
            node.world = world;
            node.rotation = rotation;
        }
        let draws = self
            .nodes
            .iter()
            .flat_map(|node| node.draws.iter().map(|&row| (row, node.world)));
        let lights = self.nodes.iter().flat_map(|node| {
            node.lights
                .iter()
                .map(|(id, spot)| (id.as_str(), node.world, spot.then_some(node.rotation)))
        });
        self.projection
            .set_sampled_rows(draws, lights)
            .map_err(|error| ("preview_unavailable", error.to_string()))?;
        self.seconds = seconds;
        Ok(true)
    }

    pub(super) fn controls(&self) -> (&str, f32, f32) {
        (
            self.component_id.as_str(),
            self.seconds,
            self.clip.duration(),
        )
    }

    #[cfg(unix)]
    pub(super) fn observation(&self) -> serde_json::Value {
        serde_json::json!({
            "owner_id":self.owner_id, "component_id":self.component_id,
            "clip_id":self.clip_id, "model_hash":self.model_hash,
            "asset_revision":self.asset_revision.to_string(),
            "seconds":self.seconds, "duration":self.clip.duration(),
        })
    }
}

pub(in crate::scene_editor) enum PreviewEdit {
    Start(String),
    Seek(String, f64),
    Clear,
}

/// Scene preview controls are independent of the component draft.
pub(in crate::scene_editor) fn controls(
    ui: &mut egui::Ui,
    component_id: &str,
    enabled: bool,
    active: Option<(f32, f32)>,
) -> Option<PreviewEdit> {
    let Some((current, duration)) = active else {
        return ui
            .add_enabled(enabled, egui::Button::new(Text::AnimationPreview.plain()))
            .clicked()
            .then(|| PreviewEdit::Start(component_id.into()));
    };
    let mut seconds = f64::from(current);
    let range = 0.0..=f64::from(duration);
    let sliding =
        ui_kit::property_row_with_label(ui, Text::AnimationPreviewTime.plain(), |ui, label| {
            ui.spacing_mut().slider_width = ui.available_width().max(28.0);
            ui.add_enabled(
                enabled,
                egui::Slider::new(&mut seconds, range.clone())
                    .smart_aim(false)
                    .show_value(false),
            )
            .labelled_by(label)
            .changed()
        });
    let typing =
        ui_kit::property_row_with_label(ui, Text::AnimationPreviewSeconds.plain(), |ui, label| {
            ui.add_enabled(
                enabled,
                egui::DragValue::new(&mut seconds)
                    .range(range)
                    .speed(0.01)
                    .max_decimals(6),
            )
            .labelled_by(label)
            .changed()
        });
    if ui.button(Text::AnimationPreviewExit.plain()).clicked() {
        Some(PreviewEdit::Clear)
    } else if sliding || typing {
        Some(PreviewEdit::Seek(component_id.into(), seconds))
    } else {
        None
    }
}

impl ComponentWorkspace {
    pub(super) fn current_animation_preview(&self) -> Option<&AnimationPreview> {
        self.animation_preview
            .as_ref()
            .filter(|preview| preview.matches(self))
    }

    pub(super) fn clear_animation_preview(&mut self) -> bool {
        if self.animation_preview.take().is_none() {
            return false;
        }
        #[cfg(unix)]
        self.invalidate_display_capture();
        true
    }

    pub(super) fn validate_animation_preview(&mut self) {
        if self.current_animation_preview().is_none() || self.runtime.in_play_mode() {
            self.clear_animation_preview();
        }
    }

    pub(super) fn seek_animation_preview(
        &mut self,
        owner: &str,
        component: &str,
        seconds: f64,
    ) -> Result<bool, Error> {
        if self.runtime.in_play_mode() {
            return Err((
                "play_active",
                "Stop Play before previewing an Animator.".into(),
            ));
        }
        if self.authored_edit_pending() || self.ai_transition_pending() {
            return Err((
                "drafts_pending",
                "Finish the pending edit before previewing an Animator.".into(),
            ));
        }
        let changed = if self
            .current_animation_preview()
            .is_some_and(|preview| preview.owner_id == owner && preview.component_id == component)
        {
            self.animation_preview
                .as_mut()
                .expect("current preview")
                .seek(seconds)?
        } else {
            let mut preview = AnimationPreview::prepare(self, owner, component)?;
            preview.seek(seconds)?;
            self.animation_preview = Some(preview);
            self.collider_tool.exit();
            true
        };
        if changed {
            #[cfg(unix)]
            self.invalidate_display_capture();
        }
        Ok(changed)
    }

    pub(super) fn apply_animation_preview_ui(&mut self, owner: &str, edit: PreviewEdit) {
        let (component, seconds) = match edit {
            PreviewEdit::Start(component) => (component, 0.0),
            PreviewEdit::Seek(component, seconds) => (component, seconds),
            PreviewEdit::Clear => {
                self.clear_animation_preview();
                return;
            }
        };
        self.error = self
            .seek_animation_preview(owner, &component, seconds)
            .err()
            .map(|(_, error)| error);
    }
}
