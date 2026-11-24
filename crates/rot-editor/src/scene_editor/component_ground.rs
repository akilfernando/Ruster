//! One authored world-Y move from the indexed geometry of selected subtrees.
use super::ComponentWorkspace;
use crate::strings::Text;
use glam::{DMat3, DVec3};
use rot_assets::Mesh;
use rot_scene::component_scene::{Component, Geometry};
use std::collections::{HashMap, HashSet};

type GroundError = (&'static str, String);
const GROUND_EPSILON: f64 = 2e-6;

pub(super) struct Placement {
    pub object_ids: Vec<String>,
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
                affected.push(index);
            }
        }
        if let Some(id) = ids.iter().find(|id| unknown.contains(**id)) {
            return Err(("object_not_found", format!("Object {id} no longer exists.")));
        }

        let mut minimum = f64::INFINITY;
        // Repeated instances with the same world Y basis share a support value.
        // Translation is applied individually after looking it up.
        let mut imported_support = HashMap::new();
        for index in affected {
            let node = &objects[index];
            let world = self.preview_world[index];
            let basis = DVec3::new(world.x_axis.y, world.y_axis.y, world.z_axis.y);
            for component in &node.components {
                let Component::MeshRenderer { geometry, .. } = &component.kind else {
                    continue;
                };
                let support = match geometry {
                    Geometry::Cube => local_support(rot_render::builtin_cube(), basis),
                    Geometry::Primitive { shape } => {
                        local_support(rot_render::builtin_mesh(*shape), basis)
                    }
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
                        let mesh = self
                            .assets
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
                            })?;
                        *imported_support
                            .entry((asset.as_str(), basis.to_array().map(f64::to_bits)))
                            .or_insert_with(|| local_support(mesh, basis))
                    }
                }
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
                world_delta_y: 0.0,
                changed: false,
            });
        }
        let world_delta_y = -minimum;
        let mut edits = Vec::with_capacity(roots.len());
        for index in roots {
            let mut node = objects[index].clone();
            let basis = self.parents[index].map_or(DMat3::IDENTITY, |parent| {
                DMat3::from_mat4(self.preview_world[parent])
            });
            node.transform.translation =
                translate_world_y(basis, node.transform.translation, world_delta_y).ok_or_else(
                    || {
                        (
                            "world_transform_unrepresentable",
                            format!("{} {}", Text::GroundCannotPlace.plain(), node.name),
                        )
                    },
                )?;
            edits.push(node);
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
            world_delta_y,
            changed: next != revision,
        })
    }
}

fn translate_world_y(parent: DMat3, local: [f32; 3], y: f64) -> Option<[f32; 3]> {
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
        .then(|| stored.as_vec3().to_array())
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
