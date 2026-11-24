//! One authored world-Y move from the indexed geometry of selected subtrees.
use super::ComponentWorkspace;
use crate::strings::Text;
use glam::{DMat3, DVec3};
use rot_assets::Mesh;
use rot_scene::{
    component_scene::{Component, Geometry},
    Lighting,
};
use std::collections::{HashMap, HashSet};

type GroundError = (&'static str, String);
const GROUND_EPSILON: f64 = 2e-6;

pub(super) struct Placement {
    pub object_ids: Vec<String>,
    pub minimum_world_y: f64,
    pub world_delta_y: f64,
    pub changed: bool,
}

impl ComponentWorkspace {
    pub(super) fn ground_from_ui(&mut self, hit: Option<&str>) {
        let targets = self.target_ids(hit);
        let ids: Vec<_> = targets.iter().map(String::as_str).collect();
        self.error = self
            .place_on_ground(
                self.session.revision(),
                Some(self.material_asset_revision()),
                &ids,
            )
            .err()
            .map(|(_, message)| message);
    }

    pub(super) fn place_on_ground(
        &mut self,
        revision: u64,
        asset_revision: Option<u64>,
        ids: &[&str],
    ) -> Result<Placement, GroundError> {
        if self.session.revision() != revision {
            return Err((
                "stale_revision",
                "Inspect the current scene before placing it on the ground.".into(),
            ));
        }
        if self.runtime.in_play_mode() {
            return Err((
                "play_active",
                "Stop Play before placing objects on the ground.".into(),
            ));
        }
        if self.authored_edit_pending() || self.ai_transition_pending() {
            return Err((
                "drafts_pending",
                "Apply or cancel the pending edit before placing objects on the ground.".into(),
            ));
        }
        if ids.is_empty() {
            return Err((
                "invalid_arguments",
                "Select objects to place on the ground.".into(),
            ));
        }
        let objects = &self.session.scene().objects;
        if self.preview_world.len() != objects.len() {
            return Err((
                "world_transform_unavailable",
                "The scene's world transforms are unavailable.".into(),
            ));
        }
        let mut unknown: HashSet<_> = ids.iter().copied().collect();
        let mut root = None;
        let mut roots = Vec::new();
        let mut affected = Vec::new();
        for &(index, depth) in &self.rows {
            let id = objects[index].id.as_str();
            let wanted = unknown.remove(id);
            if root.is_some_and(|start| depth <= start) {
                root = None;
            }
            if root.is_none() && wanted {
                if self.view_flags.get(index).is_some_and(|flags| flags.hidden) {
                    return Err((
                        "objects_hidden",
                        Text::ComponentTransformHidden.plain().into(),
                    ));
                }
                roots.push(index);
                root = Some(depth);
            }
            if root.is_some() {
                if self.view_flags.get(index).is_some_and(|flags| flags.locked) {
                    return Err(("objects_locked", Text::GroundUnlock.plain().into()));
                }
                affected.push((index, roots.len() - 1));
            }
        }
        if let Some(id) = ids.iter().find(|id| unknown.contains(**id)) {
            return Err(("object_not_found", format!("Object {id} no longer exists.")));
        }

        let mut minimum = f64::INFINITY;
        // Repeated instances with the same world Y basis share a support value.
        // Translation is applied individually after looking it up.
        let mut support_cache = HashMap::new();
        for &(index, _) in &affected {
            let node = &objects[index];
            let world = self.preview_world[index];
            let basis = DVec3::new(world.x_axis.y, world.y_axis.y, world.z_axis.y);
            for component in &node.components {
                let Component::MeshRenderer { geometry, .. } = &component.kind else {
                    continue;
                };
                let mesh = match geometry {
                    Geometry::Cube => rot_render::builtin_cube(),
                    Geometry::Primitive { shape } => rot_render::builtin_mesh(*shape).as_ref(),
                    Geometry::Imported { asset } => {
                        if asset_revision.is_none() {
                            return Err((
                                "asset_revision_required",
                                "Inspect project assets and pin expected_asset_revision.".into(),
                            ));
                        }
                        if self.assets_loading() {
                            return Err((
                                "assets_loading",
                                "Wait for the project assets to load.".into(),
                            ));
                        }
                        if asset_revision != Some(self.material_asset_revision()) {
                            return Err((
                                "stale_asset_revision",
                                "Inspect the project assets again.".into(),
                            ));
                        }
                        self.assets
                            .as_ref()
                            .and_then(|store| store.mesh(asset))
                            .ok_or_else(|| {
                                (
                                    "mesh_unavailable",
                                    format!(
                                        "{} uses mesh {asset}, which has no accepted geometry.",
                                        node.name
                                    ),
                                )
                            })?
                    }
                };
                let key = (std::ptr::from_ref(mesh), basis.to_array().map(f64::to_bits));
                let support = support_cache
                    .entry(key)
                    .or_insert_with(|| local_support(mesh, basis))
                    .ok_or_else(|| {
                        (
                            "mesh_unavailable",
                            format!(
                                "Mesh Renderer {} on {} has no usable indexed geometry.",
                                component.id, node.name
                            ),
                        )
                    })?;
                let bottom = support + world.w_axis.y;
                if !bottom.is_finite() {
                    return Err((
                        "world_transform_unrepresentable",
                        Text::GroundCannotPlace.plain().into(),
                    ));
                }
                minimum = minimum.min(bottom);
            }
        }
        if minimum == f64::INFINITY {
            return Err((
                "mesh_renderer_required",
                "Select objects with mesh geometry in their hierarchies.".into(),
            ));
        }
        let object_ids = roots
            .iter()
            .map(|&index| objects[index].id.clone())
            .collect();
        if minimum.abs() <= GROUND_EPSILON {
            return Ok(Placement {
                object_ids,
                minimum_world_y: minimum,
                world_delta_y: 0.0,
                changed: false,
            });
        }
        let world_delta_y = -minimum;
        let mut edits = Vec::with_capacity(roots.len());
        let mut displacements = Vec::with_capacity(roots.len());
        for index in roots {
            let mut node = objects[index].clone();
            let basis = self.parents[index].map_or(DMat3::IDENTITY, |parent| {
                DMat3::from_mat4(self.preview_world[parent])
            });
            let (translation, applied) =
                translate_world_y(basis, node.transform.translation, world_delta_y).ok_or_else(
                    || {
                        (
                            "world_transform_unrepresentable",
                            format!("{} {}", Text::GroundCannotPlace.plain(), node.name),
                        )
                    },
                )?;
            node.transform.translation = translation;
            edits.push(node);
            displacements.push(applied);
        }
        let preview = self
            .preview
            .as_ref()
            .expect("world matrices require a scene preview");
        let projected_lights = match &preview.scene().lighting {
            Lighting::Authored { lights, .. } if !lights.is_empty() => Some(lights),
            _ => None,
        };
        if let Some(lights) = projected_lights {
            for (index, root) in affected {
                let node = &objects[index];
                let position = self.preview_world[index].w_axis.truncate() + displacements[root];
                for light in node
                    .components
                    .iter()
                    .filter_map(|item| lights.get(&item.id))
                {
                    let mut transform = light.transform.clone();
                    transform.translation = position.as_vec3().to_array();
                    transform.validate().map_err(|error| {
                        (
                            "world_transform_unrepresentable",
                            format!(
                                "Placing {} would move its light beyond the world limits. {error}",
                                node.name
                            ),
                        )
                    })?;
                }
            }
        }
        let next = self
            .session
            .put_objects(revision, edits)
            .map_err(|error| ("invalid_placement", error.to_string()))?;
        if next != revision {
            self.refresh();
        }
        Ok(Placement {
            object_ids,
            minimum_world_y: minimum,
            world_delta_y,
            changed: next != revision,
        })
    }
}

fn translate_world_y(parent: DMat3, local: [f32; 3], y: f64) -> Option<([f32; 3], DVec3)> {
    let start = DVec3::from_array(local.map(f64::from));
    let displacement = DVec3::new(0.0, y, 0.0);
    let requested = start + parent.inverse() * displacement;
    if !requested.is_finite() || requested.abs().max_element() > 10_000.0 {
        return None;
    }
    let stored = requested.as_vec3().as_dvec3();
    let applied = parent * (stored - start);
    // Check world displacement itself so a large world coordinate cannot mask
    // sideways drift or disagreement between independently stored roots.
    ((applied - displacement).abs().max_element() <= GROUND_EPSILON)
        .then(|| (stored.as_vec3().to_array(), applied))
}

fn local_support(mesh: &Mesh, basis: DVec3) -> Option<f64> {
    if mesh.indices.is_empty() || !mesh.indices.len().is_multiple_of(3) {
        return None;
    }
    let mut low = f64::INFINITY;
    for &index in &mesh.indices {
        let position = mesh.vertices.get(index as usize)?.position;
        let y = basis.dot(DVec3::from_array(position.map(f64::from)));
        if !y.is_finite() {
            return None;
        }
        low = low.min(y);
    }
    Some(low)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::workspace::Action;
    use rot_scene::{
        component_scene::{ComponentScene, ComponentSession, ObjectNode},
        Scene, SceneError, Session, Transform,
    };
    use serde_json::json;

    #[test]
    fn grounded_hierarchies_share_one_world_displacement_and_one_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("workshop.scene.json");
        std::fs::create_dir(directory.path().join("Assets")).unwrap();
        let vertices = [[1.0, -2.0, 2.0], [2.0, 0.0, -1.0], [0.0, -3.0, 1.0]];
        std::fs::write(
            directory.path().join("Assets/Off-center.obj"),
            "v 1 -2 2\nv 2 0 -1\nv 0 -3 1\nf 1 2 3\n",
        )
        .unwrap();
        let mut store = rot_assets::AssetStore::open(directory.path()).unwrap();
        store.import("barrier", "Assets/Off-center.obj").unwrap();

        let legacy = Session::new(Scene::demo()).unwrap();
        let mut scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.objects.truncate(2);
        scene.objects[0].parent = Some("assembly".into());
        scene.objects[0].transform.translation = [1.0, 2.0, 1.0];
        scene.objects[0].transform.rotation = [0.2, 0.5, 0.3];
        scene.objects[0].components[0].enabled = false;
        let Component::MeshRenderer { geometry, .. } = &mut scene.objects[0].components[0].kind
        else {
            panic!("demo cube");
        };
        *geometry = Geometry::Imported {
            asset: "barrier".into(),
        };
        scene.objects[1].transform.translation = [4.0, 18.0, -2.0];
        let empty = |id: &str, parent: Option<&str>, transform| ObjectNode {
            id: id.into(),
            name: id.into(),
            enabled: true,
            parent: parent.map(str::to_owned),
            transform,
            components: vec![],
        };
        scene.objects.push(empty(
            "mount",
            None,
            Transform {
                translation: [0.0, 8.0, 0.0],
                rotation: [0.31, 0.16, -0.4],
                scale: [2.0, 0.8, 1.2],
            },
        ));
        scene.objects.push(empty(
            "assembly",
            Some("mount"),
            Transform {
                translation: [0.25, 0.5, 0.1],
                rotation: [0.23, 0.12, -0.18],
                ..Default::default()
            },
        ));
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.assets = Some(store.project_snapshot());
        let start = editor.preview_world.clone();
        let minimum = |world: glam::DMat4| {
            vertices
                .iter()
                .map(|v| world.transform_point3(DVec3::from_array(*v)).y)
                .fold(f64::INFINITY, f64::min)
        };
        let expected_delta = -minimum(start[0]);

        let submit = |editor: &mut ComponentWorkspace, args: &serde_json::Value| {
            editor.ai_command("guard", "scene_place_on_ground", args)
        };
        let mut args = json!({
            "expected_session_token":"guard","expected_revision":"0",
            "object_ids":["assembly","left","center","left"],
        });
        assert_eq!(
            submit(&mut editor, &args)["error"]["code"],
            "asset_revision_required"
        );
        args["expected_asset_revision"] = json!((store.revision() + 1).to_string());
        assert_eq!(
            submit(&mut editor, &args)["error"]["code"],
            "stale_asset_revision"
        );
        args["expected_asset_revision"] = json!(store.revision().to_string());
        editor
            .view_flags
            .resize(scene.objects.len(), Default::default());
        editor.view_flags[0].locked = true;
        assert_eq!(
            submit(&mut editor, &args)["error"]["code"],
            "objects_locked"
        );
        editor.view_flags.clear();
        assert_eq!(editor.session.scene(), &scene);
        assert!(matches!(editor.session.undo(0), Err(SceneError::NoUndo)));

        let placed = submit(&mut editor, &args);
        assert_eq!(placed["result"]["revision"], "1", "{placed}");
        assert_eq!(
            placed["result"]["object_ids"],
            json!(["center", "assembly"])
        );
        assert!(
            (placed["result"]["world_delta_y"].as_f64().unwrap() - expected_delta).abs() < 1e-10
        );
        assert!(minimum(editor.preview_world[0]).abs() <= GROUND_EPSILON);
        assert_eq!(editor.session.scene().objects[0], scene.objects[0]);
        for row in [0, 1, 3] {
            let actual = editor.preview_world[row];
            let original = start[row];
            assert_eq!(actual.x_axis, original.x_axis);
            assert_eq!(actual.y_axis, original.y_axis);
            assert_eq!(actual.z_axis, original.z_axis);
            let delta = (actual.w_axis - original.w_axis).truncate();
            assert!((delta - DVec3::Y * expected_delta).abs().max_element() <= GROUND_EPSILON);
        }
        assert!(
            (placed["result"]["minimum_world_y"].as_f64().unwrap() + expected_delta).abs() < 1e-10
        );
        args["expected_revision"] = json!("1");
        let no_op = submit(&mut editor, &args);
        assert_eq!(no_op["result"]["changed"], false);
        assert_eq!(no_op["result"]["world_delta_y"], 0.0);
        assert!(
            (no_op["result"]["minimum_world_y"].as_f64().unwrap()
                - minimum(editor.preview_world[0]))
            .abs()
                < 1e-10
        );
        assert_eq!(editor.session.revision(), 1);
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
        assert!(matches!(editor.session.undo(2), Err(SceneError::NoUndo)));
    }

    #[test]
    fn grounding_refuses_the_same_projected_light_overflow_as_move() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("lights.scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.objects[0].transform.translation = [0.0, 3.0, 0.0];
        scene.objects[1].parent = Some("left".into());
        scene.objects[1].transform.translation = [0.0, -10_000.0, 0.0];
        let light = rot_scene::Light::point(None);
        scene.objects[1].components = vec![rot_scene::component_scene::ComponentInstance {
            id: "lamp".into(),
            enabled: true,
            kind: Component::Light {
                color: light.color,
                intensity: light.intensity,
                kind: light.kind,
            },
        }];
        // A mesh-less group between the selected root and light places the
        // light one metre lower while keeping its local Transform valid.
        scene.objects.push(ObjectNode {
            id: "lamp-base".into(),
            name: "Lamp Base".into(),
            enabled: true,
            parent: Some("left".into()),
            transform: Transform {
                translation: [0.0, -2.0, 0.0],
                ..Default::default()
            },
            components: vec![],
        });
        scene.objects[1].parent = Some("lamp-base".into());
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let mut staged_move = editor.preview.as_ref().unwrap().clone();
        let origin = editor.preview_world[1].w_axis.truncate();
        assert_eq!(origin.y, -9_999.0);
        assert!(staged_move
            .set_translated_rows(&[], &[("lamp".into(), origin)], DVec3::new(0.0, -2.5, 0.0))
            .is_err());
        let failure = editor
            .place_on_ground(0, None, &["left"])
            .err()
            .expect("refuse the same world light bounds");
        assert_eq!(failure.0, "world_transform_unrepresentable");
        assert_eq!(editor.session.scene(), &scene);
        assert!(matches!(editor.session.undo(0), Err(SceneError::NoUndo)));
    }

    #[test]
    fn grounding_checks_world_precision_after_storing_local_translation() {
        let parent = DMat3::from_rotation_z(0.3) * DMat3::from_diagonal(DVec3::new(2.0, 0.5, 2.0));
        assert!(translate_world_y(parent, [0.0; 3], 2.0).is_some());
        assert!(translate_world_y(parent, [9000.0, 0.0, 0.0], 0.001).is_none());
    }
}
