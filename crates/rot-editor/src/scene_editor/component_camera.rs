use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text, ui_kit};
use eframe::egui;
use rot_scene::{
    component_scene::{
        CameraView, Component, ComponentInstance, ObjectNode, OrthographicClip, TransformLens,
        TransformOrthographic,
    },
    PlayerAnchor, PlayerView, SceneError, Transform,
};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct Authoring {
    draft: Option<Draft>,
    submitted: Option<SceneShot>,
    rendered: Option<SceneShot>,
    scene_navigation: Option<Option<rot_render::CameraState>>,
}

struct SceneShot {
    state: rot_render::CameraState,
    matrices: rot_render::CameraMatrices,
    rect: egui::Rect,
    receipt: rot_render::FrameReceipt,
}

enum CameraShot {
    Perspective(PlayerView),
    Orthographic {
        transform: Transform,
        lens: TransformOrthographic,
    },
}

struct Draft {
    shot: CameraShot,
    revision: u64,
    destination: Option<String>,
    choices: Vec<CameraChoice>,
    opened_frame: Option<u64>,
    error: Option<String>,
}

struct CameraChoice {
    id: String,
    label: String,
}

fn camera_choices(objects: &[ObjectNode]) -> Vec<CameraChoice> {
    let mut names = HashMap::new();
    let mut choices = Vec::new();
    for object in objects {
        for component in &object.components {
            if let Component::Camera { view } = &component.kind {
                *names.entry(object.name.as_str()).or_insert(0usize) += 1;
                choices.push((&object.name, component.id.as_str(), camera_mode(view)));
            }
        }
    }
    choices
        .into_iter()
        .map(|(name, id, mode)| CameraChoice {
            id: id.to_owned(),
            label: if names[name.as_str()] > 1 {
                format!("{name} · {mode} · {id}")
            } else {
                format!("{name} · {mode}")
            },
        })
        .collect()
}

fn fixed_shot(
    state: rot_render::CameraState,
    matrices: rot_render::CameraMatrices,
    aspect: f32,
) -> Result<PlayerView, &'static str> {
    if state.orthographic {
        return Err(Text::CameraCapturePerspective.plain());
    }
    let view = PlayerView {
        anchor: PlayerAnchor::Fixed {
            position: state.target,
        },
        yaw: state.yaw,
        pitch: state.pitch,
        distance: state.distance,
        vertical_fov_degrees: 45.,
    };
    let refused = Text::CameraCaptureBounds.plain();
    view.validate_settings().map_err(|_| refused)?;
    let projected = rot_render::player_view_projection(
        rot_scene::ResolvedPlayerView {
            anchor: state.target,
            yaw: state.yaw,
            pitch: state.pitch,
            distance: state.distance,
            vertical_fov_degrees: view.vertical_fov_degrees,
        },
        aspect,
    )
    .ok_or(refused)?;
    if matrices != (projected.view, projected.projection) {
        return Err(refused);
    }
    Ok(view)
}

fn orthographic_shot(
    matrices: rot_render::CameraMatrices,
    aspect: f32,
) -> Result<(Transform, TransformOrthographic), &'static str> {
    use glam::{DMat3, DQuat, EulerRot, Mat4};

    let refused = Text::CameraCaptureOrthoBounds.plain();
    let (view, projection) = matrices;
    let world = Mat4::from_cols_array_2d(&view).as_dmat4().inverse();
    if !world.is_finite() {
        return Err(refused);
    }
    let (z, y, x) = DQuat::from_mat3(&DMat3::from_mat4(world))
        .normalize()
        .to_euler(EulerRot::ZYX);
    let transform = Transform {
        translation: world.w_axis.truncate().as_vec3().to_array(),
        rotation: [x as f32, y as f32, z as f32],
        ..Default::default()
    };
    transform.validate().map_err(|_| refused)?;
    let depth = f64::from(projection[2][2]);
    let near = f64::from(projection[3][2]) / depth;
    let lens = TransformOrthographic::new(
        (2.0 / f64::from(projection[1][1])) as f32,
        Some(aspect),
        OrthographicClip {
            near: near as f32,
            far: (near - depth.recip()) as f32,
        },
    );
    let camera = CameraView::Transform(TransformLens::Orthographic(lens.clone()));
    camera.validate_settings().map_err(|_| refused)?;
    let CameraView::Transform(authored_lens) = camera else {
        unreachable!()
    };
    let [x, y, z] = transform.rotation.map(f64::from);
    let pose = authored_lens
        .resolve_pose(
            transform.affine_matrix(),
            DQuat::from_euler(EulerRot::ZYX, z, y, x),
        )
        .map_err(|_| refused)?;
    let projected = rot_render::transform_camera_projection(pose, aspect).ok_or(refused)?;
    let close = |a: f32, b: f32, scale: f32| {
        a.is_finite() && b.is_finite() && (a - b).abs() <= 24.0 * f32::EPSILON * scale
    };
    let translation_scale = glam::Vec3::from_array(pose.eye).length().max(1.0);
    let view_matches = view
        .iter()
        .zip(projected.view)
        .enumerate()
        .all(|(column, (a, b))| {
            a.iter()
                .zip(b)
                .all(|(&a, b)| close(a, b, if column == 3 { translation_scale } else { 1.0 }))
        });
    let projection_matches = projection
        .iter()
        .flatten()
        .zip(projected.projection.into_iter().flatten())
        .all(|(&a, b)| close(a, b, a.abs().max(b.abs()).max(f32::MIN_POSITIVE)));
    if !view_matches || !projection_matches {
        return Err(refused);
    }
    Ok((transform, lens))
}

impl Authoring {
    pub(super) fn pending(&self) -> bool {
        self.draft.is_some()
    }

    fn observe_paint(&mut self) {
        if self
            .submitted
            .as_ref()
            .is_some_and(|shot| shot.receipt.is_fresh_painted())
        {
            self.rendered = self.submitted.take();
        }
    }

    pub(super) fn begin_frame(
        &mut self,
        game: bool,
        navigation: &mut rot_render::NavigationOptions,
    ) {
        if game {
            self.scene_navigation.get_or_insert(navigation.camera);
            self.submitted = None;
            self.rendered = None;
        } else {
            if let Some(scene_camera) = self.scene_navigation.take() {
                navigation.camera = scene_camera;
            }
            self.observe_paint();
        }
    }

    pub(super) fn submit_scene_view(
        &mut self,
        state: Option<rot_render::CameraState>,
        viewport: &rot_render::ViewportInteraction,
    ) {
        self.submitted = state.map(|state| SceneShot {
            state,
            matrices: (viewport.view, viewport.projection),
            rect: viewport.rect,
            receipt: viewport.receipt.clone(),
        });
    }

    fn painted_shot(&mut self) -> Result<&SceneShot, &'static str> {
        self.observe_paint();
        self.rendered
            .as_ref()
            .ok_or(Text::CameraCaptureUnavailable.plain())
    }

    fn shot(&mut self) -> Result<CameraShot, &'static str> {
        self.painted_shot()?;
        let shot = self.rendered.as_ref().expect("painted shot");
        let aspect = shot.rect.aspect_ratio();
        if shot.state.orthographic {
            if self.submitted.as_ref().is_some_and(|pending| {
                pending.rect != shot.rect || pending.matrices != shot.matrices
            }) {
                return Err(Text::CameraCaptureUnavailable.plain());
            }
            orthographic_shot(shot.matrices, aspect)
                .map(|(transform, lens)| CameraShot::Orthographic { transform, lens })
        } else {
            fixed_shot(shot.state, shot.matrices, aspect).map(CameraShot::Perspective)
        }
    }
}

impl ComponentWorkspace {
    pub(super) fn default_camera_destination(&self) -> Option<String> {
        self.session
            .scene()
            .designated_camera()
            .and_then(|(_, component, view)| view.is_fixed().then(|| component.id.clone()))
    }

    pub(super) fn begin_camera_capture(&mut self) {
        if self.authored_edit_pending()
            || self.runtime.in_play_mode()
            || self.ai_transition_pending()
        {
            self.error = Some(Text::CameraFinishEdit.plain().into());
            return;
        }
        let shot = match self.camera_authoring.shot() {
            Ok(shot) => shot,
            Err(error) => {
                self.error = Some(error.into());
                return;
            }
        };
        let orthographic = matches!(shot, CameraShot::Orthographic { .. });
        if orthographic && self.current_animation_preview().is_some() {
            self.error = Some(Text::CameraCapturePreview.plain().into());
            return;
        }
        self.camera_authoring.draft = Some(Draft {
            shot,
            revision: self.session.revision(),
            destination: if orthographic {
                None
            } else {
                self.default_camera_destination()
            },
            choices: if orthographic {
                Vec::new()
            } else {
                camera_choices(&self.session.scene().objects)
            },
            opened_frame: None,
            error: None,
        });
        #[cfg(unix)]
        self.invalidate_display_capture();
    }

    pub(super) fn update_fixed_camera(
        &mut self,
        revision: u64,
        destination: Option<&str>,
        view: PlayerView,
    ) -> Result<u64, SceneError> {
        view.validate_settings()?;
        if view.followed_target().is_some() {
            return Err(SceneError::Invalid(Text::CameraOnlyFixed.plain().into()));
        }
        let Some(id) = destination else {
            return self.create_game_camera(revision, view.into(), Transform::default());
        };
        let scene = self.session.scene();
        let mut object = scene
            .objects
            .iter()
            .find(|object| {
                object.components.iter().any(|component| {
                    component.id == id && matches!(component.kind, Component::Camera { .. })
                })
            })
            .cloned()
            .ok_or_else(|| SceneError::Invalid(Text::CameraDestinationMissing.plain().into()))?;
        let camera = object
            .components
            .iter_mut()
            .find(|component| component.id == id)
            .expect("camera owner was found");
        camera.kind = Component::Camera { view: view.into() };
        self.put_game_camera(revision, id.to_owned(), object)
    }

    fn create_game_camera(
        &mut self,
        revision: u64,
        view: CameraView,
        transform: Transform,
    ) -> Result<u64, SceneError> {
        let scene = self.session.scene();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let object_id = (revision..)
            .map(|n| format!("game-camera-{stamp:x}-{n:x}"))
            .find(|id| {
                !scene.objects.iter().any(|object| {
                    object.id == *id
                        || object.components.iter().any(|component| {
                            component.id.strip_suffix("-camera") == Some(id.as_str())
                        })
                })
            })
            .expect("finite camera IDs");
        let id = format!("{object_id}-camera");
        let object = ObjectNode {
            id: object_id,
            name: Text::GameCameraName.plain().into(),
            parent: None,
            enabled: true,
            transform,
            components: vec![ComponentInstance {
                id: id.clone(),
                enabled: true,
                kind: Component::Camera { view },
            }],
        };
        self.put_game_camera(revision, id, object)
    }

    fn put_game_camera(
        &mut self,
        revision: u64,
        id: String,
        object: ObjectNode,
    ) -> Result<u64, SceneError> {
        let next = self.session.put_gameplay_camera(revision, id, object)?;
        if next != revision {
            self.refresh();
        }
        Ok(next)
    }

    #[cfg(unix)]
    pub(super) fn create_camera_from_painted_orthographic(
        &mut self,
        revision: u64,
        rect: egui::Rect,
        matrices: rot_render::CameraMatrices,
    ) -> Result<u64, (&'static str, String)> {
        let painted = self
            .camera_authoring
            .painted_shot()
            .map_err(|message| ("viewport_pending", message.into()))?;
        if painted.rect != rect || painted.matrices != matrices {
            return Err(("stale_view", Text::CameraCaptureUnavailable.plain().into()));
        }
        if !painted.state.orthographic {
            return Err((
                "invalid_player_view",
                Text::CameraCaptureOrthographic.plain().into(),
            ));
        }
        let (transform, lens) = orthographic_shot(matrices, rect.aspect_ratio())
            .map_err(|message| ("invalid_player_view", message.into()))?;
        self.create_game_camera(
            revision,
            CameraView::Transform(TransformLens::Orthographic(lens)),
            transform,
        )
        .map_err(|error| ("invalid_player_view", error.to_string()))
    }

    pub(super) fn set_camera_designation(
        &mut self,
        revision: u64,
        destination: Option<String>,
    ) -> Result<u64, SceneError> {
        let next = self.session.set_gameplay_camera(revision, destination)?;
        if next != revision {
            self.refresh();
        }
        Ok(next)
    }

    pub(super) fn defer_camera_close(&mut self, ctx: &egui::Context) -> bool {
        let Some(draft) = self.camera_authoring.draft.as_mut() else {
            return false;
        };
        if !ctx.input(|input| input.viewport().close_requested()) {
            return false;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        draft.error = Some(Text::GameCameraFinishBeforeClose.plain().into());
        true
    }

    pub(super) fn show_camera_draft(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.camera_authoring.draft.take() else {
            return;
        };
        let frame = ctx.cumulative_frame_nr();
        let popup_open = egui::Popup::is_any_open(ctx);
        let enter = draft.opened_frame.is_some_and(|opened| opened != frame)
            && ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let label = draft
            .destination
            .as_deref()
            .and_then(|id| {
                draft
                    .choices
                    .iter()
                    .find(|choice| choice.id == id)
                    .map(|choice| choice.label.as_str())
            })
            .unwrap_or(Text::CameraNew.plain());
        let response = egui::Modal::new(egui::Id::unique("v23_game_camera"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(360.));
                ui.heading(Text::SetGameCameraFromView.plain());
                if matches!(draft.shot, CameraShot::Perspective(_)) {
                    ui_kit::property_row_with_label(
                        ui,
                        Text::ComponentCamera.plain(),
                        |ui, name| {
                            egui::ComboBox::from_id_salt("destination")
                                .selected_text(label)
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(
                                        &mut draft.destination,
                                        None,
                                        Text::CameraNew.plain(),
                                    );
                                    for choice in &draft.choices {
                                        ui.selectable_value(
                                            &mut draft.destination,
                                            Some(choice.id.clone()),
                                            choice.label.as_str(),
                                        )
                                        .on_hover_text(&choice.id);
                                    }
                                })
                                .response
                                .labelled_by(name);
                        },
                    );
                } else {
                    ui_kit::property_row(ui, Text::ComponentCamera.plain(), |ui| {
                        ui.label(Text::CameraNew.plain());
                    });
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                ui.horizontal(|ui| {
                    let cancel = ui.button(Text::ComponentCancel.plain()).clicked();
                    let apply = editor_theme::primary_button(ui, Text::CameraSet.plain());
                    if draft.opened_frame.is_none() {
                        apply.request_focus();
                    }
                    (apply.clicked(), cancel)
                })
                .inner
            });
        draft.opened_frame.get_or_insert(frame);
        let (apply, cancel) = response.inner;
        let escape = response.is_top_modal
            && !response.any_popup_open
            && !popup_open
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape {
            return;
        }
        if apply || (enter && !popup_open && !egui::Popup::is_any_open(ctx)) {
            let result = match &draft.shot {
                CameraShot::Perspective(view) => self.update_fixed_camera(
                    draft.revision,
                    draft.destination.as_deref(),
                    view.clone(),
                ),
                CameraShot::Orthographic { transform, lens } => self.create_game_camera(
                    draft.revision,
                    CameraView::Transform(TransformLens::Orthographic(lens.clone())),
                    transform.clone(),
                ),
            };
            match result {
                Ok(_) => {
                    let scene = self.session.scene();
                    if let Some((index, _, _)) = scene.designated_camera() {
                        self.select(Some(scene.objects[index].id.clone()));
                    }
                    self.error = None;
                    return;
                }
                Err(error) => draft.error = Some(error.to_string()),
            }
        }
        self.camera_authoring.draft = Some(draft);
    }

    pub(super) fn camera_owner_enabled(&self, mut index: usize) -> bool {
        let objects = &self.session.scene().objects;
        loop {
            if !objects[index].enabled {
                return false;
            }
            match self.parents.get(index).copied().flatten() {
                Some(parent) => index = parent,
                None => return true,
            }
        }
    }

    pub(super) fn camera_controls(&mut self, ui: &mut egui::Ui) {
        let Some((index, object)) = self.selected_index.zip(self.draft.as_ref()) else {
            return;
        };
        let active = self.session.scene().gameplay_camera.as_deref();
        let owner_enabled = self.camera_owner_enabled(index);
        let editable = !self.authored_edit_pending() && !self.runtime.in_play_mode();
        let mut intent = None;
        for component in &object.components {
            let Component::Camera { .. } = &component.kind else {
                continue;
            };
            let designated = active == Some(component.id.as_str());
            ui.horizontal(|ui| {
                ui.label(if designated {
                    if owner_enabled && component.enabled {
                        Text::CameraActive.plain()
                    } else {
                        Text::CameraSuspended.plain()
                    }
                } else {
                    Text::CameraInactive.plain()
                });
                let label = if designated {
                    Text::GameCameraClear
                } else {
                    Text::CameraMakeActive
                };
                if ui
                    .push_id(&component.id, |ui| {
                        ui.add_enabled(editable, egui::Button::new(label.plain()))
                    })
                    .inner
                    .clicked()
                {
                    intent = Some((!designated).then(|| component.id.clone()));
                }
            });
        }
        if let Some(intent) = intent {
            self.error = self
                .set_camera_designation(self.session.revision(), intent)
                .err()
                .map(|error| error.to_string());
        }
    }
}

fn camera_mode(view: &CameraView) -> &'static str {
    match view {
        CameraView::Transform(rot_scene::component_scene::TransformLens::Perspective(_)) => {
            Text::CameraTransformPerspective.plain()
        }
        CameraView::Transform(rot_scene::component_scene::TransformLens::Orthographic(_)) => {
            Text::CameraTransformOrthographic.plain()
        }
        CameraView::Orbit(view) if view.followed_target().is_some() => Text::CameraFollow.plain(),
        CameraView::Orbit(_) => Text::CameraFixed.plain(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orthographic_capture_keeps_top_orientation_and_tilted_lens() {
        for (yaw, pitch, aspect, distance) in [
            (0.0, std::f32::consts::FRAC_PI_2, 16.0 / 9.0, 10.0),
            (0.81, -0.4, 4.0 / 3.0, 7.2),
        ] {
            let state = rot_render::CameraState {
                target: [3.0, 2.0, -8.0],
                yaw,
                pitch,
                distance,
                orthographic: true,
            };
            let matrices = state.matrices(aspect).unwrap();
            let (transform, lens) = orthographic_shot(matrices, aspect).unwrap();
            assert_eq!(lens.aspect_ratio, Some(aspect));
            assert!(
                (lens.vertical_size - 2.0 * distance * (45_f32.to_radians() * 0.5).tan()).abs()
                    < 0.00001
            );
            assert!((lens.clip.near - distance * 0.001).abs() < 0.00001);
            assert!((lens.clip.far - distance * 100.0).abs() < 0.001);
            if yaw == 0.0 {
                assert!(
                    glam::Vec3::from_array(transform.translation)
                        .distance(glam::vec3(3.0, 12.0, -8.0))
                        < 0.0001
                );
                let [x, y, z] = transform.rotation;
                let rotation = glam::Quat::from_euler(glam::EulerRot::ZYX, z, y, x);
                assert!((rotation * -glam::Vec3::Z).distance(-glam::Vec3::Y) < 0.00001);
                assert!((rotation * glam::Vec3::Y).distance(glam::Vec3::Z) < 0.00001);
            }
            assert!(orthographic_shot(matrices, aspect + 0.1).is_err());
            let outside = rot_render::CameraState {
                target: [20_000.0, 0.0, 0.0],
                ..state
            };
            assert!(orthographic_shot(outside.matrices(aspect).unwrap(), aspect).is_err());
        }
    }

    #[cfg(unix)]
    #[test]
    fn exact_painted_orthographic_capture_uses_one_camera_undo() {
        use rot_scene::component_scene::{ComponentScene, ComponentSession};
        use rot_scene::{Scene, Session};

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("camera.scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let camera = ComponentInstance {
            id: "prior-camera".into(),
            enabled: true,
            kind: Component::Camera {
                view: PlayerView {
                    anchor: PlayerAnchor::Fixed { position: [0.0; 3] },
                    yaw: 0.8,
                    pitch: 0.4,
                    distance: 6.0,
                    vertical_fov_degrees: 45.0,
                }
                .into(),
            },
        };
        scene.gameplay_camera = Some(camera.id.clone());
        scene.objects[0].components.push(camera);
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path, ComponentSession::new(scene.clone()).unwrap());
        let state = rot_render::CameraState {
            target: [1.0, 3.0, 2.0],
            yaw: 0.0,
            pitch: std::f32::consts::FRAC_PI_2,
            distance: 5.0,
            orthographic: true,
        };
        let rect = egui::Rect::from_min_size(egui::pos2(20.0, 40.0), egui::vec2(640.0, 360.0));
        let matrices = state.matrices(rect.aspect_ratio()).unwrap();
        let shot = SceneShot {
            state,
            matrices,
            rect,
            receipt: Default::default(),
        };
        editor.camera_authoring.submitted = Some(shot);
        assert_eq!(
            editor
                .create_camera_from_painted_orthographic(0, rect, matrices)
                .unwrap_err()
                .0,
            "viewport_pending"
        );
        editor.camera_authoring.rendered = editor.camera_authoring.submitted.take();
        let resized = egui::Rect::from_min_size(rect.min, egui::vec2(641.0, 360.0));
        assert_eq!(
            editor
                .create_camera_from_painted_orthographic(0, resized, matrices)
                .unwrap_err()
                .0,
            "stale_view"
        );
        assert_eq!(
            editor.capture_orthographic_camera(0, 1).unwrap_err().0,
            "stale_view"
        );
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(
            editor
                .create_camera_from_painted_orthographic(0, rect, matrices)
                .unwrap(),
            1
        );
        let authored = editor.session.scene();
        let (owner, _, camera) = authored.designated_camera().unwrap();
        assert_eq!(authored.objects[owner].parent, None);
        let CameraView::Transform(TransformLens::Orthographic(lens)) = camera else {
            panic!("expected a root Orthographic Camera");
        };
        assert_eq!(lens.aspect_ratio, Some(rect.aspect_ratio()));
        assert_eq!(authored.objects.len(), scene.objects.len() + 1);
        editor.command(super::super::Action::Undo);
        assert_eq!(editor.session.scene(), &scene);
    }

    #[test]
    fn capture_uses_the_rendered_perspective_and_checks_both_matrices() {
        let state = rot_render::CameraState {
            target: [3., 2., -8.],
            yaw: 0.81,
            pitch: -0.4,
            distance: 7.2,
            orthographic: false,
        };
        for aspect in [4. / 3., 16. / 9.] {
            let matrices = state.matrices(aspect).unwrap();
            let view = fixed_shot(state, matrices, aspect).unwrap();
            assert_eq!(
                view.anchor,
                PlayerAnchor::Fixed {
                    position: state.target
                }
            );
            assert_eq!(view.vertical_fov_degrees, 45.);
            assert!(fixed_shot(state, matrices, aspect + 0.1).is_err());
            let orthographic = rot_render::CameraState {
                orthographic: true,
                ..state
            };
            assert_eq!(
                fixed_shot(orthographic, orthographic.matrices(aspect).unwrap(), aspect),
                Err(Text::CameraCapturePerspective.plain())
            );
            let overhead = rot_render::CameraState {
                pitch: 88.9_f32.to_radians(),
                ..state
            };
            assert_eq!(
                fixed_shot(overhead, overhead.matrices(aspect).unwrap(), aspect),
                Err(Text::CameraCaptureBounds.plain())
            );
        }
    }
}
