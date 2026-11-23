//! Disposable render input for native viewports while the shared viewport
//! consumes legacy draw objects. This projection is never written to disk.
use super::{
    Camera, CameraMatrices, CameraState, DrawScene, NavigationOptions, ViewPreset,
    ViewportInteraction, ViewportSurface,
};
use eframe::egui;
use glam::{DMat4, DVec3, EulerRot, Mat3, Mat4, Quat, Vec3};
use rot_scene::component_scene::{self, Component, ComponentScene, Geometry};
use rot_scene::{
    Light, LightKind, Lighting, Object, Scene, SceneError, Transform, MAX_DIRECTIONAL_LIGHTS,
    MAX_POINT_LIGHTS,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

/// Render-only draw data. The matrices stay aligned with the draw rows even
/// when several mesh components belong to one object.
#[derive(Clone, Debug)]
pub struct ComponentPreview {
    scene: Arc<Scene>,
    matrices: Vec<Mat4>,
    unpickable: BTreeSet<String>,
    light_budget_exceeded: bool,
    transform_camera: Option<rot_scene::component_scene::TransformCameraPose>,
}

impl ComponentPreview {
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    pub fn has_player_camera(&self) -> bool {
        self.transform_camera.is_some() || self.scene.player_view.is_some()
    }

    pub fn transform_camera(&self) -> Option<rot_scene::component_scene::TransformCameraPose> {
        self.transform_camera
    }

    pub fn scene_snapshot(&self) -> Arc<Scene> {
        Arc::clone(&self.scene)
    }

    /// A display-only edit leaves draw rows, poses and asset bindings intact.
    pub fn set_display(&mut self, display: rot_scene::DisplayProfile) {
        Arc::make_mut(&mut self.scene).display = display;
    }

    pub fn draw_matrices(&self) -> &[Mat4] {
        &self.matrices
    }

    /// Refresh transient translations from committed double-precision world positions.
    pub fn set_translated_rows(
        &mut self,
        draw_rows: &[(usize, DVec3)],
        light_positions: &[(String, DVec3)],
        displacement: DVec3,
    ) -> Result<(), SceneError> {
        for &(row, original) in draw_rows {
            let position = (original + displacement).as_vec3();
            if !position.is_finite() {
                return Err(SceneError::Invalid(
                    "world move exceeds renderer precision".into(),
                ));
            }
            self.matrices[row].w_axis = position.extend(1.0);
        }
        if !light_positions.is_empty() {
            let Lighting::Authored { lights, .. } = &mut Arc::make_mut(&mut self.scene).lighting
            else {
                unreachable!("preview light IDs require authored lighting");
            };
            for (id, original) in light_positions {
                let light = lights.get_mut(id).expect("preview light ID");
                light.transform.translation = (original + displacement).as_vec3().to_array();
                light.transform.validate()?;
            }
        }
        Ok(())
    }

    /// Stage matrices and light poses for touched objects using the same projection
    /// as an authored refresh. Light kind and shadow settings stay on the light.
    pub fn set_transformed_rows<'a>(
        &mut self,
        draws: impl Iterator<Item = (usize, DMat4)>,
        lights: impl Iterator<Item = (&'a str, DMat4)>,
    ) -> Result<(), SceneError> {
        for (row, world) in draws {
            let object = &self.scene.objects[row];
            self.matrices[row] = draw_matrix(world, &object.id)?;
        }
        let mut lights = lights.peekable();
        if lights.peek().is_some() {
            let Lighting::Authored {
                lights: authored, ..
            } = &mut Arc::make_mut(&mut self.scene).lighting
            else {
                unreachable!("preview light IDs require authored lighting");
            };
            for (id, world) in lights {
                let light = authored.get_mut(id).expect("preview light ID");
                light.transform = light_transform(world, &light.kind, id)?;
            }
        }
        Ok(())
    }

    pub fn light_budget_exceeded(&self) -> bool {
        self.light_budget_exceeded
    }

    fn draw_scene(&self) -> DrawScene<'_> {
        DrawScene {
            scene: &self.scene,
            matrices: Some(&self.matrices),
            unpickable: (!self.unpickable.is_empty()).then_some(&self.unpickable),
            hidden_mesh_renderers: None,
            transform_camera: self.transform_camera,
        }
    }

    /// Fit only the requested visible objects, including each mesh component's
    /// world matrix. The caller supplies the current viewport aspect and view.
    pub fn framed_camera(
        &self,
        ids: &std::collections::HashSet<&str>,
        meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
        aspect: f32,
        current: Option<CameraState>,
        direction: Option<ViewPreset>,
    ) -> Option<CameraState> {
        let mut matches = self
            .draw_scene()
            .objects()
            .filter(|(object, _)| ids.contains(object.id.as_str()))
            .peekable();
        matches.peek()?;
        let fit = Camera::fit_objects(matches, meshes, aspect)?;
        let mut camera = current.map_or(fit, Camera::from_state);
        camera.frame(fit);
        if let Some(direction) = direction {
            camera.set_preset(direction);
        }
        camera.checked_matrices(aspect).map(|_| camera.state())
    }

    /// Query the same draw objects, world matrices and picking locks as the
    /// editor, using the projection from the captured viewport frame.
    pub fn pick_at(
        &self,
        meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
        matrices: CameraMatrices,
        rect: egui::Rect,
        position: egui::Pos2,
    ) -> Option<String> {
        if !rect.is_finite() || !rect.is_positive() || !rect.contains(position) {
            return None;
        }
        let (view, projection) = matrices;
        super::pick(
            self.draw_scene(),
            meshes,
            Mat4::from_cols_array_2d(&projection) * Mat4::from_cols_array_2d(&view),
            (position.x - rect.left()) / rect.width() * 2. - 1.,
            1. - (position.y - rect.top()) / rect.height() * 2.,
        )
    }

    pub fn render(
        &self,
        ui: &mut egui::Ui,
        selected: &[&str],
        meshes: &BTreeMap<String, Arc<rot_assets::Mesh>>,
        textures: &BTreeMap<String, Arc<rot_assets::Texture>>,
        navigation: &mut NavigationOptions,
        surface: ViewportSurface,
    ) -> ViewportInteraction {
        super::render_draw_scene(
            ui,
            self.draw_scene(),
            selected,
            meshes,
            textures,
            navigation,
            surface,
        )
    }
}

fn draw_matrix(world: DMat4, id: &str) -> Result<Mat4, SceneError> {
    let matrix = world.as_mat4();
    let linear = Mat3::from_mat4(matrix);
    let determinant = linear.determinant();
    if !matrix.is_finite()
        || !determinant.is_finite()
        || determinant <= 0.0
        || !linear.inverse().is_finite()
    {
        return Err(SceneError::Invalid(format!(
            "world transform on {id} exceeds renderer precision"
        )));
    }
    Ok(matrix)
}

fn light_transform(world: DMat4, kind: &LightKind, id: &str) -> Result<Transform, SceneError> {
    let mut transform = Transform {
        translation: world.w_axis.truncate().as_vec3().to_array(),
        ..Transform::default()
    };
    if matches!(kind, LightKind::Directional { .. }) {
        let direction = world.z_axis.truncate();
        let magnitude = direction.abs().max_element();
        if !magnitude.is_finite() || magnitude == 0.0 {
            return Err(SceneError::Invalid(format!(
                "world direction on {id} exceeds renderer precision"
            )));
        }
        let axis = (direction / magnitude).normalize().as_vec3();
        let (z, y, x) = Quat::from_rotation_arc(Vec3::Z, axis).to_euler(EulerRot::ZYX);
        transform.rotation = [x, y, z];
    }
    transform.validate()?;
    Ok(transform)
}

pub enum LoadedPreview {
    Legacy(Scene),
    Components(ComponentPreview),
}

impl LoadedPreview {
    pub fn scene(&self) -> &Scene {
        match self {
            Self::Legacy(scene) => scene,
            Self::Components(preview) => preview.scene(),
        }
    }
}

pub fn open(path: &Path) -> Result<LoadedPreview, SceneError> {
    match ComponentScene::open(path) {
        Ok(scene) => project(&scene).map(LoadedPreview::Components),
        Err(SceneError::UnsupportedVersion(version))
            if version < u64::from(component_scene::FORMAT_VERSION) =>
        {
            Scene::load(path).map(LoadedPreview::Legacy)
        }
        Err(error) => Err(error),
    }
}

pub fn project(component_scene: &ComponentScene) -> Result<ComponentPreview, SceneError> {
    project_editor(component_scene, &BTreeSet::new(), BTreeSet::new())
}

/// Build an editor-only preview from effective visibility and picking state.
pub fn project_editor(
    component_scene: &ComponentScene,
    hidden: &BTreeSet<String>,
    unpickable: BTreeSet<String>,
) -> Result<ComponentPreview, SceneError> {
    project_editor_with_world(component_scene, hidden, unpickable).map(|(preview, _)| preview)
}

/// Return the world poses already evaluated for this projection.
pub fn project_editor_with_world(
    component_scene: &ComponentScene,
    hidden: &BTreeSet<String>,
    unpickable: BTreeSet<String>,
) -> Result<(ComponentPreview, Vec<DMat4>), SceneError> {
    let hierarchy = component_scene.validate()?;
    use rot_scene::component_scene::CameraView;
    let world = component_scene.world_matrices_in(&hierarchy)?;
    let mut scene = Scene::empty(component_scene.id.clone());
    scene.materials.clone_from(&component_scene.materials);
    scene.display = component_scene.display;
    let transform_camera = match component_scene.effective_camera(&hierarchy) {
        Some((index, CameraView::Transform(lens))) => {
            Some(component_scene.transform_camera_pose(index, &hierarchy, &world, lens)?)
        }
        Some((_, CameraView::Orbit(view))) if view.followed_target().is_none() => {
            scene.player_view = Some(view.clone());
            None
        }
        _ => None,
    };
    let mut lights = BTreeMap::new();
    let mut matrices = Vec::new();
    let mut point_lights = 0;
    let mut directional_lights = 0;
    let mut light_budget_exceeded = false;
    let mut authored_lighting = component_scene.ambient.is_some();
    for (index, object) in component_scene.objects.iter().enumerate() {
        authored_lighting |= object
            .components
            .iter()
            .any(|component| matches!(component.kind, Component::Light { .. }));
        if !hierarchy.effectively_enabled[index]
            || !hidden.is_empty() && hidden.contains(&object.id)
        {
            continue;
        }
        for component in object.components.iter().filter(|item| item.enabled) {
            match &component.kind {
                Component::MeshRenderer {
                    geometry,
                    tint,
                    material,
                } => {
                    let mut draw = Object::cube(object.id.clone(), Some(object.name.clone()));
                    draw.transform = object.transform.clone();
                    matrices.push(draw_matrix(world[index], &object.id)?);
                    draw.color = *tint;
                    draw.material.clone_from(material);
                    match geometry {
                        Geometry::Cube => {}
                        Geometry::Primitive { shape } => draw.primitive = Some(*shape),
                        Geometry::Imported { asset } => draw.mesh = Some(asset.clone()),
                    }
                    scene.objects.push(draw);
                }
                Component::Light {
                    color,
                    intensity,
                    kind,
                } => {
                    let (count, limit) = match kind {
                        LightKind::Point { .. } => (&mut point_lights, MAX_POINT_LIGHTS),
                        LightKind::Directional { .. } => {
                            (&mut directional_lights, MAX_DIRECTIONAL_LIGHTS)
                        }
                    };
                    if *count == limit {
                        light_budget_exceeded = true;
                        continue;
                    }
                    *count += 1;
                    let transform = light_transform(world[index], kind, &object.id)?;
                    lights.insert(
                        component.id.clone(),
                        Light {
                            name: Some(object.name.clone()),
                            transform,
                            enabled: true,
                            color: *color,
                            intensity: *intensity,
                            kind: kind.clone(),
                        },
                    );
                }
                Component::Camera { .. }
                | Component::Collider { .. }
                | Component::Animator { .. }
                | Component::Script { .. } => {}
            }
        }
    }
    if authored_lighting {
        scene.lighting = Lighting::Authored {
            ambient: component_scene.ambient.clone().unwrap_or_default(),
            lights,
        };
    }
    scene.lighting.validate()?;
    Ok((
        ComponentPreview {
            scene: Arc::new(scene),
            matrices,
            unpickable,
            light_budget_exceeded,
            transform_camera,
        },
        world,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{Group, Session};
    use std::fs;

    #[test]
    fn parented_directional_light_uses_rotated_local_axis_in_gpu_packet() {
        use crate::lighting_gpu::LightingPacket;
        use rot_scene::component_scene::{ComponentInstance, ObjectNode};
        use std::f32::consts::{FRAC_PI_2, FRAC_PI_6};

        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("sun")).unwrap()).unwrap();
        scene.objects = vec![
            ObjectNode {
                id: "parent".into(),
                name: "Parent".into(),
                enabled: true,
                parent: None,
                transform: Transform {
                    rotation: [0.0, FRAC_PI_2, 0.0],
                    scale: [2.0, 1.0, 3.0],
                    ..Transform::default()
                },
                components: Vec::new(),
            },
            ObjectNode {
                id: "lamp".into(),
                name: "Directional Light".into(),
                enabled: true,
                parent: Some("parent".into()),
                transform: Transform {
                    rotation: [-FRAC_PI_6, 0.0, 0.0],
                    ..Transform::default()
                },
                components: vec![ComponentInstance {
                    id: "directional".into(),
                    enabled: true,
                    kind: Component::Light {
                        color: [1.0; 3],
                        intensity: 0.72,
                        kind: Light::directional(None).kind,
                    },
                }],
            },
        ];
        // +Z is the shader's direction to the light; the emitted ray is -Z.
        let expected = Vec3::new(3.0 * FRAC_PI_6.cos(), FRAC_PI_6.sin(), 0.0).normalize();
        let packet = |scene: &ComponentScene| {
            LightingPacket::try_pack(&project(scene).unwrap().scene().lighting).unwrap()
        };
        let incoming = packet(&scene).direction_to_light;
        assert!(Vec3::from_slice(&incoming[..3]).abs_diff_eq(expected, 1e-5));
        scene.objects[0].transform.translation = [20.0, 8.0, -10.0];
        scene.objects[1].transform.translation = [-2.0, 3.0, 4.0];
        assert_eq!(packet(&scene).direction_to_light, incoming);
    }

    #[test]
    fn frame_uses_only_target_world_geometry_in_all_three_views() {
        let mut setup = Scene::demo();
        setup.objects[0].transform.translation = [120., 2., -25.];
        let scene = ComponentScene::from_legacy(&Session::new(setup).unwrap()).unwrap();
        let preview = project(&scene).unwrap();
        let ids = std::collections::HashSet::from(["left"]);
        for (preset, yaw, pitch) in [
            (ViewPreset::Front, 0., 0.),
            (ViewPreset::Right, std::f32::consts::FRAC_PI_2, 0.),
            (ViewPreset::Top, 0., std::f32::consts::FRAC_PI_2),
        ] {
            let camera = preview
                .framed_camera(&ids, &BTreeMap::new(), 0.6, None, Some(preset))
                .unwrap();
            assert_eq!(camera.target, [120., 2., -25.]);
            assert_eq!(
                (camera.yaw, camera.pitch, camera.orthographic),
                (yaw, pitch, true)
            );
            assert!(camera.distance < 5. && camera.matrices(0.6).is_some());
        }
    }

    #[test]
    fn sheared_mesh_draw_transform_is_shared_by_fit_and_hit_testing() {
        use rot_scene::component_scene::ObjectNode;
        let mut legacy = Scene::demo();
        legacy.objects.truncate(1);
        legacy.objects[0].transform.rotation = [0.0, 0.0, 0.6];
        let mut components = ComponentScene::from_legacy(&Session::new(legacy).unwrap()).unwrap();
        components.objects[0].parent = Some("parent".into());
        components.objects.push(ObjectNode {
            id: "parent".into(),
            name: "Parent".into(),
            enabled: true,
            parent: None,
            transform: Transform {
                translation: [8.0, 5.0, 2.0],
                rotation: [0.0, 0.4, 0.0],
                scale: [2.0, 0.5, 3.0],
            },
            components: Vec::new(),
        });
        let world = components.world_matrices().unwrap()[0];
        assert!(Transform::try_from_affine(world).is_err());
        let preview = project(&components).unwrap();
        assert_eq!(preview.matrices, [world.as_mat4()]);
        let draw = DrawScene {
            scene: preview.scene(),
            matrices: Some(&preview.matrices),
            unpickable: None,
            hidden_mesh_renderers: None,
            transform_camera: None,
        };
        let meshes = BTreeMap::new();
        let camera = super::super::Camera::fit_objects(draw.objects(), &meshes, 1.0).unwrap();
        let target = world.w_axis.truncate().as_vec3();
        assert!(camera.target.abs_diff_eq(target, 1e-5));
        let (view, projection, _) = camera.checked_matrices(1.0).unwrap();
        let vp = projection * view;
        let ndc = vp.project_point3(target);
        assert_eq!(
            super::super::pick(draw, &meshes, vp, ndc.x, ndc.y).as_deref(),
            Some("left")
        );
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 100.0));
        let sample = egui::pos2((ndc.x + 1.) * 50., (1. - ndc.y) * 50.);
        let matrices = (view.to_cols_array_2d(), projection.to_cols_array_2d());
        assert_eq!(
            preview.pick_at(&meshes, matrices, rect, sample).as_deref(),
            Some("left")
        );
        let select = egui::Rect::from_center_size(rect.center(), egui::vec2(5.0, 5.0));
        assert_eq!(
            super::super::objects_in_rect(draw, &meshes, vp, rect, select),
            ["left"]
        );

        // The locked object remains drawn and framed, but picking must use
        // the same geometry filter for both a click and a rectangle.
        let locked = ["left".into()].into_iter().collect();
        let draw = DrawScene {
            unpickable: Some(&locked),
            hidden_mesh_renderers: None,
            transform_camera: None,
            ..draw
        };
        let locked_preview = project_editor(&components, &BTreeSet::new(), locked.clone()).unwrap();
        assert_eq!(
            locked_preview.pick_at(&meshes, matrices, rect, sample),
            None
        );
        assert_eq!(draw.objects().count(), 1);
        assert_eq!(super::super::pick(draw, &meshes, vp, ndc.x, ndc.y), None);
        assert!(super::super::objects_in_rect(draw, &meshes, vp, rect, select).is_empty());
    }

    #[test]
    fn viewer_loads_both_formats_and_preserves_projected_geometry() {
        let directory = tempfile::tempdir().unwrap();
        let legacy_file = directory.path().join("legacy.json");
        let component_file = directory.path().join("component.json");
        let mut legacy = Scene::demo();
        legacy.hierarchy.groups.insert(
            "parent".into(),
            Group {
                name: Some("Parent".into()),
                transform: Transform {
                    translation: [1.0, 1.0, 1.0],
                    scale: [2.0; 3],
                    ..Transform::default()
                },
            },
        );
        legacy
            .hierarchy
            .parents
            .insert("left".into(), "parent".into());
        let poses = legacy.resolve_poses().unwrap();
        let mut session = Session::new(legacy.clone()).unwrap();
        session.save(&legacy_file).unwrap();
        assert_eq!(open(&legacy_file).unwrap().scene(), &legacy);
        let mut converted = ComponentScene::from_legacy(&session).unwrap();
        converted.objects[1].enabled = false;
        converted.save_new(&component_file).unwrap();
        let original = fs::read(&component_file).unwrap();

        let LoadedPreview::Components(preview) = open(&component_file).unwrap() else {
            panic!("component preview")
        };
        assert_eq!(preview.scene().objects.len(), legacy.objects.len() - 1);
        assert_eq!(preview.scene().objects[0].id, "left");
        assert!(preview.matrices[0].abs_diff_eq(
            super::super::model(&Object {
                transform: poses.transforms["left"].clone(),
                ..legacy.objects[0].clone()
            }),
            1e-6
        ));
        assert_eq!(preview.scene().objects[0].color, legacy.objects[0].color);
        assert!(preview.scene().hierarchy.is_empty());
        assert_eq!(fs::read(&component_file).unwrap(), original);
    }
}
