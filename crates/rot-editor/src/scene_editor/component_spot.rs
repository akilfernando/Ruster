//! One-shot authored Spot aiming, shared by the Inspector and guarded tools.
use super::{viewport_transform::authored_rotation, ComponentWorkspace};
use crate::{editor_theme, strings::Text};
use eframe::egui;
use glam::{DQuat, DVec3};
use rot_scene::{component_scene::Component, LightKind, SceneError, Transform};

type AimError = (&'static str, String);

pub(super) struct AimPicker {
    revision: u64,
    owner: String,
    component: String,
    candidates: Vec<usize>,
    filtered: Vec<usize>,
    search: String,
    selected: usize,
    first_frame: bool,
    error: Option<String>,
}

impl ComponentWorkspace {
    fn aimed_at_descendant(&self, owner: usize, target: usize) -> bool {
        let mut row = Some(target);
        while let Some(index) = row {
            if index == owner {
                return true;
            }
            row = self.parents[index];
        }
        false
    }

    pub(super) fn begin_spot_aim(&mut self, component: String) {
        if self.runtime.in_play_mode()
            || self.authored_edit_pending()
            || self.ai_transition_pending()
        {
            return;
        }
        let Some(owner) = self.selected_index else {
            return;
        };
        let scene = self.session.scene();
        let Some(start) = self.rows.iter().position(|&(row, _)| row == owner) else {
            return;
        };
        let depth = self.rows[start].1;
        let end = self.rows[start + 1..]
            .iter()
            .position(|&(_, other)| other <= depth)
            .map_or(self.rows.len(), |index| start + 1 + index);
        let candidates: Vec<_> = self.rows[..start]
            .iter()
            .chain(&self.rows[end..])
            .map(|&(row, _)| row)
            .collect();
        self.spot_aim = Some(AimPicker {
            revision: self.session.revision(),
            owner: scene.objects[owner].id.clone(),
            component,
            filtered: candidates.clone(),
            candidates,
            search: String::new(),
            selected: 0,
            first_frame: true,
            error: None,
        });
        #[cfg(unix)]
        self.invalidate_display_capture();
    }

    pub(super) fn show_spot_aim(&mut self, ctx: &egui::Context) {
        let Some(mut picker) = self.spot_aim.take() else {
            return;
        };
        if picker.revision != self.session.revision() || self.ai_transition_pending() {
            self.error = Some(Text::SpotAimChanged.plain().into());
            return;
        }
        let objects = &self.session.scene().objects;
        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let response = egui::Modal::new(egui::Id::unique("component_spot_aim"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(420.));
                ui.heading(Text::SpotAim.plain());
                ui.label(Text::SpotAimOwnerEffect.plain());
                let search = ui.add(
                    egui::TextEdit::singleline(&mut picker.search)
                        .desired_width(ui.available_width())
                        .hint_text(Text::SpotAimSearch.plain()),
                );
                if picker.first_frame {
                    search.request_focus();
                }
                if search.changed() {
                    let query = picker.search.trim().to_lowercase();
                    picker.filtered.clear();
                    picker
                        .filtered
                        .extend(picker.candidates.iter().copied().filter(|&row| {
                            let object = &objects[row];
                            object.name.to_lowercase().contains(&query)
                                || object.id.to_lowercase().contains(&query)
                        }));
                    picker.selected = 0;
                    picker.error = None;
                }
                let navigation = ui.input_mut(|input| {
                    let down = input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown);
                    let up = input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp);
                    (down, up)
                });
                if navigation.0 {
                    picker.selected =
                        (picker.selected + 1).min(picker.filtered.len().saturating_sub(1));
                }
                if navigation.1 {
                    picker.selected = picker.selected.saturating_sub(1);
                }
                if picker.filtered.is_empty() {
                    ui.weak(Text::SpotAimNoObjects.plain());
                }
                let row_height = ui.spacing().interact_size.y;
                let mut scroll = egui::ScrollArea::vertical().max_height(224.);
                if navigation.0 || navigation.1 || search.changed() {
                    scroll = scroll.vertical_scroll_offset(
                        (picker.selected as f32 * (row_height + ui.spacing().item_spacing.y) - 96.)
                            .max(0.),
                    );
                }
                scroll.show_rows(ui, row_height, picker.filtered.len(), |ui, range| {
                    for index in range {
                        let object = &objects[picker.filtered[index]];
                        if ui
                            .selectable_label(
                                picker.selected == index,
                                format!("{} · {}", object.name, object.id),
                            )
                            .clicked()
                        {
                            picker.selected = index;
                            picker.error = None;
                        }
                    }
                });
                if let Some(error) = &picker.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                let ready = !picker.filtered.is_empty();
                let (cancel, apply) = ui
                    .horizontal(|ui| {
                        let cancel = ui.button(Text::ComponentCancel.plain()).clicked();
                        let aim = ui
                            .add_enabled_ui(ready, |ui| {
                                editor_theme::primary_button(ui, Text::SpotAimApply.plain())
                            })
                            .inner;
                        (cancel, aim.clicked())
                    })
                    .inner;
                (
                    cancel,
                    ready && (apply || enter && (search.has_focus() || search.lost_focus())),
                )
            });
        picker.first_frame = false;
        if response.inner.0
            || response.is_top_modal
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            return;
        }
        if response.inner.1 {
            let target = objects[picker.filtered[picker.selected]].id.clone();
            match self.aim_spot(picker.revision, &picker.owner, &picker.component, &target) {
                Ok(_) => return,
                Err((_, error)) => picker.error = Some(error),
            }
        }
        self.spot_aim = Some(picker);
    }

    pub(super) fn aim_spot(
        &mut self,
        revision: u64,
        owner_id: &str,
        component_id: &str,
        target_id: &str,
    ) -> Result<bool, AimError> {
        if self.session.revision() != revision {
            return Err(("stale_revision", Text::SpotAimChanged.plain().into()));
        }
        if self.runtime.in_play_mode() {
            return Err((
                "play_active",
                "Stop Play before aiming a Spot Light.".into(),
            ));
        }
        if self.authored_edit_pending() || self.ai_transition_pending() {
            return Err((
                "drafts_pending",
                "Finish the pending editor change before aiming a Spot Light.".into(),
            ));
        }
        let scene = self.session.scene();
        let row = |id: &str| scene.objects.iter().position(|node| node.id == id);
        let owner_index = row(owner_id).ok_or((
            "object_not_found",
            "The Spot Light owner no longer exists.".into(),
        ))?;
        let target = row(target_id).ok_or((
            "object_not_found",
            "The aim target no longer exists.".into(),
        ))?;
        let owner = &scene.objects[owner_index];
        let component = owner
            .components
            .iter()
            .find(|component| component.id == component_id)
            .ok_or((
                "component_not_found",
                "The Spot Light component no longer exists on this object.".into(),
            ))?;
        if !matches!(
            component.kind,
            Component::Light {
                kind: LightKind::Spot { .. },
                ..
            }
        ) {
            return Err((
                "invalid_arguments",
                "Choose a Spot Light component on its owner.".into(),
            ));
        }
        if self.aimed_at_descendant(owner_index, target) {
            return Err((
                "invalid_arguments",
                "Choose a target outside the Spot Light owner's hierarchy.".into(),
            ));
        }
        let unavailable = || {
            (
                "invalid_arguments",
                "Couldn't resolve the Spot Light and target world pivots.".into(),
            )
        };
        let origin = self.spot_origin(owner_index).ok_or_else(unavailable)?;
        let target = self.spot_origin(target).ok_or_else(unavailable)?;
        let parent = self.parents[owner_index].map_or(DQuat::IDENTITY, |index| {
            scene.world_rotation(index, &self.parents)
        });
        let next = aimed_rotation(&owner.transform, parent, target - origin)?;
        let Some(rotation) = next else {
            return Ok(false);
        };
        let mut updated = owner.clone();
        updated.transform.rotation = rotation;
        let next = self
            .session
            .put_object(revision, updated)
            .map_err(|error| {
                let code = if matches!(error, SceneError::StaleRevision { .. }) {
                    "stale_revision"
                } else {
                    "invalid_arguments"
                };
                (code, error.to_string())
            })?;
        if next != revision {
            self.refresh();
        }
        Ok(next != revision)
    }

    pub(super) fn spot_origin(&self, index: usize) -> Option<DVec3> {
        self.preview_world
            .get(index)
            .map(|world| world.w_axis.truncate())
    }
}

fn aimed_rotation(
    original: &Transform,
    parent: DQuat,
    offset: DVec3,
) -> Result<Option<[f32; 3]>, AimError> {
    let Some(target) = offset.try_normalize() else {
        return Err((
            "invalid_arguments",
            "The Spot Light and target share a world pivot. Move one first.".into(),
        ));
    };
    let parent = parent.normalize();
    let world = (parent * local_rotation(original.rotation)).normalize();
    let forward = world * -DVec3::Z;
    let cross = forward.cross(target);
    let sine = cross.length();
    let cosine = forward.dot(target);
    if cosine > 0. && sine <= 2e-6 {
        return Ok(None);
    }
    let delta = if sine <= 1e-12 {
        // At a half turn the shortest axis is ambiguous. Keep the current up.
        DQuat::from_axis_angle(world * DVec3::Y, std::f64::consts::PI)
    } else {
        DQuat::from_axis_angle(cross / sine, sine.atan2(cosine))
    };
    let local = (parent.conjugate() * delta * world).normalize();
    let invalid = || {
        (
            "invalid_arguments",
            "The aim is outside authored rotation precision. Try a different target.".into(),
        )
    };
    let stored = authored_rotation(local, original).ok_or_else(invalid)?;
    let represented = (parent * local_rotation(stored)).normalize() * -DVec3::Z;
    if !represented.abs_diff_eq(target, 2e-6) {
        return Err(invalid());
    }
    Ok(Some(stored))
}

fn local_rotation(rotation: [f32; 3]) -> DQuat {
    let [x, y, z] = rotation.map(f64::from);
    DQuat::from_euler(glam::EulerRot::ZYX, z, y, x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::component_scene::{
        ComponentInstance, ComponentScene, ComponentSession, ObjectNode,
    };
    use rot_scene::{Scene, Session};

    fn workspace(path: &std::path::Path) -> ComponentWorkspace {
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("aim")).unwrap()).unwrap();
        let mut node = |id: &str, name: &str, parent: Option<&str>, transform| {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: name.into(),
                enabled: true,
                parent: parent.map(str::to_owned),
                transform,
                components: Vec::new(),
            });
        };
        node(
            "rig",
            "Rig",
            None,
            Transform {
                translation: [5., -1., 2.],
                rotation: [0.22, 0.6, 0.],
                scale: [2., 1., 3.],
            },
        );
        node(
            "lamp",
            "Lamp",
            Some("rig"),
            Transform {
                translation: [1., 2., 3.],
                rotation: [0.3, 0.2, 0.1],
                scale: [1.5, 1., 0.5],
            },
        );
        node("child", "Housing", Some("lamp"), Transform::default());
        node(
            "target",
            "Yellow Plate",
            None,
            Transform {
                translation: [5., 4., -6.],
                ..Default::default()
            },
        );
        scene.objects[1].components = vec![ComponentInstance {
            id: "spot".into(),
            enabled: true,
            kind: Component::Light {
                color: [1.; 3],
                intensity: 1.,
                kind: LightKind::Spot {
                    range: 12.,
                    inner_degrees: 12.,
                    outer_degrees: 90.,
                },
            },
        }];
        ComponentWorkspace::new(path.to_path_buf(), ComponentSession::new(scene).unwrap())
    }

    #[test]
    fn spot_aim_handles_aligned_coincident_and_antiparallel_directions() {
        let before = Transform::default();
        assert!(aimed_rotation(&before, DQuat::IDENTITY, DVec3::NEG_Z)
            .unwrap()
            .is_none());
        assert!(aimed_rotation(&before, DQuat::IDENTITY, DVec3::ZERO).is_err());
        let rotation = aimed_rotation(&before, DQuat::IDENTITY, DVec3::Z)
            .unwrap()
            .unwrap();
        assert_eq!(
            rotation,
            aimed_rotation(&before, DQuat::IDENTITY, DVec3::Z)
                .unwrap()
                .unwrap()
        );
        let aimed = local_rotation(rotation);
        assert!((aimed * DVec3::NEG_Z).abs_diff_eq(DVec3::Z, 1e-6));
        assert!((aimed * DVec3::Y).abs_diff_eq(DVec3::Y, 1e-6));
    }

    #[cfg(unix)]
    #[test]
    fn spot_aim_pins_mcp_revision_and_records_linked_rotation_only() {
        use rot_scene::component_scene::{PrefabFields, PrefabSource};
        use serde_json::json;
        let folder = tempfile::tempdir().unwrap();
        let mut editor = workspace(&folder.path().join("scene.json"));
        let source = PrefabSource::from_selection(editor.session.scene(), "rig").unwrap();
        editor
            .session
            .link_prefab(
                0,
                "rig-prefab".into(),
                "Assets/rig.prefab".into(),
                &source,
                |_| Ok(()),
            )
            .unwrap();
        editor.refresh();
        let before = editor.session.scene().clone();
        let request = |revision: u64, target: &str| {
            json!({
                "expected_session_token": "session", "expected_revision": revision.to_string(),
                "edits": [{"operation":"aim_spot_at_object","object_id":"lamp","component_id":"spot","target_object_id":target}]
            })
        };
        let revision = editor.session.revision();
        for (expected, target, code) in [
            (0, "target", "stale_revision"),
            (revision, "child", "invalid_arguments"),
        ] {
            assert_eq!(
                editor.ai_command(
                    "session",
                    "scene_components_edit",
                    &request(expected, target)
                )["error"]["code"],
                code
            );
            assert_eq!(editor.session.scene(), &before);
        }
        let reply = editor.ai_command(
            "session",
            "scene_components_edit",
            &request(revision, "target"),
        );
        assert_eq!(reply["result"]["changed"], true, "{reply}");
        let scene = editor.session.scene();
        let part = scene.prefabs["rig"]
            .parts
            .values()
            .find(|part| part.object == "lamp")
            .unwrap();
        assert_eq!(part.transform_overrides, [PrefabFields::Rotation].into());
        let lamp = &scene.objects[1];
        assert_eq!(
            lamp.transform.translation,
            before.objects[1].transform.translation
        );
        assert_eq!(lamp.transform.scale, before.objects[1].transform.scale);
        assert_eq!(lamp.components, before.objects[1].components);
        assert_eq!(scene.objects[2], before.objects[2]);

        let spot = editor.ai_command(
            "session",
            "scene_components_inspect",
            &json!({"object_id":"lamp","component_id":"spot"}),
        );
        let observation = &spot["result"]["spot_world"];
        let forward: [f64; 3] = serde_json::from_value(observation["forward"].clone()).unwrap();
        let origin: [f64; 3] = serde_json::from_value(observation["origin"].clone()).unwrap();
        let world_pivot = before.objects[0]
            .transform
            .affine_matrix()
            .transform_point3(DVec3::new(1., 2., 3.));
        let direction = (DVec3::new(5., 4., -6.) - world_pivot).normalize();
        assert!(DVec3::from(origin).abs_diff_eq(world_pivot, 1e-9));
        assert!(DVec3::from(forward).abs_diff_eq(direction, 2e-6));
        assert_eq!(observation["outer_degrees"], 90.);
        let next = editor.session.revision();
        let aligned =
            editor.ai_command("session", "scene_components_edit", &request(next, "target"));
        assert_eq!(aligned["result"]["changed"], false);
        assert_eq!(editor.session.revision(), next);
        editor.session.undo(next).unwrap();
        assert_eq!(editor.session.scene(), &before);
    }

    #[test]
    fn spot_aim_picker_accepts_search_enter_and_cancels_escape() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = workspace(&folder.path().join("scene.json"));
        editor.select(Some("lamp".into()));
        let ctx = egui::Context::default();
        let frame = |editor: &mut ComponentWorkspace, events| {
            ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show_spot_aim(ui.ctx());
                },
            )
            .textures_delta
            .clear();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        editor.begin_spot_aim("spot".into());
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.spot_aim.is_none());
        assert_eq!(editor.session.revision(), 0);

        editor.begin_spot_aim("spot".into());
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![egui::Event::Text("Yellow Plate".into())]);
        assert_eq!(editor.spot_aim.as_ref().unwrap().filtered, [3]);
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(
            editor.spot_aim.is_none(),
            "Enter commits the selected pivot"
        );
        assert_eq!(editor.session.revision(), 1);
    }
}
