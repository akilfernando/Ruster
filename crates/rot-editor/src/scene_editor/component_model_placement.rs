//! Material and hierarchy copies for model assets in v23 scenes.
use super::{ComponentWorkspace, Geometry};
use crate::scene_editor::model_placement::imported_material;
use rot_assets::ModelLight;
use rot_scene::{
    component_scene::{Component, ComponentInstance, ObjectNode},
    AnimationBinding, LightKind, SceneError, SunShadow, Transform,
};
use std::collections::BTreeMap;
use std::path::Path;

impl ComponentWorkspace {
    pub(super) fn place_model(
        &mut self,
        asset: &str,
        name: Option<&str>,
        parent: Option<String>,
    ) -> Result<String, SceneError> {
        let invalid = |message: &str| SceneError::Invalid(message.to_owned());
        if self.runtime.in_play_mode() {
            return Err(invalid("Stop Play before placing a model."));
        }
        let store = self
            .assets
            .as_ref()
            .ok_or_else(|| invalid("Project assets are unavailable."))?;
        let model = store
            .model(asset)
            .ok_or_else(|| invalid("Import the GLB before placing it."))?;
        let scene = self.session.scene();
        let revision = self.session.revision();

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let root = (revision..)
            .map(|suffix| format!("model-{stamp:x}-{suffix:x}"))
            .find(|root| {
                let prefix = format!("{root}-");
                let used = |id: &str| id == root || id.starts_with(&prefix);
                !scene.materials.keys().any(|id| used(id))
                    && !scene.objects.iter().any(|object| {
                        used(&object.id)
                            || object
                                .components
                                .iter()
                                .any(|component| used(&component.id))
                    })
            })
            .expect("finite scene IDs");
        let node_ids: BTreeMap<_, _> = model
            .nodes
            .keys()
            .enumerate()
            .map(|(index, path)| (path.as_str(), format!("{root}-node-{index}")))
            .collect();
        let material_ids: BTreeMap<_, _> = model
            .materials
            .keys()
            .enumerate()
            .map(|(index, name)| (name.as_str(), format!("{root}-material-{index}")))
            .collect();
        let mut materials = BTreeMap::new();
        for (name, source) in &model.materials {
            if source
                .base_color_texture
                .iter()
                .chain(&source.roughness_texture)
                .chain(&source.metallic_texture)
                .chain(&source.normal_texture)
                .chain(&source.emissive_texture)
                .chain(&source.occlusion_texture)
                .any(|texture| !self.textures.contains_key(texture))
            {
                return Err(invalid(
                    "A model texture is unavailable. Reimport its source.",
                ));
            }
            materials.insert(
                material_ids[name.as_str()].clone(),
                imported_material(name, source),
            );
        }

        let name = name.unwrap_or_else(|| {
            Path::new(&model.source)
                .file_stem()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty() && name.len() <= 128)
                .unwrap_or(asset)
        });
        let mut objects = Vec::with_capacity(model.nodes.len() + 1);
        let animation = model
            .clips
            .values()
            .next()
            .map(|clip| {
                if store.clip(&clip.id).is_none() {
                    return Err(invalid(
                        "A model animation is unavailable. Reimport its source.",
                    ));
                }
                Ok(ComponentInstance {
                    id: format!("{root}-animator"),
                    enabled: true,
                    kind: Component::Animator {
                        start_playback: Default::default(),
                        playback_speed: 1.0,
                        binding: AnimationBinding {
                            clip: clip.id.clone(),
                            looping: true,
                            targets: node_ids
                                .iter()
                                .filter(|(path, _)| !rot_assets::ModelInfo::is_primitive_path(path))
                                .map(|(path, id)| ((*path).to_owned(), id.clone()))
                                .collect(),
                        },
                    },
                })
            })
            .transpose()?;
        objects.push(ObjectNode {
            id: root.clone(),
            name: name.to_owned(),
            enabled: true,
            parent,
            transform: Transform::default(),
            components: animation.into_iter().collect(),
        });
        for (path, node) in &model.nodes {
            let id = &node_ids[path.as_str()];
            let parent = node
                .parent
                .as_deref()
                .map(|parent| {
                    node_ids
                        .get(parent)
                        .ok_or_else(|| invalid("A model node's parent is missing."))
                })
                .transpose()?
                .unwrap_or(&root);
            let mut components = Vec::new();
            if let Some(mesh) = &node.mesh_id {
                if !self.meshes.contains_key(mesh) {
                    return Err(invalid("A model mesh is unavailable. Reimport its source."));
                }
                let material = node
                    .material
                    .as_deref()
                    .map(|material| {
                        material_ids
                            .get(material)
                            .cloned()
                            .ok_or_else(|| invalid("A model material is missing."))
                    })
                    .transpose()?;
                components.push(ComponentInstance {
                    id: format!("{id}-mesh"),
                    enabled: true,
                    kind: Component::MeshRenderer {
                        geometry: Geometry::Imported {
                            asset: mesh.clone(),
                        },
                        tint: [1.0; 4],
                        material,
                    },
                });
            }
            if let Some(skin_index) = node.skin {
                let skin = model
                    .skins
                    .get(skin_index)
                    .ok_or_else(|| invalid("A model skin binding is missing."))?;
                let joints = skin
                    .joints
                    .iter()
                    .map(|joint_path| {
                        node_ids
                            .get(joint_path.as_str())
                            .cloned()
                            .ok_or_else(|| invalid("A model skin joint node is missing."))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if skin.inverse_bind_matrices.len() != joints.len() {
                    return Err(invalid(
                        "A model skin inverse bind matrix count does not match its joints.",
                    ));
                }
                components.push(ComponentInstance {
                    id: format!("{id}-skin"),
                    enabled: true,
                    kind: Component::Skin {
                        joints,
                        inverse_bind_matrices: skin.inverse_bind_matrices.clone(),
                    },
                });
            }
            if let Some(light) = node.light {
                let (color, intensity, kind) = match light {
                    ModelLight::Point {
                        color,
                        intensity,
                        range,
                    } => (
                        color,
                        intensity,
                        LightKind::Point {
                            range,
                            shadow: Default::default(),
                        },
                    ),
                    ModelLight::Spot {
                        color,
                        intensity,
                        range,
                        inner_degrees,
                        outer_degrees,
                    } => (
                        color,
                        intensity,
                        LightKind::Spot {
                            range,
                            inner_degrees,
                            outer_degrees,
                            shadow: Default::default(),
                        },
                    ),
                    ModelLight::Directional { color, intensity } => (
                        color,
                        intensity,
                        LightKind::Directional {
                            shadow: SunShadow::default(),
                        },
                    ),
                };
                components.push(ComponentInstance {
                    id: format!("{id}-light"),
                    enabled: true,
                    kind: Component::Light {
                        color,
                        intensity,
                        kind,
                    },
                });
            }
            if let Some(camera) = node.camera {
                use rot_assets::ModelCamera;
                use rot_scene::component_scene::{
                    CameraClip, CameraView, OrthographicClip, TransformLens, TransformOrthographic,
                };
                let view = match camera {
                    ModelCamera::Perspective(camera) => {
                        CameraView::transform_perspective_with_lens(
                            camera.vertical_fov_degrees,
                            camera.aspect_ratio,
                            Some(CameraClip {
                                near: camera.near,
                                far: camera.far,
                            }),
                        )
                    }
                    ModelCamera::Orthographic(camera) => CameraView::Transform(
                        TransformLens::Orthographic(TransformOrthographic::new(
                            camera.vertical_size,
                            Some(camera.aspect_ratio),
                            OrthographicClip {
                                near: camera.near,
                                far: camera.far,
                            },
                        )),
                    ),
                };

                components.push(ComponentInstance {
                    id: format!("{id}-camera"),
                    enabled: true,
                    kind: Component::Camera { view },
                });
            }
            objects.push(ObjectNode {
                id: id.clone(),
                name: node.name.clone(),
                enabled: true,
                parent: Some(parent.clone()),
                transform: Transform {
                    translation: node.translation,
                    rotation: node.rotation,
                    scale: node.scale,
                },
                components,
            });
        }

        self.session
            .put_objects_with_materials(revision, objects, materials)?;
        self.select(Some(root.clone()));
        self.refresh();
        Ok(root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_assets::{
        fixtures::{animated_glb, AnimatedFixture},
        AssetStore,
    };
    use rot_scene::{
        component_scene::{ComponentScene, ComponentSession},
        Scene, Session,
    };
    use std::fs;

    #[test]
    fn v23_places_shared_orthographic_lens_once_per_original_node() {
        use rot_assets::fixtures::punctual_glb;
        use serde_json::json;
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        fs::write(
            project.path().join("Studio.glb"),
            punctual_glb(|doc| {
                doc["cameras"] = json!([{
                    "type":"orthographic",
                    "orthographic":{"xmag":6.0,"ymag":4.0,"znear":0.5,"zfar":25.0},
                }]);
                doc["nodes"][1]["camera"] = json!(0);
                doc["nodes"][2]["camera"] = json!(0);
            }),
        )
        .unwrap();
        let mut assets = AssetStore::open(project.path()).unwrap();
        let model = assets.import_glb_at(0, "studio", "Studio.glb").unwrap();
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("stage")).unwrap()).unwrap();
        let mut workspace = ComponentWorkspace::new(path, ComponentSession::new(scene).unwrap());
        workspace.meshes = assets.mesh_snapshots();
        workspace.assets = Some(assets);
        workspace.place_model(&model.id, None, None).unwrap();
        let placed = workspace.session.scene();
        assert!(placed.gameplay_camera.is_none());
        let cameras: Vec<_> = placed
            .objects
            .iter()
            .flat_map(|object| {
                object.components.iter().filter_map(move |component| {
                    let Component::Camera { view } = &component.kind else {
                        return None;
                    };
                    Some((object, component, view))
                })
            })
            .collect();
        assert_eq!(cameras.len(), 2);
        assert_ne!(cameras[0].1.id, cameras[1].1.id);
        let expected = json!({"type":"transform_orthographic", "vertical_size":8.0,
            "aspect_ratio":1.5, "clip":{"near":0.5, "far":25.0}});
        for (object, _, view) in cameras {
            let source = &model.nodes[&format!("Rig/{}", object.name)];
            assert_eq!(object.transform.translation, source.translation);
            assert_eq!(object.transform.rotation, source.rotation);
            assert_eq!(serde_json::to_value(view).unwrap(), expected);
            assert_eq!(
                placed
                    .objects
                    .iter()
                    .filter(|child| child.parent.as_ref() == Some(&object.id))
                    .count(),
                2
            );
        }
        workspace.session.undo(1).unwrap();
        assert!(workspace.session.scene().objects.is_empty());
    }

    #[test]
    fn v23_places_punctual_lights_on_original_nodes_and_retains_edits_on_refresh() {
        use rot_assets::fixtures::punctual_glb;
        use serde_json::json;

        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        let source = project.path().join("Lantern.glb");
        fs::write(&source, punctual_glb(|_| {})).unwrap();
        let mut store = AssetStore::open(project.path()).unwrap();
        let model = store.import_glb_at(0, "lantern", "Lantern.glb").unwrap();
        let initial =
            ComponentScene::from_legacy(&Session::new(Scene::empty("placement")).unwrap()).unwrap();
        let mut workspace = ComponentWorkspace::new(path, ComponentSession::new(initial).unwrap());
        workspace.meshes = store.mesh_snapshots();
        workspace.assets = Some(store);
        workspace.place_model(&model.id, None, None).unwrap();
        let placed = workspace.session.scene().clone();
        let owners: Vec<_> = placed
            .objects
            .iter()
            .filter(|node| {
                node.components
                    .iter()
                    .any(|component| matches!(component.kind, Component::Light { .. }))
            })
            .collect();
        assert_eq!(
            owners
                .iter()
                .map(|node| node.name.as_str())
                .collect::<Vec<_>>(),
            ["Arm", "Base", "Spot", "Sun"]
        );
        for owner in &owners[..2] {
            assert!(!owner
                .components
                .iter()
                .any(|component| matches!(component.kind, Component::MeshRenderer { .. })));
            assert_eq!(
                placed
                    .objects
                    .iter()
                    .filter(|node| node.parent.as_ref() == Some(&owner.id))
                    .count(),
                2
            );
        }
        let spot = owners[2];
        let rig = placed
            .objects
            .iter()
            .find(|node| node.name == "Rig")
            .unwrap();
        assert_eq!(spot.parent.as_deref(), Some(rig.id.as_str()));
        assert_eq!(spot.transform.translation, [1., 2., 3.]);
        assert_eq!(spot.components.len(), 1);
        assert_eq!(
            spot.components[0].kind,
            Component::Light {
                color: [1.; 3],
                intensity: 0.01,
                kind: LightKind::Spot {
                    range: 5.,
                    inner_degrees: 0.,
                    outer_degrees: 45.,
                    shadow: Default::default(),
                }
            }
        );
        assert_eq!(
            rot_render::component_preview::project(&placed)
                .unwrap()
                .scene()
                .lighting
                .light_count(),
            4
        );
        let mut edited = spot.clone();
        let Component::Light {
            intensity,
            kind: LightKind::Spot { inner_degrees, .. },
            ..
        } = &mut edited.components[0].kind
        else {
            panic!("expected placed light");
        };
        *intensity = 2.5;
        *inner_degrees = 18.;
        workspace.session.put_object(1, edited).unwrap();
        let placed = workspace.session.scene().clone();
        let store = workspace.assets.as_mut().unwrap();
        fs::write(
            &source,
            punctual_glb(|value| {
                value["extensions"]["KHR_lights_punctual"]["lights"][2]["intensity"] = json!(75);
            }),
        )
        .unwrap();
        store.refresh_glb_at(store.revision(), "lantern").unwrap();
        workspace
            .place_model(&model.id, Some("Second lantern"), None)
            .unwrap();
        assert_eq!(
            &workspace.session.scene().objects[..placed.objects.len()],
            &placed.objects
        );
        let new_spot = workspace.session.scene().objects[placed.objects.len()..]
            .iter()
            .find(|node| node.name == "Spot")
            .unwrap();
        assert_eq!(
            new_spot.components[0].kind,
            Component::Light {
                color: [1.; 3],
                intensity: 0.75,
                kind: LightKind::Spot {
                    range: 5.,
                    inner_degrees: 0.,
                    outer_degrees: 45.,
                    shadow: Default::default(),
                }
            }
        );
        workspace.session.undo(3).unwrap();
        assert_eq!(workspace.session.scene(), &placed);
    }

    #[cfg(unix)]
    #[test]
    fn v23_animator_edits_share_guards_and_keep_the_placement_map() {
        use serde_json::{json, Value};

        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        let source = project.path().join("Bridge.glb");
        let fixture = animated_glb(AnimatedFixture {
            clip_name: "Lower",
            second_clip_name: Some("Raise"),
            ..Default::default()
        });
        fs::write(&source, &fixture).unwrap();
        fs::write(project.path().join("Foreign.glb"), fixture).unwrap();
        let mut store = AssetStore::open(project.path()).unwrap();
        let model = store.import_glb_at(0, "bridge", "Bridge.glb").unwrap();
        let foreign = store.import_glb_at(1, "other", "Foreign.glb").unwrap();
        let initial =
            ComponentScene::from_legacy(&Session::new(Scene::empty("animator")).unwrap()).unwrap();
        let mut workspace = ComponentWorkspace::new(path, ComponentSession::new(initial).unwrap());
        workspace.meshes = store.mesh_snapshots();
        workspace.assets = Some(store);
        let owner = workspace.place_model(&model.id, None, None).unwrap();
        let placed = workspace.session.scene().clone();
        let component = placed.objects[0].components[0].clone();
        let Component::Animator { binding, .. } = &component.kind else {
            unreachable!()
        };
        let mut request = json!({"expected_session_token":"session", "expected_revision":"1",
            "owner_id":owner,"clip_id":model.clips["Raise"].id});
        for (asset_revision, clip, code) in [
            (
                Value::Null,
                &model.clips["Raise"].id,
                "asset_revision_required",
            ),
            (json!("1"), &model.clips["Raise"].id, "stale_asset_revision"),
            (json!("2"), &foreign.clips["Raise"].id, "invalid_clip"),
        ] {
            if !asset_revision.is_null() {
                request["expected_asset_revision"] = asset_revision;
            }
            request["clip_id"] = json!(clip);
            let reply = workspace.ai_command("session", "scene_animation_set", &request);
            assert_eq!(reply["error"]["code"], code, "{reply}");
            assert_eq!(workspace.session.revision(), 1);
            assert_eq!(workspace.session.scene(), &placed);
        }
        let mut malformed = serde_json::to_value(&component).unwrap();
        malformed["binding"]["targets"]
            .as_object_mut()
            .unwrap()
            .remove("Rig/Arm");
        let reply = workspace.ai_command("session", "scene_components_edit", &json!({
            "expected_session_token":"session","expected_revision":"1","expected_asset_revision":"2",
            "edits":[{"operation":"replace","object_id":owner,"component":malformed}]
        }));
        assert_eq!(reply["error"]["code"], "invalid_clip", "{reply}");
        assert_eq!(workspace.session.revision(), 1);

        let stale_loop = workspace.ai_command("session", "scene_animation_set", &json!({
            "expected_session_token":"session","expected_revision":"1","expected_asset_revision":"1",
            "owner_id":owner,"looping":false
        }));
        assert_eq!(stale_loop["error"]["code"], "stale_asset_revision");
        assert_eq!(workspace.session.scene(), &placed);
        request["clip_id"] = json!(model.clips["Raise"].id);
        let reply = workspace.ai_command("session", "scene_animation_set", &request);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["result"]["animation"]["source"]["clip_name"], "Raise");
        assert_eq!(reply["result"]["animation"]["looping"], true);
        let Component::Animator { binding: next, .. } =
            &workspace.session.scene().objects[0].components[0].kind
        else {
            unreachable!()
        };
        assert_eq!(next.targets, binding.targets);
        let reply = workspace.ai_command("session", "scene_animation_set", &json!({
            "expected_session_token":"session","expected_revision":"2","owner_id":owner,"looping":false
        }));
        assert_eq!(reply["result"]["animation"]["looping"], false, "{reply}");
        workspace.session.undo(3).unwrap();
        workspace.session.undo(4).unwrap();
        workspace.refresh();
        assert_eq!(workspace.session.scene(), &placed);

        for (value, expected) in [
            (json!(0), "invalid_arguments"),
            (json!(1e70), "invalid_arguments"),
        ] {
            let bad = workspace.ai_command(
                "session",
                "scene_animation_set",
                &json!({
                    "expected_session_token":"session","expected_revision":"5","owner_id":owner,
                    "playback_speed":value
                }),
            );
            assert_eq!(bad["error"]["code"], expected, "{bad}");
        }
        assert_eq!(workspace.session.scene(), &placed);
        let speed = workspace.ai_command(
            "session",
            "scene_animation_set",
            &json!({
                "expected_session_token":"session","expected_revision":"5","owner_id":owner,
                "playback_speed":2.0
            }),
        );
        assert_eq!(
            speed["result"]["animation"]["playback_speed"], 2.0,
            "{speed}"
        );
        let after = workspace.session.scene().objects[0].components[0].clone();
        let mut expected = component.clone();
        let Component::Animator { playback_speed, .. } = &mut expected.kind else {
            unreachable!()
        };
        *playback_speed = 2.0;
        assert_eq!(after, expected);
        let omitted = workspace.ai_command(
            "session",
            "scene_components_edit",
            &json!({
                "expected_session_token":"session","expected_revision":"6",
                "edits":[{"operation":"replace","object_id":owner,"component":component}]
            }),
        );
        assert_eq!(omitted["ok"], true, "{omitted}");
        assert_eq!(workspace.session.revision(), 6);
        workspace.session.undo(6).unwrap();
        assert_eq!(workspace.session.scene(), &placed);
    }

    #[test]
    fn v23_model_placement_preserves_materials_and_animator_in_one_undo() {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        let assets = project.path().join("Assets");
        fs::create_dir(&assets).unwrap();
        fs::write(
            assets.join("Sample.glb"),
            animated_glb(AnimatedFixture {
                split_materials: true,
                ..Default::default()
            }),
        )
        .unwrap();
        let (store, warnings) = AssetStore::open_project(project.path()).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        let model = store.models().next().unwrap();
        let model_id = model.id.clone();
        let clip_id = model.clips.values().next().unwrap().id.clone();
        let initial =
            ComponentScene::from_legacy(&Session::new(Scene::empty("placement")).unwrap()).unwrap();
        initial.save_new(&path).unwrap();
        let mut workspace =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        workspace.meshes = store.mesh_snapshots();
        workspace.textures = store.texture_snapshots();
        workspace.assets = Some(store);
        assert!(workspace
            .place_model(&model_id, None, Some("missing-parent".into()))
            .is_err());
        assert_eq!(workspace.session.scene(), &initial);
        assert_eq!(workspace.session.revision(), 0);

        let id = workspace.place_model(&model_id, None, None).unwrap();
        let placed = workspace.session.scene().clone();
        assert_eq!(workspace.session.revision(), 1);
        let root = &placed.objects[0];
        assert_eq!(root.id, id);
        assert_eq!(root.name, "Sample");
        let Component::Animator { binding, .. } = &root.components[0].kind else {
            panic!("expected imported transform animation");
        };
        assert_eq!(binding.clip, clip_id);
        assert!(!binding.targets.is_empty());
        assert_eq!(placed.materials.len(), 2);
        assert_eq!(binding.targets.len(), 3);
        let arm = placed
            .objects
            .iter()
            .find(|object| object.name == "Arm")
            .unwrap();
        assert_eq!(binding.targets["Rig/Arm"], arm.id);
        assert_eq!(
            placed
                .objects
                .iter()
                .filter(|object| object.parent.as_deref() == Some(arm.id.as_str()))
                .count(),
            2
        );
        assert!(placed.objects.iter().any(|object| {
            object.components.iter().any(|item| matches!(&item.kind,
                Component::MeshRenderer { geometry: Geometry::Imported { asset }, material: Some(material), .. }
                if workspace.meshes.contains_key(asset) && placed.materials.contains_key(material)))
        }));
        workspace.session.undo(1).unwrap();
        assert_eq!(workspace.session.scene(), &initial);
        workspace.session.redo(2).unwrap();
        workspace.session.save(&path).unwrap();
        assert_eq!(ComponentSession::open(path).unwrap().scene(), &placed);
    }

    #[test]
    fn places_skinned_glb_with_skin_component_and_joint_bindings() {
        use rot_scene::component_scene::ObjectHierarchy;
        use serde_json::json;

        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");

        let mut bin = Vec::new();
        let push_f32 = |buf: &mut Vec<u8>, data: &[f32]| {
            let offset = buf.len();
            for &v in data {
                buf.extend_from_slice(&v.to_le_bytes());
            }
            (offset, buf.len() - offset)
        };
        let push_bytes = |buf: &mut Vec<u8>, data: &[u8]| {
            let offset = buf.len();
            buf.extend_from_slice(data);
            (offset, buf.len() - offset)
        };
        let (p_off, p_len) = push_f32(&mut bin, &[0.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
        let (n_off, n_len) = push_f32(&mut bin, &[0.0_f32, 0.0, 1.0].repeat(3));
        let (j_off, j_len) = push_bytes(&mut bin, &[0_u8, 1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0]);
        let (w_off, w_len) = push_bytes(&mut bin, &[255_u8, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0]);
        let ibms = [
            glam::Mat4::IDENTITY.to_cols_array(),
            glam::Mat4::from_translation(glam::Vec3::new(-1.0, 0.0, 0.0)).to_cols_array(),
        ];
        let (ibm_off, ibm_len) = push_f32(&mut bin, ibms.as_flattened());
        let doc = json!({
            "asset": {"version": "2.0"},
            "buffers": [{"byteLength": bin.len()}],
            "bufferViews": [
                {"buffer": 0, "byteOffset": p_off, "byteLength": p_len},
                {"buffer": 0, "byteOffset": n_off, "byteLength": n_len},
                {"buffer": 0, "byteOffset": j_off, "byteLength": j_len},
                {"buffer": 0, "byteOffset": w_off, "byteLength": w_len},
                {"buffer": 0, "byteOffset": ibm_off, "byteLength": ibm_len}
            ],
            "accessors": [
                {"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0,0,0], "max": [1,1,0]},
                {"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC3"},
                {"bufferView": 2, "componentType": 5121, "count": 3, "type": "VEC4"},
                {"bufferView": 3, "componentType": 5121, "normalized": true, "count": 3, "type": "VEC4"},
                {"bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4"}
            ],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0, "NORMAL": 1, "JOINTS_0": 2, "WEIGHTS_0": 3}}]}],
            "skins": [{"joints": [0, 1], "inverseBindMatrices": 4}],
            "nodes": [
                {"name": "Hip", "children": [1]},
                {"name": "Shoulder", "translation": [1.0, 2.0, 0.0]},
                {"name": "Body", "mesh": 0, "skin": 0}
            ],
            "scenes": [{"nodes": [0, 2]}],
            "scene": 0
        });
        let mut json_bytes = serde_json::to_vec(&doc).unwrap();
        while !json_bytes.len().is_multiple_of(4) {
            json_bytes.push(b' ');
        }
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
        let mut glb = Vec::with_capacity(total);
        glb.extend_from_slice(b"glTF");
        glb.extend_from_slice(&2u32.to_le_bytes());
        glb.extend_from_slice(&(total as u32).to_le_bytes());
        glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"JSON");
        glb.extend_from_slice(&json_bytes);
        glb.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        glb.extend_from_slice(b"BIN\0");
        glb.extend_from_slice(&bin);
        fs::write(project.path().join("Character.glb"), &glb).unwrap();

        let mut assets = AssetStore::open(project.path()).unwrap();
        let model = assets
            .import_glb_at(0, "character", "Character.glb")
            .unwrap();
        assert_eq!(model.skins.len(), 1);
        let scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("stage")).unwrap()).unwrap();
        let mut workspace = ComponentWorkspace::new(path, ComponentSession::new(scene).unwrap());
        workspace.meshes = assets.mesh_snapshots();
        workspace.assets = Some(assets);
        workspace.place_model(&model.id, None, None).unwrap();

        let placed = workspace.session.scene();
        let hip = placed
            .objects
            .iter()
            .find(|object| object.name == "Hip")
            .unwrap();
        let shoulder = placed
            .objects
            .iter()
            .find(|object| object.name == "Shoulder")
            .unwrap();
        let body = placed
            .objects
            .iter()
            .find(|object| object.name == "Body")
            .unwrap();
        let skin_comp = body
            .components
            .iter()
            .find(|comp| matches!(comp.kind, Component::Skin { .. }))
            .expect("Body should carry Component::Skin");
        let Component::Skin {
            joints,
            inverse_bind_matrices,
        } = &skin_comp.kind
        else {
            unreachable!()
        };
        assert_eq!(joints, &[hip.id.clone(), shoulder.id.clone()]);
        assert_eq!(inverse_bind_matrices.len(), 2);

        ObjectHierarchy::build(&placed.objects).unwrap();
        let frame = rot_scene::ComponentDrawPlan::build_initial_frame(placed, 1).unwrap();
        let skin_mats = frame.hierarchy().unwrap().skin_matrices();
        assert_eq!(skin_mats.len(), 2);
        // Joint 1 ("Shoulder") sits at (1, 2, 0) with IBM translating -1 in X -> maps (1, 0, 0) to (1, 2, 0).
        let mapped = skin_mats[1].transform_point3(glam::Vec3::new(1.0, 0.0, 0.0));
        assert!((mapped - glam::Vec3::new(1.0, 2.0, 0.0)).length() < 1e-5);
    }
}
