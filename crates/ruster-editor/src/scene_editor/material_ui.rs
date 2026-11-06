//! One recoverable material form, using the same validated commands as agents.
use super::SceneEditor;
use eframe::egui;
use ruster_scene::{OpaqueMaterial, Scene, TextureAddress, TextureFilter};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum MaterialIntent {
    EditShared,
    Create,
    MakeUnique,
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
    pub texture: Option<String>,
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
            texture: value.base_color_texture,
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
            && self.name.len() <= 512
            && self.name.chars().count() <= 128
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
        let value = OpaqueMaterial {
            name: (!self.name.is_empty()).then(|| self.name.clone()),
            tint: self.tint_value()?,
            base_color_texture: self.texture.clone(),
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
            && !self.manipulation.active()
            && !self.manipulation.precision_pending()
            && !self.manipulation.offset_pending()
            && self.draft == self.baseline_fields
            && !self.behavior.pending()
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
                if value.base_color_texture.is_some()
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
        match result {
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
        }
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
    pub(super) fn stage_material_texture(&mut self, texture_id: &str) -> Result<(), String> {
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
                        && info.status == ruster_assets::AssetStatus::Ready
                })
        });
        if !ready {
            return Err("Choose an imported PNG texture with a ready accepted source.".into());
        }
        if draft.texture.as_deref() == Some(texture_id) {
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
        let candidate = OpaqueMaterial {
            base_color_texture: Some(texture_id.into()),
            filter: draft.filter,
            address: draft.address,
            ..Default::default()
        };
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
        draft.texture = Some(texture_id.into());
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
            egui::ComboBox::from_id_salt("object_material_assignment")
                .selected_text(
                    choices
                        .iter()
                        .find(|(id, _)| Some(id) == assigned.as_ref())
                        .map(|(_, name)| name.as_str())
                        .unwrap_or("None"),
                )
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut assigned, None, "None");
                    for (id, name) in &choices {
                        ui.selectable_value(&mut assigned, Some(id.clone()), name)
                            .on_hover_text(id);
                    }
                });
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
            let before_texture = draft.texture.clone();
            let textures = self
                .assets
                .as_ref()
                .map(|store| {
                    store
                        .infos()
                        .filter(|info| info.kind == ruster_assets::AssetKind::Texture)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let texture_text = match &draft.texture {
                None => "None".to_string(),
                Some(id) => textures
                    .iter()
                    .find(|info| &info.id == id)
                    .map(|info| {
                        std::path::Path::new(&info.source)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .unwrap_or_else(|| format!("Missing: {id}")),
            };
            ui.label("Base color texture");
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
            let texture_response = egui::ComboBox::from_id_salt("material_texture")
                .selected_text(texture_text)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut draft.texture, None, "None");
                    for info in &textures {
                        let ready = uv_ready
                            && info.status == ruster_assets::AssetStatus::Ready
                            && self
                                .assets
                                .as_ref()
                                .is_some_and(|store| store.texture(&info.id).is_some());
                        ui.add_enabled_ui(ready, |ui| {
                            ui.selectable_value(
                                &mut draft.texture,
                                Some(info.id.clone()),
                                &info.source,
                            )
                            .on_hover_text(&info.id);
                        });
                    }
                });
            let response = texture_response.response;
            if response
                .dnd_hover_payload::<super::AssetDrag>()
                .is_some_and(|payload| self.resolved_textures.contains_key(&payload.asset_id))
            {
                ui.painter().rect_stroke(
                    response.rect,
                    3.0,
                    egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                    egui::StrokeKind::Inside,
                );
            }
            if editable {
                if let Some(payload) = response.dnd_release_payload::<super::AssetDrag>() {
                    dropped_texture = Some((*payload).clone());
                }
            }
            fields.push(response);
            changed |= draft.texture != before_texture;
            if let Some(id) = &draft.texture {
                let info = textures.iter().find(|info| &info.id == id);
                if !info.is_some_and(|info| info.status == ruster_assets::AssetStatus::Ready) {
                    let message = info.and_then(|info| info.diagnostic.as_deref()).unwrap_or(
                        "Texture is missing. Import or repair its source, or choose None.",
                    );
                    ui.colored_label(ui.visuals().warn_fg_color, message);
                }
            }
            let filter = draft.filter;
            let address = draft.address;
            ui.horizontal(|ui| {
                ui.label("Filter");
                let filter_response = egui::ComboBox::from_id_salt("material_filter")
                    .selected_text(match draft.filter {
                        TextureFilter::Linear => "Linear",
                        TextureFilter::Nearest => "Nearest",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut draft.filter, TextureFilter::Linear, "Linear");
                        ui.selectable_value(&mut draft.filter, TextureFilter::Nearest, "Nearest");
                    });
                fields.push(filter_response.response);
            });
            ui.horizontal(|ui| {
                ui.label("Wrap");
                let wrap_response = egui::ComboBox::from_id_salt("material_wrap")
                    .selected_text(match draft.address {
                        TextureAddress::Repeat => "Repeat",
                        TextureAddress::Clamp => "Clamp",
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut draft.address, TextureAddress::Repeat, "Repeat");
                        ui.selectable_value(&mut draft.address, TextureAddress::Clamp, "Clamp");
                    });
                fields.push(wrap_response.response);
            });
            changed |= filter != draft.filter || address != draft.address;
            for response in &fields {
                let owned = response.has_focus()
                    || response.lost_focus()
                    || ui.memory(|memory| memory.had_focus_last_frame(response.id));
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
        if let Some(payload) = dropped_texture {
            if let Err(error) = self
                .check_asset_drag(&payload)
                .and_then(|()| self.stage_material_texture(&payload.asset_id))
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
    use serde_json::json;
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
        draft.tint[1] = "unfinished green".into();
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
        assert_eq!(payload["version"], 6);
        assert_eq!(payload["material"]["tint"][1], "unfinished green");
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
        for case in ["old-label", "old-null", "stale", "compound", "oversized"] {
            let mut bad = payload.clone();
            match case {
                "old-label" => bad["version"] = json!(3),
                "old-null" => {
                    bad["version"] = json!(3);
                    bad["material"] = json!(null);
                }
                "stale" => bad["material"]["revision"] = json!(1),
                "compound" => bad["manipulation"]["delta"][0] = json!("1"),
                _ => bad["material"]["tint"][0] = json!("x".repeat(257)),
            }
            assert!(restored.restore_recovery_drafts(bad).is_err(), "{case}");
            assert_eq!(
                restored.session.snapshot_bytes().unwrap(),
                authored,
                "{case}"
            );
            assert_eq!(restored.material_draft, exact, "{case}");
        }
        restored.material_draft.as_mut().unwrap().tint[1] = "0.5".into();
        restored.request_close();
        assert!(restored.close_pending);
        assert!(
            restored.apply(),
            "Save-before-close must accept a valid material draft: {}",
            restored.error
        );
        assert_eq!(restored.session.scene().materials["shared"].tint[1], 0.5);
        assert_eq!(restored.session.undo_len(), 1);
    }
}
