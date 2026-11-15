//! Example/test-only hierarchy adapter. No serde, shipped API or retained cache.
use glam::{DMat4, DQuat, DVec3, EulerRot};
use ruster_scene::{Collider, ColliderRole, ColliderShape, Light, Lighting, Scene, Transform};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DEPTH: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: String,
    pub parent: Option<String>,
    pub local: Transform,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Assembly {
    pub scene: Scene,
    pub groups: Vec<Group>,
    pub parents: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
struct Pose {
    translation: DVec3,
    rotation: DQuat,
    scale: DVec3,
}

impl Pose {
    fn local(t: &Transform) -> Self {
        let [x, y, z] = t.rotation.map(f64::from);
        Self {
            translation: DVec3::from_array(t.translation.map(f64::from)),
            rotation: DQuat::from_euler(EulerRot::ZYX, z, y, x),
            scale: DVec3::from_array(t.scale.map(f64::from)),
        }
    }

    fn compose(self, local: Self) -> Self {
        // The parent is exactly uniform by admission: no shear/decomposition.
        Self {
            translation: self.translation + self.rotation * (self.scale * local.translation),
            rotation: (self.rotation * local.rotation).normalize(),
            scale: self.scale * local.scale,
        }
    }

    fn relative_to(self, parent: Self) -> Self {
        Self {
            translation: (parent.rotation.inverse() * (self.translation - parent.translation))
                / parent.scale,
            rotation: (parent.rotation.inverse() * self.rotation).normalize(),
            scale: self.scale / parent.scale,
        }
    }

    fn matrix(self) -> DMat4 {
        DMat4::from_scale_rotation_translation(self.scale, self.rotation, self.translation)
    }

    fn quantize(self) -> Result<Transform, String> {
        // Reject out-of-range f64 values before rounding could admit them as f32.
        if !self.translation.is_finite()
            || self.translation.abs().max_element() > 10_000.0
            || !self.scale.is_finite()
            || self.scale.min_element() < 0.001
            || self.scale.max_element() > 1000.0
            || !self.rotation.is_finite()
        {
            return Err("resolved pose exceeds prototype numeric bounds".into());
        }
        let (z, y, x) = self.rotation.to_euler(EulerRot::ZYX);
        let result = Transform {
            translation: self.translation.as_vec3().to_array(),
            rotation: [x as f32, y as f32, z as f32],
            scale: self.scale.as_vec3().to_array(),
        };
        result.validate().map_err(|error| error.to_string())?;
        if !matrices_close(self.matrix(), Self::local(&result).matrix()) {
            return Err("world pose cannot be represented within prototype tolerance".into());
        }
        Ok(result)
    }
}

/// Explicit prototype tolerance: per element abs(a-b) <= 2e-6*max(1,abs(a)).
pub fn matrices_close(a: DMat4, b: DMat4) -> bool {
    a.to_cols_array()
        .into_iter()
        .zip(b.to_cols_array())
        .all(|(a, b)| a.is_finite() && b.is_finite() && (a - b).abs() <= 2e-6 * a.abs().max(1.0))
}

pub fn matrix(t: &Transform) -> DMat4 {
    Pose::local(t).matrix()
}

impl Assembly {
    fn parent(&self, id: &str) -> Option<&str> {
        self.groups
            .iter()
            .find(|group| group.id == id)
            .and_then(|group| group.parent.as_deref())
            .or_else(|| self.parents.get(id).map(String::as_str))
    }

    fn local(&self, id: &str) -> Result<&Transform, String> {
        self.groups
            .iter()
            .find(|group| group.id == id)
            .map(|group| &group.local)
            .or_else(|| self.scene.target_transform(id))
            .ok_or_else(|| format!("missing target {id}"))
    }

    fn local_mut(&mut self, id: &str) -> Result<&mut Transform, String> {
        if let Some(group) = self.groups.iter_mut().find(|group| group.id == id) {
            return Ok(&mut group.local);
        }
        if let Some(object) = self.scene.objects.iter_mut().find(|object| object.id == id) {
            return Ok(&mut object.transform);
        }
        if let Lighting::Authored { lights, .. } = &mut self.scene.lighting {
            if let Some(light) = lights.get_mut(id) {
                return Ok(&mut light.transform);
            }
        }
        Err(format!("missing target {id}"))
    }

    fn group_pose(
        &self,
        id: &str,
        path: &mut BTreeSet<String>,
        memo: &mut BTreeMap<String, Pose>,
    ) -> Result<Pose, String> {
        if path.contains(id) {
            return Err("cycle in group ancestry".into());
        }
        if path.len() >= MAX_DEPTH {
            return Err("prototype depth limit exceeded".into());
        }
        // Depth admission must inspect the path even if a shallower path was memoized.
        let group = self
            .groups
            .iter()
            .find(|group| group.id == id)
            .ok_or_else(|| format!("missing group parent {id}"))?;
        group.local.validate().map_err(|error| error.to_string())?;
        if group.local.scale[0] != group.local.scale[1]
            || group.local.scale[0] != group.local.scale[2]
        {
            return Err("prototype requires exactly uniform group scale (no shear)".into());
        }
        path.insert(id.into());
        let pose = if let Some(parent) = &group.parent {
            self.group_pose(parent, path, memo)?
                .compose(Pose::local(&group.local))
        } else {
            Pose::local(&group.local)
        };
        path.remove(id);
        // Keep numeric admission at existing Transform bounds, before flattening.
        pose.quantize()?;
        memo.insert(id.into(), pose);
        Ok(pose)
    }

    fn worlds(&self) -> Result<BTreeMap<String, Pose>, String> {
        self.scene.validate().map_err(|error| error.to_string())?;
        if self.scene.target_count() + self.groups.len() > ruster_scene::MAX_OBJECTS {
            return Err("prototype combined target capacity exceeded".into());
        }
        let mut ids = BTreeSet::new();
        let mut memo = BTreeMap::new();
        for group in &self.groups {
            if group.id.is_empty()
                || self.scene.target_transform(&group.id).is_some()
                || !ids.insert(&group.id)
            {
                return Err("duplicate or empty group identity".into());
            }
        }
        for (child, parent) in &self.parents {
            if self.scene.target_transform(child).is_none() {
                return Err(format!("missing leaf {child}"));
            }
            if !ids.contains(parent) {
                return Err(format!("missing group parent {parent}"));
            }
            if let Some(light) = self
                .scene
                .lighting
                .lights()
                .find(|(id, _)| *id == child)
                .map(|(_, light)| light)
            {
                if !matches!(light.kind, ruster_scene::LightKind::Point { .. }) {
                    return Err("prototype only parents point lights".into());
                }
            }
        }
        for group in &self.groups {
            self.group_pose(&group.id, &mut BTreeSet::new(), &mut memo)?;
        }
        for object in &self.scene.objects {
            let local = Pose::local(&object.transform);
            let world = self
                .parents
                .get(&object.id)
                .map_or(local, |id| memo[id].compose(local));
            memo.insert(object.id.clone(), world);
        }
        for (id, light) in self.scene.lighting.lights() {
            let local = Pose::local(&light.transform);
            let world = self
                .parents
                .get(id)
                .map_or(local, |id| memo[id].compose(local));
            memo.insert(id.into(), world);
        }
        Ok(memo)
    }

    /// Single flattened existing Scene shared by actual render and spatial consumers.
    pub fn flatten(&self) -> Result<Scene, String> {
        let worlds = self.worlds()?;
        let mut flat = self.scene.clone();
        for object in &mut flat.objects {
            if self.parents.contains_key(&object.id) {
                object.transform = worlds[&object.id].quantize()?;
            }
        }
        if let Lighting::Authored { lights, .. } = &mut flat.lighting {
            for (id, light) in lights {
                if self.parents.contains_key(id) {
                    let mut pose = worlds[id];
                    // Experimental light policy only; not a production format decision.
                    pose.scale = DVec3::ONE;
                    light.transform = pose.quantize()?;
                }
            }
        }
        flat.validate().map_err(|error| error.to_string())?;
        Ok(flat)
    }

    /// Clone/stage/validate/swap: no new Session history contract is implied.
    pub fn edit(&mut self, change: impl FnOnce(&mut Self)) -> Result<(), String> {
        let mut staged = self.clone();
        change(&mut staged);
        staged.flatten()?;
        *self = staged;
        Ok(())
    }

    pub fn reparent(&mut self, id: &str, parent: Option<&str>) -> Result<(), String> {
        let worlds = self.worlds()?;
        let before = *worlds
            .get(id)
            .ok_or_else(|| format!("missing target {id}"))?;
        // Light scale/range policy is deliberately not generalized by this experiment.
        if self.scene.lighting.lights().any(|(light, _)| light == id) {
            return Err("light reparent semantics await product decision".into());
        }
        let local = if let Some(parent) = parent {
            if !self.groups.iter().any(|group| group.id == parent) {
                return Err("parent must be a group".into());
            }
            before.relative_to(worlds[parent])
        } else {
            before
        };
        let quantized = local.quantize()?;
        let mut staged = self.clone();
        *staged.local_mut(id)? = quantized;
        if let Some(group) = staged.groups.iter_mut().find(|group| group.id == id) {
            group.parent = parent.map(String::from);
        } else if let Some(parent) = parent {
            staged.parents.insert(id.into(), parent.into());
        } else {
            staged.parents.remove(id);
        }
        staged.flatten()?;
        if !matrices_close(before.matrix(), staged.worlds()?[id].matrix()) {
            return Err("reparent exceeds prototype world-preservation tolerance".into());
        }
        *self = staged;
        Ok(())
    }

    pub fn translate_selection(
        &mut self,
        selected: &[&str],
        delta: [f64; 3],
    ) -> Result<(), String> {
        let worlds = self.worlds()?;
        let selected: BTreeSet<_> = selected.iter().copied().collect();
        let mut roots = Vec::new();
        for id in &selected {
            self.local(id)?;
            let mut parent = self.parent(id);
            let mut covered = false;
            while let Some(ancestor) = parent {
                covered |= selected.contains(ancestor);
                parent = self.parent(ancestor);
            }
            if !covered {
                roots.push(*id);
            }
        }
        let mut staged = self.clone();
        for id in roots {
            let delta = self.parent(id).map_or(DVec3::from_array(delta), |parent| {
                (worlds[parent].rotation.inverse() * DVec3::from_array(delta))
                    / worlds[parent].scale
            });
            let local = staged.local_mut(id)?;
            local.translation = (DVec3::from_array(local.translation.map(f64::from)) + delta)
                .as_vec3()
                .to_array();
        }
        staged.flatten()?;
        *self = staged;
        Ok(())
    }
}

pub fn fixture() -> Assembly {
    let mut scene = Scene::demo();
    scene.objects.truncate(1);
    scene.objects[0].transform = Transform {
        translation: [1.0, 0.0, 0.0],
        scale: [2.0, 1.0, 0.5],
        ..Transform::default()
    };
    scene.objects[0].collider = Some(Collider {
        enabled: true,
        role: ColliderRole::Sensor,
        offset: [0.25, 0.0, 0.0],
        shape: ColliderShape::Box {
            half_extents: [0.5; 3],
        },
    });
    scene.lighting = Lighting::authored_default("sun");
    if let Lighting::Authored { lights, .. } = &mut scene.lighting {
        let mut light = Light::point(None);
        light.transform.translation = [1.0, 0.5, 0.0];
        lights.insert("bulb".into(), light);
    }
    Assembly {
        scene,
        groups: vec![
            Group {
                id: "root".into(),
                parent: None,
                local: Transform {
                    translation: [3.0, 0.0, 0.0],
                    ..Transform::default()
                },
            },
            Group {
                id: "hinge".into(),
                parent: Some("root".into()),
                local: Transform {
                    translation: [0.0, 2.0, 0.0],
                    ..Transform::default()
                },
            },
        ],
        parents: BTreeMap::from([
            ("left".into(), "hinge".into()),
            ("bulb".into(), "hinge".into()),
        ]),
    }
}

pub fn run_gate() {
    let mut assembly = fixture();
    let flat = assembly.flatten().unwrap();
    assert_eq!(
        flat.objects[0].transform.translation,
        [4.0, 2.0, 0.0],
        "nested ancestry must reach existing Scene consumers"
    );
    assembly
        .edit(|assembly| {
            assembly.groups[0].local.translation[0] += 2.0;
            assembly.groups[1].local.rotation[2] = std::f32::consts::FRAC_PI_2;
        })
        .unwrap();
    let moved = assembly.flatten().unwrap();
    let object = &moved.objects[0];
    let collider = object
        .collider
        .unwrap()
        .resolve(&object.transform)
        .unwrap()
        .unwrap();
    let expected = matrix(&object.transform).transform_point3(DVec3::new(0.25, 0.0, 0.0));
    assert!((DVec3::from_array(collider.center) - expected).length() < 1e-6);
    assert!((object.transform.translation[0] - 5.0).abs() < 1e-6);
    assert!((object.transform.translation[1] - 3.0).abs() < 1e-6);
    let before = matrix(&object.transform);
    assembly.reparent("left", Some("root")).unwrap();
    assert!(matrices_close(
        before,
        matrix(&assembly.flatten().unwrap().objects[0].transform)
    ));
    let before = assembly.flatten().unwrap().objects[0].transform.translation;
    assembly
        .translate_selection(&["root", "hinge", "left"], [1.0, 0.0, 0.0])
        .unwrap();
    let after = assembly.flatten().unwrap().objects[0].transform.translation;
    assert!((after[0] - before[0] - 1.0).abs() < 2e-6);
    println!("nested assembly CPU prototype passed: root move + hinge, nonuniform box, point light, world-preserving reparent, ancestor selection once");
}
