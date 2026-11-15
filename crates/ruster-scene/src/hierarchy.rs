//! Scene-owned local topology; resolution lives only for one invocation.
use crate::*;
use glam::{DMat4, DQuat, DVec3, EulerRot};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: Option<String>,
    pub transform: Transform,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hierarchy {
    pub groups: BTreeMap<String, Group>,
    /// Target ID -> group ID. Missing entry means scene root.
    pub parents: BTreeMap<String, String>,
}
impl Hierarchy {
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty() && self.parents.is_empty()
    }
}

/// Original-precision command values, admitted before narrowing into authored f32.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransformRequest {
    pub translation: [f64; 3],
    pub rotation: [f64; 3],
    pub scale: [f64; 3],
}

/// World-space observations. Authored roots are copied exactly, not decomposed.
pub struct ResolvedPoses {
    pub transforms: BTreeMap<String, Transform>,
    poses: BTreeMap<String, Pose>,
}
#[derive(Clone, Copy)]
struct Pose {
    position: DVec3,
    rotation: DQuat,
    scale: DVec3,
}
fn invalid(message: &str) -> SceneError {
    SceneError::Invalid(message.into())
}
impl Pose {
    fn from_local(t: &Transform) -> Self {
        let [x, y, z] = t.rotation.map(f64::from);
        Self {
            position: DVec3::from_array(t.translation.map(f64::from)),
            rotation: DQuat::from_euler(EulerRot::ZYX, z, y, x),
            scale: DVec3::from_array(t.scale.map(f64::from)),
        }
    }
    fn compose(self, local: Self) -> Self {
        Self {
            position: self.position + self.rotation * (self.scale * local.position),
            rotation: (self.rotation * local.rotation).normalize(),
            scale: self.scale * local.scale,
        }
    }
    fn matrix(self) -> DMat4 {
        DMat4::from_scale_rotation_translation(self.scale, self.rotation, self.position)
    }
    fn stored(self) -> Result<Transform, SceneError> {
        if !self.position.is_finite()
            || self.position.abs().max_element() > 10000.
            || !self.rotation.is_finite()
            || !self.scale.is_finite()
            || self.scale.min_element() < 0.001
            || self.scale.max_element() > 1000.
        {
            return Err(invalid(
                "requested/resolved hierarchy pose exceeds numeric bounds",
            ));
        }
        let (z, y, x) = self.rotation.to_euler(EulerRot::ZYX);
        let t = Transform {
            translation: self.position.as_vec3().to_array(),
            rotation: [x as f32, y as f32, z as f32],
            scale: self.scale.as_vec3().to_array(),
        };
        t.validate()?;
        if !close(self.matrix(), Self::from_local(&t).matrix()) {
            return Err(invalid("hierarchy pose exceeds storage tolerance"));
        }
        Ok(t)
    }
}
// Frozen per-element bound, separately checked for high precision and stored TRS.
fn close(a: DMat4, b: DMat4) -> bool {
    a.to_cols_array()
        .into_iter()
        .zip(b.to_cols_array())
        .all(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= 2e-6 * a.abs().max(1.))
}
impl Scene {
    /// Remove descendants of selected ancestors once; preserve input order.
    pub fn normalize_assembly_selection(&self, ids: &[String]) -> Result<Vec<String>, SceneError> {
        if ids.len() > MAX_OBJECTS {
            return Err(invalid("selection exceeds combined target budget"));
        }
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(id.as_str()) || self.target_transform(id).is_none() {
                return Err(invalid("missing or repeated selection target"));
            }
        }
        self.resolve_poses()?;
        Ok(ids
            .iter()
            .filter(|id| {
                let mut current = id.as_str();
                while let Some(parent) = self.hierarchy.parents.get(current) {
                    if seen.contains(parent.as_str()) {
                        return false;
                    }
                    current = parent;
                }
                true
            })
            .cloned()
            .collect())
    }
    /// Convert world-space manipulation into admitted local edits. Roots retain
    /// their original TRS values/arithmetic; only parented targets need inversion.
    pub fn local_edits_for_world(
        &self,
        edits: &[(String, Transform)],
    ) -> Result<Vec<(String, Transform)>, SceneError> {
        if self.hierarchy.is_empty() {
            return Ok(edits.to_vec());
        }
        let ids = edits.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>();
        let normalized = self.normalize_assembly_selection(&ids)?;
        let old = self.resolve_poses()?;
        let mut overrides = BTreeMap::new();
        let mut locals = Vec::new();
        for (id, world) in edits.iter().filter(|(id, _)| normalized.contains(id)) {
            world.validate()?;
            if old.transforms[id] == *world {
                continue;
            }
            let w = Pose::from_local(world);
            let local = if let Some(parent) = self.hierarchy.parents.get(id) {
                let p = old.poses[parent];
                Pose {
                    position: (p.rotation.inverse() * (w.position - p.position)) / p.scale,
                    rotation: (p.rotation.inverse() * w.rotation).normalize(),
                    scale: if self.lighting.light(id).is_some() {
                        DVec3::ONE
                    } else {
                        w.scale / p.scale
                    },
                }
            } else {
                w
            };
            overrides.insert(id.clone(), local);
            locals.push((
                id.clone(),
                if self.hierarchy.parents.contains_key(id) {
                    local.stored()?
                } else {
                    world.clone()
                },
            ));
        }
        // Admit unrounded requested world closure, then stored/resolved closure.
        let requested = self.resolve_requested(&overrides)?;
        let mut staged = self.clone();
        for (id, t) in &locals {
            staged.set_local(id, t.clone())?;
        }
        staged.validate()?;
        let stored = staged.resolve_poses()?;
        for (id, p) in requested.poses {
            if !close(p.matrix(), stored.poses[&id].matrix()) {
                return Err(invalid(
                    "world manipulation exceeds descendant storage tolerance",
                ));
            }
        }
        Ok(locals)
    }
    pub fn preview_world_transforms(
        &self,
        edits: &[(String, Transform)],
    ) -> Result<Scene, SceneError> {
        let locals = self.local_edits_for_world(edits)?;
        let mut scene = self.clone();
        for (id, t) in locals {
            scene.set_local(&id, t)?;
        }
        scene.validate()?;
        Ok(scene)
    }
    fn set_local(&mut self, id: &str, t: Transform) -> Result<(), SceneError> {
        if let Some(group) = self.hierarchy.groups.get_mut(id) {
            group.transform = t;
            return Ok(());
        }
        if let Some(object) = self.objects.iter_mut().find(|o| o.id == id) {
            object.transform = t;
            return Ok(());
        }
        if let Lighting::Authored { lights, .. } = &mut self.lighting {
            if let Some(light) = lights.get_mut(id) {
                light.transform = t;
                return Ok(());
            }
        }
        Err(SceneError::ObjectNotFound(id.into()))
    }
    pub fn require_flat(&self) -> Result<(), SceneError> {
        if !self.hierarchy.is_empty() {
            return Err(invalid(
                "assembly hierarchy is unsupported by this runtime/export capability",
            ));
        }
        Ok(())
    }
    /// Resolve after topology admission. No persistent cache and no Scene clone.
    pub fn resolve_poses(&self) -> Result<ResolvedPoses, SceneError> {
        self.resolve_requested(&BTreeMap::new())
    }
    /// Admit original-precision world edits, including unselected descendants.
    pub fn admit_world_transform_requests(
        &self,
        requests: &[(String, TransformRequest)],
    ) -> Result<(), SceneError> {
        let ids = requests
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        let normalized = self.normalize_assembly_selection(&ids)?;
        let old = self.resolve_poses()?;
        let mut locals = Vec::new();
        for (id, t) in requests.iter().filter(|(id, _)| normalized.contains(id)) {
            let [x, y, z] = t.rotation;
            let world = Pose {
                position: DVec3::from_array(t.translation),
                rotation: DQuat::from_euler(EulerRot::ZYX, z, y, x),
                scale: DVec3::from_array(t.scale),
            };
            world.stored()?;
            let local = if let Some(parent) = self.hierarchy.parents.get(id) {
                let p = old.poses[parent];
                Pose {
                    position: (p.rotation.inverse() * (world.position - p.position)) / p.scale,
                    rotation: (p.rotation.inverse() * world.rotation).normalize(),
                    scale: if self.lighting.light(id).is_some() {
                        DVec3::ONE
                    } else {
                        world.scale / p.scale
                    },
                }
            } else {
                world
            };
            let (z, y, x) = local.rotation.to_euler(EulerRot::ZYX);
            locals.push((
                id.clone(),
                TransformRequest {
                    translation: local.position.to_array(),
                    rotation: [x, y, z],
                    scale: local.scale.to_array(),
                },
            ));
        }
        self.admit_transform_requests(&locals)
    }
    /// Validate every requested local and complete world closure before f32 storage.
    pub fn admit_transform_requests(
        &self,
        requests: &[(String, TransformRequest)],
    ) -> Result<(), SceneError> {
        let mut overrides = BTreeMap::new();
        for (id, t) in requests {
            if self.target_transform(id).is_none() || overrides.contains_key(id) {
                return Err(invalid("missing or repeated transform target"));
            }
            if !t
                .translation
                .iter()
                .chain(&t.rotation)
                .all(|v| v.is_finite() && v.abs() <= 10000.)
                || !t
                    .scale
                    .iter()
                    .all(|v| v.is_finite() && *v >= 0.001 && *v <= 1000.)
            {
                return Err(invalid("requested local transform exceeds numeric bounds"));
            }
            if self.hierarchy.groups.contains_key(id)
                && (t.scale[0] != t.scale[1] || t.scale[0] != t.scale[2])
            {
                return Err(invalid("requested group scale must be uniform"));
            }
            if self.lighting.light(id).is_some() && t.scale != [1.; 3] {
                return Err(invalid("lights retain unit scale"));
            }
            let [x, y, z] = t.rotation;
            overrides.insert(
                id.clone(),
                Pose {
                    position: DVec3::from_array(t.translation),
                    rotation: DQuat::from_euler(EulerRot::ZYX, z, y, x),
                    scale: DVec3::from_array(t.scale),
                },
            );
        }
        self.resolve_requested(&overrides)?;
        Ok(())
    }
    fn resolve_requested(
        &self,
        overrides: &BTreeMap<String, Pose>,
    ) -> Result<ResolvedPoses, SceneError> {
        if self.target_count() > MAX_OBJECTS {
            return Err(invalid("hierarchy combined target budget exceeded"));
        }
        for (id, parent) in &self.hierarchy.parents {
            if self.target_transform(id).is_none() || !self.hierarchy.groups.contains_key(parent) {
                return Err(invalid("missing hierarchy target/group parent"));
            }
            if self
                .lighting
                .light(id)
                .is_some_and(|l| matches!(l.kind, LightKind::Directional { .. }))
            {
                return Err(invalid("Sun remains scene-level"));
            }
        }
        for group in self.hierarchy.groups.values() {
            group.transform.validate()?;
            if group.transform.scale[0] != group.transform.scale[1]
                || group.transform.scale[0] != group.transform.scale[2]
            {
                return Err(invalid("groups require positive uniform scale"));
            }
        }
        let mut result = ResolvedPoses {
            transforms: BTreeMap::new(),
            poses: BTreeMap::new(),
        };
        let ids = self
            .objects
            .iter()
            .map(|o| o.id.as_str())
            .chain(self.lighting.lights().map(|(id, _)| id))
            .chain(self.hierarchy.groups.keys().map(String::as_str));
        for id in ids {
            let mut chain = Vec::new();
            let mut current = id;
            while let Some(parent) = self.hierarchy.parents.get(current) {
                if chain.len() >= 8 || chain.contains(&parent.as_str()) || parent == id {
                    return Err(invalid(
                        "hierarchy cycle or eight-level qualification bound exceeded",
                    ));
                }
                chain.push(parent.as_str());
                current = parent;
            }
            if chain.len() + usize::from(self.hierarchy.groups.contains_key(id)) > 8 {
                return Err(invalid("eight group levels exceeded"));
            }
            let local = self
                .target_transform(id)
                .ok_or_else(|| invalid("missing local transform"))?;
            let mut pose = overrides
                .get(id)
                .copied()
                .unwrap_or_else(|| Pose::from_local(local));
            // Apply nearest parent first, then its ancestors.
            for parent in chain {
                pose = overrides
                    .get(parent)
                    .copied()
                    .unwrap_or_else(|| Pose::from_local(&self.hierarchy.groups[parent].transform))
                    .compose(pose);
            }
            // Lights follow position/orientation, but keep authored scale/range/intensity.
            if self.lighting.light(id).is_some() {
                pose.scale = DVec3::ONE;
            }
            let stored = if self.hierarchy.parents.contains_key(id) || overrides.contains_key(id) {
                pose.stored()?
            } else {
                local.clone()
            };
            if let Some(object) = self.objects.iter().find(|o| o.id == id) {
                if let Some(collider) = object.collider {
                    collider.resolve(&stored)?;
                }
            }
            result.transforms.insert(id.into(), stored);
            result.poses.insert(id.into(), pose);
        }
        Ok(result)
    }
}

pub(crate) fn reject_legacy_fields(value: &serde_json::Value) -> Result<(), SceneError> {
    let scene_field = value
        .get("scene")
        .and_then(|s| s.as_object())
        .is_some_and(|s| s.contains_key("hierarchy"));
    let history_field = ["undo", "redo"]
        .iter()
        .filter_map(|key| value.get(key).and_then(|v| v.as_array()))
        .flatten()
        .any(|change| {
            let is_hierarchy = |delta: &serde_json::Value| {
                delta.get("kind").and_then(|v| v.as_str()) == Some("hierarchy")
            };
            is_hierarchy(change)
                || change
                    .get("scene")
                    .and_then(|v| v.as_array())
                    .is_some_and(|a| a.iter().any(is_hierarchy))
        });
    if scene_field || history_field {
        return Err(invalid("assembly hierarchy requires scene format 19"));
    }
    Ok(())
}

impl Session {
    pub fn edit_world_transforms(
        &mut self,
        expected_revision: u64,
        edits: &[(String, Transform)],
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        let locals = self.scene().local_edits_for_world(edits)?;
        self.edit_transforms(expected_revision, &locals)
    }
    pub fn allocate_group_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        allocate_ids("group", count, MAX_OBJECTS, self.reserved_target_ids())
    }
    pub fn create_group(
        &mut self,
        expected_revision: u64,
        id: &str,
        name: Option<String>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        validate_name(name.as_deref())?;
        if self.contains_object_id(id) {
            return Err(invalid("group ID is present or reserved by history"));
        }
        let before = self.scene().hierarchy.clone();
        let mut after = before.clone();
        after.groups.insert(
            id.into(),
            Group {
                name,
                transform: Transform::default(),
            },
        );
        self.commit_change(SceneDelta::Hierarchy { before, after })
    }
    /// Preserve the complete world closure or refuse before changing document/history.
    pub fn reparent(
        &mut self,
        expected_revision: u64,
        id: &str,
        parent: Option<&str>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        // Legacy scripts have root/world coordinate semantics, not local semantics.
        if !self.authoring().is_empty() {
            return Err(invalid(
                "hierarchy parenting with legacy authoring is unsupported",
            ));
        }
        let local = self
            .scene()
            .target_transform(id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.into()))?;
        if self.scene().hierarchy.parents.get(id).map(String::as_str) == parent {
            return Ok(self.scene().revision);
        }
        let old = self.scene().resolve_poses()?;
        let world = old.poses[id];
        let relative = if let Some(parent) = parent {
            if !self.scene().hierarchy.groups.contains_key(parent) {
                return Err(invalid("only groups may parent"));
            }
            let p = old.poses[parent];
            Pose {
                position: (p.rotation.inverse() * (world.position - p.position)) / p.scale,
                rotation: (p.rotation.inverse() * world.rotation).normalize(),
                scale: if self.scene().lighting.light(id).is_some() {
                    DVec3::ONE
                } else {
                    world.scale / p.scale
                },
            }
        } else {
            world
        };
        let stored = relative.stored()?;
        let before = self.scene().hierarchy.clone();
        let mut after = before.clone();
        if let Some(parent) = parent {
            after.parents.insert(id.into(), parent.into());
        } else {
            after.parents.remove(id);
        }
        let mut changes = vec![SceneDelta::Hierarchy { before, after }];
        if local != &stored {
            if self.scene().lighting.light(id).is_some() {
                let before = self.scene().lighting.clone();
                let mut after = before.clone();
                let Lighting::Authored { lights, .. } = &mut after else {
                    unreachable!()
                };
                lights.get_mut(id).expect("existing light").transform = stored;
                changes.push(SceneDelta::Lighting { before, after });
            } else {
                changes.push(SceneDelta::Transform {
                    id: id.into(),
                    before: local.clone(),
                    after: stored,
                });
            }
        }
        let change = Change {
            scene: changes,
            ..Change::default()
        };
        let mut staged = self.scene().clone();
        let mut authoring = self.authoring().clone();
        change.apply(&mut staged, &mut authoring, false)?;
        let resolved = staged.resolve_poses()?;
        for (id, pose) in &old.poses {
            if !close(pose.matrix(), resolved.poses[id].matrix())
                || !close(
                    Pose::from_local(&old.transforms[id]).matrix(),
                    Pose::from_local(&resolved.transforms[id]).matrix(),
                )
            {
                return Err(invalid(
                    "reparent exceeds complete descendant world preservation tolerance",
                ));
            }
        }
        self.commit_change(change)
    }
}
