//! Shared native and MCP prefab authoring against one component session.
use super::{project_directory, ComponentWorkspace, Transform};
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

impl ComponentWorkspace {
    #[cfg(target_os = "linux")]
    pub(super) fn adopt_prefabs(
        &mut self,
        files: &mut Vec<PrefabFile>,
        issues: &mut Vec<ProjectFileIssue>,
    ) {
        files.retain(|file| {
            if let Err(error) = PrefabSource::read_json(&file.bytes) {
                issues.push(ProjectFileIssue {
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
        if !self.prefab_refresh_pending || self.draft_dirty() || self.runtime.in_play_mode() {
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
        for id in pending {
            let file = &self.prefab_files[&id];
            let result = PrefabSource::read_json(&file.bytes).and_then(|source| {
                self.session
                    .refresh_prefab(self.session.revision(), &id, &file.hash, &source)
            });
            if let Err(error) = result {
                self.error = Some(format!("Couldn't update {}. {error}", file.source));
            }
        }
        if revision != self.session.revision() {
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
        ui.strong("Prefab");
        if let Some(file) = file {
            ui.label(
                Path::new(&file.source)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&file.source),
            );
        } else {
            ui.colored_label(ui.visuals().warn_fg_color, "Prefab Source Unavailable");
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
        if let Some((property, apply)) = action {
            let object = object.id.clone();
            self.error = self
                .change_prefab_override(&object, &property, apply)
                .err()
                .map(|error| error.to_string());
        }
    }
}
