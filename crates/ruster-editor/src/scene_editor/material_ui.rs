//! One recoverable material form, using the same validated commands as agents.
use super::{combo_focus::ComboFocus, SceneEditor};
use eframe::egui;
use ruster_assets::{AssetKind, AssetStatus, TextureRole};
use ruster_scene::{MaterialResponse, OpaqueMaterial, Scene, TextureAddress, TextureFilter};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MaterialIntent {
    EditShared,
    Create,
    MakeUnique,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MaterialResponseMode {
    Diffuse,
    Dielectric,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MaterialDraft {
    pub revision: u64,
    pub object_id: String,
    pub original_assignment: Option<String>,
    pub original_material: Option<OpaqueMaterial>,
    pub intent: MaterialIntent,
    pub name: String,
    pub tint: [String; 3],
    pub response_mode: MaterialResponseMode,
    pub roughness: String,
    pub texture: Option<String>,
    pub roughness_texture: Option<String>,
    pub filter: TextureFilter,
    pub address: TextureAddress,
}
impl MaterialDraft {
    fn new(scene: &Scene, object_id: &str, intent: MaterialIntent) -> Option<Self> {
        let object = scene.objects.iter().find(|object| object.id == object_id)?;
        let original = object
            .material
            .as_ref()
            .and_then(|id| scene.materials.get(id))
            .cloned();
        if intent != MaterialIntent::Create && original.is_none() {
            return None;
        }
        let value = if intent == MaterialIntent::Create {
            OpaqueMaterial::default()
        } else {
            original.clone()?
        };
        let name = match intent {
            MaterialIntent::Create => "Material".to_string(),
            MaterialIntent::MakeUnique => {
                let original = value.name.as_deref().unwrap_or("Material");
                let copy = format!("{original} copy");
                if copy.len() <= 128 {
                    copy
                } else {
                    original.to_owned()
                }
            }
            MaterialIntent::EditShared => value.name.clone().unwrap_or_default(),
        };
        Some(Self {
            revision: scene.revision,
            object_id: object_id.into(),
            original_assignment: object.material.clone(),
            original_material: original,
            intent,
            name,
            tint: value.tint.map(|channel| channel.to_string()),
            response_mode: if value.response.is_legacy() {
                MaterialResponseMode::Diffuse
            } else {
                MaterialResponseMode::Dielectric
            },
            roughness: value.response.roughness().unwrap_or(0.5).to_string(),
            texture: value.base_color_texture,
            roughness_texture: value.roughness_texture,
            filter: value.filter,
            address: value.address,
        })
    }
    pub fn storage_valid(&self) -> bool {
        ruster_scene::validate_id(&self.object_id).is_ok()
            && self
                .original_assignment
                .as_ref()
                .is_none_or(|id| ruster_scene::validate_id(id).is_ok())
            && self
                .original_material
                .as_ref()
                .is_none_or(|value| value.validate().is_ok())
            && self
                .texture
                .as_ref()
                .is_none_or(|id| ruster_scene::validate_id(id).is_ok())
            && self
                .roughness_texture
                .as_ref()
                .is_none_or(|id| ruster_scene::validate_id(id).is_ok())
            && self.name.len() <= 512
            && self.name.chars().count() <= 128
            && self.roughness.len() <= 256
            && self.roughness.chars().count() <= 64
            && self
                .tint
                .iter()
                .all(|text| text.len() <= 256 && text.chars().count() <= 64)
    }
    pub fn validate_snapshot(&self, scene: &Scene, active: Option<&str>) -> bool {
        self.storage_valid()
            && self.revision == scene.revision
            && active == Some(self.object_id.as_str())
            && scene
                .objects
                .iter()
                .find(|object| object.id == self.object_id)
                .is_some_and(|object| {
                    object.material == self.original_assignment
                        && object
                            .material
                            .as_ref()
                            .and_then(|id| scene.materials.get(id))
                            == self.original_material.as_ref()
                        && (self.intent == MaterialIntent::Create
                            || self.original_material.is_some())
                })
    }
    fn tint_value(&self) -> Result<[f32; 3], String> {
        let mut tint = [0.0; 3];
        for (output, text) in tint.iter_mut().zip(&self.tint) {
            let value = text
                .trim()
                .parse::<f64>()
                .map_err(|_| "Enter three finite tint channels from 0 to 1.")?;
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err("Enter three finite tint channels from 0 to 1.".into());
            }
            *output = value as f32;
        }
        Ok(tint)
    }
    fn parse(&self) -> Result<OpaqueMaterial, String> {
        let response = match self.response_mode {
            MaterialResponseMode::Diffuse => MaterialResponse::LegacyDiffuse,
            MaterialResponseMode::Dielectric => {
                let original = self
                    .roughness
                    .trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|value| value.is_finite() && (0.08..=1.0).contains(value))
                    .ok_or("Roughness requires a finite number from 0.08 to 1.")?;
                MaterialResponse::Dielectric {
                    roughness: original as f32,
                }
            }
        };
        let value = OpaqueMaterial {
            name: (!self.name.is_empty()).then(|| self.name.clone()),
            response,
            tint: self.tint_value()?,
            base_color_texture: self.texture.clone(),
            roughness_texture: self.roughness_texture.clone(),
            filter: self.filter,
            address: self.address,
        };
        value.validate().map_err(|error| error.to_string())?;
        Ok(value)
    }
}

impl SceneEditor {
    fn material_editable(&self) -> bool {
        self.material_editable_for_save(false)
    }
    fn material_editable_for_save(&self, saving: bool) -> bool {
        !self.asset_publishing()
            && !self.asset_loading()
            && self.color_draft.is_none()
            && self.lighting_draft.is_none()
            && self.display_draft.is_none()
            && self.collider_draft.is_none()
            && !self.manipulation.active()
            && !self.manipulation.precision_pending()
            && !self.manipulation.offset_pending()
            && self.draft == self.baseline_fields
            && !self.behavior.pending()
            && !self.input_actions.pending()
            && (self.recovered_input.is_none() || self.material_draft.is_some())
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
    fn begin_material_draft(&mut self, ui: &egui::Ui, intent: MaterialIntent) {
        if !self.material_editable() || self.material_draft.is_some() {
            return;
        }
        if let Some(id) = &self.selected {
            self.material_draft = MaterialDraft::new(self.session.scene(), id, intent);
            if self.material_draft.is_some() {
                self.draft_generation = self.draft_generation.wrapping_add(1);
                ui.memory_mut(|memory| memory.request_focus(egui::Id::unique("material_name")));
            }
        }
    }
    pub(super) fn apply_material_draft(&mut self) -> bool {
        let Some(draft) = self.material_draft.clone() else {
            return true;
        };
        #[cfg(feature = "inspection")]
        let observed = (
            self.asset_ops
                .timing_immediate_begin("scene_material_set", "human"),
            self.session.scene().revision,
        );
        let result = (|| {
            if !self.material_editable_for_save(true) {
                return Err(
                    "Finish other edits and resolve dialogs before applying the material.".into(),
                );
            }
            if !draft.validate_snapshot(self.session.scene(), self.selected.as_deref()) {
                return Err("Material target changed; cancel this draft and inspect again.".into());
            }
            let value = draft.parse()?;
            if draft.intent == MaterialIntent::EditShared {
                let id = draft
                    .original_assignment
                    .as_deref()
                    .ok_or("Material no longer assigned.")?;
                self.validate_material_candidate(id, &value)?;
                self.session
                    .set_material(draft.revision, id, value)
                    .map_err(|error| error.to_string())?;
            } else {
                let ids = self
                    .session
                    .allocate_material_ids(1)
                    .map_err(|error| error.to_string())?;
                let id = &ids[0];
                self.validate_material_candidate(id, &value)?;
                // Validate the new user's UVs before introducing its reference.
                if (value.base_color_texture.is_some() || value.roughness_texture.is_some())
                    && !self
                        .session
                        .scene()
                        .objects
                        .iter()
                        .find(|object| object.id == draft.object_id)
                        .and_then(|object| object.mesh.as_ref())
                        .and_then(|id| self.resolved.get(id))
                        .is_some_and(|mesh| mesh.has_complete_uvs())
                {
                    return Err(
                        "Assign a mesh with complete UVs before using a textured material.".into(),
                    );
                }
                self.session
                    .create_and_assign_material(draft.revision, &draft.object_id, id, value)
                    .map_err(|error| error.to_string())?;
            }
            Ok::<_, String>(())
        })();
        let success = match result {
            Ok(()) => {
                self.material_draft = None;
                self.draft_generation = self.draft_generation.wrapping_add(1);
                self.sync_draft();
                self.error.clear();
                self.notice =
                    "Material applied. Undo restores the edit; Reset updates the preview.".into();
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
    fn cancel_material_draft(&mut self) {
        self.material_draft = None;
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.error.clear();
        self.notice = "Material draft cancelled; authored state unchanged.".into();
    }
    fn assign_material_ui(&mut self, object_id: &str, material: Option<String>) {
        if !self.material_editable() || self.material_draft.is_some() {
            return;
        }
        let result = self
            .validate_material_assignment(object_id, &material)
            .and_then(|()| {
                self.session
                    .assign_material(self.session.scene().revision, object_id, material)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(_) => {
                self.sync_draft();
                self.error.clear();
                self.notice = "Material assignment updated.".into();
            }
            Err(error) => self.error = error,
        }
    }
    /// Choose content inside the existing form; importing pixels remains a catalog operation.
    pub(super) fn stage_material_texture(
        &mut self,
        texture_id: &str,
        role: TextureRole,
    ) -> Result<(), String> {
        if !self.material_editable() {
            return Err(
                "Finish other edits or resolve dialogs before choosing a material texture.".into(),
            );
        }
        let object_id = self.selected.clone().ok_or("Select an object first.")?;
        let object = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == object_id)
            .ok_or("Selected object disappeared.")?;
        let intent = if object.material.is_some() {
            MaterialIntent::EditShared
        } else {
            MaterialIntent::Create
        };
        let mut draft = self
            .material_draft
            .clone()
            .or_else(|| MaterialDraft::new(self.session.scene(), &object_id, intent))
            .ok_or("Material target unavailable.")?;
        if !draft.validate_snapshot(self.session.scene(), Some(&object_id)) {
            return Err(
                "Material target changed; cancel this draft before choosing another texture."
                    .into(),
            );
        }
        let ready = self.assets.as_ref().is_some_and(|store| {
            store.texture(texture_id).is_some()
                && store.inspect().iter().any(|info| {
                    info.id == texture_id
                        && info.kind == ruster_assets::AssetKind::Texture
                        && info.texture_role == Some(role)
                        && info.status == ruster_assets::AssetStatus::Ready
                })
        });
        if !ready {
            return Err(match role {
                TextureRole::BaseColor => {
                    "Choose a ready Base color (sRGB) PNG import for this slot."
                }
                TextureRole::Roughness => {
                    "Choose a ready Roughness map (linear) PNG import for this slot."
                }
            }
            .into());
        }
        let current = match role {
            TextureRole::BaseColor => &draft.texture,
            TextureRole::Roughness => &draft.roughness_texture,
        };
        if current.as_deref() == Some(texture_id) {
            return Ok(());
        }
        let id = if draft.intent == MaterialIntent::EditShared {
            draft
                .original_assignment
                .clone()
                .ok_or("No material assigned.")?
        } else {
            self.session
                .allocate_material_ids(1)
                .map_err(|error| error.to_string())?
                .remove(0)
        };
        // Validate dependencies independently of unfinished numeric/name text;
        // choosing an image must not discard those text fields or apply them.
        let mut candidate = OpaqueMaterial {
            base_color_texture: draft.texture.clone(),
            roughness_texture: draft.roughness_texture.clone(),
            filter: draft.filter,
            address: draft.address,
            ..Default::default()
        };
        match role {
            TextureRole::BaseColor => candidate.base_color_texture = Some(texture_id.into()),
            TextureRole::Roughness => candidate.roughness_texture = Some(texture_id.into()),
        }
        self.validate_material_candidate(&id, &candidate)?;
        if draft.intent != MaterialIntent::EditShared
            && !object
                .mesh
                .as_ref()
                .and_then(|id| self.resolved.get(id))
                .is_some_and(|mesh| mesh.has_complete_uvs())
        {
            return Err(
                "Assign a mesh with complete UVs before choosing a material texture.".into(),
            );
        }
        match role {
            TextureRole::BaseColor => draft.texture = Some(texture_id.into()),
            TextureRole::Roughness => draft.roughness_texture = Some(texture_id.into()),
        }
        self.material_draft = Some(draft);
        self.draft_generation = self.draft_generation.wrapping_add(1);
        self.error.clear();
        self.notice = "Texture chosen. Apply material to commit the edit.".into();
        Ok(())
    }

    pub(super) fn material_inspector_ui(&mut self, ui: &mut egui::Ui) {
        let Some(object) = self
            .selected
            .as_ref()
            .and_then(|id| {
                self.session
                    .scene()
                    .objects
                    .iter()
                    .find(|object| &object.id == id)
            })
            .cloned()
        else {
            return;
        };
        let editable = self.material_editable();
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Material");
            ui.label("Opaque").on_hover_text(
                "Base-color PNGs render as opaque surfaces; texture alpha is not used.",
            );
        });
        let choices = self
            .session
            .scene()
            .materials
            .iter()
            .map(|(id, material)| {
                (
                    id.clone(),
                    material.name.clone().unwrap_or_else(|| id.clone()),
                )
            })
            .collect::<Vec<_>>();
        let mut remove = None;
        ui.add_enabled_ui(editable && self.material_draft.is_none(), |ui| {
            ui.menu_button("Manage materials", |ui| {
                egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                    if choices.is_empty() { ui.label("No materials yet."); }
                    for (id,name) in &choices {
                        let users = self.session.scene().objects.iter().filter(|object| object.material.as_deref()==Some(id.as_str())).count();
                        ui.push_id(id, |ui| ui.horizontal(|ui| {
                            ui.label(name).on_hover_text(id);
                            if ui.add_enabled(users==0,egui::Button::new("Delete"))
                                .on_hover_text(if users==0 {"Delete the unused material; Undo restores it.".to_string()} else {format!("Used by {users} objects; clear their assignments first.")}).clicked() {
                                remove=Some(id.clone());ui.close();
                            }
                        }));
                    }
                });
            });
        });
        if let Some(id) = remove {
            match self
                .session
                .remove_material(self.session.scene().revision, &id)
            {
                Ok(_) => {
                    self.sync_draft();
                    self.error.clear();
                    self.notice = "Unused material deleted. Undo restores it.".into();
                }
                Err(error) => self.error = error.to_string(),
            }
        }
        let mut assigned = object.material.clone();
        ui.add_enabled_ui(editable && self.material_draft.is_none(), |ui| {
            let focus = ComboFocus::capture(ui, "object_material_assignment");
            let mut selected = false;
            let response = egui::ComboBox::from_id_salt("object_material_assignment")
                .selected_text(
                    choices
                        .iter()
                        .find(|(id, _)| Some(id) == assigned.as_ref())
                        .map(|(_, name)| name.as_str())
                        .unwrap_or("None"),
                )
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut assigned, None, "None").clicked() {
                        selected = true;
                        ui.close();
                    }
                    for (id, name) in &choices {
                        if ui
                            .selectable_value(&mut assigned, Some(id.clone()), name)
                            .on_hover_text(id)
                            .clicked()
                        {
                            selected = true;
                            ui.close();
                        }
                    }
                });
            focus.restore(ui, &response.response, selected);
        });
        if assigned != object.material {
            self.assign_material_ui(&object.id, assigned);
        }
        let mut started_draft = false;
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(editable && self.material_draft.is_none(), egui::Button::new("New material")).clicked() {
                started_draft = true;
                self.begin_material_draft(ui, MaterialIntent::Create);
            }
            if object.material.is_some() && ui.add_enabled(editable && self.material_draft.is_none(), egui::Button::new("Make unique"))
                .on_hover_text("Copy this material for the active object; other objects keep the original.").clicked() {
                started_draft = true;
                self.begin_material_draft(ui, MaterialIntent::MakeUnique);
            }
        });
        let Some(mut draft) = self.material_draft.clone().or_else(|| {
            MaterialDraft::new(self.session.scene(), &object.id, MaterialIntent::EditShared)
        }) else {
            return;
        };
        if draft.intent == MaterialIntent::EditShared {
            let users = self
                .session
                .scene()
                .objects
                .iter()
                .filter(|other| other.material == draft.original_assignment)
                .count();
            ui.label(format!(
                "Shared by {users} {}",
                if users == 1 { "object" } else { "objects" }
            ));
        } else {
            ui.label(if draft.intent == MaterialIntent::Create {
                "New material"
            } else {
                "Independent material copy"
            });
        }
        let popup_at_start = egui::Popup::is_any_open(ui.ctx());
        let mut changed = false;
        let mut enter = false;
        let mut escape = false;
        let mut submitting_field = None;
        let mut dropped_texture = None;
        ui.add_enabled_ui(editable, |ui| {
            ui.label("Name");
            let name = ui.add(
                egui::TextEdit::singleline(&mut draft.name)
                    .id(egui::Id::unique("material_name"))
                    .char_limit(128)
                    .desired_width(ui.available_width()),
            );
            changed |= name.changed();
            let mut fields = vec![name];
            ui.horizontal(|ui| {
                ui.label("Tint").on_hover_text(
                    "Linear material tint, multiplied by the texture and Object tint.",
                );
                let tint = draft.tint_value();
                let mut rgb = tint.as_ref().copied().unwrap_or([1.0; 3]);
                let swatch = ui
                    .add_enabled_ui(tint.is_ok(), |ui| ui.color_edit_button_rgb(&mut rgb))
                    .inner;
                if swatch.changed() {
                    draft.tint = rgb.map(|value| value.to_string());
                    changed = true;
                }
                fields.push(swatch);
            });
            // Keep all three labelled channels together at the inspector's
            // minimum width; wrapping a label away from its field is ambiguous.
            ui.horizontal(|ui| {
                for (index, label) in ["R", "G", "B"].into_iter().enumerate() {
                    ui.label(label);
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut draft.tint[index])
                            .id(egui::Id::unique(("material_tint", index)))
                            .char_limit(64)
                            .desired_width(46.0),
                    );
                    changed |= response.changed();
                    fields.push(response);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Shading").on_hover_text("Diffuse preserves the legacy material response. Nonmetallic adds a neutral dielectric highlight controlled by roughness.");
                let before = draft.response_mode;
                let focus = ComboFocus::capture(ui, "material_response_mode");
                let mut selected = false;
                let response = egui::ComboBox::from_id_salt("material_response_mode")
                    .width(120.0)
                    .selected_text(match draft.response_mode {
                        MaterialResponseMode::Diffuse => "Diffuse",
                        MaterialResponseMode::Dielectric => "Nonmetallic",
                    })
                    .show_ui(ui, |ui| {
                        for (mode, label) in [(MaterialResponseMode::Diffuse, "Diffuse"), (MaterialResponseMode::Dielectric, "Nonmetallic")] {
                            if ui.selectable_value(&mut draft.response_mode, mode, label).clicked() {
                                selected = true;
                                ui.close();
                            }
                        }
                    });
                changed |= before != draft.response_mode;
                focus.restore(ui, &response.response, selected);
                fields.push(response.response);
            });
            if draft.response_mode == MaterialResponseMode::Dielectric {
                ui.label(if draft.roughness_texture.is_some() { "Roughness multiplier" } else { "Roughness" }).on_hover_text("Perceptual roughness, 0.08 to 1: lower values give a tighter highlight; higher values spread it. This is a nonmetallic surface response, without environment reflections.");
                ui.horizontal(|ui| {
                    let response = ui.add(egui::TextEdit::singleline(&mut draft.roughness)
                        .id(egui::Id::unique("material_roughness"))
                        .char_limit(64).desired_width(82.0));
                    ui.label("0.08–1");
                    changed |= response.changed();
                    fields.push(response);
                });
                if self.session.scene().display.is_legacy() {
                    ui.label("Surface shading needs Surface HDR in Scene display.");
                }
                if !self.shader_builtin {
                    ui.label("Custom shader controls appearance; surface shading is unsupported. Settings are retained.");
                }
            }
            let uv_ready = if draft.intent == MaterialIntent::EditShared {
                self.session
                    .scene()
                    .objects
                    .iter()
                    .filter(|object| object.material == draft.original_assignment)
                    .all(|object| {
                        object
                            .mesh
                            .as_ref()
                            .and_then(|id| self.resolved.get(id))
                            .is_some_and(|mesh| mesh.has_complete_uvs())
                    })
            } else {
                object
                    .mesh
                    .as_ref()
                    .and_then(|id| self.resolved.get(id))
                    .is_some_and(|mesh| mesh.has_complete_uvs())
            };
            if !uv_ready {
                ui.label(if draft.intent == MaterialIntent::EditShared {
                    "All shared users need UV-mapped meshes. Make unique to texture this object only."
                } else { "Choose a UV-mapped mesh before adding a texture." });
            }
            let roughness_active = draft.response_mode == MaterialResponseMode::Dielectric
                && !self.session.scene().display.is_legacy() && self.shader_builtin;
            let infos = self.assets.as_ref().map(|store| store.infos().collect::<Vec<_>>()).unwrap_or_default();
            for (role, label, salt, reference) in [
                (TextureRole::BaseColor, "Base color texture", "material_texture", &mut draft.texture),
                (TextureRole::Roughness, "Roughness map", "material_roughness_texture", &mut draft.roughness_texture),
            ] {
                let before = reference.clone();
                let selected_info = reference.as_ref().and_then(|id| infos.iter().find(|info| &info.id == id).copied());
                let selected_text = match reference.as_ref() {
                    None => "None".to_owned(),
                    Some(id) => match selected_info {
                        Some(info) if info.kind == AssetKind::Texture && info.texture_role == Some(role) => std::path::Path::new(&info.source).file_name().unwrap_or_default().to_string_lossy().into_owned(),
                        Some(_) => format!("Wrong role: {id}"),
                        None => format!("Missing: {id}"),
                    },
                };
                ui.label(label);
                let focus = ComboFocus::capture(ui, salt);
                let mut selected = false;
                let response = egui::ComboBox::from_id_salt(salt).selected_text(selected_text).show_ui(ui, |ui| {
                    let clear = if role == TextureRole::Roughness { "None (clear)" } else { "None" };
                    if ui.selectable_value(reference, None, clear).clicked() { selected = true; ui.close(); }
                    for info in infos.iter().filter(|info| info.kind == AssetKind::Texture && info.texture_role == Some(role)) {
                        let ready = uv_ready && info.status == AssetStatus::Ready && self.assets.as_ref().is_some_and(|store| store.texture(&info.id).is_some_and(|texture| texture.role == role));
                        ui.add_enabled_ui(ready, |ui| {
                            if ui.selectable_value(reference, Some(info.id.clone()), &info.source).on_hover_text(&info.id).clicked() { selected = true; ui.close(); }
                        });
                    }
                });
                focus.restore(ui, &response.response, selected);
                let response = response.response;
                if response.dnd_hover_payload::<super::AssetDrag>().is_some_and(|payload| self.resolved_textures.get(&payload.asset_id).is_some_and(|texture| texture.role == role)) {
                    ui.painter().rect_stroke(response.rect, 3.0, egui::Stroke::new(2.0, ui.visuals().selection.bg_fill), egui::StrokeKind::Inside);
                }
                if editable {
                    if let Some(payload) = response.dnd_release_payload::<super::AssetDrag>() { dropped_texture = Some(((*payload).clone(), role)); }
                }
                fields.push(response);
                changed |= *reference != before;
                if let Some(id) = reference.as_ref() {
                    let info = infos.iter().find(|info| &info.id == id).copied();
                    let accepted = self.assets.as_ref().is_some_and(|store| store.texture(id).is_some_and(|texture| texture.role == role));
                    if let Some(info) = info { ui.add(egui::Label::new(format!("Source: {}", info.source)).wrap()); }
                    if !info.is_some_and(|info| info.kind == AssetKind::Texture && info.texture_role == Some(role)) {
                        ui.colored_label(ui.visuals().warn_fg_color, "Reference is missing or has the wrong import role. Repair it or choose None.");
                    } else if !info.is_some_and(|info| info.status == AssetStatus::Ready) {
                        ui.colored_label(ui.visuals().warn_fg_color, if accepted { "Source unavailable; last-good content is retained. Repair and Refresh." } else { "Texture unavailable. Repair and Refresh before faithful export." });
                    }
                    if role == TextureRole::Roughness && !accepted && roughness_active {
                        ui.label("Scalar viewport fallback is provisional; export requires this map.");
                    }
                }
            }
            if draft.roughness_texture.is_some() {
                if draft.response_mode == MaterialResponseMode::Diffuse {
                    ui.label("Roughness map is inactive with Diffuse shading. Settings are retained.");
                    if !self.shader_builtin { ui.label("Custom shader controls appearance; roughness maps are unsupported."); }
                } else {
                    ui.label("Map × multiplier, clamped to 0.08–1. Use 1 for the full map range.");
                }
                ui.label("Filter and Wrap affect both texture slots.");
            }
            let filter = draft.filter;
            let address = draft.address;
            ui.horizontal(|ui| {
                ui.label("Filter");
                let focus = ComboFocus::capture(ui, "material_filter");
                let mut selected = false;
                let filter_response = egui::ComboBox::from_id_salt("material_filter")
                    .selected_text(match draft.filter {
                        TextureFilter::Linear => "Linear",
                        TextureFilter::Nearest => "Nearest",
                    })
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut draft.filter, TextureFilter::Linear, "Linear").clicked() { selected = true; ui.close(); }
                        if ui.selectable_value(&mut draft.filter, TextureFilter::Nearest, "Nearest").clicked() { selected = true; ui.close(); }
                    });
                focus.restore(ui, &filter_response.response, selected);
                fields.push(filter_response.response);
            });
            ui.horizontal(|ui| {
                ui.label("Wrap");
                let focus = ComboFocus::capture(ui, "material_wrap");
                let mut selected = false;
                let wrap_response = egui::ComboBox::from_id_salt("material_wrap")
                    .selected_text(match draft.address {
                        TextureAddress::Repeat => "Repeat",
                        TextureAddress::Clamp => "Clamp",
                    })
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut draft.address, TextureAddress::Repeat, "Repeat").clicked() { selected = true; ui.close(); }
                        if ui.selectable_value(&mut draft.address, TextureAddress::Clamp, "Clamp").clicked() { selected = true; ui.close(); }
                    });
                focus.restore(ui, &wrap_response.response, selected);
                fields.push(wrap_response.response);
            });
            changed |= filter != draft.filter || address != draft.address;
            for response in &fields {
                let owned = editable && (response.has_focus()
                    || response.lost_focus()
                    || ui.memory(|memory| memory.had_focus_last_frame(response.id)));
                if owned && !started_draft && !popup_at_start && !egui::Popup::is_any_open(ui.ctx())
                {
                    if ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    }) {
                        enter = true;
                        submitting_field = Some(response.id);
                    }
                    escape |= ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                }
            }
        });
        if changed {
            self.material_draft = Some(draft);
            self.draft_generation = self.draft_generation.wrapping_add(1);
        }
        if let Some((payload, role)) = dropped_texture {
            if let Err(error) = self
                .check_asset_drag(&payload)
                .and_then(|()| self.stage_material_texture(&payload.asset_id, role))
            {
                self.error = error;
            }
        }
        if self.material_draft.is_some() {
            ui.horizontal(|ui| {
                let apply = ui.add_enabled(editable, egui::Button::new("Apply material"));
                let cancel = ui.add_enabled(editable, egui::Button::new("Cancel material"));
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
                if (apply.clicked() || (editable && enter)) && !self.apply_material_draft() {
                    if let Some(id) = submitting_field {
                        ui.memory_mut(|memory| memory.request_focus(id));
                    }
                }
                if cancel.clicked() || escape {
                    self.cancel_material_draft();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    fn editor() -> SceneEditor {
        let mut scene = Scene::demo();
        scene.materials.insert(
            "shared".into(),
            OpaqueMaterial {
                name: Some("Shared".into()),
                ..Default::default()
            },
        );
        scene.objects[0].material = Some("shared".into());
        scene.objects[1].material = Some("shared".into());
        let mut editor = SceneEditor::new(ruster_scene::Session::new(scene).unwrap());
        editor.select_object(Some("left".into()), false);
        editor
    }
    // Full inspector ownership risk: failed Enter must leave editable text,
    // Tab/Escape must still work, and UI Make unique must be one transaction.
    #[test]
    fn material_form_keyboard_and_shared_unique_transactions() {
        let mut editor = editor();
        // Retain a map authored through another path while editing other fields
        // or making a unique copy.
        let mut mapped = editor.session.scene().clone();
        mapped
            .materials
            .get_mut("shared")
            .unwrap()
            .roughness_texture = Some("retained-map".into());
        let mut raw = MaterialDraft::new(&mapped, "left", MaterialIntent::EditShared).unwrap();
        raw.name = "Renamed".into();
        raw.tint[0] = "0.25".into();
        assert_eq!(
            raw.parse().unwrap().roughness_texture.as_deref(),
            Some("retained-map")
        );
        assert_eq!(
            MaterialDraft::new(&mapped, "left", MaterialIntent::MakeUnique)
                .unwrap()
                .roughness_texture,
            raw.roughness_texture
        );
        let preview = editor.preview.clone();
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut frame = |editor: &mut SceneEditor, events| {
            time += 0.1;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 1500.0),
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
        frame(&mut editor, vec![]);
        let red = egui::Id::unique(("material_tint", 0usize));
        let point = ctx.read_response(red).unwrap().rect.center();
        let pointer = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(point), pointer(true)],
        );
        frame(
            &mut editor,
            vec![pointer(false), egui::Event::Text("invalid".into())],
        );
        assert!(editor.material_draft.as_ref().unwrap().tint[0].contains("invalid"));
        let original = editor.session.snapshot_bytes().unwrap();
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.material_draft.is_some());
        assert!(!editor.error.is_empty());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), original);
        frame(&mut editor, vec![key(egui::Key::Tab)]);
        frame(&mut editor, vec![]);
        assert_eq!(
            ctx.memory(|memory| memory.focused()),
            Some(egui::Id::unique(("material_tint", 1usize)))
        );
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.material_draft.is_none());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), original);
        // Actual text replacement and Enter commit, not a direct field mutation.
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(point), pointer(true)],
        );
        frame(&mut editor, vec![pointer(false)]);
        frame(
            &mut editor,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
                egui::Event::Text("0.25".into()),
            ],
        );
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.material_draft.is_none(), "{}", editor.error);
        assert_eq!(editor.session.scene().materials["shared"].tint[0], 0.25);
        assert_eq!(editor.session.undo_len(), 1);
        assert_eq!(
            editor.session.scene().objects[1].material.as_deref(),
            Some("shared")
        );
        let label_point = |shapes: &[egui::epaint::ClippedShape], label: &str| {
            let matches = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == label => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(matches.len(), 1, "Expected one visible {label} widget");
            matches[0]
        };
        let click = |point, pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let replace = |text: &str| {
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                },
                egui::Event::Text(text.into()),
            ]
        };
        let unchanged = editor.session.snapshot_bytes().unwrap();
        // Selecting the new mode opens one raw form, never a separate commit.
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Diffuse");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Nonmetallic");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert_eq!(
            editor.material_draft.as_ref().unwrap().response_mode,
            MaterialResponseMode::Dielectric
        );
        let shapes = frame(&mut editor, vec![]);
        assert!(shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Surface shading needs Surface HDR in Scene display.")));
        editor.shader_builtin = false;
        let shapes = frame(&mut editor, vec![]);
        assert!(shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.contains("Custom shader controls appearance; surface shading is unsupported"))));
        editor.shader_builtin = true;
        let roughness = egui::Id::unique("material_roughness");
        let point = ctx.read_response(roughness).unwrap().rect.center();
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(point), click(point, true)],
        );
        frame(&mut editor, vec![click(point, false)]);
        frame(&mut editor, replace("0.07999999821186066"));
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert_eq!(
            editor.material_draft.as_ref().unwrap().roughness,
            "0.07999999821186066"
        );
        assert!(editor.error.contains("0.08 to 1"));
        assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
        frame(&mut editor, vec![key(egui::Key::Tab)]);
        frame(&mut editor, vec![]);
        assert!(ctx
            .memory(|memory| memory.focused())
            .is_some_and(|id| id != roughness));
        // Inactive values remain raw while changing modes. Keyboard selection
        // closes the popup without submitting the grouped material draft.
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Nonmetallic");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        let shapes = frame(&mut editor, vec![]);
        let diffuse = label_point(&shapes, "Diffuse");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(diffuse), click(diffuse, true)],
        );
        frame(&mut editor, vec![click(diffuse, false)]);
        let draft = editor.material_draft.as_ref().unwrap();
        assert_eq!(draft.response_mode, MaterialResponseMode::Diffuse);
        assert_eq!(draft.roughness, "0.07999999821186066");
        assert_eq!(
            draft.parse().unwrap().response,
            MaterialResponse::LegacyDiffuse
        );
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Diffuse");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        frame(&mut editor, vec![]);
        // Opening by pointer leaves no menu entry focused. The first Down
        // focuses Diffuse; the next moves to Nonmetallic.
        frame(&mut editor, vec![key(egui::Key::ArrowDown)]);
        frame(&mut editor, vec![key(egui::Key::ArrowDown)]);
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(
            !egui::Popup::is_any_open(&ctx),
            "Keyboard selection closes the shading popup"
        );
        assert_eq!(
            editor.material_draft.as_ref().unwrap().response_mode,
            MaterialResponseMode::Dielectric
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
        // Choosing the current entry still closes the popup and returns focus
        // to its trigger, so Tab continues to Roughness after idle frames.
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Nonmetallic");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![key(egui::Key::ArrowDown)]);
        frame(&mut editor, vec![key(egui::Key::ArrowDown)]);
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert_eq!(
            editor.material_draft.as_ref().unwrap().response_mode,
            MaterialResponseMode::Dielectric
        );
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![]);
        assert!(ctx.memory(|memory| memory.focused()).is_some());
        frame(&mut editor, vec![key(egui::Key::Tab)]);
        frame(&mut editor, vec![]);
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(roughness));
        assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
        // An outside click owns its new field; closing a dropdown must not
        // request the trigger's focus over that explicit pointer choice.
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Nonmetallic");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        let name = egui::Id::unique("material_name");
        let point = ctx.read_response(name).unwrap().rect.center();
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(point), click(point, true)],
        );
        frame(&mut editor, vec![click(point, false)]);
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![]);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(name));
        assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Nonmetallic");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert!(
            editor.material_draft.is_some(),
            "First Escape belongs to the popup"
        );
        // Native frames can pass between keys: last-frame focus alone must
        // not be the reason the raw form still owns the following Escape.
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(
            editor.material_draft.is_none(),
            "Next Escape cancels the raw form"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
        // One valid Enter commits mode and minimum roughness together; shared
        // users retain their identity, and one Undo restores the complete value.
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Diffuse");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        let shapes = frame(&mut editor, vec![]);
        let mode = label_point(&shapes, "Nonmetallic");
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(mode), click(mode, true)],
        );
        frame(&mut editor, vec![click(mode, false)]);
        frame(&mut editor, vec![]);
        let point = ctx.read_response(roughness).unwrap().rect.center();
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(point), click(point, true)],
        );
        frame(&mut editor, vec![click(point, false)]);
        frame(&mut editor, replace("0.08"));
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.material_draft.is_none(), "{}", editor.error);
        assert_eq!(
            editor.session.scene().materials["shared"].response,
            MaterialResponse::Dielectric { roughness: 0.08 }
        );
        assert_eq!(editor.session.undo_len(), 2);
        assert!(
            editor.session.scene().display.is_legacy(),
            "Material edits never change scene display implicitly"
        );
        editor
            .session
            .undo(editor.session.scene().revision)
            .unwrap();
        assert!(editor.session.scene().materials["shared"]
            .response
            .is_legacy());
        editor
            .session
            .redo(editor.session.scene().revision)
            .unwrap();
        editor.sync_draft();
        let count = editor.session.undo_len();
        editor.material_draft =
            MaterialDraft::new(editor.session.scene(), "left", MaterialIntent::MakeUnique);
        assert!(editor.apply(), "{}", editor.error);
        let unique = editor.session.scene().objects[0].material.clone().unwrap();
        assert_ne!(unique, "shared");
        assert_eq!(editor.session.undo_len(), count + 1);
        assert_eq!(
            editor.session.scene().materials[&unique].tint,
            [0.25, 1.0, 1.0]
        );
        assert_eq!(
            editor.session.scene().materials[&unique].response,
            MaterialResponse::Dielectric { roughness: 0.08 }
        );
        assert_eq!(
            editor.session.scene().objects[1].material.as_deref(),
            Some("shared")
        );
        editor
            .session
            .undo(editor.session.scene().revision)
            .unwrap();
        assert_eq!(
            editor.session.scene().objects[0].material.as_deref(),
            Some("shared")
        );
        assert!(!editor.session.scene().materials.contains_key(&unique));
        assert_eq!(editor.preview, preview);
        // Ordinary texture drops stage the accepted role without applying raw
        // fields or promoting shading/display. Repair and Apply remain grouped.
        let directory = tempfile::tempdir().unwrap();
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[128, 0, 0])
                .unwrap();
        }
        std::fs::write(directory.path().join("pattern.png"), png).unwrap();
        let mut store = ruster_assets::AssetStore::open(directory.path()).unwrap();
        store
            .import_texture("pattern", "pattern.png", TextureRole::Roughness)
            .unwrap();
        let mesh = std::sync::Arc::new(
            ruster_assets::parse_obj(
                b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nf 1/1 2/2 3/3\n",
            )
            .unwrap(),
        );
        let mut scene = Scene::demo();
        scene
            .materials
            .insert("shared".into(), OpaqueMaterial::default());
        for object in scene.objects.iter_mut().take(2) {
            object.mesh = Some("mesh".into());
            object.material = Some("shared".into());
        }
        let mut mapped = SceneEditor::new(ruster_scene::Session::new(scene).unwrap());
        mapped.resolved.insert("mesh".into(), mesh);
        mapped.resolved_textures = store.texture_snapshots();
        mapped.assets = Some(store);
        mapped.select_object(Some("left".into()), false);
        let mut raw =
            MaterialDraft::new(mapped.session.scene(), "left", MaterialIntent::EditShared).unwrap();
        raw.roughness = "unfinished".into();
        raw.name = "Patterned".into();
        mapped.material_draft = Some(raw);
        let before = mapped.session.snapshot_bytes().unwrap();
        let payload = mapped.asset_payload("pattern").unwrap();
        mapped.queued_drop = Some(("left".into(), payload));
        mapped.process_content_intents();
        let raw = mapped.material_draft.as_ref().unwrap();
        assert_eq!(raw.roughness, "unfinished");
        assert_eq!(raw.roughness_texture.as_deref(), Some("pattern"));
        assert_eq!(raw.response_mode, MaterialResponseMode::Diffuse);
        assert!(mapped.session.scene().display.is_legacy());
        assert_eq!(mapped.session.snapshot_bytes().unwrap(), before);
        mapped.material_draft.as_mut().unwrap().response_mode = MaterialResponseMode::Dielectric;
        assert!(!mapped.apply_material_draft());
        assert_eq!(
            mapped.material_draft.as_ref().unwrap().roughness,
            "unfinished"
        );
        mapped.material_draft.as_mut().unwrap().roughness = "0.5".into();
        assert!(mapped.apply_material_draft(), "{}", mapped.error);
        assert_eq!(mapped.session.undo_len(), 1);
        assert_eq!(
            mapped.session.scene().materials["shared"]
                .roughness_texture
                .as_deref(),
            Some("pattern")
        );
        assert_eq!(
            mapped.session.scene().objects[1].material.as_deref(),
            Some("shared")
        );
        let shapes = frame(&mut mapped, vec![]);
        let point = label_point(&shapes, "pattern.png");
        frame(
            &mut mapped,
            vec![egui::Event::PointerMoved(point), click(point, true)],
        );
        frame(&mut mapped, vec![click(point, false)]);
        let shapes = frame(&mut mapped, vec![]);
        let clear = label_point(&shapes, "None (clear)");
        frame(
            &mut mapped,
            vec![egui::Event::PointerMoved(clear), click(clear, true)],
        );
        frame(&mut mapped, vec![click(clear, false)]);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert!(mapped
            .material_draft
            .as_ref()
            .unwrap()
            .roughness_texture
            .is_none());
        assert_eq!(
            mapped.session.undo_len(),
            1,
            "Clear waits for grouped Apply"
        );
        assert!(mapped.apply_material_draft(), "{}", mapped.error);
        assert_eq!(mapped.session.undo_len(), 2);
        mapped
            .session
            .undo(mapped.session.scene().revision)
            .unwrap();
        mapped.sync_draft();
        assert_eq!(
            mapped.session.scene().materials["shared"]
                .roughness_texture
                .as_deref(),
            Some("pattern")
        );
        mapped.material_draft =
            MaterialDraft::new(mapped.session.scene(), "left", MaterialIntent::MakeUnique);
        assert!(mapped.apply_material_draft(), "{}", mapped.error);
        let unique = mapped.session.scene().objects[0].material.as_ref().unwrap();
        assert_ne!(unique, "shared");
        assert_eq!(
            mapped.session.scene().materials[unique]
                .roughness_texture
                .as_deref(),
            Some("pattern")
        );
        // A mapped material's longer scalar label must fit the actual minimum
        // inspector panel, rather than pushing a right panel under its clip.
        let compact = egui::Context::default();
        crate::editor_theme::configure(&compact);
        for _ in 0..2 {
            let mut central_right = 0.0;
            let mut output = compact.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 480.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::Panel::right("compact_mapped_material")
                        .resizable(false)
                        .exact_size(240.0)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical()
                                .show(ui, |ui| mapped.material_inspector_ui(ui));
                        });
                    central_right = egui::CentralPanel::default()
                        .show(ui, |_| {})
                        .response
                        .rect
                        .right();
                },
            );
            output.textures_delta.clear();
            let label = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.job.text == "Roughness multiplier" => {
                        Some((
                            shape.clip_rect,
                            egui::Rect::from_min_size(text.pos, text.galley.size()),
                        ))
                    }
                    _ => None,
                })
                .expect("Mapped scalar label must be drawn");
            assert!(
                label.0.contains_rect(label.1),
                "Mapped scalar label is clipped: {label:?}"
            );
            assert!(
                central_right <= label.1.left(),
                "Central panel covers mapped scalar label: {central_right} vs {label:?}"
            );
        }
    }
    // Exercise the production frame capture path, then restored text/authority
    // and pre-adoption rejection. Generic history/migration is covered by Scene.
    #[test]
    fn material_production_checkpoint_keeps_invalid_text_and_rejects_compound_or_old_labels() {
        let directory = tempfile::tempdir().unwrap();
        let store = crate::recovery::Store::open(directory.path().join("recovery")).unwrap();
        let mut source = editor();
        source.recovery = crate::recovery_ui::RecoveryUi::at(store.clone());
        let mut draft =
            MaterialDraft::new(source.session.scene(), "left", MaterialIntent::EditShared).unwrap();
        draft.tint[1] = "0.5".into();
        draft.response_mode = MaterialResponseMode::Dielectric;
        draft.roughness = "unfinished roughness".into();
        draft.roughness_texture = Some("pending-map".into());
        source.material_draft = Some(draft);
        source.draft_generation = 1;
        let exact = source.material_draft.clone();
        let authored = source.session.snapshot_bytes().unwrap();
        source.recovery.observe(
            source.session.scene().revision,
            source.manipulation.draft_generation(),
            source
                .draft_generation
                .wrapping_add(source.behavior.generation()),
            true,
            std::time::Instant::now() - std::time::Duration::from_secs(2),
        );
        assert!(source.pending_recovery_drafts());
        assert!(
            source.recovery.due(std::time::Instant::now()),
            "production checkpoint should be due"
        );
        source.recovery_frame(&egui::Context::default());
        source.wait_recovery();
        assert!(
            store.scan().unwrap().is_empty(),
            "live journals must not be offered for recovery"
        );
        let warning = source.recovery.warning.clone();
        // Worker Drop intentionally does not block the UI. End and join its
        // actual handle before treating this durable checkpoint as orphaned.
        source.recovery.disconnect_wait();
        drop(source);
        let candidates = store.scan().unwrap();
        assert_eq!(candidates.len(), 1, "{warning}");
        let id = candidates[0].id.clone();
        let (_journal, snapshot) = store.claim(&id).unwrap();
        let payload = snapshot.drafts().unwrap();
        assert_eq!(payload["version"], 11);
        assert_eq!(payload["material"]["roughness"], "unfinished roughness");
        assert_eq!(payload["material"]["response_mode"], "dielectric");
        assert_eq!(payload["material"]["roughness_texture"], "pending-map");
        let mut explicit_clear = payload.clone();
        explicit_clear["material"]["roughness_texture"] = Value::Null;
        let mut cleared = SceneEditor::new(snapshot.restore().unwrap());
        cleared.restore_recovery_drafts(explicit_clear).unwrap();
        assert_eq!(cleared.material_draft.unwrap().roughness_texture, None);
        let mut old7 = payload.clone();
        old7["version"] = json!(7);
        old7["material"]
            .as_object_mut()
            .unwrap()
            .remove("roughness_texture");
        let mut migrated7 = SceneEditor::new(snapshot.restore().unwrap());
        migrated7.restore_recovery_drafts(old7.clone()).unwrap();
        assert_eq!(
            migrated7.material_draft.as_ref().unwrap().roughness_texture,
            None
        );
        assert_eq!(
            migrated7.material_draft.as_ref().unwrap().roughness,
            "unfinished roughness"
        );
        for version in 1..=7 {
            for pointer in [
                "/material/roughness_texture",
                "/material/original_material/roughness_texture",
            ] {
                let mut bad = old7.clone();
                bad["version"] = json!(version);
                let (parent, key) = pointer.rsplit_once('/').unwrap();
                bad.pointer_mut(parent)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .insert(key.into(), Value::Null);
                let mut target = SceneEditor::new(snapshot.restore().unwrap());
                assert!(target
                    .restore_recovery_drafts(bad)
                    .unwrap_err()
                    .contains("version8"));
                assert!(target.material_draft.is_none());
            }
        }
        // Genuine Recovery6 has neither raw surface controls nor a response
        // key in its typed baseline. Only that old shape gains defaults.
        let mut old = payload.clone();
        old["version"] = json!(6);
        old["material"]
            .as_object_mut()
            .unwrap()
            .remove("roughness_texture");
        old["material"]
            .as_object_mut()
            .unwrap()
            .remove("response_mode");
        old["material"].as_object_mut().unwrap().remove("roughness");
        old["material"]["original_material"]
            .as_object_mut()
            .unwrap()
            .remove("response");
        let mut migrated = SceneEditor::new(snapshot.restore().unwrap());
        migrated.restore_recovery_drafts(old.clone()).unwrap();
        let migrated = migrated.material_draft.unwrap();
        assert_eq!(migrated.response_mode, MaterialResponseMode::Diffuse);
        assert_eq!(migrated.roughness, "0.5");
        assert_eq!(migrated.roughness_texture, None);
        assert_eq!(migrated.tint[1], "0.5");
        for (pointer, value) in [
            ("/material/response_mode", serde_json::Value::Null),
            ("/material/roughness", json!("0.5")),
            (
                "/material/original_material/response",
                serde_json::Value::Null,
            ),
            (
                "/material/original_material/response",
                json!({"type":"legacy_diffuse"}),
            ),
        ] {
            let mut bad = old.clone();
            let (parent, key) = pointer.rsplit_once('/').unwrap();
            bad.pointer_mut(parent)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(key.into(), value);
            let mut target = SceneEditor::new(snapshot.restore().unwrap());
            assert!(target.restore_recovery_drafts(bad).is_err(), "{pointer}");
            assert!(target.material_draft.is_none());
        }
        let mut restored = SceneEditor::new(snapshot.restore().unwrap());
        restored.restore_recovery_drafts(payload.clone()).unwrap();
        assert_eq!(restored.material_draft, exact);
        assert!(restored.session.path().is_none());
        assert_eq!(restored.session.snapshot_bytes().unwrap(), authored);
        assert!(!restored.apply());
        assert_eq!(restored.material_draft, exact);
        let args = json!({"expected_session_token":"scene","expected_revision":"0","expected_asset_revision":"0","material_id":"shared","material":{"tint":[1,1,1],"base_color_texture":null,"filter":"linear","address":"repeat"}});
        assert_eq!(
            restored
                .material_command("scene", "scene_material_set", &args)
                .unwrap_err()
                .0,
            "drafts_pending"
        );
        for case in [
            "old-label",
            "old-null",
            "stale",
            "compound",
            "oversized",
            "missing-mode",
            "missing-roughness",
            "missing-map",
            "invalid-map",
        ] {
            let mut bad = payload.clone();
            match case {
                "old-label" => bad["version"] = json!(3),
                "old-null" => {
                    bad["version"] = json!(3);
                    bad["material"] = json!(null);
                }
                "stale" => bad["material"]["revision"] = json!(1),
                "compound" => bad["manipulation"]["delta"][0] = json!("1"),
                "missing-mode" => {
                    bad["material"]
                        .as_object_mut()
                        .unwrap()
                        .remove("response_mode");
                }
                "missing-roughness" => {
                    bad["material"].as_object_mut().unwrap().remove("roughness");
                }
                "missing-map" => {
                    bad["material"]
                        .as_object_mut()
                        .unwrap()
                        .remove("roughness_texture");
                }
                "invalid-map" => bad["material"]["roughness_texture"] = json!("bad/map"),
                _ => bad["material"]["roughness"] = json!("x".repeat(257)),
            }
            assert!(restored.restore_recovery_drafts(bad).is_err(), "{case}");
            assert_eq!(
                restored.session.snapshot_bytes().unwrap(),
                authored,
                "{case}"
            );
            assert_eq!(restored.material_draft, exact, "{case}");
        }
        restored.material_draft.as_mut().unwrap().roughness = "0.08".into();
        // Repair the pending, unavailable selection explicitly before Apply.
        restored.material_draft.as_mut().unwrap().roughness_texture = None;
        restored.request_close();
        assert!(restored.close_pending);
        assert!(
            restored.apply(),
            "Save-before-close must accept a valid material draft: {}",
            restored.error
        );
        assert_eq!(restored.session.scene().materials["shared"].tint[1], 0.5);
        assert_eq!(
            restored.session.scene().materials["shared"].response,
            MaterialResponse::Dielectric { roughness: 0.08 }
        );
        assert_eq!(restored.session.undo_len(), 1);
    }
}
