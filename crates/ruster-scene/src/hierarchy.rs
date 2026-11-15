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
            let mut pose = Pose::from_local(local);
            // Apply nearest parent first, then its ancestors.
            for parent in chain {
                pose = Pose::from_local(&self.hierarchy.groups[parent].transform).compose(pose);
            }
            // Lights follow position/orientation, but keep authored scale/range/intensity.
            if self.lighting.light(id).is_some() {
                pose.scale = DVec3::ONE;
            }
            let stored = if self.hierarchy.parents.contains_key(id) {
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
