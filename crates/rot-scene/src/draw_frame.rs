//! Immutable runtime scene data shared by simulation and viewport consumers.
use crate::component_scene::TransformCameraPose;
use crate::{Lighting, ResolvedPlayerView, ResolvedPoses, Scene, Transform};
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
    lighting: Lighting,
    player_view: Option<ResolvedPlayerView>,
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
            lighting,
            player_view,
        }
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

    /// The topology and light transforms are unchanged during a leaf-only step.
    pub fn update_leaves(
        &mut self,
        leaves: impl IntoIterator<Item = (usize, Mat4)>,
        player_view: Option<ResolvedPlayerView>,
    ) {
        for (index, matrix) in leaves {
            self.matrices[index] = matrix;
        }
        self.player_view = player_view;
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
