//! Shared material transactions; neither human drafts nor preview are applied here.
use super::SceneEditor;
use ruster_assets::{AssetKind, AssetStatus};
use ruster_scene::{OpaqueMaterial, SceneError};
use serde_json::{json, Map, Value};

type CommandError = (String, String);

fn error(code: &str, message: impl Into<String>) -> CommandError {
    (code.into(), message.into())
}

fn exact_keys(map: &Map<String, Value>, required: &[&str], optional: &[&str]) -> bool {
    required.iter().all(|field| map.contains_key(*field))
        && map
            .keys()
            .all(|field| required.contains(&field.as_str()) || optional.contains(&field.as_str()))
}

fn number(value: &Value) -> Result<u64, CommandError> {
    let text = value
        .as_str()
        .filter(|text| {
            !text.is_empty()
                && (*text == "0" || !text.starts_with('0'))
                && text.bytes().all(|byte| byte.is_ascii_digit())
        })
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                "Expected a canonical decimal u64 string",
            )
        })?;
    text.parse()
        .map_err(|_| error("invalid_arguments", "Decimal value exceeds u64"))
}

fn id(value: &Value) -> Result<&str, CommandError> {
    let id = value
        .as_str()
        .ok_or_else(|| error("invalid_arguments", "Expected a stable ID string"))?;
    ruster_scene::validate_id(id)
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    Ok(id)
}

fn optional_name(value: Option<&Value>) -> Result<Option<String>, CommandError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(name)) => {
            let candidate = OpaqueMaterial {
                name: Some(name.clone()),
                ..OpaqueMaterial::default()
            };
            candidate
                .validate()
                .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
            Ok(candidate.name)
        }
        _ => Err(error(
            "invalid_arguments",
            "Display name must be string or null",
        )),
    }
}

fn material(value: &Value) -> Result<OpaqueMaterial, CommandError> {
    let map = value
        .as_object()
        .filter(|map| {
            exact_keys(
                map,
                &["tint", "base_color_texture", "filter", "address"],
                &["name", "response"],
            )
        })
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                "Expected the complete opaque material shape",
            )
        })?;
    let tint = map["tint"]
        .as_array()
        .filter(|tint| tint.len() == 3)
        .ok_or_else(|| error("invalid_arguments", "Tint requires three numeric channels"))?;
    let mut channels = [0.0; 3];
    for (channel, value) in channels.iter_mut().zip(tint) {
        // Validate the wire number before f32 narrowing can round into range.
        *channel = value
            .as_f64()
            .filter(|value| value.is_finite() && (0.0..=1.0).contains(value))
            .ok_or_else(|| error("invalid_arguments", "Tint must be finite and within0..1"))?
            as f32;
    }
    let candidate = OpaqueMaterial {
        name: optional_name(map.get("name"))?,
        response: map
            .get("response")
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()
            .map_err(|e| {
                error(
                    "invalid_arguments",
                    format!("Invalid material response: {e}"),
                )
            })?
            .unwrap_or_default(),
        tint: channels,
        base_color_texture: if map["base_color_texture"].is_null() {
            None
        } else {
            Some(id(&map["base_color_texture"])?.into())
        },
        filter: serde_json::from_value(map["filter"].clone())
            .map_err(|_| error("invalid_arguments", "Filter must be nearest or linear"))?,
        address: serde_json::from_value(map["address"].clone())
            .map_err(|_| error("invalid_arguments", "Address must be repeat or clamp"))?,
    };
    candidate
        .validate()
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    Ok(candidate)
}

fn scene_error(failure: SceneError) -> CommandError {
    let code = match failure {
        SceneError::ObjectNotFound(_) => "object_not_found",
        SceneError::RevisionOverflow => "revision_exhausted",
        SceneError::StaleRevision { .. } => "stale_revision",
        _ => "invalid_arguments",
    };
    error(code, failure.to_string())
}

impl SceneEditor {
    fn validate_ready_texture(&self, texture_id: &str) -> Result<(), String> {
        let store = self
            .assets
            .as_ref()
            .ok_or("Repair/open a valid project catalog before introducing a texture reference")?;
        let info = store
            .inspect()
            .into_iter()
            .find(|info| info.id == texture_id)
            .ok_or_else(|| format!("Texture {texture_id} is not registered"))?;
        if info.kind != AssetKind::Texture {
            return Err(format!("Asset {texture_id} is not a texture"));
        }
        if info.status != AssetStatus::Ready || store.texture(texture_id).is_none() {
            return Err(format!(
                "Texture {texture_id} requires accepted ready content; repair its source and Refresh"
            ));
        }
        Ok(())
    }

    fn validate_material_object_uvs(&self, object_id: &str) -> Result<(), String> {
        let object = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == object_id)
            .ok_or_else(|| format!("Object {object_id} does not exist"))?;
        if !object
            .mesh
            .as_ref()
            .and_then(|id| self.resolved.get(id))
            .is_some_and(|mesh| mesh.has_complete_uvs())
        {
            return Err(format!(
                "Object {object_id} requires an accepted mesh with complete UVs before using a texture"
            ));
        }
        Ok(())
    }

    /// Preserve repairable references during unrelated edits. New/changed
    /// texture references require accepted content and complete UVs for all users.
    pub(super) fn validate_material_candidate(
        &self,
        material_id: &str,
        candidate: &OpaqueMaterial,
    ) -> Result<(), String> {
        ruster_scene::validate_id(material_id).map_err(|failure| failure.to_string())?;
        candidate
            .validate()
            .map_err(|failure| failure.to_string())?;
        let previous = self.session.scene().materials.get(material_id);
        if previous.is_some_and(|value| value.base_color_texture == candidate.base_color_texture) {
            return Ok(());
        }
        if let Some(texture) = &candidate.base_color_texture {
            self.validate_ready_texture(texture)?;
            for object in &self.session.scene().objects {
                if object.material.as_deref() == Some(material_id) {
                    self.validate_material_object_uvs(&object.id)?;
                }
            }
        }
        Ok(())
    }

    /// Assignments that introduce a textured user must validate both resources;
    /// clearing and actual no-ops remain available while repairing old content.
    pub(super) fn validate_material_assignment(
        &self,
        object_id: &str,
        material: &Option<String>,
    ) -> Result<(), String> {
        ruster_scene::validate_id(object_id).map_err(|failure| failure.to_string())?;
        let object = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == object_id)
            .ok_or_else(|| format!("Object {object_id} does not exist"))?;
        if object.material == *material || material.is_none() {
            return Ok(());
        }
        let id = material.as_deref().unwrap();
        ruster_scene::validate_id(id).map_err(|failure| failure.to_string())?;
        let candidate = self
            .session
            .scene()
            .materials
            .get(id)
            .ok_or_else(|| format!("Material {id} does not exist"))?;
        if let Some(texture) = &candidate.base_color_texture {
            self.validate_ready_texture(texture)?;
            self.validate_material_object_uvs(object_id)?;
        }
        Ok(())
    }

    fn inspect_materials(&self, token: &str, args: &Value) -> Result<Value, CommandError> {
        let offset = args.get("offset").map(number).transpose()?.unwrap_or(0);
        let limit = args.get("limit").map(number).transpose()?.unwrap_or(16);
        if !(1..=16).contains(&limit) {
            return Err(error("invalid_arguments", "Limit must be1..16"));
        }
        let offset = usize::try_from(offset)
            .map_err(|_| error("invalid_arguments", "Offset exceeds range"))?;
        let count = self.session.scene().materials.len();
        if offset > count {
            return Err(error("invalid_arguments", "Offset exceeds material count"));
        }
        let limit = limit as usize;
        let materials: Vec<_> = self.session.scene().materials.iter().skip(offset).take(limit)
            .map(|(id, value)| {
                let users: Vec<_> = self.session.scene().objects.iter()
                    .filter(|object| object.material.as_deref() == Some(id.as_str())).collect();
                let invalid_uv_users: Vec<_> = users.iter().filter(|object| {
                    value.base_color_texture.is_some()
                        && self.validate_material_object_uvs(&object.id).is_err()
                }).collect();
                let texture = value.base_color_texture.as_ref().map(|texture_id| {
                    let info = self.assets.as_ref().and_then(|store| {
                        store.infos().find(|info| &info.id == texture_id)
                    });
                    let accepted = self.assets.as_ref()
                        .is_some_and(|store| store.texture(texture_id).is_some());
                    let (status, diagnostic) = match info {
                        Some(info) if info.kind != AssetKind::Texture => (
                            "wrong_kind", Some("Referenced asset is not a texture")
                        ),
                        Some(info) => (
                            match info.status {
                                AssetStatus::Ready if !accepted => "unavailable",
                                AssetStatus::Ready => "ready",
                                AssetStatus::Missing => "missing",
                                AssetStatus::Invalid => "invalid",
                            }, info.diagnostic.as_deref()
                        ),
                        None if self.assets.is_none() => (
                            "catalog_unavailable", Some("Repair/open a valid project catalog")
                        ),
                        None => ("unregistered", Some("Texture is not registered in this project")),
                    };
                    json!({
                        "asset_id":texture_id, "status":status,
                        "hash":info.and_then(|info| info.hash.as_ref()), "accepted":accepted,
                        "diagnostic":diagnostic.map(|text|text.chars().take(256).collect::<String>())
                    })
                });
                json!({
                    "material_id":id,
                    "material":{
                        "name":value.name, "tint":value.tint,
                        "base_color_texture":value.base_color_texture,
                        "filter":value.filter, "address":value.address, "response":value.response
                    },
                    "assigned_count":users.len(),
                    "user_ids":users.iter().take(4).map(|object|&object.id).collect::<Vec<_>>(),
                    "users_truncated":users.len()>4, "texture":texture,
                    "uv":{
                        "invalid_user_count":invalid_uv_users.len(),
                        "object_ids":invalid_uv_users.iter().take(4).map(|object|&object.id).collect::<Vec<_>>(),
                        "truncated":invalid_uv_users.len()>4
                    }
                })
            }).collect();
        let asset_revision = (!self.asset_loading()).then(|| {
            self.assets
                .as_ref()
                .map_or(0, |store| store.revision())
                .to_string()
        });
        Ok(
            json!({"session_token":token,"scene_id":self.session.scene().id,"revision":self.session.scene().revision.to_string(),"asset_revision":asset_revision,"asset_operation":self.asset_observation(),"catalog_available":self.assets.is_some(),"display":self.session.scene().display,"viewport_display_report":self.surface_feedback(),"materials":materials,"material_count":count,"next_offset":if offset+limit<count {Some((offset+limit).to_string())}else{None},"dirty":self.session.is_dirty(),"drafts_pending":self.drafts()||self.manipulation.active()||self.manipulation.offset_pending()}),
        )
    }

    pub(super) fn material_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value, CommandError> {
        let (specific, optional): (&[&str], &[&str]) = match op {
            "material_inspect" => (&[], &["offset", "limit"]),
            "scene_material_set" => (&["material_id", "material"], &["assign_object_id"]),
            "scene_material_assign" => (&["object_id", "material_id"], &[]),
            "scene_material_copy" => (&["source_id", "new_material_id"], &["name"]),
            "scene_material_remove" => (&["material_id"], &[]),
            _ => return Err(error("invalid_arguments", "Unknown material operation")),
        };
        let mut required = specific.to_vec();
        if op != "material_inspect" {
            required.extend([
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
            ]);
        }
        let map = args
            .as_object()
            .filter(|map| exact_keys(map, &required, optional))
            .ok_or_else(|| {
                error(
                    "invalid_arguments",
                    "Unexpected or missing material arguments",
                )
            })?;
        if op == "material_inspect" {
            return self.inspect_materials(token, args);
        }
        let expected_token = args["expected_session_token"]
            .as_str()
            .filter(|token| !token.is_empty() && token.len() <= 256)
            .ok_or_else(|| {
                error(
                    "invalid_arguments",
                    "Expected a bounded scene session token",
                )
            })?;
        if expected_token != token {
            return Err(error("stale_session", "Inspect the current scene session"));
        }
        self.guard()
            .and_then(|()| {
                if self.manipulation.offset_pending() {
                    Err("drafts_pending".into())
                } else {
                    Ok(())
                }
            })
            .map_err(|code| {
                error(
                    &code,
                    "Apply/revert drafts or resolve dialogs, then inspect again",
                )
            })?;
        let revision = number(&args["expected_revision"])?;
        let asset_revision = number(&args["expected_asset_revision"])?;
        if revision != self.session.scene().revision {
            return Err(error("stale_revision", "Inspect authored materials again"));
        }
        if asset_revision != self.assets.as_ref().map_or(0, |store| store.revision()) {
            return Err(error(
                "stale_asset_revision",
                "Inspect the current asset catalog",
            ));
        }
        let material_id: String;
        let mut object_id = None;
        let result = match op {
            "scene_material_set" => {
                material_id = id(&args["material_id"])?.into();
                let candidate = material(&args["material"])?;
                self.validate_material_candidate(&material_id, &candidate)
                    .map_err(|message| error("material_unavailable", message))?;
                if let Some(assign) = map.get("assign_object_id") {
                    let assign = id(assign)?;
                    if self.session.contains_material_id(&material_id) {
                        return Err(error(
                            "invalid_arguments",
                            "Atomic create-and-assign requires a fresh material ID",
                        ));
                    }
                    if candidate.base_color_texture.is_some() {
                        self.validate_material_object_uvs(assign)
                            .map_err(|message| error("material_unavailable", message))?;
                    }
                    object_id = Some(assign.to_owned());
                    self.session.create_and_assign_material(
                        revision,
                        assign,
                        &material_id,
                        candidate,
                    )
                } else {
                    self.session.set_material(revision, &material_id, candidate)
                }
            }
            "scene_material_assign" => {
                let assign = id(&args["object_id"])?;
                let material = if args["material_id"].is_null() {
                    None
                } else {
                    Some(id(&args["material_id"])?.to_string())
                };
                self.validate_material_assignment(assign, &material)
                    .map_err(|message| error("material_unavailable", message))?;
                material_id = material.clone().unwrap_or_default();
                object_id = Some(assign.to_owned());
                self.session.assign_material(revision, assign, material)
            }
            "scene_material_copy" => {
                let source = id(&args["source_id"])?;
                material_id = id(&args["new_material_id"])?.into();
                // Copies are unassigned data: preserving an unavailable reference
                // is safe and keeps it available for explicit repair.
                self.session.copy_material(
                    revision,
                    source,
                    &material_id,
                    optional_name(map.get("name"))?,
                )
            }
            "scene_material_remove" => {
                material_id = id(&args["material_id"])?.into();
                self.session.remove_material(revision, &material_id)
            }
            _ => unreachable!(),
        }
        .map_err(scene_error)?;
        let changed = result != revision;
        if changed {
            self.sync_draft();
            self.notice = format!(
                "Agent {op}: revision{result}; authored material changed. Preview unchanged."
            );
        }
        let id = (!material_id.is_empty()).then_some(material_id.as_str());
        let assigned_count = id.map_or(0, |id| {
            self.session
                .scene()
                .objects
                .iter()
                .filter(|object| object.material.as_deref() == Some(id))
                .count()
        });
        Ok(
            json!({"session_token":token,"revision":result.to_string(),"asset_revision":asset_revision.to_string(),"changed":changed,"material_id":id,"object_id":object_id,"assigned_count":assigned_count,"dirty":self.session.is_dirty()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Editor boundary risk: all shared users, typed accepted content, human
    // drafts and independent revisions must protect one real history/preview.
    #[test]
    fn material_commands_guard_shared_assets_and_preserve_repairable_history() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        std::fs::write(
            root.join("uv.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\nf 1/1 2/2 3/3\n",
        )
        .unwrap();
        std::fs::write(
            root.join("plain.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[255, 128, 0])
                .unwrap();
        }
        std::fs::write(root.join("texture.png"), png).unwrap();
        let mut store = ruster_assets::AssetStore::open(root).unwrap();
        for (id, source) in [
            ("uv", "uv.obj"),
            ("plain", "plain.obj"),
            ("texture", "texture.png"),
        ] {
            store.import(id, source).unwrap();
        }
        let mut scene = ruster_scene::Scene::demo();
        scene.objects.retain(|object| object.id != "center");
        scene
            .materials
            .insert("shared".into(), OpaqueMaterial::default());
        for (object, mesh) in scene.objects.iter_mut().zip(["uv", "plain"]) {
            object.mesh = Some(mesh.into());
            object.material = Some("shared".into());
        }
        let mut editor = SceneEditor::new(ruster_scene::Session::new(scene).unwrap());
        editor.resolved = store.mesh_snapshots();
        editor.assets = Some(store);
        let preview = editor.preview.clone();
        let make_args = |revision: &str, texture: Value| json!({"expected_session_token":"scene","expected_revision":revision,"expected_asset_revision":"3","material_id":"shared","material":{"tint":[1,1,1],"base_color_texture":texture,"filter":"linear","address":"repeat"}});
        let before = editor.session.snapshot_bytes().unwrap();
        for texture in [json!("plain"), json!("texture")] {
            assert_eq!(
                editor
                    .material_command("scene", "scene_material_set", &make_args("0", texture))
                    .unwrap_err()
                    .0,
                "material_unavailable"
            );
            assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        }
        editor.draft.as_mut().unwrap().translation[0] = "-".into();
        assert_eq!(
            editor
                .material_command("scene", "scene_material_set", &make_args("0", Value::Null))
                .unwrap_err()
                .0,
            "drafts_pending"
        );
        assert_eq!(editor.draft.as_ref().unwrap().translation[0], "-");
        editor.sync_draft();
        editor
            .session
            .set_mesh(0, "right", Some("uv".into()))
            .unwrap();
        editor.sync_draft();
        let applied = editor
            .material_command(
                "scene",
                "scene_material_set",
                &make_args("1", json!("texture")),
            )
            .unwrap();
        assert_eq!(applied["changed"], true);
        assert_eq!(applied["assigned_count"], 2);
        assert_eq!(editor.session.undo_len(), 2);
        assert_eq!(editor.preview, preview);
        let no_op = editor
            .material_command(
                "scene",
                "scene_material_set",
                &make_args("2", json!("texture")),
            )
            .unwrap();
        assert_eq!(no_op["changed"], false);
        assert_eq!(editor.session.undo_len(), 2);
        let before = editor.session.snapshot_bytes().unwrap();
        for (field, value, code) in [
            ("expected_revision", json!("1"), "stale_revision"),
            (
                "expected_asset_revision",
                json!("2"),
                "stale_asset_revision",
            ),
            ("expected_session_token", json!("old"), "stale_session"),
        ] {
            let mut args = make_args("2", json!("texture"));
            args[field] = value;
            assert_eq!(
                editor
                    .material_command("scene", "scene_material_set", &args)
                    .unwrap_err()
                    .0,
                code
            );
        }
        for value in [json!([-1e-50, 0, 1]), json!([1.00000001, 0, 1])] {
            let mut args = make_args("2", json!("texture"));
            args["material"]["tint"] = value;
            assert_eq!(
                editor
                    .material_command("scene", "scene_material_set", &args)
                    .unwrap_err()
                    .0,
                "invalid_arguments"
            );
        }
        let mut extra = make_args("2", json!("texture"));
        extra["material"]["path"] = json!("outside.png");
        assert!(editor
            .material_command("scene", "scene_material_set", &extra)
            .is_err());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);

        std::fs::write(root.join("texture.png"), b"broken").unwrap();
        editor.assets = Some(ruster_assets::AssetStore::open(root).unwrap());
        let mut tinted = make_args("2", json!("texture"));
        tinted["material"]["tint"] = json!([0.5, 1, 1]);
        tinted["material"]["response"] = json!({"type":"dielectric","roughness":0.08});
        editor
            .material_command("scene", "scene_material_set", &tinted)
            .unwrap();
        assert!(editor
            .validate_material_assignment("left", &Some("shared".into()))
            .is_ok());
        let inspected = editor
            .material_command("scene", "material_inspect", &json!({"limit":"1"}))
            .unwrap();
        assert_eq!(
            inspected["materials"][0]["material"]["response"],
            json!({"type":"dielectric","roughness":0.08})
        );
        assert_eq!(inspected["materials"][0]["texture"]["status"], "invalid");
        assert_eq!(inspected["materials"][0]["texture"]["accepted"], true);
        assert_eq!(inspected["materials"][0]["uv"]["invalid_user_count"], 0);
        editor
            .material_command("scene", "scene_material_set", &make_args("3", Value::Null))
            .unwrap();
        editor.session.undo(4).unwrap();
        assert_eq!(
            editor.session.scene().materials["shared"].tint,
            [0.5, 1.0, 1.0]
        );
        assert_eq!(
            editor.session.scene().materials["shared"]
                .base_color_texture
                .as_deref(),
            Some("texture")
        );
        let revision = editor.session.scene().revision.to_string();
        editor.material_command("scene","scene_material_copy",&json!({"expected_session_token":"scene","expected_revision":revision,"expected_asset_revision":"3","source_id":"shared","new_material_id":"copy","name":null})).unwrap();
        assert_eq!(
            editor.session.scene().materials["copy"],
            editor.session.scene().materials["shared"]
        );
        assert!(editor
            .validate_material_assignment("left", &Some("copy".into()))
            .is_err());
        assert!(editor.validate_material_assignment("left", &None).is_ok());
        assert_eq!(editor.preview, preview);
        let inspected = editor
            .material_command("scene", "material_inspect", &json!({"limit":"1"}))
            .unwrap();
        assert_eq!(inspected["next_offset"], "1");
        assert_eq!(inspected["asset_revision"], "3");
    }
}
