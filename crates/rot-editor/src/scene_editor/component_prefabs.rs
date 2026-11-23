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

    pub(super) fn prefab_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(object) = self.draft.as_ref() else {
            return;
        };
        let Some((link, part)) = self.session.scene().prefabs.values().find_map(|link| {
            link.parts
                .values()
                .find(|part| part.object == object.id)
                .map(|part| (link, part))
        }) else {
            return;
        };
        let file = self.prefab_files.get(&link.asset);
        ui.separator();
        ui.strong(Text::Prefab.plain());
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
