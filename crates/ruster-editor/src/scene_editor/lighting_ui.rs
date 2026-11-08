//! Recoverable light/ambient text with one shared authored transaction.
use super::SceneEditor;
use eframe::egui;
use ruster_scene::{Ambient, Light, LightKind, Lighting, Scene, ShadowQuality, SunShadow};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ShadowDraft {
    pub(super) enabled: bool,
    pub(super) quality: ShadowQuality,
    pub(super) depth_bias_texels: String,
    pub(super) normal_bias_texels: String,
}

impl From<SunShadow> for ShadowDraft {
    fn from(shadow: SunShadow) -> Self {
        Self {
            enabled: shadow.enabled,
            quality: shadow.quality,
            depth_bias_texels: shadow.depth_bias_texels.to_string(),
            normal_bias_texels: shadow.normal_bias_texels.to_string(),
        }
    }
}

impl ShadowDraft {
    fn value(&self) -> Result<SunShadow, String> {
        Ok(SunShadow {
            enabled: self.enabled,
            quality: self.quality,
            depth_bias_texels: scalar(&self.depth_bias_texels, 0.0, 4.0, "Shadow depth bias")?,
            normal_bias_texels: scalar(&self.normal_bias_texels, 0.0, 4.0, "Shadow normal offset")?,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum LightingDraft {
    Light {
        id: String,
        revision: u64,
        baseline: Light,
        enabled: bool,
        color: [String; 3],
        intensity: String,
        range: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        shadow: Option<ShadowDraft>,
    },
    Ambient {
        revision: u64,
        baseline: Lighting,
        color: [String; 3],
        intensity: String,
    },
}

fn bounded(text: &str) -> bool {
    text.len() <= 256 && text.chars().count() <= 64
}
fn scalar(text: &str, min: f64, max: f64, label: &str) -> Result<f32, String> {
    let number = text
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|number| bounded(text) && number.is_finite() && (min..=max).contains(number));
    number
        .map(|number| number as f32)
        .ok_or_else(|| format!("{label} requires a finite number from {min} to {max}."))
}
fn rgb(text: &[String; 3]) -> Result<[f32; 3], String> {
    Ok([
        scalar(&text[0], 0.0, 1.0, "Red")?,
        scalar(&text[1], 0.0, 1.0, "Green")?,
        scalar(&text[2], 0.0, 1.0, "Blue")?,
    ])
}

impl LightingDraft {
    pub(super) fn light(scene: &Scene, id: &str) -> Option<Self> {
        let baseline = scene.lighting.light(id)?.clone();
        Some(Self::Light {
            id: id.into(),
            revision: scene.revision,
            enabled: baseline.enabled,
            color: baseline.color.map(|value| value.to_string()),
            intensity: baseline.intensity.to_string(),
            range: match baseline.kind {
                LightKind::Directional { .. } => None,
                LightKind::Point { range } => Some(range.to_string()),
            },
            shadow: match baseline.kind {
                LightKind::Directional { shadow } => Some(shadow.into()),
                LightKind::Point { .. } => None,
            },
            baseline,
        })
    }
    pub(super) fn ambient(scene: &Scene) -> Self {
        let value = match &scene.lighting {
            Lighting::Legacy => Ambient::default(),
            Lighting::Authored { ambient, .. } => ambient.clone(),
        };
        Self::Ambient {
            revision: scene.revision,
            baseline: scene.lighting.clone(),
            color: value.color.map(|value| value.to_string()),
            intensity: value.intensity.to_string(),
        }
    }
    pub(super) fn target_id(&self) -> Option<&str> {
        match self {
            Self::Light { id, .. } => Some(id),
            Self::Ambient { .. } => None,
        }
    }
    pub(super) fn is_ambient(&self) -> bool {
        matches!(self, Self::Ambient { .. })
    }
    /// Recovery owns the raw-version guard. Call only for a pre6 draft whose
    /// format never carried shadow fields; a current missing field stays invalid.
    pub(super) fn upgrade_legacy_shadow(&mut self) {
        if let Self::Light {
            baseline, shadow, ..
        } = self
        {
            if shadow.is_none() {
                if let LightKind::Directional { shadow: baseline } = baseline.kind {
                    *shadow = Some(baseline.into());
                }
            }
        }
    }
    pub(super) fn storage_valid(&self) -> bool {
        let (color, intensity) = match self {
            Self::Light {
                id,
                baseline,
                color,
                intensity,
                range,
                shadow,
                ..
            } => {
                if ruster_scene::validate_id(id).is_err()
                    || baseline.validate().is_err()
                    || !matches!(
                        (&baseline.kind, range, shadow),
                        (LightKind::Directional { .. }, None, Some(_))
                            | (LightKind::Point { .. }, Some(_), None)
                    )
                    || range.as_ref().is_some_and(|text| !bounded(text))
                    || shadow.as_ref().is_some_and(|shadow| {
                        !bounded(&shadow.depth_bias_texels) || !bounded(&shadow.normal_bias_texels)
                    })
                {
                    return false;
                }
                (color, intensity)
            }
            Self::Ambient {
                baseline,
                color,
                intensity,
                ..
            } => {
                if baseline.validate().is_err() {
                    return false;
                }
                (color, intensity)
            }
        };
        color.iter().all(|text| bounded(text)) && bounded(intensity)
    }
    pub(super) fn validate_snapshot(&self, scene: &Scene, selected: Option<&str>) -> bool {
        self.storage_valid()
            && match self {
                Self::Light {
                    id,
                    revision,
                    baseline,
                    ..
                } => {
                    *revision == scene.revision
                        && selected == Some(id.as_str())
                        && scene.lighting.light(id) == Some(baseline)
                }
                Self::Ambient {
                    revision, baseline, ..
                } => *revision == scene.revision && &scene.lighting == baseline,
            }
    }
    fn light_value(&self) -> Result<Light, String> {
        let Self::Light {
            baseline,
            enabled,
            color,
            intensity,
            range,
            shadow,
            ..
        } = self
        else {
            return Err("This is an ambient draft.".into());
        };
        let mut value = baseline.clone();
        value.enabled = *enabled;
        value.color = rgb(color)?;
        value.intensity = scalar(intensity, 0.0, 16.0, "Light intensity")?;
        match &mut value.kind {
            LightKind::Directional { shadow: target } => {
                if range.is_some() {
                    return Err("Directional lights cannot have point range.".into());
                }
                *target = shadow
                    .as_ref()
                    .ok_or("Sun shadow settings are missing.")?
                    .value()?;
            }
            LightKind::Point { range: target } => {
                if shadow.is_some() {
                    return Err("Point lights cannot have Sun shadow settings.".into());
                }
                *target = scalar(
                    range.as_deref().ok_or("Point range is missing.")?,
                    0.1,
                    10_000.0,
                    "Point range",
                )?;
            }
        }
        value.validate().map_err(|error| error.to_string())?;
        Ok(value)
    }
    fn ambient_value(&self) -> Result<Ambient, String> {
        let Self::Ambient {
            color, intensity, ..
        } = self
        else {
            return Err("This is a light draft.".into());
        };
        Ok(Ambient {
            color: rgb(color)?,
            intensity: scalar(intensity, 0.0, 1.0, "Ambient intensity")?,
        })
    }
}

impl SceneEditor {
    fn lighting_editable(&self, saving: bool) -> bool {
        !self.asset_publishing()
            && !self.asset_loading()
            && self.color_draft.is_none()
            && self.material_draft.is_none()
            && self.display_draft.is_none()
            && self.collider_draft.is_none()
            && !self.manipulation.active()
            && !self.manipulation.precision_pending()
            && !self.manipulation.offset_pending()
            && self.draft == self.baseline_fields
            && !self.behavior.pending()
            && !self.input_actions.pending()
            && (self.recovered_input.is_none() || self.lighting_draft.is_some())
            && self.dialog.is_none()
            && !self.export_dialog
            && !self.asset_dialog
            && !self.rename_pending
            && (saving || !self.new_scene_pending)
            && (saving || !self.close_pending)
            && !self.close_approved
            && !self.workspace.settings_open
            && !self.recovery.open
            && self.recovery_transition.is_none()
    }
    pub(super) fn apply_lighting_draft(&mut self) -> bool {
        #[cfg(feature = "inspection")]
        let observed = self
            .lighting_draft
            .as_ref()
            .map(|draft| {
                if draft.is_ambient() {
                    "scene_ambient_set"
                } else {
                    "scene_light_set"
                }
            })
            .map(|op| {
                (
                    self.asset_ops.timing_immediate_begin(op, "human"),
                    self.session.scene().revision,
                )
            });
        let Some(draft) = self.lighting_draft.clone() else {
            return true;
        };
        let result = (|| {
            if !self.lighting_editable(true) {
                return Err(
                    "Finish other edits and resolve dialogs before applying lighting.".into(),
                );
            }
            if !draft.validate_snapshot(self.session.scene(), self.selected.as_deref()) {
                return Err("Lighting target changed; cancel this draft and inspect again.".into());
            }
            match &draft {
                LightingDraft::Light { id, revision, .. } => {
                    self.session
                        .set_light(*revision, id, draft.light_value()?)
                        .map_err(|error| error.to_string())?;
                    Ok(Vec::new())
                }
                LightingDraft::Ambient { revision, .. } => self
                    .session
                    .set_ambient(*revision, draft.ambient_value()?)
                    .map(|edit| edit.created_ids)
                    .map_err(|error| error.to_string()),
            }
        })();
        let success = match result {
            Ok(created) => {
                self.lighting_draft = None;
                self.draft_generation = self.draft_generation.wrapping_add(1);
                self.sync_draft();
                self.error.clear();
                self.notice = if self.runtime.visible {
                    "Lighting updated. Reset preview to use the changes."
                } else {
                    "Lighting updated."
                }
                .into();
                if !created.is_empty() {
                    self.notice
                        .push_str(" Added Sun to preserve the previous illumination.");
                }
                true
            }
            Err(error) => {
                self.error = error;
                false
            }
        };
        #[cfg(feature = "inspection")]
        if let Some((started, before)) = observed {
            self.asset_ops.timing_immediate_finished(
                started,
                success,
                self.session.scene().revision != before,
            );
        }
        success
    }
    pub(super) fn cancel_lighting_draft(&mut self) {
        self.lighting_draft = None;
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.error.clear();
        self.notice = "Lighting draft cancelled; authored state unchanged.".into();
    }
    pub(super) fn create_light_ui(&mut self, kind: LightKind) -> bool {
        #[cfg(feature = "inspection")]
        let observed = (
            self.asset_ops
                .timing_immediate_begin("scene_light_create", "human"),
            self.session.scene().revision,
        );
        if !self.lighting_editable(false) || self.lighting_draft.is_some() {
            self.error =
                "Apply or cancel drafts and resolve dialogs before creating a light.".into();
            #[cfg(feature = "inspection")]
            self.asset_ops
                .timing_immediate_finished(observed.0, false, false);
            return false;
        }
        let result = (|| {
            let id = self
                .session
                .allocate_light_ids(1)
                .map_err(|error| error.to_string())?
                .remove(0);
            let light = match kind {
                LightKind::Directional { .. } => {
                    Light::directional(Some("Directional light".into()))
                }
                LightKind::Point { .. } => Light::point(Some("Point light".into())),
            };
            let edit = self
                .session
                .create_light(self.session.scene().revision, &id, light)
                .map_err(|error| error.to_string())?;
            Ok::<_, String>((id, edit.created_ids))
        })();
        let success = match result {
            Ok((id, created)) => {
                self.select_object(Some(id), false);
                self.error.clear();
                self.notice = "Light added.".into();
                if created.len() > 1 {
                    self.notice
                        .push_str(" Added Sun to preserve the previous illumination.");
                }
                if self.runtime.visible {
                    self.notice.push_str(" Reset preview to use the changes.");
                }
                true
            }
            Err(error) => {
                self.error = error;
                false
            }
        };
        #[cfg(feature = "inspection")]
        self.asset_ops.timing_immediate_finished(
            observed.0,
            success,
            self.session.scene().revision != observed.1,
        );
        success
    }
    pub(super) fn light_inspector_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self
            .selected
            .clone()
            .filter(|id| self.session.scene().lighting.light(id).is_some())
        else {
            return;
        };
        let own = self
            .lighting_draft
            .as_ref()
            .is_some_and(|draft| draft.target_id() == Some(id.as_str()));
        let draft = if own {
            self.lighting_draft.clone().unwrap()
        } else {
            LightingDraft::light(self.session.scene(), &id).unwrap()
        };
        ui.separator();
        ui.strong("Light");
        ui.label(
            match self.session.scene().lighting.light(&id).unwrap().kind {
                LightKind::Directional { .. } => "Directional · rotation sets emitted direction",
                LightKind::Point { .. } => "Point · position sets source; range is in scene units",
            },
        );
        if !self.shader_builtin {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "Custom shader controls appearance and may ignore lighting.",
            );
        }
        self.lighting_form(ui, draft, own);
        if self
            .session
            .scene()
            .lighting
            .light(&id)
            .is_some_and(|light| matches!(light.kind, LightKind::Directional { .. }))
        {
            self.shadow_feedback_ui(ui);
        }
    }
    pub(super) fn ambient_inspector_ui(&mut self, ui: &mut egui::Ui) {
        let own = self
            .lighting_draft
            .as_ref()
            .is_some_and(LightingDraft::is_ambient);
        ui.separator();
        egui::CollapsingHeader::new("Scene lighting")
            .id_salt("scene_lighting")
            .open(own.then_some(true))
            .show(ui, |ui| {
                if !self.shader_builtin {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        "Custom shader controls appearance and may ignore lighting.",
                    );
                }
                let draft = if own {
                    self.lighting_draft.clone().unwrap()
                } else {
                    LightingDraft::ambient(self.session.scene())
                };
                self.lighting_form(ui, draft, own);
            });
    }
    fn lighting_form(&mut self, ui: &mut egui::Ui, mut draft: LightingDraft, own: bool) {
        let editable = self.lighting_editable(false) && (self.lighting_draft.is_none() || own);
        let ambient = draft.is_ambient();
        let committed_color = match &draft {
            LightingDraft::Light { baseline, .. } => baseline.color,
            LightingDraft::Ambient { baseline, .. } => match baseline {
                Lighting::Legacy => Ambient::default().color,
                Lighting::Authored { ambient, .. } => ambient.color,
            },
        };
        let prefix = if ambient { "ambient" } else { "light" };
        let clean_at_start = self.lighting_draft.is_none();
        let mut changed = false;
        let mut discrete_changed = false;
        let mut raw_changed = false;
        let mut enter = false;
        let mut escape = false;
        let mut submitting = None;
        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
        ui.add_enabled_ui(editable, |ui| {
            let mut fields = Vec::new();
            let mut foldout_owns_focus = false;
            let (color, intensity) = match &mut draft {
                LightingDraft::Light { enabled, color, intensity, .. } => {
                    let response = ui.checkbox(enabled, "Enabled"); changed |= response.changed(); discrete_changed |= response.changed(); fields.push(response);
                    (color, intensity)
                }
                LightingDraft::Ambient { color, intensity, .. } => { ui.label("Ambient light"); (color, intensity) }
            };
            // Use the editor's familiar color picker while retaining exact raw
            // numeric text for keyboard entry and invalid-draft recovery.
            ui.horizontal(|ui| {
                ui.label("Color");
                let mut picked = rgb(color).unwrap_or(committed_color);
                let response = ui.color_edit_button_rgb(&mut picked);
                if response.changed() {
                    *color = picked.map(|value| value.to_string());
                    changed = true;
                    raw_changed = true;
                }
                fields.push(response);
            });
            // At minimum inspector width keep each label with its numeric field.
            ui.horizontal_wrapped(|ui| {
                for (index, label) in ["R", "G", "B"].into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        let response = ui.add(egui::TextEdit::singleline(&mut color[index])
                            .id(egui::Id::unique(("lighting_color", prefix, index)))
                            .char_limit(64).desired_width(46.0));
                        changed |= response.changed(); raw_changed |= response.changed(); fields.push(response);
                    });
                }
            });
            ui.horizontal(|ui| {
                ui.label("Intensity").on_hover_text(if ambient { "Relative artistic fill, 0 to 1. Linear RGB channels are 0 to 1." } else { "Relative artistic diffuse intensity, 0 to 16. Linear RGB channels are 0 to 1; not lux/lumens." });
                let response = ui.add(egui::TextEdit::singleline(intensity).id(egui::Id::unique(("lighting_intensity", prefix))).char_limit(64).desired_width(82.0));
                changed |= response.changed(); raw_changed |= response.changed(); fields.push(response);
            });
            if let LightingDraft::Light { range: Some(range), .. } = &mut draft {
                ui.horizontal(|ui| {
                    ui.label("Range").on_hover_text("Smooth influence distance, from 0.1 to 10000 scene units.");
                    let response = ui.add(egui::TextEdit::singleline(range).id(egui::Id::unique("lighting_point_range")).char_limit(64).desired_width(82.0));
                    changed |= response.changed(); raw_changed |= response.changed(); fields.push(response);
                });
            }
            if let LightingDraft::Light { shadow: Some(shadow), .. } = &mut draft {
                ui.separator();
                let response = ui.checkbox(&mut shadow.enabled, "Shadows")
                    .on_hover_text("This Sun casts shadows from opaque scene geometry. Ambient and point lights remain independent.");
                changed |= response.changed();
                discrete_changed |= response.changed();
                fields.push(response);
                ui.horizontal(|ui| {
                    ui.label("Quality");
                    let before = shadow.quality;
                    let response = egui::ComboBox::from_id_salt("lighting_shadow_quality")
                        .width(110.0)
                        .selected_text(match shadow.quality {
                            ShadowQuality::Standard => "Standard",
                            ShadowQuality::High => "High",
                        })
                        .show_ui(ui, |ui| {
                            for (quality, label) in [(ShadowQuality::Standard, "Standard · 1024"), (ShadowQuality::High, "High · 2048")] {
                                if ui.selectable_value(&mut shadow.quality, quality, label).clicked() {
                                    ui.close();
                                }
                            }
                        });
                    changed |= before != shadow.quality;
                    discrete_changed |= before != shadow.quality;
                    fields.push(response.response.on_hover_text("One shadow map covering the scene. High resolves finer details and uses more GPU memory."));
                });
                // Expose an unfinished recovered bias instead of hiding its
                // repair field. Valid settings remain behind the foldout.
                let advanced = egui::CollapsingHeader::new("Advanced shadows")
                    .id_salt("lighting_shadow_advanced")
                    .open((own && shadow.value().is_err()).then_some(true))
                    .show(ui, |ui| {
                        for (text, label, id, hint) in [
                            (&mut shadow.depth_bias_texels, "Depth bias", "lighting_shadow_depth_bias", "Depth offset in world shadow texels, 0 to 4. More bias reduces self-shadowing but can detach shadows from surfaces."),
                            (&mut shadow.normal_bias_texels, "Normal offset", "lighting_shadow_normal_bias", "Surface-normal offset in world shadow texels, 0 to 4. More offset can reduce self-shadowing but lose thin details."),
                        ] {
                            ui.horizontal(|ui| {
                                ui.label(label).on_hover_text(hint);
                                let response = ui.add(egui::TextEdit::singleline(text)
                                    .id(egui::Id::unique(id)).char_limit(64).desired_width(82.0))
                                    .on_hover_text("One world shadow texel is the scene coverage width divided by the map resolution.");
                                changed |= response.changed();
                                raw_changed |= response.changed();
                                fields.push(response);
                            });
                        }
                    });
                // Enter belongs to the foldout's own activation. Escape still
                // cancels this draft when its header owns keyboard focus.
                let response = advanced.header_response;
                foldout_owns_focus = response.has_focus();
                if editable && response.has_focus() && !popup_at_start && !egui::Popup::is_any_open(ui.ctx()) {
                    escape |= ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                }
            }
            for response in &fields {
                let owned = editable && (response.has_focus() || response.lost_focus() || ui.memory(|memory| memory.had_focus_last_frame(response.id)));
                if owned && !popup_at_start && !egui::Popup::is_any_open(ui.ctx()) {
                    if !foldout_owns_focus && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) { enter = true; submitting = Some(response.id); }
                    escape |= ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
                }
            }
        });
        if changed {
            self.lighting_draft = Some(draft);
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        // Complete standalone controls behave like ordinary inspector toggles.
        // Preserve the entire pending form whenever raw/color text changed too,
        // or a draft already existed; only explicit Apply/Enter may submit it.
        if editable
            && clean_at_start
            && discrete_changed
            && !raw_changed
            && !escape
            && self.apply_lighting_draft()
        {
            return;
        }
        if own || changed {
            ui.horizontal_wrapped(|ui| {
                let apply = ui.add_enabled(
                    editable,
                    egui::Button::new(if ambient {
                        "Apply ambient"
                    } else {
                        "Apply light"
                    }),
                );
                let cancel = ui.add_enabled(editable, egui::Button::new("Cancel"));
                let action_owns = editable
                    && [&apply, &cancel].iter().any(|response| {
                        response.has_focus()
                            || ui.memory(|memory| {
                                memory.focused().is_none()
                                    && memory.had_focus_last_frame(response.id)
                            })
                    });
                if action_owns && !popup_at_start && !egui::Popup::is_any_open(ui.ctx()) {
                    escape |= ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                }
                if (apply.clicked() || (editable && enter)) && !self.apply_lighting_draft() {
                    if let Some(id) = submitting {
                        ui.memory_mut(|memory| memory.request_focus(id));
                    }
                }
                if cancel.clicked() || escape {
                    self.cancel_lighting_draft();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_lighting_storage_preserves_invalid_text_and_guards_baselines() {
        let scene = Scene::authored_empty("drafts");
        let mut draft = LightingDraft::light(&scene, "light-1").unwrap();
        if let LightingDraft::Light { intensity, .. } = &mut draft {
            *intensity = "unfinished".into();
        }
        assert!(draft.storage_valid());
        assert!(draft.validate_snapshot(&scene, Some("light-1")));
        assert!(draft.light_value().is_err());
        assert!(!draft.validate_snapshot(&scene, None));
        let bytes = serde_json::to_vec(&draft).unwrap();
        assert_eq!(
            serde_json::from_slice::<LightingDraft>(&bytes).unwrap(),
            draft
        );
        if let LightingDraft::Light { intensity, .. } = &mut draft {
            *intensity = "1.0000000001".into();
        }
        assert!(draft.light_value().is_ok());
        if let LightingDraft::Light { color, .. } = &mut draft {
            color[0] = "1.0000000001".into();
        }
        assert!(draft.light_value().is_err());
        if let LightingDraft::Light { color, .. } = &mut draft {
            color[0] = "é".repeat(65);
        }
        assert!(!draft.storage_valid());

        // Shadow text remains recoverable while disabled or unfinished. Check
        // original f64 bounds before narrowing so 4.0000000001 cannot round in.
        let mut shadow_draft = LightingDraft::light(&scene, "light-1").unwrap();
        if let LightingDraft::Light {
            shadow: Some(shadow),
            ..
        } = &mut shadow_draft
        {
            shadow.quality = ShadowQuality::High;
            shadow.depth_bias_texels = "unfinished".into();
        }
        assert!(shadow_draft.storage_valid());
        assert!(shadow_draft.light_value().is_err());
        let bytes = serde_json::to_vec(&shadow_draft).unwrap();
        assert_eq!(
            serde_json::from_slice::<LightingDraft>(&bytes).unwrap(),
            shadow_draft
        );
        if let LightingDraft::Light {
            shadow: Some(shadow),
            ..
        } = &mut shadow_draft
        {
            shadow.depth_bias_texels = "4.0000000001".into();
        }
        assert!(shadow_draft.light_value().is_err());
        if let LightingDraft::Light {
            shadow: Some(shadow),
            ..
        } = &mut shadow_draft
        {
            shadow.depth_bias_texels = "0.45".into();
            shadow.normal_bias_texels = "-0.000000000000001".into();
        }
        assert!(shadow_draft.light_value().is_err());
        if let LightingDraft::Light {
            shadow: Some(shadow),
            ..
        } = &mut shadow_draft
        {
            shadow.normal_bias_texels = "0.25".into();
        }
        assert_eq!(
            shadow_draft.light_value().unwrap().kind,
            LightKind::Directional {
                shadow: SunShadow {
                    enabled: false,
                    quality: ShadowQuality::High,
                    depth_bias_texels: 0.45,
                    normal_bias_texels: 0.25
                }
            }
        );
        if let LightingDraft::Light {
            shadow: Some(shadow),
            ..
        } = &mut shadow_draft
        {
            shadow.normal_bias_texels = "é".repeat(65);
        }
        assert!(!shadow_draft.storage_valid());
        let legacy = Scene::empty("ambient");
        let mut ambient = LightingDraft::ambient(&legacy);
        assert!(ambient.validate_snapshot(&legacy, None));
        assert!(ambient.validate_snapshot(&legacy, Some("unrelated-selection")));
        if let LightingDraft::Ambient { intensity, .. } = &mut ambient {
            *intensity = "-0.000000000000001".into();
        }
        assert!(ambient.ambient_value().is_err());
        let mut changed = legacy.clone();
        changed.revision += 1;
        assert!(!ambient.validate_snapshot(&changed, None));
        changed = legacy;
        changed.lighting = Lighting::authored_default("sun");
        assert!(!ambient.validate_snapshot(&changed, None));

        // Ambient owns scene authority even without a selected node. Invalid
        // text stays repairable; Save-before-close may commit the repaired form.
        let mut editor =
            SceneEditor::new(ruster_scene::Session::new(Scene::empty("apply")).unwrap());
        editor.lighting_draft = Some(LightingDraft::ambient(editor.session.scene()));
        if let Some(LightingDraft::Ambient { intensity, .. }) = &mut editor.lighting_draft {
            *intensity = "unfinished".into();
        }
        assert!(!editor.apply_lighting_draft());
        assert!(editor.lighting_draft.is_some());
        assert!(editor.session.scene().lighting.is_legacy());
        if let Some(LightingDraft::Ambient { intensity, .. }) = &mut editor.lighting_draft {
            *intensity = "0".into();
        }
        editor.close_pending = true;
        assert!(editor.apply_lighting_draft(), "{}", editor.error);
        assert_eq!(editor.selected, None);
        assert_eq!(editor.session.scene().lighting.light_count(), 1);
        assert_eq!(editor.session.undo_len(), 1);
        assert!(editor.lighting_draft.is_none());
        assert_eq!(editor.draft_generation, 1);
    }

    #[test]
    fn full_inspector_lighting_fields_own_enter_tab_and_escape() {
        fn label_point(shapes: &[egui::epaint::ClippedShape], label: &str) -> egui::Pos2 {
            fn point(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
                match shape {
                    egui::epaint::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.rect.center().to_vec2())
                    }
                    egui::epaint::Shape::Vec(shapes) => {
                        shapes.iter().find_map(|shape| point(shape, label))
                    }
                    _ => None,
                }
            }
            shapes
                .iter()
                .find_map(|shape| point(&shape.shape, label))
                .unwrap_or_else(|| panic!("visible control label {label:?}"))
        }
        for mode in ["light", "ambient", "shadow"] {
            let ambient = mode == "ambient";
            let shadow = mode == "shadow";
            let mut editor = SceneEditor::new(
                ruster_scene::Session::new(Scene::authored_empty("keyboard")).unwrap(),
            );
            editor.select_object(
                if ambient {
                    None
                } else {
                    Some("light-1".into())
                },
                false,
            );
            if ambient {
                editor.lighting_draft = Some(LightingDraft::ambient(editor.session.scene()));
            } else if shadow {
                let mut draft = LightingDraft::light(editor.session.scene(), "light-1").unwrap();
                if let LightingDraft::Light {
                    shadow: Some(shadow),
                    ..
                } = &mut draft
                {
                    shadow.enabled = true;
                    shadow.quality = ShadowQuality::High;
                    shadow.depth_bias_texels.clear();
                }
                editor.lighting_draft = Some(draft);
            }
            let prefix = if ambient { "ambient" } else { "light" };
            let ctx = egui::Context::default();
            let mut time = 0.;
            let mut frame = |editor: &mut SceneEditor, events| {
                time += 0.1;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(640., 1200.),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| editor.inspector_contents(ui),
                );
                output.textures_delta.clear();
                output.shapes
            };
            let key = |key| egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(&mut editor, vec![]);
            let red = if shadow {
                egui::Id::unique("lighting_shadow_depth_bias")
            } else {
                egui::Id::unique(("lighting_color", prefix, 0_usize))
            };
            let point = ctx
                .read_response(red)
                .expect("visible lighting field")
                .rect
                .center();
            frame(
                &mut editor,
                vec![egui::Event::PointerMoved(point), pointer(point, true)],
            );
            frame(
                &mut editor,
                vec![
                    pointer(point, false),
                    egui::Event::Text("unfinished".into()),
                ],
            );
            let unchanged = editor.session.snapshot_bytes().unwrap();
            frame(&mut editor, vec![key(egui::Key::Enter)]);
            assert!(editor.lighting_draft.is_some());
            assert!(!editor.error.is_empty());
            assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
            frame(&mut editor, vec![key(egui::Key::Tab)]);
            frame(&mut editor, vec![]);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(if shadow {
                    egui::Id::unique("lighting_shadow_normal_bias")
                } else {
                    egui::Id::unique(("lighting_color", prefix, 1_usize))
                })
            );
            frame(&mut editor, vec![key(egui::Key::Escape)]);
            assert!(editor.lighting_draft.is_none());
            assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
            let mut draft = if ambient {
                LightingDraft::ambient(editor.session.scene())
            } else {
                LightingDraft::light(editor.session.scene(), "light-1").unwrap()
            };
            if shadow {
                if let LightingDraft::Light {
                    shadow: Some(shadow),
                    ..
                } = &mut draft
                {
                    shadow.enabled = true;
                    shadow.quality = ShadowQuality::High;
                    shadow.depth_bias_texels = "0.45".into();
                    shadow.normal_bias_texels = "0.1".into();
                }
            } else {
                match &mut draft {
                    LightingDraft::Light { intensity, .. }
                    | LightingDraft::Ambient { intensity, .. } => *intensity = "0.45".into(),
                }
            }
            editor.lighting_draft = Some(draft);
            frame(&mut editor, vec![]);
            let intensity = if shadow {
                egui::Id::unique("lighting_shadow_depth_bias")
            } else {
                egui::Id::unique(("lighting_intensity", prefix))
            };
            let point = ctx.read_response(intensity).unwrap().rect.center();
            frame(
                &mut editor,
                vec![egui::Event::PointerMoved(point), pointer(point, true)],
            );
            frame(&mut editor, vec![pointer(point, false)]);
            frame(&mut editor, vec![key(egui::Key::Enter)]);
            assert!(editor.lighting_draft.is_none(), "{}", editor.error);
            assert_eq!(editor.session.scene().revision, 1);
            assert_eq!(editor.session.undo_len(), 1);
            if shadow {
                assert_eq!(
                    editor
                        .session
                        .scene()
                        .lighting
                        .light("light-1")
                        .unwrap()
                        .kind,
                    LightKind::Directional {
                        shadow: SunShadow {
                            enabled: true,
                            quality: ShadowQuality::High,
                            depth_bias_texels: 0.45,
                            normal_bias_texels: 0.1
                        }
                    }
                );
            }
            editor.session.undo(1).unwrap();
            assert!(
                matches!(&editor.session.scene().lighting, Lighting::Authored { ambient,lights } if ambient.intensity==0.28 && lights["light-1"].intensity==0.72)
            );
            if shadow {
                // Actual pointer activation of a clean discrete control must
                // commit immediately, without exposing a transient Apply draft.
                let before = editor.session.scene().revision;
                let shapes = frame(&mut editor, vec![]);
                let point = label_point(&shapes, "Shadows");
                frame(
                    &mut editor,
                    vec![egui::Event::PointerMoved(point), pointer(point, true)],
                );
                frame(&mut editor, vec![pointer(point, false)]);
                assert!(
                    editor.lighting_draft.is_none(),
                    "clean Shadows click must commit without Apply"
                );
                assert_eq!(editor.session.scene().revision, before + 1);
                assert_eq!(editor.session.undo_len(), 1);
                assert!(
                    matches!(editor.session.scene().lighting.light("light-1").unwrap().kind,
                    LightKind::Directional { shadow } if shadow.enabled)
                );
                editor.session.undo(before + 1).unwrap();
                assert!(
                    matches!(editor.session.scene().lighting.light("light-1").unwrap().kind,
                    LightKind::Directional { shadow } if !shadow.enabled)
                );

                // Discrete changes cannot submit an already pending raw draft,
                // even if a checkbox or dropdown itself has a complete value.
                let mut draft = LightingDraft::light(editor.session.scene(), "light-1").unwrap();
                if let LightingDraft::Light {
                    shadow: Some(shadow),
                    ..
                } = &mut draft
                {
                    shadow.depth_bias_texels = "unfinished".into();
                }
                editor.lighting_draft = Some(draft);
                let unchanged = editor.session.snapshot_bytes().unwrap();
                let shapes = frame(&mut editor, vec![]);
                let point = label_point(&shapes, "Shadows");
                frame(
                    &mut editor,
                    vec![egui::Event::PointerMoved(point), pointer(point, true)],
                );
                let shapes = frame(&mut editor, vec![pointer(point, false)]);
                assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
                assert!(matches!(&editor.lighting_draft,
                    Some(LightingDraft::Light { shadow: Some(shadow), .. })
                        if shadow.enabled && shadow.depth_bias_texels == "unfinished"));
                let point = label_point(&shapes, "Standard");
                frame(
                    &mut editor,
                    vec![egui::Event::PointerMoved(point), pointer(point, true)],
                );
                frame(&mut editor, vec![pointer(point, false)]);
                assert!(
                    egui::Popup::is_any_open(&ctx),
                    "quality pointer activation opens its popup"
                );
                // Popup placement/fade begins on activation; observe a later
                // genuine egui pass before selecting its rendered entry.
                let shapes = frame(&mut editor, vec![]);
                let point = label_point(&shapes, "High · 2048");
                frame(
                    &mut editor,
                    vec![egui::Event::PointerMoved(point), pointer(point, true)],
                );
                frame(&mut editor, vec![pointer(point, false)]);
                assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
                assert!(matches!(&editor.lighting_draft,
                    Some(LightingDraft::Light { shadow: Some(shadow), .. })
                        if shadow.enabled && shadow.quality == ShadowQuality::High
                            && shadow.depth_bias_texels == "unfinished"));

                // Enter activates a menu entry without a pointer click. It must
                // dismiss the popup while preserving the surrounding raw draft.
                let shapes = frame(&mut editor, vec![]);
                assert!(!egui::Popup::is_any_open(&ctx));
                let point = label_point(&shapes, "High");
                frame(
                    &mut editor,
                    vec![egui::Event::PointerMoved(point), pointer(point, true)],
                );
                frame(&mut editor, vec![pointer(point, false)]);
                frame(&mut editor, vec![]);
                frame(&mut editor, vec![key(egui::Key::ArrowDown)]);
                frame(&mut editor, vec![key(egui::Key::Enter)]);
                assert!(
                    !egui::Popup::is_any_open(&ctx),
                    "keyboard quality selection must close its popup"
                );
                assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
                assert!(matches!(&editor.lighting_draft,
                    Some(LightingDraft::Light { shadow: Some(shadow), .. })
                        if shadow.enabled && shadow.quality == ShadowQuality::Standard
                            && shadow.depth_bias_texels == "unfinished"));
            }
        }
    }
}
