use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text, ui_kit};
use eframe::egui;
use rot_scene::{
    component_scene::{Component, ComponentInstance, ObjectNode},
    PlayerAnchor, PlayerView, SceneError, Transform,
};

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
    aspect: f32,
    receipt: rot_render::FrameReceipt,
}

struct Draft {
    view: PlayerView,
    revision: u64,
    destination: Option<String>,
    opened_frame: Option<u64>,
    error: Option<String>,
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
            aspect: viewport.rect.aspect_ratio(),
            receipt: viewport.receipt.clone(),
        });
    }

    fn shot(&mut self) -> Result<PlayerView, &'static str> {
        self.observe_paint();
        let shot = self
            .rendered
            .as_ref()
            .ok_or(Text::CameraCaptureUnavailable.plain())?;
        fixed_shot(shot.state, shot.matrices, shot.aspect)
    }
}

impl ComponentWorkspace {
    pub(super) fn default_camera_destination(&self) -> Option<String> {
        self.session
            .scene()
            .designated_camera()
            .and_then(|(_, component, view)| {
                view.followed_target()
                    .is_none()
                    .then(|| component.id.clone())
            })
    }

    pub(super) fn begin_camera_capture(&mut self) {
        if self.authored_edit_pending()
            || self.runtime.in_play_mode()
            || self.ai_transition_pending()
        {
            self.error = Some(Text::CameraFinishEdit.plain().into());
            return;
        }
        let view = match self.camera_authoring.shot() {
            Ok(view) => view,
            Err(error) => {
                self.error = Some(error.into());
                return;
            }
        };
        self.camera_authoring.draft = Some(Draft {
            view,
            revision: self.session.revision(),
            destination: self.default_camera_destination(),
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
        let scene = self.session.scene();
        let (id, object) = if let Some(id) = destination {
            let mut object = scene
                .objects
                .iter()
                .find(|object| {
                    object.components.iter().any(|component| {
                        component.id == id && matches!(component.kind, Component::Camera { .. })
                    })
                })
                .cloned()
                .ok_or_else(|| {
                    SceneError::Invalid(Text::CameraDestinationMissing.plain().into())
                })?;
            let camera = object
                .components
                .iter_mut()
                .find(|component| component.id == id)
                .expect("camera owner was found");
            camera.kind = Component::Camera { view };
            (id.to_owned(), object)
        } else {
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
                transform: Transform::default(),
                components: vec![ComponentInstance {
                    id: id.clone(),
                    enabled: true,
                    kind: Component::Camera { view },
                }],
            };
            (id, object)
        };
        let next = self.session.put_gameplay_camera(revision, id, object)?;
        if next != revision {
            self.refresh();
        }
        Ok(next)
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
        let scene = self.session.scene();
        let label = draft
            .destination
            .as_deref()
            .and_then(|id| {
                scene
                    .objects
                    .iter()
                    .find(|object| object.components.iter().any(|component| component.id == id))
                    .map(|object| object.name.as_str())
            })
            .unwrap_or(Text::CameraNew.plain());
        let response = egui::Modal::new(egui::Id::unique("v23_game_camera"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(360.));
                ui.heading(Text::SetGameCameraFromView.plain());
                ui_kit::property_row_with_label(ui, Text::ComponentCamera.plain(), |ui, name| {
                    egui::ComboBox::from_id_salt("destination")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut draft.destination,
                                None,
                                Text::CameraNew.plain(),
                            );
                            for object in &scene.objects {
                                for component in &object.components {
                                    if let Component::Camera { view } = &component.kind {
                                        ui.selectable_value(
                                            &mut draft.destination,
                                            Some(component.id.clone()),
                                            format!("{} · {}", object.name, camera_mode(view)),
                                        )
                                        .on_hover_text(&component.id);
                                    }
                                }
                            }
                        })
                        .response
                        .labelled_by(name);
                });
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
            match self.update_fixed_camera(
                draft.revision,
                draft.destination.as_deref(),
                draft.view.clone(),
            ) {
                Ok(_) => {
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

pub(in crate::scene_editor) fn camera_mode(view: &PlayerView) -> &'static str {
    if view.followed_target().is_some() {
        Text::CameraFollow.plain()
    } else {
        Text::CameraFixed.plain()
    }
}
