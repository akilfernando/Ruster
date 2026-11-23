//! Immutable runtime scene data shared by simulation and viewport consumers.
use crate::component_scene::{AttachedLights, TransformCameraPose};
use crate::{Lighting, ResolvedPlayerView, ResolvedPoses, Scene, SceneError, Transform};
use glam::{EulerRot, Mat4, Quat, Vec3};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct SceneDrawFrame {
    scene: Arc<Scene>,
    hierarchy: Option<Arc<HierarchyDraw>>,
    transform_camera: Option<TransformCameraPose>,
}

impl SceneDrawFrame {
    pub fn new(scene: Scene, hierarchy: Option<Arc<HierarchyDraw>>) -> Self {
        Self::from_snapshot(Arc::new(scene), hierarchy)
    }

    pub fn from_snapshot(scene: Arc<Scene>, hierarchy: Option<Arc<HierarchyDraw>>) -> Self {
        Self {
            scene,
            hierarchy,
            transform_camera: None,
        }
    }

    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn snapshot(&self) -> Arc<Scene> {
        self.scene.clone()
    }

    pub fn hierarchy(&self) -> Option<&Arc<HierarchyDraw>> {
        self.hierarchy.as_ref()
    }

    pub fn with_transform_camera(mut self, camera: Option<TransformCameraPose>) -> Self {
        self.transform_camera = camera;
        self
    }

    pub fn transform_camera(&self) -> Option<TransformCameraPose> {
        self.transform_camera
    }

    /// Unrestricted mutation invalidates prepared matrices before any write.
    pub fn scene_mut(&mut self) -> &mut Scene {
        self.hierarchy = None;
        self.transform_camera = None;
        Arc::make_mut(&mut self.scene)
    }
}

#[derive(Clone, Debug)]
pub struct HierarchyDraw {
    matrices: Vec<Mat4>,
    lighting: Arc<Lighting>,
    player_view: Option<ResolvedPlayerView>,
    has_attached_lights: bool,
}

impl HierarchyDraw {
    /// Both the scene and its validated poses must describe the same commit.
    pub fn resolved(
        scene: &Scene,
        poses: &ResolvedPoses,
        player_view: Option<ResolvedPlayerView>,
    ) -> Self {
        let matrices = scene
            .objects
            .iter()
            .map(|object| draw_matrix(&poses.transforms[&object.id]))
            .collect();
        let mut lighting = scene.lighting.clone();
        if let Lighting::Authored { lights, .. } = &mut lighting {
            for (id, light) in lights {
                light.transform = poses.transforms[id].clone();
            }
        }
        Self {
            matrices,
            lighting: Arc::new(lighting),
            player_view,
            has_attached_lights: false,
        }
    }

    pub fn with_attached_lights(
        mut self,
        scene: &Scene,
        poses: &ResolvedPoses,
        attached: &AttachedLights,
    ) -> Result<Self, SceneError> {
        let Lighting::Authored { lights, .. } = Arc::make_mut(&mut self.lighting) else {
            return Err(SceneError::Invalid(
                "Attached Lights require authored lighting".into(),
            ));
        };
        for entry in attached.iter() {
            if scene
                .objects
                .get(entry.owner_index)
                .map(|owner| owner.id.as_str())
                != Some(entry.owner_id.as_str())
                || lights.contains_key(&entry.owner_id)
            {
                return Err(SceneError::Invalid(format!(
                    "Attached Light owner {} changed",
                    entry.owner_id
                )));
            }
            let transform = poses
                .transforms
                .get(&entry.owner_id)
                .ok_or_else(|| SceneError::ObjectNotFound(entry.owner_id.clone()))?;
            let mut light = entry.light.clone();
            light.transform = light_pose(transform);
            lights.insert(entry.owner_id.clone(), light);
        }
        self.lighting.validate()?;
        self.has_attached_lights = true;
        Ok(self)
    }

    pub fn attached_lighting(&self) -> Option<&Lighting> {
        self.has_attached_lights.then_some(self.lighting.as_ref())
    }

    pub fn matrices(&self) -> &[Mat4] {
        &self.matrices
    }

    pub fn lighting(&self) -> &Lighting {
        &self.lighting
    }

    pub fn player_view(&self) -> Option<ResolvedPlayerView> {
        self.player_view
    }

    /// Leaf commits update only the validated changed matrices and light poses.
    pub fn update_leaves<'a>(
        &mut self,
        leaves: impl IntoIterator<Item = (usize, Mat4)>,
        attached: impl IntoIterator<Item = (&'a str, Transform)>,
        player_view: Option<ResolvedPlayerView>,
    ) {
        for (index, matrix) in leaves {
            self.matrices[index] = matrix;
        }
        let mut attached = attached.into_iter().peekable();
        if attached.peek().is_some() {
            let Lighting::Authored { lights, .. } = Arc::make_mut(&mut self.lighting) else {
                unreachable!("bound attached lighting");
            };
            for (id, transform) in attached {
                lights.get_mut(id).expect("bound attached light").transform =
                    light_pose(&transform);
            }
        }
        self.player_view = player_view;
    }
}

fn light_pose(world: &Transform) -> Transform {
    Transform {
        scale: [1.; 3],
        ..world.clone()
    }
}

pub fn draw_matrix(transform: &Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::from(transform.scale),
        Quat::from_euler(
            EulerRot::ZYX,
            transform.rotation[2],
            transform.rotation[1],
            transform.rotation[0],
        ),
        Vec3::from(transform.translation),
    )
}
