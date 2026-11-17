use rot_assets::ModelInfo;
use rot_scene::{
    Group, Hierarchy, ImportedPlacement, MaterialResponse, Object, OpaqueMaterial, SceneError,
    Session, TextureAddress, TextureFilter, Transform,
};
use std::collections::{BTreeMap, BTreeSet};

/// Map the admitted source template to a scene-owned copy. Scene IDs are fresh
/// for each placement and resource IDs remain shared across copies and refreshes.
pub(super) fn model_placement(
    session: &Session,
    model: &ModelInfo,
    name: String,
) -> Result<ImportedPlacement, SceneError> {
    let parents: BTreeSet<_> = model
        .nodes
        .values()
        .filter_map(|node| node.parent.as_deref())
        .collect();
    let group_nodes: Vec<_> = model
        .nodes
        .iter()
        .filter(|(path, node)| node.mesh_id.is_none() || parents.contains(path.as_str()))
        .collect();
    let mut group_ids = session
        .allocate_group_ids(group_nodes.len() + 1)?
        .into_iter();
    let root_id = group_ids.next().expect("placement allocates one root");
    let groups_by_path: BTreeMap<_, _> = group_nodes
        .into_iter()
        .map(|(path, _)| path.as_str())
        .zip(group_ids)
        .collect();
    let object_ids = session.allocate_object_ids(
        model
            .nodes
            .values()
            .filter(|node| node.mesh_id.is_some())
            .count(),
    )?;
    let mut object_ids = object_ids.into_iter();
    let material_ids: BTreeMap<_, _> = model
        .materials
        .keys()
        .map(String::as_str)
        .zip(session.allocate_material_ids(model.materials.len())?)
        .collect();
    let materials = model
        .materials
        .iter()
        .map(|(name, source)| {
            let material = OpaqueMaterial {
                name: Some(name.clone()),
                tint: source.base_color_factor,
                base_color_texture: source.base_color_texture.clone(),
                roughness_texture: None,
                filter: if source.linear {
                    TextureFilter::Linear
                } else {
                    TextureFilter::Nearest
                },
                address: if source.repeat {
                    TextureAddress::Repeat
                } else {
                    TextureAddress::Clamp
                },
                response: MaterialResponse::Dielectric {
                    roughness: source.roughness,
                },
            };
            (material_ids[name.as_str()].clone(), material)
        })
        .collect();

    let mut hierarchy = Hierarchy::default();
    let mut objects = Vec::new();
    for (path, node) in &model.nodes {
        let parent_id = match &node.parent {
            Some(parent) => groups_by_path.get(parent.as_str()).ok_or_else(|| {
                SceneError::Invalid(format!("Imported model node {path:?} has no parent group"))
            })?,
            None => &root_id,
        };
        let local = Transform {
            translation: node.translation,
            rotation: node.rotation,
            scale: node.scale,
        };
        let group_id = groups_by_path.get(path.as_str());
        if let Some(id) = group_id {
            hierarchy.groups.insert(
                id.clone(),
                Group {
                    name: Some(node.name.clone()),
                    transform: local.clone(),
                },
            );
            hierarchy.parents.insert(id.clone(), parent_id.clone());
        }
        if let Some(mesh) = &node.mesh_id {
            let id = object_ids
                .next()
                .expect("one object allocated per mesh node");
            let mut object = Object::cube(id.clone(), Some(node.name.clone()));
            object.mesh = Some(mesh.clone());
            object.material = node
                .material
                .as_deref()
                .map(|material| {
                    material_ids.get(material).cloned().ok_or_else(|| {
                        SceneError::Invalid(format!(
                            "Imported model node {path:?} has no material definition"
                        ))
                    })
                })
                .transpose()?;
            object.transform = if group_id.is_some() {
                Transform::default()
            } else {
                local
            };
            hierarchy
                .parents
                .insert(id, group_id.unwrap_or(parent_id).clone());
            objects.push(object);
        }
    }
    Ok(ImportedPlacement {
        root_id,
        root: Group {
            name: Some(name),
            transform: Transform::default(),
        },
        hierarchy,
        objects,
        materials,
    })
}

impl super::SceneEditor {
    pub(super) fn place_model(&mut self, id: &str, name: Option<&str>) -> Result<String, String> {
        self.guard()?;
        if self.manipulation.offset_pending() {
            return Err("Apply or cancel the pending transform offset first.".into());
        }
        let store = self
            .assets
            .as_ref()
            .ok_or("Open a project with a valid catalog")?;
        let model = store
            .model(id)
            .ok_or("Imported model not found; import its GLB source first")?;
        for node in model.nodes.values() {
            if node
                .mesh_id
                .as_ref()
                .is_some_and(|mesh| !self.resolved.contains_key(mesh))
            {
                return Err("Model mesh unavailable; refresh its source or reopen the project before placing.".into());
            }
        }
        for material in model.materials.values() {
            if material
                .base_color_texture
                .as_ref()
                .is_some_and(|texture| !self.resolved_textures.contains_key(texture))
            {
                return Err("Model texture unavailable; refresh its source or reopen the project before placing.".into());
            }
        }
        let placement = model_placement(&self.session, model, name.unwrap_or(id).into())
            .map_err(|error| error.to_string())?;
        let root = placement.root_id.clone();
        self.session
            .place_imported(self.session.scene().revision, placement)
            .map_err(|error| error.to_string())?;
        self.select_object(Some(root.clone()), false);
        self.error.clear();
        self.notice = format!("Placed {id}. Scene Undo removes this placement; source refresh preserves scene edits. Preview updates on Reset.");
        Ok(root)
    }

    pub(super) fn models_ui(&mut self, ui: &mut eframe::egui::Ui) {
        let query = self.content_query.to_lowercase();
        // Borrow only bounded row metadata, never copy imported graphs per frame.
        let rows: Vec<_> = self
            .assets
            .as_ref()
            .into_iter()
            .flat_map(|store| store.models())
            .filter(|model| {
                query.is_empty()
                    || model.id.to_lowercase().contains(&query)
                    || model.source.to_lowercase().contains(&query)
            })
            .map(|model| (model.id.clone(), model.source.clone(), model.hash.clone()))
            .collect();
        if rows.is_empty() {
            return;
        }
        ui.label("Models");
        let mut place = None;
        let mut refresh = None;
        let enabled = self.guard().is_ok() && !self.asset_ops.busy();
        eframe::egui::ScrollArea::vertical()
            .id_salt("content_models_scroll")
            .max_height(150.0)
            .show(ui, |ui| {
                for (id, source, hash) in rows {
                    ui.push_id(&id, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("{id} · Model")).on_hover_text(format!(
                                "Source: {source}\nAccepted revision: {hash}"
                            ));
                            if ui
                                .add_enabled(enabled, eframe::egui::Button::new("Place"))
                                .clicked()
                            {
                                place = Some(id.clone());
                            }
                            if ui
                                .add_enabled(enabled, eframe::egui::Button::new("Refresh"))
                                .clicked()
                            {
                                refresh = Some(id.clone());
                            }
                        });
                    });
                }
            });
        ui.small("Place creates an editable scene copy. Refresh keeps placement and uses the last accepted model if refused.");
        if let Some(id) = place {
            if let Err(error) = self.place_model(&id, None) {
                self.error = format!("Placement refused: {error}. The imported model is retained; repair the scene and retry Place.");
            }
        }
        if let Some(id) = refresh {
            self.queued_refresh = Some(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creation_export::{scene_dependencies, Controller, Identity, Request};
    use rot_assets::AssetStore;
    use rot_scene::{
        BehaviorBinding, BehaviorSource, BindingTarget, PlayerAnchor, PlayerView, Scene,
    };
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};
    use std::{fs, thread};

    // Real asset-to-command boundary: ownership/freshness, repeat placement and
    // graph-free inspection are distinct from the lower scene transaction test.
    #[test]
    #[ignore = "requires pinned Blender GLB family in ROT_TEST_GLB_FIXTURE"]
    fn guarded_model_commands_and_resource_only_refresh() {
        let fixtures = PathBuf::from(std::env::var_os("ROT_TEST_GLB_FIXTURE").unwrap());
        let dir = tempfile::tempdir().unwrap();
        fs::copy(fixtures.join("beacon-A.glb"), dir.path().join("model.glb")).unwrap();
        let path = dir.path().join("scene.json");
        Session::new(Scene::empty("test"))
            .unwrap()
            .save(&path)
            .unwrap();
        let mut editor = super::super::SceneEditor::test_loaded(Session::open(&path).unwrap());
        let token =
            editor.command("test", "scene_inspect", json!({}))["result"]["session_token"].clone();
        let import = json!({"expected_session_token":token,"expected_revision":"0","expected_asset_revision":"0","asset_id":"beacon","source":"model.glb"});
        let mut overrides = import.clone();
        overrides["normal_policy"] = json!("flat");
        assert_eq!(
            editor.command("test", "asset_import", overrides)["error"]["code"],
            "invalid_arguments"
        );
        let result = editor.command("test", "asset_import", import);
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(result["result"]["model"]["id"], "beacon");
        editor.test_finish_asset_work();
        let inspection = editor.command("test", "asset_inspect", json!({}));
        assert_eq!(inspection["result"]["model_count"], 1);
        assert!(inspection["result"]["models"][0].get("nodes").is_none());
        assert!(serde_json::to_vec(&inspection).unwrap().len() < 16384);
        let mut args = json!({"expected_session_token":token,"expected_revision":"0","expected_asset_revision":"1","asset_id":"beacon"});
        let initial = editor.session.snapshot_bytes().unwrap();
        for (key, value, code) in [
            ("expected_session_token", "bad", "stale_session"),
            ("expected_revision", "8", "stale_revision"),
            ("expected_asset_revision", "8", "stale_asset_revision"),
        ] {
            let mut bad = args.clone();
            bad[key] = json!(value);
            assert_eq!(
                editor.command("test", "scene_model_place", bad)["error"]["code"],
                code
            );
            assert_eq!(editor.session.snapshot_bytes().unwrap(), initial);
        }
        let mut bad = args.clone();
        bad["name"] = Value::Null;
        assert_eq!(
            editor.command("test", "scene_model_place", bad)["error"]["code"],
            "invalid_arguments"
        );
        let result = editor.command("test", "scene_model_place", args.clone());
        assert_eq!(result["ok"], true, "{result}");
        assert_eq!(
            editor.selected.as_deref(),
            result["result"]["root_id"].as_str()
        );
        let first = editor.selected.clone();
        args["expected_revision"] = json!("1");
        let result = editor.command("test", "scene_model_place", args);
        assert_eq!(result["ok"], true, "{result}");
        assert_ne!(editor.selected, first);
        assert_eq!(editor.session.undo_len(), 2);
        let placed = editor.session.snapshot_bytes().unwrap();
        fs::copy(
            fixtures.join("beacon-B-reordered.glb"),
            dir.path().join("model.glb"),
        )
        .unwrap();
        let refresh = json!({"expected_session_token":token,"expected_revision":"2","expected_asset_revision":"1","asset_id":"beacon"});
        let result = editor.command("test", "asset_refresh", refresh.clone());
        assert_eq!(result["ok"], true, "{result}");
        editor.test_finish_asset_work();
        assert_eq!(editor.session.snapshot_bytes().unwrap(), placed);
        fs::copy(
            fixtures.join("beacon-malformed.glb"),
            dir.path().join("model.glb"),
        )
        .unwrap();
        let mut refresh = refresh;
        refresh["expected_asset_revision"] = json!("2");
        assert_eq!(
            editor.command("test", "asset_refresh", refresh)["error"]["code"],
            "asset_error"
        );
        editor.test_finish_asset_work();
        assert_eq!(editor.session.snapshot_bytes().unwrap(), placed);
        assert_eq!(editor.assets.as_ref().unwrap().revision(), 2);
    }

    fn refresh_from(
        store: &mut AssetStore,
        source: &Path,
        fixtures: &Path,
        filename: &str,
    ) -> Result<ModelInfo, rot_assets::AssetError> {
        fs::copy(fixtures.join(filename), source).unwrap();
        store.refresh_glb_at(store.revision(), "beacon")
    }

    fn export(
        session: &Session,
        store: &AssetStore,
        scene_path: &Path,
        kit: &Path,
        destination: PathBuf,
    ) -> Value {
        let (meshes, textures) = scene_dependencies(session.scene()).unwrap();
        assert_eq!((meshes.len(), textures.len()), (2, 1));
        let assets = store
            .export_snapshot(&meshes.union(&textures).cloned().collect())
            .unwrap();
        let identity = Identity {
            generation: 1,
            revision: session.scene().revision,
            persistence_token: "glb-tracer".into(),
            asset_revision: Some(assets.revision()),
            shader_revision: 0,
        };
        let mut controller = Controller::disabled();
        controller.configure_runtime_kit(kit).unwrap();
        let kit = controller.runtime_kit().unwrap().to_owned();
        controller
            .start(
                &eframe::egui::Context::default(),
                Request {
                    identity: identity.clone(),
                    title: "Static GLB tracer".into(),
                    scene_path: scene_path.to_owned(),
                    scene_bytes: session.snapshot_bytes().unwrap(),
                    assets: Some(assets),
                    destination,
                    runtime_kit: kit,
                    mcp_scope: false,
                    initial_camera: None,
                },
            )
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(75);
        while controller.busy() && Instant::now() < deadline {
            controller.poll(&identity);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!controller.busy(), "export failed to retire");
        let status = controller.inspect(None).unwrap();
        assert_eq!(status["operation"]["state"], "committed", "{status}");
        status["operation"]["receipt"].clone()
    }

    #[test]
    #[ignore = "requires explicitly supplied Blender fixture, fresh output directory and matched runtime kit"]
    fn blender_model_catalog_scene_export_tracer() {
        let fixture = PathBuf::from(std::env::var_os("ROT_TEST_GLB_FIXTURE").unwrap());
        let result = PathBuf::from(std::env::var_os("ROT_TEST_GLB_OUTPUT").unwrap());
        let kit = PathBuf::from(std::env::var_os("ROT_TEST_GLB_KIT").unwrap());
        fs::create_dir(&result).unwrap();
        let source = result.join("beacon.glb");
        fs::copy(fixture.join("beacon-A.glb"), &source).unwrap();
        let mut store = AssetStore::open(&result).unwrap();
        let first = store
            .import_glb_at(store.revision(), "beacon", "beacon.glb")
            .unwrap();
        let mut session = Session::new(Scene::empty("beacon-tracer")).unwrap();
        let placement = model_placement(&session, &first, "Beacon placement".into()).unwrap();
        let root = placement.root_id.clone();
        session.place_imported(0, placement).unwrap();
        let placed = session.scene().clone();
        session.undo(session.scene().revision).unwrap();
        assert!(session.scene().objects.is_empty());
        assert!(session.scene().materials.is_empty());
        session.redo(session.scene().revision).unwrap();
        assert_eq!(session.scene().objects, placed.objects);
        assert_eq!(session.scene().materials, placed.materials);
        assert_eq!(session.scene().hierarchy, placed.hierarchy);
        let position = [3.0, 1.0, -2.0];
        session
            .edit_transforms(
                session.scene().revision,
                &[(
                    root.clone(),
                    Transform {
                        translation: position,
                        ..Transform::default()
                    },
                )],
            )
            .unwrap();
        let source_record = BehaviorSource {
            label: None,
            language_id: rot_behavior::LANGUAGE_ID.into(),
            host_api_version: rot_scene::GROUP_ROTATION_HOST_API_VERSION,
            text: "return {update=function(rotation, props, dt) if dt > 0 then rotation[2] = 0.5 end return rotation end}".into(),
        };
        let binding = BehaviorBinding {
            target: BindingTarget::Group(root.clone()),
            source_id: "motion".into(),
            enabled: true,
            properties: Default::default(),
            state_defaults: Default::default(),
            commands: Default::default(),
        };
        session
            .create_behavior(
                session.scene().revision,
                "motion",
                source_record,
                "spin",
                binding,
            )
            .unwrap();
        session
            .set_player_view(
                session.scene().revision,
                Some(PlayerView {
                    anchor: PlayerAnchor::Follow {
                        target_id: root,
                        offset: [0.0, 1.0, 0.0],
                    },
                    yaw: 0.0,
                    pitch: 0.0,
                    distance: 6.0,
                    vertical_fov_degrees: 60.0,
                }),
            )
            .unwrap();
        let scene_path = result.join("scene.json");
        session.save(&scene_path).unwrap();
        let saved = fs::read(&scene_path).unwrap();
        let before = export(
            &session,
            &store,
            &scene_path,
            &kit,
            result.join("before.tar.gz"),
        );
        let second = refresh_from(&mut store, &source, &fixture, "beacon-B-reordered.glb").unwrap();
        assert_ne!(first.hash, second.hash);
        assert_eq!(
            first
                .nodes
                .iter()
                .map(|(path, node)| (path, &node.mesh_id))
                .collect::<Vec<_>>(),
            second
                .nodes
                .iter()
                .map(|(path, node)| (path, &node.mesh_id))
                .collect::<Vec<_>>(),
        );
        let after = export(
            &session,
            &store,
            &scene_path,
            &kit,
            result.join("after.tar.gz"),
        );
        assert_eq!(before["scene_sha256"], after["scene_sha256"]);
        let accepted_revision = store.revision();
        let accepted_meshes = store.mesh_snapshots();
        let accepted_textures = store.texture_snapshots();
        for filename in ["beacon-malformed.glb", "beacon-removed.glb"] {
            assert!(
                refresh_from(&mut store, &source, &fixture, filename).is_err(),
                "{filename}"
            );
            assert_eq!(store.model("beacon").unwrap().hash, second.hash);
            assert_eq!(store.revision(), accepted_revision);
            assert_eq!(store.mesh_snapshots(), accepted_meshes);
            assert_eq!(store.texture_snapshots(), accepted_textures);
        }
        drop(store);
        let cache = result.join(".rot-cache");
        if cache.exists() {
            fs::remove_dir_all(cache).unwrap();
        }
        let store = AssetStore::open(&result).unwrap();
        assert_eq!(store.model("beacon").unwrap().hash, second.hash);
        assert_eq!(store.mesh_snapshots(), accepted_meshes);
        assert_eq!(store.texture_snapshots(), accepted_textures);
        let session = Session::open(&scene_path).unwrap();
        assert_eq!(fs::read(&scene_path).unwrap(), saved);
        let recovered = export(
            &session,
            &store,
            &scene_path,
            &kit,
            result.join("recovered.tar.gz"),
        );
        assert_eq!(after["scene_sha256"], recovered["scene_sha256"]);
        let poses = session.scene().resolve_poses().unwrap();
        let world: BTreeMap<_, _> = session
            .scene()
            .objects
            .iter()
            .map(|object| {
                (
                    object.name.as_deref().unwrap(),
                    poses.transforms[&object.id].translation,
                )
            })
            .collect();
        fs::write(
            result.join("tracer.json"),
            serde_json::to_vec_pretty(&json!({
                "world": world, "placement": position, "before": before,
                "after": after, "recovered": recovered,
            }))
            .unwrap(),
        )
        .unwrap();
    }
}
