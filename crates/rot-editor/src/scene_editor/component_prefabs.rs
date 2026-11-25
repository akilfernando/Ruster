//! Shared native and MCP prefab authoring against one component session.
use super::{project_directory, ComponentWorkspace, Transform};
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_assets::{AssetStatus, PrefabFile, ProjectFileIssue};
use rot_scene::{
    component_scene::{PrefabFields, PrefabFileEdit, PrefabProperty, PrefabSource},
    SceneError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct UnpackDraft {
    root: String,
    name: String,
    count: usize,
    revision: u64,
    asset: String,
    source_hash: String,
    focus_cancel: bool,
    error: Option<String>,
}

fn invalid(message: impl Into<String>) -> SceneError {
    SceneError::Invalid(message.into())
}

fn publish(
    root: &Path,
    files: &BTreeMap<String, PrefabFile>,
    edit: &PrefabFileEdit,
) -> Result<PrefabFileEdit, SceneError> {
    let path = files
        .get(&edit.asset)
        .map_or(edit.path_hint.as_str(), |file| &file.source);
    rot_assets::publish_project_prefab(
        root,
        path,
        &edit.asset,
        edit.before.as_deref(),
        edit.after.as_deref(),
    )
    .map_err(|error| invalid(error.to_string()))?;
    let mut published = edit.clone();
    path.clone_into(&mut published.path_hint);
    Ok(published)
}

fn unique_id(prefix: &str) -> String {
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{prefix}-{:x}", time.as_nanos())
}

pub(super) struct PrefabFeedback {
    action: Text,
    detail: String,
}

impl PrefabFeedback {
    pub(super) fn show(&self, ui: &mut egui::Ui, details_open: &mut bool) -> bool {
        let mut dismiss = false;
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(editor_theme::colors(ui).error, self.action.plain());
            *details_open |= ui.button(Text::ErrorDetails.plain()).clicked();
            dismiss = ui.button(Text::DismissError.plain()).clicked();
        });
        if *details_open {
            let dialog = egui::Modal::new(egui::Id::unique("prefab_error_details"))
                .frame(editor_theme::dialog_frame(ui.ctx()))
                .show(ui.ctx(), |ui| {
                    ui.set_max_width(480.0);
                    ui.heading(Text::ErrorDetails.plain());
                    ui.label(self.action.plain());
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(&self.detail).wrap().selectable(true));
                        });
                    ui.button(Text::CloseErrorDetails.plain()).clicked()
                });
            *details_open = !(dialog.inner || dialog.should_close());
        }
        dismiss
    }
}

impl ComponentWorkspace {
    pub(super) fn report_prefab_result<T>(
        &mut self,
        action: Text,
        result: Result<T, impl std::fmt::Display>,
    ) {
        match result {
            Ok(_)
                if self
                    .prefab_feedback
                    .as_ref()
                    .is_some_and(|old| old.action == action) =>
            {
                self.prefab_feedback = None;
                self.prefab_details_open = false;
            }
            Ok(_) => {}
            Err(error) => {
                self.prefab_feedback = Some(PrefabFeedback {
                    action,
                    detail: error.to_string(),
                });
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn adopt_prefabs(
        &mut self,
        files: &mut Vec<PrefabFile>,
        issues: &mut Vec<ProjectFileIssue>,
    ) {
        files.retain(|file| {
            if let Err(error) = PrefabSource::read_json(&file.bytes) {
                issues.push(ProjectFileIssue {
                    asset_id: None,
                    texture_role: None,
                    path: file.source.clone(),
                    status: AssetStatus::Invalid,
                    message: error.to_string(),
                });
                return false;
            }
            true
        });
        self.prefab_files = files
            .iter()
            .cloned()
            .map(|file| (file.id.clone(), file))
            .collect();
        self.prefab_refresh_pending = true;
    }

    #[cfg(target_os = "linux")]
    pub(super) fn reconcile_prefabs(&mut self) {
        if !self.prefab_refresh_pending
            || self.authored_edit_pending()
            || self.runtime.in_play_mode()
        {
            return;
        }
        self.prefab_refresh_pending = false;
        let pending: BTreeSet<_> = self
            .session
            .scene()
            .prefabs
            .values()
            .filter(|instance| {
                self.prefab_files
                    .get(&instance.asset)
                    .is_some_and(|file| file.hash != instance.source_hash)
            })
            .map(|instance| instance.asset.clone())
            .collect();
        let revision = self.session.revision();
        let mut failed = false;
        for id in pending {
            let file = &self.prefab_files[&id];
            let result = PrefabSource::read_json(&file.bytes).and_then(|source| {
                self.session
                    .refresh_prefab(self.session.revision(), &id, &file.hash, &source)
            });
            if let Err(error) = result {
                failed = true;
                let detail = format!("{}\n{error}", file.source);
                self.report_prefab_result(Text::PrefabRefresh, Err::<(), _>(detail));
            }
        }
        if revision != self.session.revision() {
            if !failed {
                self.report_prefab_result(Text::PrefabRefresh, Ok::<_, &str>(()));
            }
            self.refresh();
        }
    }

    fn accept_prefab_edit(&mut self, edit: PrefabFileEdit) {
        if let Some(bytes) = edit.after {
            let file = PrefabFile::from_bytes(edit.asset.clone(), edit.path_hint, bytes);
            self.prefab_files.insert(edit.asset, file);
        } else {
            self.prefab_files.remove(&edit.asset);
        }
    }

    pub(super) fn prefab_history(&mut self, revision: u64, undo: bool) -> Result<u64, SceneError> {
        let mut published = None;
        let root = project_directory(&self.path);
        let files = &self.prefab_files;
        let revision = self.session.move_history(revision, undo, |edit| {
            let edit = publish(root, files, edit)?;
            let path = edit.path_hint.clone();
            published = Some(edit);
            Ok(path)
        })?;
        if let Some(edit) = published {
            self.accept_prefab_edit(edit);
        }
        Ok(revision)
    }

    pub(super) fn save_prefab(
        &mut self,
        object: &str,
        path: Option<&str>,
    ) -> Result<String, SceneError> {
        let source = PrefabSource::from_selection(self.session.scene(), object)?;
        let path = path.map_or_else(
            || {
                let name = source
                    .scene()
                    .objects
                    .iter()
                    .find(|part| part.id == object)
                    .expect("prefab root")
                    .name
                    .trim();
                let stem: String = name
                    .chars()
                    .map(|ch| {
                        if ch.is_control() || "<>:\"/\\|?*".contains(ch) {
                            '_'
                        } else {
                            ch
                        }
                    })
                    .collect();
                let stem = stem.trim_matches('.').trim();
                let stem = if stem.is_empty() { "Prefab" } else { stem };
                Path::new("Assets")
                    .join(format!("{stem}.prefab"))
                    .to_string_lossy()
                    .into_owned()
            },
            str::to_owned,
        );
        let id = unique_id("prefab");
        let root = project_directory(&self.path);
        let files = &self.prefab_files;
        let mut published = None;
        self.session
            .link_prefab(self.session.revision(), id.clone(), path, &source, |edit| {
                published = Some(publish(root, files, edit)?);
                Ok(())
            })?;
        if let Some(edit) = published {
            self.accept_prefab_edit(edit);
        }
        self.refresh();
        Ok(id)
    }

    pub(super) fn place_prefab(
        &mut self,
        asset: &str,
        id: Option<&str>,
        transform: Transform,
        parent: Option<String>,
    ) -> Result<String, SceneError> {
        let file = self
            .prefab_files
            .get(asset)
            .ok_or_else(|| invalid("The prefab source is unavailable."))?;
        let source = PrefabSource::read_json(&file.bytes)?;
        let id = id.map_or_else(|| unique_id("instance"), str::to_owned);
        self.session.place_prefab(
            self.session.revision(),
            asset.to_owned(),
            file.hash.clone(),
            &source,
            &id,
            transform,
            parent,
        )?;
        self.select(Some(id.clone()));
        self.refresh();
        Ok(id)
    }

    pub(super) fn change_prefab_override(
        &mut self,
        object: &str,
        property: &PrefabProperty,
        apply: bool,
        expected_source_hash: Option<&str>,
    ) -> Result<(), SceneError> {
        let (instance, link) = self
            .session
            .scene()
            .prefabs
            .iter()
            .find(|(_, link)| link.parts.values().any(|part| part.object == object))
            .ok_or_else(|| invalid("Choose a prefab instance property."))?;
        let file = self
            .prefab_files
            .get(&link.asset)
            .ok_or_else(|| invalid("The prefab source is unavailable."))?
            .clone();
        if expected_source_hash
            .is_some_and(|expected| expected != link.source_hash || expected != file.hash)
        {
            return Err(SceneError::Conflict);
        }
        if !apply && matches!(property, PrefabProperty::Material(_)) {
            rot_assets::verify_project_prefab(
                project_directory(&self.path),
                &file.source,
                &link.asset,
                &file.bytes,
            )
            .map_err(|error| invalid(error.to_string()))?;
        }
        let instance = instance.clone();
        let revision = self.session.revision();
        if apply {
            let root = project_directory(&self.path);
            let files = &self.prefab_files;
            let mut published = None;
            self.session.apply_prefab_override(
                revision,
                &instance,
                object,
                property,
                file.bytes,
                file.source,
                |edit| {
                    published = Some(publish(root, files, edit)?);
                    Ok(())
                },
            )?;
            if let Some(edit) = published {
                self.accept_prefab_edit(edit);
            }
        } else {
            let source = PrefabSource::read_json(&file.bytes)?;
            self.session.revert_prefab_override(
                revision, &instance, object, property, &source, &file.hash,
            )?;
        }
        self.refresh();
        Ok(())
    }

    pub(super) fn unpack_prefab(
        &mut self,
        revision: u64,
        root: &str,
        asset: &str,
        source_hash: &str,
    ) -> Result<usize, SceneError> {
        let count = self
            .session
            .scene()
            .prefabs
            .get(root)
            .map_or(0, |link| link.parts.len());
        self.session
            .unpack_prefab(revision, root, asset, source_hash)?;
        self.refresh();
        Ok(count)
    }

    pub(super) fn show_unpack_prefab(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.unpack_prefab_draft.take() else {
            return;
        };
        let modal = egui::Modal::new(egui::Id::unique("unpack_prefab_instance"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(420.0));
                ui.heading(Text::PrefabUnpackTitle.plain());
                ui.label(&draft.name);
                ui.label(format!("ID {}", draft.root));
                ui.label(
                    Text::PrefabUnpackEffect
                        .plain()
                        .replace("{count}", &draft.count.to_string()),
                );
                ui.label(Text::PrefabUnpackSource.plain());
                if let Some(error) = &draft.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.horizontal(|ui| {
                    let cancel = ui.button(Text::ComponentCancel.plain());
                    if draft.focus_cancel {
                        cancel.request_focus();
                        draft.focus_cancel = false;
                    }
                    let unpack =
                        editor_theme::primary_button(ui, Text::PrefabUnpackConfirm.plain());
                    (cancel.clicked(), unpack.clicked())
                })
                .inner
            });
        let (cancel, unpack) = modal.inner;
        if cancel
            || modal.is_top_modal
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            return;
        }
        if unpack {
            match self.unpack_prefab(
                draft.revision,
                &draft.root,
                &draft.asset,
                &draft.source_hash,
            ) {
                Ok(_) => return,
                Err(error) => draft.error = Some(error.to_string()),
            }
        }
        self.unpack_prefab_draft = Some(draft);
    }

    pub(super) fn prefab_inspector(&mut self, ui: &mut egui::Ui, blocked: bool) {
        let Some(object) = self.draft.as_ref() else {
            return;
        };
        let Some((root, link, part)) =
            self.session
                .scene()
                .prefabs
                .iter()
                .find_map(|(root, link)| {
                    link.parts
                        .values()
                        .find(|part| part.object == object.id)
                        .map(|part| (root, link, part))
                })
        else {
            return;
        };
        let file = self.prefab_files.get(&link.asset);
        ui.separator();
        ui.strong(Text::Prefab.plain());
        if root == &object.id
            && ui
                .add_enabled(
                    !self.authored_edit_pending() && !self.runtime.in_play_mode(),
                    egui::Button::new(Text::PrefabUnpackAction.plain()),
                )
                .clicked()
        {
            self.unpack_prefab_draft = Some(UnpackDraft {
                root: object.id.clone(),
                name: object.name.clone(),
                count: link.parts.len(),
                revision: self.session.revision(),
                asset: link.asset.clone(),
                source_hash: link.source_hash.clone(),
                focus_cancel: true,
                error: None,
            });
        }
        if root != &object.id
            && ui
                .add_enabled(
                    !blocked
                        && !self.runtime.in_play_mode()
                        && !self.authored_edit_pending()
                        && !self.ai_transition_pending()
                        && self.hierarchy_rename.is_none()
                        && self.current_animation_preview().is_none(),
                    egui::Button::new(Text::PrefabSelectRoot.plain()),
                )
                .on_disabled_hover_text(Text::PrefabSelectRootBlocked.plain())
                .clicked()
        {
            self.select_and_reveal_hierarchy(root.clone(), ui.ctx());
            return;
        }
        if let Some(file) = file {
            ui.label(
                Path::new(&file.source)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&file.source),
            );
        } else {
            ui.colored_label(ui.visuals().warn_fg_color, Text::PrefabUnavailable.plain());
            ui.label(Text::PrefabRecovery.plain());
            return;
        }
        let fields = part
            .transform_overrides
            .iter()
            .map(|&field| {
                let label = match field {
                    PrefabFields::Position => "Position",
                    PrefabFields::Rotation => "Rotation",
                    PrefabFields::Scale => "Scale",
                };
                (label, PrefabProperty::Transform(field))
            })
            .chain(part.tint_overrides.iter().filter_map(|id| {
                part.components
                    .get(id)
                    .map(|id| ("Tint", PrefabProperty::Tint(id.clone())))
            }));
        let can_edit = !self.draft_dirty() && !self.runtime.in_play_mode();
        let mut action = None;
        for (label, property) in fields {
            ui.horizontal_wrapped(|ui| {
                ui.label(label);
                if ui
                    .add_enabled(can_edit, egui::Button::new(format!("Apply {label}")))
                    .clicked()
                {
                    action = Some((property.clone(), true));
                }
                if ui
                    .add_enabled(can_edit, egui::Button::new(format!("Revert {label}")))
                    .clicked()
                {
                    action = Some((property, false));
                }
            });
        }
        let renderers = object.components.iter().filter(|component| {
            matches!(
                component.kind,
                rot_scene::component_scene::Component::MeshRenderer { .. }
            )
        });
        let multiple = renderers.clone().take(2).count() == 2;
        for (index, renderer) in renderers.enumerate() {
            let local = part.components.iter().any(|(source, id)| {
                id == &renderer.id && part.material_overrides.contains_key(source)
            });
            let heading = crate::scene_editor::component_inspector::mesh_renderer_heading(
                multiple.then_some(index + 1),
            );
            let state = if local {
                Text::PrefabLocalMaterial
            } else {
                Text::PrefabInheritedMaterial
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{heading} · {}", state.plain()));
                if local {
                    if ui
                        .add_enabled(
                            can_edit,
                            egui::Button::new(Text::PrefabApplyToSource.plain()),
                        )
                        .clicked()
                    {
                        action = Some((PrefabProperty::Material(renderer.id.clone()), true));
                    }
                    if ui
                        .add_enabled(
                            can_edit,
                            egui::Button::new(Text::PrefabRevertMaterial.plain()),
                        )
                        .clicked()
                    {
                        action = Some((PrefabProperty::Material(renderer.id.clone()), false));
                    }
                }
            });
        }
        if let Some((property, apply)) = action {
            let object = object.id.clone();
            let hash =
                matches!(property, PrefabProperty::Material(_)).then(|| link.source_hash.clone());
            let result = self.change_prefab_override(&object, &property, apply, hash.as_deref());
            let label = if apply {
                Text::PrefabApply
            } else {
                Text::PrefabRevert
            };
            self.report_prefab_result(label, result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::component_scene::{ComponentScene, ComponentSession};
    use rot_scene::{Scene, Session};

    #[test]
    fn select_prefab_root_reveals_exact_link_and_preserves_pending_edits() {
        use rot_scene::component_scene::ObjectNode;

        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let root = scene.objects[0].id.clone();
        scene.objects[0].name = "Identical Root".into();
        scene.objects[0].parent = Some("bay-a".into());
        scene.objects[1].name = "Linked Child".into();
        scene.objects[1].parent = Some(root.clone());
        for id in ["bay-a", "bay-b"] {
            scene.objects.push(ObjectNode {
                id: id.into(),
                name: "Bay".into(),
                parent: None,
                enabled: true,
                transform: Transform::default(),
                components: Vec::new(),
            });
        }
        let source = PrefabSource::from_selection(&scene, &root).unwrap();
        let mut session = ComponentSession::new(scene).unwrap();
        session
            .link_prefab(
                0,
                "source".into(),
                "Assets/Missing.prefab".into(),
                &source,
                |_| Ok(()),
            )
            .unwrap();
        session
            .place_prefab(
                1,
                "source".into(),
                source.content_hash().unwrap(),
                &source,
                "second-root",
                Transform::default(),
                Some("bay-b".into()),
            )
            .unwrap();
        session.save(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let before = editor.session.scene().clone();
        assert_eq!(
            before
                .objects
                .iter()
                .filter(|obj| obj.name == "Identical Root")
                .count(),
            2
        );
        let child = before.prefabs["second-root"]
            .parts
            .values()
            .find(|part| part.object != "second-root")
            .unwrap()
            .object
            .clone();
        editor.select(Some(child.clone()));
        editor
            .collapsed_objects
            .extend(["bay-a".into(), "bay-b".into(), "second-root".into()]);
        editor.refresh_disclosure();

        let ctx = egui::Context::default();
        ctx.all_styles_mut(|style| style.animation_time = 0.);
        let frame = |editor: &mut ComponentWorkspace, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280., 960.),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    editor.show_workspace(ui, false);
                },
            );
            output.textures_delta.clear();
            output.shapes
        };
        let button = |shapes: Vec<egui::epaint::ClippedShape>| {
            shapes
                .into_iter()
                .find_map(|shape| match shape.shape {
                    egui::epaint::Shape::Text(text)
                        if text.galley.text() == Text::PrefabSelectRoot.plain() =>
                    {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .expect("missing-source child still shows Select Prefab Root")
        };
        let click = |editor: &mut ComponentWorkspace, pos| {
            frame(editor, vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(
                    editor,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
            }
        };
        editor.hierarchy_search.focus_requested = true;
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![egui::Event::Text("Linked Child".into())]);
        assert!(editor.hierarchy_search.is_active());

        editor.draft.as_mut().unwrap().transform.translation[0] = 3.;
        let pos = button(frame(&mut editor, vec![]));
        click(&mut editor, pos);
        assert_eq!(editor.selection_anchor.as_deref(), Some(child.as_str()));
        assert_eq!(editor.draft.as_ref().unwrap().transform.translation[0], 3.);
        assert!(editor.hierarchy_search.is_active());

        editor.rebuild_draft();
        let pos = button(frame(&mut editor, vec![]));
        click(&mut editor, pos);
        assert_eq!(editor.selection_anchor.as_deref(), Some("second-root"));
        assert!(!editor.hierarchy_search.is_active());
        assert_eq!(
            editor.hierarchy_search_reveal.as_deref(),
            Some("second-root")
        );
        frame(&mut editor, vec![]);
        assert!(editor.hierarchy_search_reveal.is_none());
        assert_eq!(
            editor.collapsed_objects,
            ["bay-a".into(), "second-root".into()].into()
        );
        let row_focus = ctx
            .memory(|memory| memory.focused())
            .and_then(|id| ctx.read_response(id))
            .unwrap();
        assert!(row_focus.has_focus() && row_focus.rect.center().x < 330.);
        assert_eq!(editor.session.scene(), &before);
        assert_eq!(editor.session.revision(), 0);
        assert!(!editor.session.is_dirty());
        assert!(matches!(editor.session.undo(0), Err(SceneError::NoUndo)));
    }

    #[cfg(unix)]
    #[test]
    fn guarded_unpack_keeps_objects_and_history_without_a_source_file() {
        use serde_json::json;
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        let root = scene.objects[0].id.clone();
        let child = scene.objects[1].id.clone();
        scene.objects[1].parent = Some(root.clone());
        let renderer = scene.objects[1].components[0].id.clone();
        let source = PrefabSource::from_selection(&scene, &root).unwrap();
        let hash = source.content_hash().unwrap();
        let mut session = ComponentSession::new(scene).unwrap();
        session
            .link_prefab(
                0,
                "fixture".into(),
                "Assets/Missing.prefab".into(),
                &source,
                |edit| {
                    assert!(edit.before.is_none() && edit.after.is_some());
                    Ok(())
                },
            )
            .unwrap();
        session
            .place_prefab(
                1,
                "fixture".into(),
                hash.clone(),
                &source,
                "peer",
                Transform::default(),
                None,
            )
            .unwrap();
        session
            .set_unique_material(
                2,
                "local-paint",
                rot_scene::OpaqueMaterial::default(),
                &child,
                &renderer,
            )
            .unwrap();
        session.save(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let before = editor.session.scene().clone();
        let source_path = project.path().join("Assets/Missing.prefab");
        assert!(!source_path.exists());
        let detail = editor.ai_command(
            "scene",
            "scene_components_inspect",
            &json!({"object_id": child}),
        );
        assert_eq!(
            detail["result"]["prefab_instance"],
            json!({
                "root_id":root,"asset_id":"fixture","source_hash":hash,"object_count":2
            })
        );
        let mut args = json!({
            "expected_session_token":"scene","expected_revision":"0","action":"unpack",
            "object_id":root,"asset_id":"wrong","expected_source_hash":hash
        });
        assert_eq!(
            editor.ai_command("scene", "scene_prefab", &args)["error"]["code"],
            "stale_prefab_instance"
        );
        assert_eq!(editor.session.scene(), &before);
        args["asset_id"] = json!("fixture");
        args["object_id"] = json!(child);
        assert_eq!(
            editor.ai_command("scene", "scene_prefab", &args)["ok"],
            false
        );
        args["object_id"] = json!(root);
        assert_eq!(
            editor.ai_command("scene", "scene_prefab", &args)["result"]["object_count"],
            2
        );
        let mut detached = before.clone();
        detached.prefabs.remove(&root);
        assert_eq!(editor.session.scene(), &detached);
        assert!(editor.session.scene().materials.contains_key("local-paint"));
        for (action, revision, expected) in
            [("scene_undo", "1", before), ("scene_redo", "2", detached)]
        {
            let result = editor.ai_command(
                "scene",
                action,
                &json!({
                    "expected_session_token":"scene","expected_revision":revision
                }),
            );
            assert_eq!(result["ok"], true, "{result}");
            assert_eq!(editor.session.scene(), &expected);
        }
        assert!(!source_path.exists());
    }

    #[test]
    fn prefab_save_collision_keeps_details_until_retry_succeeds() {
        let project = tempfile::tempdir().unwrap();
        let assets = project.path().join("Assets");
        std::fs::create_dir(&assets).unwrap();
        let path = project.path().join("scene.json");
        ComponentScene::from_legacy(&Session::new(Scene::empty("prefab-feedback")).unwrap())
            .unwrap()
            .save_new(&path)
            .unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let id = editor
            .create_object(Some("example"), "Example", None, None, Transform::default())
            .unwrap();
        let asset = assets.join("Example.prefab");
        std::fs::write(&asset, b"unrelated file").unwrap();

        let result = editor.save_prefab(&id, None);
        editor.report_prefab_result(Text::PrefabSave, result);
        let feedback = editor.prefab_feedback.as_ref().unwrap();
        assert_eq!(feedback.action, Text::PrefabSave);
        assert!(feedback.detail.contains("already exists"));
        assert!(editor.session.scene().prefabs.is_empty());
        assert_eq!(std::fs::read(&asset).unwrap(), b"unrelated file");

        std::fs::remove_file(&asset).unwrap();
        let result = editor.save_prefab(&id, None);
        assert!(result.is_ok());
        editor.report_prefab_result(Text::PrefabSave, result);
        assert!(editor.prefab_feedback.is_none());
        assert_eq!(editor.session.scene().prefabs.len(), 1);
    }
}
