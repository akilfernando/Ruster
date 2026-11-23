//! Scene-owned material drafts and the shared native/AI commit path.

use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text, ui_kit};
use eframe::egui;
use rot_assets::{AssetKind, AssetStatus, AssetStore, TextureRole};
use rot_scene::component_scene::{Component, ComponentScene, Geometry, ObjectNode};
use rot_scene::{MaterialResponse, OpaqueMaterial, SceneError};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
pub(super) struct Usage {
    revision: Option<u64>,
    objects: HashMap<String, Vec<usize>>,
    prefab_revision: Option<u64>,
    prefab_objects: HashSet<String>,
    prefab_materials: HashSet<String>,
}

impl Usage {
    fn update(&mut self, revision: u64, scene: &ComponentScene) {
        if self.revision == Some(revision) {
            return;
        }
        self.revision = Some(revision);
        self.objects.clear();
        for (index, object) in scene.objects.iter().enumerate() {
            for component in &object.components {
                if let Component::MeshRenderer {
                    material: Some(id), ..
                } = &component.kind
                {
                    let users = match self.objects.get_mut(id) {
                        Some(users) => users,
                        None => self.objects.entry(id.clone()).or_default(),
                    };
                    if users.last() != Some(&index) {
                        users.push(index);
                    }
                }
            }
        }
    }

    fn update_prefabs(&mut self, revision: u64, scene: &ComponentScene) {
        if self.prefab_revision == Some(revision) {
            return;
        }
        self.prefab_revision = Some(revision);
        self.prefab_objects.clear();
        self.prefab_materials.clear();
        for instance in scene.prefabs.values() {
            self.prefab_objects
                .extend(instance.parts.values().map(|part| part.object.clone()));
            self.prefab_materials
                .extend(instance.materials.values().cloned());
        }
    }

    fn users(&self, id: &str) -> &[usize] {
        self.objects.get(id).map_or(&[], Vec::as_slice)
    }
}

pub(in crate::scene_editor) enum Intent {
    Create,
    Edit(String),
    MakeUnique(String),
}

pub(in crate::scene_editor) struct Request {
    pub component: String,
    pub intent: Intent,
}

pub(super) struct Draft {
    revision: u64,
    asset_revision: u64,
    object: String,
    component: String,
    intent: Intent,
    value: OpaqueMaterial,
    affected: usize,
    copy_scope: Option<String>,
    base_colors: BTreeMap<String, String>,
    roughness_maps: BTreeMap<String, String>,
    opened_frame: Option<u64>,
    error: Option<String>,
}

fn textures(material: &OpaqueMaterial) -> [(Option<&str>, TextureRole); 2] {
    [
        (
            material.base_color_texture.as_deref(),
            TextureRole::BaseColor,
        ),
        (
            material.roughness_texture.as_deref(),
            TextureRole::Roughness,
        ),
    ]
}

fn standalone_texture(info: &rot_assets::AssetInfo, role: TextureRole) -> bool {
    info.kind == AssetKind::Texture
        && info.texture_role == Some(role)
        && info.status == AssetStatus::Ready
}

fn good_texture(assets: Option<&AssetStore>, id: &str, role: TextureRole) -> bool {
    assets.is_some_and(|store| {
        store
            .texture(id)
            .is_some_and(|texture| texture.role == role)
            && store
                .asset_info(id)
                .is_none_or(|info| standalone_texture(info, role))
    })
}

fn ready_texture_choices(
    assets: Option<&AssetStore>,
    role: TextureRole,
) -> BTreeMap<String, String> {
    let Some(assets) = assets else {
        return BTreeMap::new();
    };
    let mut textures = assets
        .infos()
        .filter(|info| standalone_texture(info, role) && assets.texture(&info.id).is_some())
        .map(|info| (info.id.clone(), file_name(&info.source).to_owned()))
        .collect::<BTreeMap<_, _>>();
    let embedded = match role {
        TextureRole::BaseColor => Text::ComponentEmbeddedBaseColor,
        TextureRole::Roughness => Text::ComponentEmbeddedRoughness,
    };
    for model in assets.models() {
        for id in model
            .materials
            .values()
            .filter_map(|material| material.texture(role))
        {
            if assets.texture(id).is_some() {
                textures.entry(id.to_owned()).or_insert_with(|| {
                    format!("{} · {}", file_name(&model.source), embedded.plain())
                });
            }
        }
    }
    textures
}

fn validate_texture(
    assets: Option<&AssetStore>,
    id: &str,
    role: TextureRole,
) -> Result<(), SceneError> {
    if good_texture(assets, id, role) {
        Ok(())
    } else {
        Err(SceneError::Invalid(format!(
            "Texture {id} is unavailable for this material slot."
        )))
    }
}

fn changed_bindings<'a>(
    previous: &'a ObjectNode,
    next: &'a ObjectNode,
) -> impl Iterator<Item = (&'a str, &'a Geometry, bool)> {
    next.components.iter().filter_map(|component| {
        let Component::MeshRenderer {
            geometry,
            material: Some(id),
            ..
        } = &component.kind
        else {
            return None;
        };
        let existing_geometry = previous.components.iter().find_map(|old| match &old.kind {
            Component::MeshRenderer {
                geometry,
                material: Some(old_id),
                ..
            } if old.id == component.id && old_id == id => Some(geometry),
            _ => None,
        });
        (existing_geometry != Some(geometry)).then_some((
            id.as_str(),
            geometry,
            existing_geometry.is_none(),
        ))
    })
}

impl ComponentWorkspace {
    fn ensure_material_usage(&mut self) {
        self.material_usage
            .update(self.session.revision(), self.session.scene());
    }

    pub(super) fn material_asset_revision(&self) -> u64 {
        self.assets.as_ref().map_or(0, AssetStore::revision)
    }

    fn ensure_material_prefabs(&mut self) {
        self.material_usage
            .update_prefabs(self.session.revision(), self.session.scene());
    }

    pub(super) fn material_controls(&mut self) -> bool {
        self.ensure_material_prefabs();
        self.selected_index.is_some_and(|index| {
            !self
                .material_usage
                .prefab_objects
                .contains(&self.session.scene().objects[index].id)
        })
    }

    fn has_uvs(&self, geometry: &Geometry) -> bool {
        match geometry {
            Geometry::Cube | Geometry::Primitive { .. } => true,
            Geometry::Imported { asset } => self
                .meshes
                .get(asset)
                .is_some_and(|mesh| mesh.has_complete_uvs()),
        }
    }

    fn validate_user(&self, object: &ObjectNode, geometry: &Geometry) -> Result<(), SceneError> {
        if self.has_uvs(geometry) {
            Ok(())
        } else {
            Err(SceneError::Invalid(format!(
                "Choose a mesh with UVs for {} before applying a textured material.",
                object.name
            )))
        }
    }

    #[cfg(unix)]
    pub(super) fn changes_textured_binding(
        &self,
        previous: &ObjectNode,
        next: &ObjectNode,
    ) -> bool {
        changed_bindings(previous, next).any(|(id, _, _)| {
            self.session
                .scene()
                .materials
                .get(id)
                .is_some_and(|material| {
                    textures(material)
                        .into_iter()
                        .any(|(texture, _)| texture.is_some())
                })
        })
    }

    /// Validate only bindings that change, so existing broken assets don't
    /// prevent unrelated edits or removal of the bad assignment.
    pub(super) fn validate_material_bindings(
        &self,
        previous: &ObjectNode,
        next: &ObjectNode,
    ) -> Result<(), SceneError> {
        for (id, geometry, new_assignment) in changed_bindings(previous, next) {
            let material = self
                .session
                .scene()
                .materials
                .get(id)
                .ok_or_else(|| SceneError::Invalid(format!("Material {id} does not exist.")))?;
            for (texture, role) in textures(material) {
                if let Some(texture) = texture {
                    if new_assignment {
                        validate_texture(self.assets.as_ref(), texture, role)?;
                    }
                    self.validate_user(next, geometry)?;
                }
            }
        }
        Ok(())
    }

    fn commit_material(
        &mut self,
        revision: u64,
        id: &str,
        value: OpaqueMaterial,
        assign: Option<(&str, &str)>,
        inherited_from: Option<&str>,
    ) -> Result<u64, SceneError> {
        self.ensure_material_prefabs();
        if self.material_usage.prefab_materials.contains(id) {
            return Err(SceneError::Invalid(
                Text::ComponentPrefabMaterialSource.plain().into(),
            ));
        }
        value.validate()?;
        if assign.is_none() {
            self.ensure_material_usage();
        }
        let scene = self.session.scene();
        let previous = if let Some(source) = inherited_from {
            let (owner, component) = assign.expect("copy has an exact destination");
            if scene.materials.contains_key(id) {
                return Err(SceneError::Invalid(
                    "Choose a new material ID to create and assign.".into(),
                ));
            }
            Some(bound_copy_source(scene, owner, component, source)?)
        } else {
            scene.materials.get(id)
        };
        let mut introduced = false;
        for ((texture, role), old) in textures(&value)
            .into_iter()
            .zip(previous.map_or([None; 2], |m| textures(m).map(|(id, _)| id)))
        {
            if let Some(texture) = texture.filter(|_| texture != old) {
                validate_texture(self.assets.as_ref(), texture, role)?;
                introduced = true;
            }
        }
        if introduced {
            if let Some((owner, component)) = assign {
                let (object, mesh) = material_target(scene, owner, Some(component))?;
                if let Component::MeshRenderer { geometry, .. } = &mesh.kind {
                    self.validate_user(object, geometry)?;
                }
            } else {
                for &index in self.material_usage.users(id) {
                    let object = &scene.objects[index];
                    for mesh in &object.components {
                        if let Component::MeshRenderer {
                            material: Some(bound),
                            geometry,
                            ..
                        } = &mesh.kind
                        {
                            if bound == id {
                                self.validate_user(object, geometry)?;
                            }
                        }
                    }
                }
            }
        }
        let next = if let (Some((owner, component)), Some(_)) = (assign, inherited_from) {
            self.session
                .set_unique_material(revision, id, value, owner, component)?
        } else {
            self.session.set_material(revision, id, value, assign)?
        };
        if next != revision {
            self.refresh();
        }
        Ok(next)
    }

    pub(super) fn begin_material(&mut self, request: Request) {
        if self.material_draft.is_some()
            || self.viewport_edit_active()
            || self.runtime.in_play_mode()
        {
            return;
        }
        if self.draft_dirty() {
            self.error = Some(Text::MaterialApplyBlocked.plain().into());
            return;
        }
        let Some(index) = self.selected_index else {
            return;
        };
        self.ensure_material_prefabs();
        if matches!(request.intent, Intent::Edit(_)) {
            self.ensure_material_usage();
        }
        let scene = self.session.scene();
        let selected = &scene.objects[index];
        let object = &selected.id;
        let Ok((_, renderer)) = material_target(scene, object, Some(&request.component)) else {
            return;
        };
        let (affected, value) = match &request.intent {
            Intent::Create => {
                if self.material_usage.prefab_objects.contains(object) {
                    self.error = Some(Text::ComponentPrefabMaterialSource.plain().into());
                    return;
                }
                (
                    1,
                    OpaqueMaterial {
                        name: Some(Text::MaterialNew.plain().into()),
                        response: MaterialResponse::dielectric(),
                        ..Default::default()
                    },
                )
            }
            Intent::MakeUnique(source) => {
                let Ok(value) = bound_copy_source(scene, object, &request.component, source) else {
                    self.error = Some(Text::MaterialTargetChanged.plain().into());
                    return;
                };
                let mut value = value.clone();
                value.name = Some(copy_display_name(scene, source, value.name.as_deref()));
                (1, value)
            }
            Intent::Edit(id) => {
                if !matches!(&renderer.kind, Component::MeshRenderer { material: Some(bound), .. } if bound == id)
                {
                    return;
                }
                if self.material_usage.prefab_materials.contains(id) {
                    self.error = Some(Text::ComponentPrefabMaterialSource.plain().into());
                    return;
                }
                let Some(value) = scene.materials.get(id) else {
                    return;
                };
                (self.material_usage.users(id).len(), value.clone())
            }
        };
        let copy_scope = matches!(request.intent, Intent::MakeUnique(_)).then(|| {
            let mut renderers = selected
                .components
                .iter()
                .filter(|component| matches!(component.kind, Component::MeshRenderer { .. }));
            let ordinal = (renderers.clone().take(2).count() == 2).then(|| {
                renderers
                    .position(|component| component.id == request.component)
                    .expect("selected renderer")
                    + 1
            });
            let heading = crate::scene_editor::component_inspector::mesh_renderer_heading(ordinal);
            format!("{heading} · {}", selected.name)
        });
        self.material_draft = Some(Draft {
            revision: self.session.revision(),
            asset_revision: self.material_asset_revision(),
            object: object.clone(),
            component: request.component,
            intent: request.intent,
            value,
            affected,
            copy_scope,
            base_colors: ready_texture_choices(self.assets.as_ref(), TextureRole::BaseColor),
            roughness_maps: ready_texture_choices(self.assets.as_ref(), TextureRole::Roughness),
            opened_frame: None,
            error: None,
        });
    }

    fn apply_material(&mut self, draft: &Draft) -> Result<(), SceneError> {
        if self.session.revision() != draft.revision
            || self.material_asset_revision() != draft.asset_revision
            || self.assets_loading()
        {
            return Err(SceneError::Invalid(
                Text::MaterialTargetChanged.plain().into(),
            ));
        }
        let scene = self.session.scene();
        let (id, assign) = match &draft.intent {
            Intent::Create | Intent::MakeUnique(_) => {
                let id = (1..)
                    .map(|i| format!("material-{i}"))
                    .find(|id| !scene.materials.contains_key(id))
                    .expect("bounded material count");
                (id, Some((draft.object.as_str(), draft.component.as_str())))
            }
            Intent::Edit(id) => (id.clone(), None),
        };
        let inherited_from = match &draft.intent {
            Intent::MakeUnique(source) => Some(source.as_str()),
            _ => None,
        };
        self.commit_material(
            draft.revision,
            &id,
            draft.value.clone(),
            assign,
            inherited_from,
        )?;
        Ok(())
    }

    pub(super) fn defer_material_close(&mut self, ctx: &egui::Context) -> bool {
        if !ctx.input(|input| input.viewport().close_requested()) {
            return false;
        }
        let Some(draft) = &mut self.material_draft else {
            return false;
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        draft.error = Some(Text::ComponentMaterialClosePending.plain().into());
        true
    }

    pub(super) fn show_material_draft(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.material_draft.take() else {
            return;
        };
        let popup_open = egui::Popup::is_any_open(ctx);
        let frame = ctx.cumulative_frame_nr();
        let enter = draft.opened_frame.is_some_and(|opened| opened != frame)
            && ctx.input(|input| input.modifiers.is_none() && input.key_pressed(egui::Key::Enter));
        let response = egui::Modal::new(egui::Id::unique("component_material"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                let width = if matches!(draft.intent, Intent::MakeUnique(_)) {
                    460.
                } else {
                    360.
                };
                ui.set_width(ui.available_width().min(width));
                ui.heading(match draft.intent {
                    Intent::Create => Text::MaterialNew.plain(),
                    Intent::Edit(_) => Text::MaterialEdit.plain(),
                    Intent::MakeUnique(_) => Text::MaterialMakeUnique.plain(),
                });
                if let Some(scope) = &draft.copy_scope {
                    ui.add(egui::Label::new(scope).truncate())
                        .on_hover_text(scope);
                } else {
                    ui.label(if draft.affected == 1 {
                        Text::MaterialAffectsOne.plain().to_owned()
                    } else {
                        Text::MaterialAffectsMany
                            .plain()
                            .replace("{count}", &draft.affected.to_string())
                    });
                }
                let mut name = draft.value.name.clone().unwrap_or_default();
                ui_kit::property_row_with_label(ui, Text::ComponentName.plain(), |ui, label| {
                    let edit = ui
                        .add(
                            egui::TextEdit::singleline(&mut name)
                                .desired_width(ui.available_width()),
                        )
                        .labelled_by(label);
                    if draft.opened_frame.is_none() {
                        edit.request_focus();
                        draft.opened_frame = Some(frame);
                    }
                });
                draft.value.name = (!name.is_empty()).then_some(name);
                ui_kit::property_row_with_label(ui, Text::ComponentTint.plain(), |ui, label| {
                    let mut color = draft.value.tint;
                    if ui
                        .color_edit_button_rgb(&mut color)
                        .labelled_by(label)
                        .changed()
                    {
                        draft.value.tint = color;
                    }
                });
                texture_choice(
                    ui,
                    Text::BaseColorSrgb,
                    &mut draft.value.base_color_texture,
                    &draft.base_colors,
                );
                ui_kit::property_row_with_label(
                    ui,
                    Text::ComponentMaterialResponse.plain(),
                    |ui, label| {
                        let mut legacy = draft.value.response.is_legacy();
                        egui::ComboBox::from_id_salt("surface_response")
                            .selected_text(if legacy {
                                Text::Diffuse.plain()
                            } else {
                                Text::Nonmetallic.plain()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut legacy, true, Text::Diffuse.plain());
                                ui.selectable_value(&mut legacy, false, Text::Nonmetallic.plain());
                            })
                            .response
                            .labelled_by(label);
                        if legacy != draft.value.response.is_legacy() {
                            draft.value.response = if legacy {
                                MaterialResponse::LegacyDiffuse
                            } else {
                                MaterialResponse::dielectric()
                            };
                        }
                    },
                );
                texture_choice(
                    ui,
                    Text::RoughnessMapLinear,
                    &mut draft.value.roughness_texture,
                    &draft.roughness_maps,
                );
                let mapped = draft.value.roughness_texture.is_some();
                if let MaterialResponse::Dielectric { roughness } = &mut draft.value.response {
                    let label = if mapped {
                        Text::RoughnessMultiplier
                    } else {
                        Text::Roughness
                    };
                    ui_kit::property_row_with_label(ui, label.plain(), |ui, label| {
                        ui.add(
                            egui::DragValue::new(roughness)
                                .range(0.08..=1.0)
                                .speed(0.01),
                        )
                        .labelled_by(label);
                    });
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                ui.horizontal(|ui| {
                    (
                        editor_theme::primary_button(ui, Text::ComponentApply.plain()).clicked(),
                        ui.button(Text::ComponentCancel.plain()).clicked(),
                    )
                })
                .inner
            });
        let (apply, cancel) = response.inner;
        let escape = response.is_top_modal
            && !response.any_popup_open
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape {
            return;
        }
        let enter = enter && !popup_open && !egui::Popup::is_any_open(ctx);
        if apply || enter {
            match self.apply_material(&draft) {
                Ok(()) => {
                    self.error = None;
                    return;
                }
                Err(error) => draft.error = Some(error.to_string()),
            }
        }
        self.material_draft = Some(draft);
    }
}

fn texture_choice(
    ui: &mut egui::Ui,
    text: Text,
    slot: &mut Option<String>,
    choices: &BTreeMap<String, String>,
) {
    ui_kit::property_row_with_label(ui, text.plain(), |ui, label| {
        let current = slot.clone();
        let selected = current
            .as_deref()
            .map_or(Text::ComponentNoTexture.plain(), |id| {
                choices.get(id).map_or(id, String::as_str)
            });
        let field = egui::ComboBox::from_id_salt(text.plain())
            .width(ui.available_width())
            .truncate()
            .selected_text(selected)
            .show_ui(ui, |ui| {
                ui.selectable_value(slot, None, Text::ComponentNoTexture.plain());
                for (id, name) in choices {
                    ui.selectable_value(slot, Some(id.clone()), name)
                        .on_hover_text(id);
                }
            })
            .response
            .labelled_by(label);
        if let Some(id) = &current {
            field.on_hover_text(id);
        }
    });
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn material_target<'a>(
    scene: &'a ComponentScene,
    object_id: &str,
    component_id: Option<&str>,
) -> Result<
    (
        &'a ObjectNode,
        &'a rot_scene::component_scene::ComponentInstance,
    ),
    SceneError,
> {
    let object = scene
        .objects
        .iter()
        .find(|object| object.id == object_id)
        .ok_or_else(|| SceneError::ObjectNotFound(object_id.to_owned()))?;
    let mut renderers = object.components.iter().filter(|component| {
        matches!(component.kind, Component::MeshRenderer { .. })
            && component_id.is_none_or(|id| id == component.id)
    });
    let mesh = renderers
        .next()
        .ok_or_else(|| SceneError::Invalid("Choose a Mesh Renderer.".into()))?;
    if renderers.next().is_some() {
        return Err(SceneError::Invalid(
            "Specify component_id for an object with several Mesh Renderers.".into(),
        ));
    }
    Ok((object, mesh))
}

fn bound_copy_source<'a>(
    scene: &'a ComponentScene,
    owner: &str,
    component: &str,
    source: &str,
) -> Result<&'a OpaqueMaterial, SceneError> {
    let (_, renderer) = material_target(scene, owner, Some(component))?;
    if !matches!(&renderer.kind, Component::MeshRenderer { material: Some(id), .. } if id == source)
    {
        return Err(SceneError::Invalid(
            "The Mesh Renderer uses a different material. Inspect its current assignment and try again.".into(),
        ));
    }
    scene
        .materials
        .get(source)
        .ok_or_else(|| SceneError::Invalid(format!("Material {source} does not exist.")))
}

fn copy_display_name(scene: &ComponentScene, source: &str, name: Option<&str>) -> String {
    let base = name.unwrap_or(source);
    (1..)
        .map(|index| {
            let suffix = if index == 1 {
                " Copy".into()
            } else {
                format!(" Copy {index}")
            };
            let end = base.floor_char_boundary(128 - suffix.len());
            format!("{}{suffix}", base[..end].trim_end())
        })
        .find(|candidate| {
            !scene
                .materials
                .values()
                .any(|material| material.name.as_ref() == Some(candidate))
        })
        .expect("bounded scene material names")
}

#[cfg(unix)]
impl ComponentWorkspace {
    pub(super) fn material_command(
        &mut self,
        token: &str,
        op: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)> {
        use crate::scene_editor::material_commands::{
            exact_keys, id, material, number, optional_name,
        };
        use serde_json::json;

        let fail = |code: &str, message: &str| (code.to_owned(), message.to_owned());
        let invalid = || fail("invalid_arguments", "Check the material fields.");
        let map = args.as_object().ok_or_else(invalid)?;
        if op == "material_inspect" {
            if !exact_keys(map, &[], &["offset", "limit"]) {
                return Err(invalid());
            }
            let offset = args.get("offset").map(number).transpose()?.unwrap_or(0);
            let limit = args.get("limit").map(number).transpose()?.unwrap_or(16);
            if offset > self.session.scene().materials.len() as u64 || !(1..=16).contains(&limit) {
                return Err(invalid());
            }
            self.ensure_material_usage();
            return Ok(self.inspect_materials(offset as usize, limit as usize));
        }

        let mut required = vec![
            "expected_session_token",
            "expected_revision",
            "expected_asset_revision",
        ];
        let optional = match op {
            "scene_material_set" => {
                required.extend(["material_id", "material"]);
                &["assign_object_id", "component_id"][..]
            }
            "scene_material_copy" => {
                required.extend([
                    "source_id",
                    "new_material_id",
                    "assign_object_id",
                    "component_id",
                ]);
                &["name"][..]
            }
            _ => {
                required.extend(["object_id", "material_id"]);
                &["component_id"][..]
            }
        };
        if !exact_keys(map, &required, optional) {
            return Err(invalid());
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(fail("stale_session", "Inspect the open scene again."));
        }
        let revision = number(&args["expected_revision"])?;
        if revision != self.session.revision() {
            return Err(fail("stale_revision", "Inspect the current state again."));
        }
        if self.ai_transition_pending() || self.authored_edit_pending() {
            return Err(fail(
                "drafts_pending",
                "Finish the pending editor change first.",
            ));
        }
        if self.runtime.in_play_mode() {
            return Err(fail("play_active", "Stop Play to edit materials."));
        }
        if self.assets_loading() {
            return Err(fail(
                "assets_loading",
                "Wait for the project assets to load.",
            ));
        }
        let asset_revision = self.material_asset_revision();
        if number(&args["expected_asset_revision"])? != asset_revision {
            return Err(fail(
                "stale_asset_revision",
                "Inspect the project assets again.",
            ));
        }
        let component_id = args.get("component_id").map(id).transpose()?;
        let assignment = args
            .get(
                if matches!(op, "scene_material_set" | "scene_material_copy") {
                    "assign_object_id"
                } else {
                    "object_id"
                },
            )
            .map(id)
            .transpose()?;
        if component_id.is_some() && assignment.is_none() {
            return Err(invalid());
        }
        let domain_error = |error: SceneError| {
            let code = match error {
                SceneError::ObjectNotFound(_) => "object_not_found",
                SceneError::RevisionOverflow => "revision_exhausted",
                SceneError::StaleRevision { .. } => "stale_revision",
                _ => "material_unavailable",
            };
            (code.to_owned(), error.to_string())
        };
        let scene = self.session.scene();
        if op == "scene_material_copy" {
            let owner = assignment.ok_or_else(invalid)?;
            let component = component_id.ok_or_else(invalid)?;
            let source = id(&args["source_id"])?;
            let destination = id(&args["new_material_id"])?;
            let mut unique = bound_copy_source(scene, owner, component, source)
                .map_err(domain_error)?
                .clone();
            unique.name = Some(
                optional_name(args.get("name"))?
                    .unwrap_or_else(|| copy_display_name(scene, source, unique.name.as_deref())),
            );
            let next = self
                .commit_material(
                    revision,
                    destination,
                    unique,
                    Some((owner, component)),
                    Some(source),
                )
                .map_err(domain_error)?;
            return Ok(json!({
                "source_id":source, "material_id":destination, "object_id":owner, "component_id":component,
                "asset_revision":asset_revision.to_string(), "changed":next != revision, "dirty":self.session.is_dirty(),
            }));
        }
        let target = assignment
            .map(|object| material_target(scene, object, component_id))
            .transpose()
            .map_err(domain_error)?;
        let exact = target.map(|(_, component)| component.id.clone());
        let material_id = (!args["material_id"].is_null())
            .then(|| id(&args["material_id"]))
            .transpose()?;
        let next = if op == "scene_material_set" {
            self.commit_material(
                revision,
                material_id.ok_or_else(invalid)?,
                material(&args["material"])?,
                assignment.zip(exact.as_deref()),
                None,
            )
            .map_err(domain_error)?
        } else {
            let (object, component) = target.expect("assignment requires an object");
            let mut edited = object.clone();
            let edited_mesh = edited
                .components
                .iter_mut()
                .find(|item| item.id == component.id)
                .expect("target exists");
            if let Component::MeshRenderer { material, .. } = &mut edited_mesh.kind {
                *material = material_id.map(str::to_owned);
            }
            self.validate_material_bindings(object, &edited)
                .map_err(domain_error)?;
            let next = self
                .session
                .put_object(revision, edited)
                .map_err(domain_error)?;
            if next != revision {
                self.refresh();
            }
            next
        };
        Ok(json!({
            "material_id":material_id, "object_id":assignment, "component_id":exact,
            "asset_revision":asset_revision.to_string(), "changed":next != revision, "dirty":self.session.is_dirty(),
        }))
    }

    fn inspect_materials(&self, offset: usize, limit: usize) -> serde_json::Value {
        use serde_json::json;
        let scene = self.session.scene();
        let inspect_texture = |id: &str, role| {
            let info = self.assets.as_ref().and_then(|store| store.asset_info(id));
            let model = info
                .is_none()
                .then(|| {
                    self.assets
                        .as_ref()
                        .and_then(|store| store.model_texture(id))
                })
                .flatten();
            let accepted = self
                .assets
                .as_ref()
                .is_some_and(|store| store.texture(id).is_some());
            let actual_role = info
                .and_then(|info| info.texture_role)
                .or_else(|| model.map(|(_, role)| role));
            let status = match info {
                Some(info) if info.kind != AssetKind::Texture => "wrong_kind",
                _ if (info.is_some() || model.is_some()) && actual_role != Some(role) => {
                    "wrong_role"
                }
                Some(info) => match info.status {
                    AssetStatus::Ready if accepted => "ready",
                    AssetStatus::Ready => "unavailable",
                    AssetStatus::Missing => "missing",
                    AssetStatus::Invalid => "invalid",
                },
                None if model.is_some() && accepted => "ready",
                None if model.is_some() => "unavailable",
                None if self.assets.is_none() => "catalog_unavailable",
                None => "unregistered",
            };
            json!({
                "asset_id":id, "status":status, "expected_role":role, "accepted":accepted,
                "texture_role":actual_role,
                "hash":info.and_then(|info|info.hash.as_ref()).or_else(|| model.map(|(model, _)| &model.hash)),
                "diagnostic":info.and_then(|info| info.diagnostic.as_deref()).map(|text| text.chars().take(256).collect::<String>()),
            })
        };
        let materials: Vec<_> = scene.materials.iter().skip(offset).take(limit).map(|(id, value)| {
            let users = self.material_usage.users(id);
            let mut invalid = 0;
            let mut invalid_ids = Vec::new();
            if textures(value).into_iter().any(|(slot, _)| slot.is_some()) {
                for &index in users {
                    let node = &scene.objects[index];
                    if node.components.iter().any(|component| matches!(&component.kind,
                        Component::MeshRenderer { material: Some(bound), geometry, .. }
                            if bound == id && !self.has_uvs(geometry)
                    )) {
                        invalid += 1;
                        if invalid_ids.len() < 4 { invalid_ids.push(&node.id); }
                    }
                }
            }
            json!({
                "material_id":id, "assigned_count":users.len(), "users_truncated":users.len()>4,
                "user_ids":users.iter().take(4).map(|&index| &scene.objects[index].id).collect::<Vec<_>>(),
                "material":{
                    "name":value.name,"tint":value.tint,"base_color_texture":value.base_color_texture,
                    "roughness_texture":value.roughness_texture,"filter":value.filter,"address":value.address,
                    "response":value.response,
                },
                "texture":value.base_color_texture.as_deref().map(|id|inspect_texture(id, TextureRole::BaseColor)),
                "roughness_texture":value.roughness_texture.as_deref().map(|id|inspect_texture(id, TextureRole::Roughness)),
                "uv":{"invalid_user_count":invalid,"object_ids":invalid_ids,"truncated":invalid>4},
            })
        }).collect();
        let count = scene.materials.len();
        let mut result = json!({
            "scene_id":scene.id, "display":scene.display, "catalog_available":self.assets.is_some(),
            "asset_revision":(!self.assets_loading()).then(||self.material_asset_revision().to_string()),
            "materials":materials, "material_count":count, "next_offset":(offset+limit<count).then(||(offset+limit).to_string()),
            "dirty":self.session.is_dirty(), "drafts_pending":self.authored_edit_pending(),
        });
        if self.display_draft.is_some() {
            result["pending_display"] = json!(true);
        }
        if let Some(draft) = &self.material_draft {
            let intent = match draft.intent {
                Intent::Create => "create",
                Intent::Edit(_) => "edit_shared",
                Intent::MakeUnique(_) => "make_unique",
            };
            let mut pending = json!({
                "intent":intent, "object_id":draft.object, "component_id":draft.component,
                "affected_object_count":draft.affected,
            });
            match &draft.intent {
                Intent::Edit(id) => pending["material_id"] = json!(id),
                Intent::MakeUnique(id) => pending["source_material_id"] = json!(id),
                Intent::Create => {}
            }
            result["pending_material"] = pending;
        }
        result
    }
}

#[cfg(all(unix, test))]
mod tests {
    use super::*;
    use rot_scene::component_scene::{ComponentInstance, ComponentSession};
    use serde_json::json;

    fn write_base_color(path: &std::path::Path) {
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, 1, 1);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[255, 128, 0])
            .unwrap();
        std::fs::write(path, png).unwrap();
    }

    #[test]
    fn make_unique_binds_only_the_exact_renderer_and_inherits_its_slots() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(
            root.join("bare.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        write_base_color(&root.join("color.png"));
        let mut assets = AssetStore::open(root).unwrap();
        assets.import("bare", "bare.obj").unwrap();
        assets.import("ready-color", "color.png").unwrap();
        let assets_revision = assets.revision().to_string();

        let base = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let mut scene = ComponentScene::from_legacy(&base).unwrap();
        let owner = scene.objects[0].id.clone();
        let source = OpaqueMaterial {
            name: Some("Varnish".into()),
            tint: [0.21, 0.37, 0.59],
            base_color_texture: Some("missing-color".into()),
            roughness_texture: Some("ready-color".into()),
            filter: rot_scene::TextureFilter::Nearest,
            address: rot_scene::TextureAddress::Clamp,
            response: MaterialResponse::Dielectric { roughness: 0.73 },
        };
        scene.materials.insert("varnish".into(), source.clone());
        let Component::MeshRenderer { material, .. } = &mut scene.objects[0].components[0].kind
        else {
            unreachable!()
        };
        *material = Some("varnish".into());
        scene.objects[0].components.push(ComponentInstance {
            id: "target-renderer".into(),
            enabled: true,
            kind: Component::MeshRenderer {
                geometry: Geometry::Imported {
                    asset: "bare".into(),
                },
                tint: [0.2, 0.4, 0.6, 1.],
                material: Some("varnish".into()),
            },
        });
        let path = root.join("studio.scene.json");
        let initial = scene.clone();
        let mut editor = ComponentWorkspace::new(path, ComponentSession::new(scene).unwrap());
        editor.meshes = assets.mesh_snapshots();
        editor.textures = assets.texture_snapshots();
        editor.assets = Some(assets);
        editor.select(Some(owner.clone()));
        let mut args = json!({
            "expected_session_token":"scene","expected_revision":"0","expected_asset_revision":assets_revision,
            "source_id":"varnish","new_material_id":"copy","assign_object_id":owner,
            "component_id":"target-renderer",
        });
        for (field, value, error) in [
            ("source_id", "unbound", "material_unavailable"),
            ("new_material_id", "varnish", "material_unavailable"),
            ("expected_asset_revision", "0", "stale_asset_revision"),
        ] {
            let mut refused = args.clone();
            refused[field] = json!(value);
            let reply = editor.ai_command("scene", "scene_material_copy", &refused);
            assert_eq!(reply["error"]["code"], error, "{reply}");
            assert_eq!(editor.session.scene(), &initial);
        }
        editor.begin_material(Request {
            component: "target-renderer".into(),
            intent: Intent::MakeUnique("varnish".into()),
        });
        let inspected = editor.ai_command("scene", "material_inspect", &json!({}));
        assert_eq!(
            inspected["result"]["pending_material"],
            json!({
                "intent":"make_unique","source_material_id":"varnish","object_id":owner,
                "component_id":"target-renderer","affected_object_count":1
            })
        );
        assert_eq!(inspected["result"]["material_count"], 1);
        assert_eq!(
            editor.ai_command("scene", "scene_material_copy", &args)["error"]["code"],
            "drafts_pending"
        );
        let mut draft = editor.material_draft.take().unwrap();
        assert_eq!(
            draft.copy_scope.as_deref(),
            Some(format!("Mesh Renderer 2 · {}", initial.objects[0].name).as_str())
        );
        let original = draft.value.clone();
        draft.value.base_color_texture = Some("ready-color".into());
        assert!(
            editor.apply_material(&draft).is_err(),
            "new ready map still needs UVs"
        );
        draft.value.base_color_texture = Some("different-missing-color".into());
        assert!(
            editor.apply_material(&draft).is_err(),
            "a new missing map cannot inherit the exception"
        );
        assert_eq!(editor.session.revision(), 0);
        assert_eq!(editor.session.scene(), &initial);
        draft.value = original;
        editor.apply_material(&draft).unwrap();
        let copied = editor.session.scene();
        let mut renamed = source.clone();
        renamed.name = Some("Varnish Copy".into());
        assert_eq!(copied.materials["material-1"], renamed);
        assert_eq!(copied.materials["varnish"], source);
        assert_eq!(
            copied.objects[0].components[0],
            initial.objects[0].components[0]
        );
        let mut expected_target = initial.objects[0].components[1].clone();
        let Component::MeshRenderer { material, .. } = &mut expected_target.kind else {
            unreachable!()
        };
        *material = Some("material-1".into());
        assert_eq!(copied.objects[0].components[1], expected_target);
        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({
                "expected_session_token":"scene","expected_revision":"1",
            }),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(editor.session.scene(), &initial);

        args["expected_revision"] = json!("2");
        let reply = editor.ai_command("scene", "scene_material_copy", &args);
        assert_eq!(reply["result"]["material_id"], "copy", "{reply}");
        assert_eq!(editor.session.scene().materials["copy"], renamed);
        editor.session.undo(3).unwrap();
        assert_eq!(editor.session.scene(), &initial);
    }

    #[test]
    fn material_modal_enter_creates_one_exact_assignment_and_shares_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("studio.scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let mut scene = ComponentScene::from_legacy(&legacy).unwrap();
        let owner = scene.objects[0].id.clone();
        scene.objects[0].components.push(ComponentInstance {
            id: "second-renderer".into(),
            enabled: true,
            kind: Component::MeshRenderer {
                geometry: Geometry::Cube,
                tint: [0.1, 0.2, 0.3, 1.0],
                material: None,
            },
        });
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        write_base_color(&directory.path().join("color.png"));
        let mut assets = AssetStore::open(directory.path()).unwrap();
        assets.import("color", "color.png").unwrap();
        assets
            .import_texture("roughness", "color.png", TextureRole::Roughness)
            .unwrap();
        editor.textures = assets.texture_snapshots();
        editor.assets = Some(assets);
        editor.select(Some(owner.clone()));
        let request = || Request {
            component: "second-renderer".into(),
            intent: Intent::Create,
        };
        editor.begin_material(request());
        let tint = [0.24, 0.59, 0.85];
        let draft = editor.material_draft.as_mut().unwrap();
        assert_eq!(draft.base_colors.keys().collect::<Vec<_>>(), &["color"]);
        assert_eq!(
            draft.roughness_maps.keys().collect::<Vec<_>>(),
            &["roughness"]
        );
        let value = &mut draft.value;
        value.tint = tint;
        value.base_color_texture = Some("color".into());
        value.roughness_texture = Some("roughness".into());
        value.response = MaterialResponse::Dielectric { roughness: 0.63 };
        value.address = rot_scene::TextureAddress::Clamp;
        let ctx = egui::Context::default();
        let frame = |editor: &mut ComponentWorkspace, events| {
            ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960., 680.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| editor.show_material_draft(ui.ctx()),
            )
            .textures_delta
            .clear();
        };
        let key = |key, modifiers| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        };
        // Opening by Enter must leave the drafting modal open.
        frame(
            &mut editor,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert!(editor.material_draft.is_some());
        frame(
            &mut editor,
            vec![
                key(
                    egui::Key::A,
                    egui::Modifiers {
                        ctrl: true,
                        command: true,
                        ..Default::default()
                    },
                ),
                egui::Event::Text("Test Finish".to_owned()),
            ],
        );
        let pending = editor.ai_command("scene", "material_inspect", &json!({}));
        assert_eq!(pending["result"]["material_count"], 0);
        assert_eq!(pending["result"]["pending_material"]["intent"], "create");
        assert_eq!(
            pending["result"]["pending_material"]["component_id"],
            "second-renderer"
        );
        assert_eq!(
            pending["result"]["pending_material"]["affected_object_count"],
            1
        );
        assert!(!pending.to_string().contains("Test Finish"));
        let backdrop = egui::pos2(5.0, 5.0);
        frame(&mut editor, vec![egui::Event::PointerMoved(backdrop)]);
        for pressed in [true, false] {
            frame(
                &mut editor,
                vec![egui::Event::PointerButton {
                    pos: backdrop,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(editor.material_draft.is_some());
        assert!(!editor.save());
        let refusal = editor.ai_command(
            "scene",
            "scene_rename",
            &json!({
                "expected_session_token":"scene","expected_revision":"0",
                "object_id":owner,"name":"Ignored",
            }),
        );
        assert_eq!(refusal["error"]["code"], "drafts_pending");
        let mut close = egui::RawInput::default();
        close
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(close, |ui| {
            assert!(editor.defer_material_close(ui.ctx()));
            editor.show_material_draft(ui.ctx());
        });
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose)));
        output.textures_delta.clear();
        assert!(!editor.session.is_dirty());
        assert_eq!(
            editor
                .material_draft
                .as_ref()
                .unwrap()
                .value
                .name
                .as_deref(),
            Some("Test Finish")
        );
        frame(
            &mut editor,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        let committed = editor.session.scene();
        assert_eq!(editor.session.revision(), 1);
        assert_eq!(committed.materials["material-1"].tint, tint);
        assert_eq!(
            committed.materials["material-1"]
                .base_color_texture
                .as_deref(),
            Some("color")
        );
        assert_eq!(
            committed.materials["material-1"].name.as_deref(),
            Some("Test Finish")
        );
        assert_eq!(
            committed.objects[0].components[0],
            scene.objects[0].components[0]
        );
        assert!(matches!(
            &committed.objects[0].components[1].kind,
            Component::MeshRenderer { material: Some(id), tint, .. }
                if id == "material-1" && *tint == [0.1, 0.2, 0.3, 1.0]
        ));

        let mut without_map = committed.materials["material-1"].clone();
        assert_eq!(without_map.roughness_texture.as_deref(), Some("roughness"));
        without_map.roughness_texture = None;
        editor.begin_material(Request {
            component: "second-renderer".into(),
            intent: Intent::Edit("material-1".into()),
        });
        editor
            .material_draft
            .as_mut()
            .unwrap()
            .value
            .roughness_texture = None;
        frame(&mut editor, vec![]);
        frame(
            &mut editor,
            vec![key(egui::Key::Enter, egui::Modifiers::NONE)],
        );
        assert_eq!(editor.session.scene().materials["material-1"], without_map);
        assert_eq!(editor.session.revision(), 2);
        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({
                "expected_session_token":"scene","expected_revision":"2",
            }),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(
            editor.session.scene().materials["material-1"]
                .roughness_texture
                .as_deref(),
            Some("roughness")
        );

        editor.begin_material(Request {
            component: "second-renderer".into(),
            intent: Intent::Edit("material-1".into()),
        });
        frame(&mut editor, vec![]);
        frame(
            &mut editor,
            vec![key(egui::Key::Escape, egui::Modifiers::NONE)],
        );
        assert!(editor.material_draft.is_none());
        assert_eq!(editor.session.revision(), 3);
        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({
                "expected_session_token":"scene","expected_revision":"3",
            }),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(editor.session.scene(), &scene);
    }

    #[test]
    fn shared_material_rejects_new_bad_slots_and_keeps_old_broken_refs_repairable() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(
            root.join("bare.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        write_base_color(&root.join("color.png"));
        let mut store = AssetStore::open(root).unwrap();
        store.import("bare", "bare.obj").unwrap();
        store.import("color", "color.png").unwrap();
        let asset_rev = store.revision().to_string();

        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let mut scene = ComponentScene::from_legacy(&legacy).unwrap();
        let id = "shared";
        let previous = OpaqueMaterial {
            name: Some("Studio Finish".into()),
            roughness_texture: Some("missing-roughness".into()),
            response: MaterialResponse::Dielectric { roughness: 0.3 },
            address: rot_scene::TextureAddress::Clamp,
            filter: rot_scene::TextureFilter::Nearest,
            ..Default::default()
        };
        scene.materials.insert(id.into(), previous.clone());
        let second_id = scene.objects[1].id.clone();
        let second_component = scene.objects[1].components[0].id.clone();
        for object in &mut scene.objects[..2] {
            let Component::MeshRenderer { material, .. } = &mut object.components[0].kind else {
                panic!()
            };
            *material = Some(id.into());
        }
        let Component::MeshRenderer { geometry, .. } = &mut scene.objects[1].components[0].kind
        else {
            panic!()
        };
        *geometry = Geometry::Imported {
            asset: "bare".into(),
        };
        let Component::MeshRenderer { geometry, .. } = &mut scene.objects[0].components[0].kind
        else {
            panic!()
        };
        *geometry = Geometry::Primitive {
            shape: rot_scene::Primitive::Sphere,
        };
        let duplicate = ComponentInstance {
            id: "additional-renderer".into(),
            ..scene.objects[0].components[0].clone()
        };
        scene.objects[0].components.push(duplicate);
        scene.objects[1].enabled = false;
        let mut editor = ComponentWorkspace::new(
            root.join("studio.scene.json"),
            ComponentSession::new(scene.clone()).unwrap(),
        );
        editor.meshes = store.mesh_snapshots();
        editor.textures = store.texture_snapshots();
        editor.assets = Some(store);
        let replace = |editor: &mut ComponentWorkspace, value: &OpaqueMaterial| {
            editor.ai_command("scene", "scene_material_set", &json!({
                "expected_session_token":"scene","expected_revision":editor.session.revision().to_string(),
                "expected_asset_revision":asset_rev,"material_id":id,
                "material":{
                    "name":value.name,"tint":value.tint,"base_color_texture":value.base_color_texture,
                    "roughness_texture":value.roughness_texture,"filter":value.filter,"address":value.address,"response":value.response,
                },
            }))
        };
        let inspect = editor.ai_command("scene", "material_inspect", &json!({}));
        assert_eq!(inspect["result"]["materials"][0]["assigned_count"], 2);
        assert_eq!(
            inspect["result"]["materials"][0]["uv"]["invalid_user_count"],
            1
        );
        let mut edited = previous.clone();
        edited.base_color_texture = Some("color".into());
        let refusal = replace(&mut editor, &edited);
        assert_eq!(refusal["error"]["code"], "material_unavailable");
        assert_eq!(editor.session.scene(), &scene);
        edited.base_color_texture = None;
        edited.roughness_texture = Some("color".into());
        assert_eq!(
            replace(&mut editor, &edited)["error"]["code"],
            "material_unavailable"
        );

        edited
            .roughness_texture
            .clone_from(&previous.roughness_texture);
        edited.tint = [0.3, 0.4, 0.5];
        assert_eq!(replace(&mut editor, &edited)["ok"], true);
        assert_eq!(editor.session.scene().materials[id], edited);
        assert_eq!(editor.session.scene().objects, scene.objects);

        let ready_material = OpaqueMaterial {
            base_color_texture: Some("color".into()),
            ..Default::default()
        };
        assert_eq!(
            editor
                .commit_material(1, "ready", ready_material, None, None)
                .unwrap(),
            2
        );
        editor.select(Some(second_id.clone()));
        let Component::MeshRenderer { material, .. } =
            &mut editor.draft.as_mut().unwrap().components[0].kind
        else {
            panic!()
        };
        *material = Some("ready".into());
        assert!(!editor.apply());
        assert_eq!(editor.session.revision(), 2);
        assert_eq!(editor.session.scene().objects[1], scene.objects[1]);
        editor.rebuild_draft();
        let refused = editor.ai_command("scene", "scene_material_assign", &json!({
            "expected_session_token":"scene","expected_revision":"2","expected_asset_revision":asset_rev,
            "object_id":second_id,"component_id":second_component,"material_id":"ready",
        }));
        assert_eq!(refused["error"]["code"], "material_unavailable");
        let cleared = editor.ai_command("scene", "scene_material_assign", &json!({
            "expected_session_token":"scene","expected_revision":"2","expected_asset_revision":asset_rev,
            "object_id":second_id,"component_id":second_component,"material_id":null,
        }));
        assert_eq!(cleared["ok"], true, "{cleared}");

        let mut generic = json!({
            "expected_session_token":"scene","expected_revision":"3",
            "edits":[{
                "operation":"add","object_id":scene.objects[0].id,
                "component":{"id":"new-slot","type":"mesh_renderer","enabled":true,"geometry":{"type":"cube"},
                    "tint":[1.0,1.0,1.0,1.0],"material":"ready"},
            }],
        });
        let assign = |editor: &mut ComponentWorkspace, args: &serde_json::Value| {
            editor.ai_command("scene", "scene_components_edit", args)
        };
        assert_eq!(
            assign(&mut editor, &generic)["error"]["code"],
            "asset_revision_required"
        );
        generic["expected_asset_revision"] = json!("0");
        assert_eq!(
            assign(&mut editor, &generic)["error"]["code"],
            "stale_asset_revision"
        );
        generic["expected_asset_revision"] = json!(asset_rev);
        let created = assign(&mut editor, &generic);
        assert_eq!(created["ok"], true, "{created}");
        assert_eq!(editor.session.revision(), 4);
    }
}
