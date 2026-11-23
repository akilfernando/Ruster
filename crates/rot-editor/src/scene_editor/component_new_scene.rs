//! Create a saved v23 sibling while the active document owns its drafts.
use super::{ComponentWorkspace, OpenedScene};
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_scene::component_scene::ComponentScene;
use rot_scene::{Scene, Session};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Default)]
pub(super) struct NewScene {
    name: String,
    save_as: bool,
    focused: bool,
    error: Option<String>,
}

pub(in crate::scene_editor) fn filename(name: &str) -> Result<String, &'static str> {
    let name = name.trim();
    let name = name.strip_suffix(".scene.json").unwrap_or(name);
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    if name.is_empty()
        || name.len() > 120
        || name.starts_with('.')
        || name.ends_with(['.', ' '])
        || name.chars().any(|ch| {
            ch.is_control()
                || matches!(ch, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
                || "<>:\"/\\|?*".contains(ch)
        })
        || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || stem
            .strip_prefix("COM")
            .or_else(|| stem.strip_prefix("LPT"))
            .is_some_and(|port| {
                matches!(
                    port,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
    {
        return Err("Choose a scene name up to 120 UTF-8 bytes without path characters.");
    }
    Ok(format!("{name}.scene.json"))
}

pub(in crate::scene_editor) fn new_scene_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("scene-{:x}-{time:x}-{sequence:x}", std::process::id())
}

impl ComponentWorkspace {
    pub(super) fn begin_new_scene(&mut self) {
        self.begin_scene_sibling(false);
    }

    pub(super) fn begin_save_scene_as(&mut self) {
        self.begin_scene_sibling(true);
    }

    fn begin_scene_sibling(&mut self, save_as: bool) {
        #[cfg(unix)]
        if self.pending_save.is_some() {
            return;
        }
        if self.ai_transition_pending() || self.surface_edit_pending() {
            return;
        }
        self.new_scene = Some(NewScene {
            save_as,
            ..NewScene::default()
        });
        self.error = None;
    }

    pub(super) fn show_new_scene(&mut self, ctx: &egui::Context) -> Option<OpenedScene> {
        let mut draft = self.new_scene.take()?;
        let dirty = !draft.save_as && (self.session.is_dirty() || self.draft_dirty());
        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let response = egui::Modal::new(egui::Id::unique("component_new_scene"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(440.0));
                ui.heading(
                    if draft.save_as {
                        Text::SaveSceneAs
                    } else {
                        Text::LabelNew
                    }
                    .plain(),
                );
                let label = ui.label(Text::NewSceneName.plain());
                let edit = ui
                    .add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .desired_width(f32::INFINITY)
                            .char_limit(160),
                    )
                    .labelled_by(label.id);
                if !draft.focused {
                    edit.request_focus();
                    draft.focused = true;
                }
                if edit.changed() {
                    draft.error = None;
                }
                let path = filename(&draft.name);
                if let Ok(path) = &path {
                    ui.label(format!("{}  {path}", Text::NewSceneFilename.plain()));
                }
                if draft.save_as {
                    ui.weak(Text::SceneVariantCurrentEdits.plain());
                } else if dirty {
                    ui.weak(Text::NewSceneUnsaved.plain());
                }
                if let Some(error) = draft.error.as_ref() {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                let (cancel, create) = ui
                    .horizontal_wrapped(|ui| {
                        let mut create = None;
                        let ready = path.is_ok();
                        let action = if draft.save_as {
                            Text::SaveSceneAs
                        } else if dirty {
                            Text::NewSceneSaveCreate
                        } else {
                            Text::NewSceneCreate
                        };
                        if ui
                            .add_enabled_ui(ready, |ui| {
                                editor_theme::primary_button(ui, action.plain())
                            })
                            .inner
                            .clicked()
                            || ready && enter && (edit.has_focus() || edit.lost_focus())
                        {
                            create = Some(dirty);
                        }
                        if dirty
                            && ui
                                .add_enabled(
                                    ready,
                                    egui::Button::new(Text::NewSceneDiscardCreate.plain()),
                                )
                                .clicked()
                        {
                            create = Some(false);
                        }
                        (ui.button(Text::ComponentCancel.plain()).clicked(), create)
                    })
                    .inner;
                let name_error = path
                    .as_ref()
                    .err()
                    .filter(|_| !draft.name.trim().is_empty() && draft.error.is_none());
                if let Some(error) = name_error {
                    ui.colored_label(editor_theme::colors(ui).error, *error);
                }
                (
                    cancel,
                    create.and_then(|save| path.ok().map(|path| (path, save))),
                )
            });
        if response.inner.0
            || response.is_top_modal
                && ctx
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            return None;
        }
        if let Some((filename, save)) = response.inner.1 {
            let result = if draft.save_as {
                self.save_variant(&filename)
            } else {
                self.create_sibling(&filename, save)
            };
            match result {
                Ok(document) => return Some(document),
                Err(error) => draft.error = Some(error),
            }
        }
        self.new_scene = Some(draft);
        None
    }

    fn save_variant(&mut self, filename: &str) -> Result<OpenedScene, String> {
        let project = self.startup_project.as_ref().map_err(Clone::clone)?;
        let mut candidate = self.session.detached_copy();
        if self.draft_dirty() {
            let edits = self.draft_objects().map_err(|error| error.to_string())?;
            Self::put_draft(&mut candidate, edits).map_err(|error| error.to_string())?;
        }
        let mut scene = candidate.into_scene();
        scene.id = new_scene_id();
        let (path, session) = project.prepare_scene(filename, &scene)?.publish()?;
        Ok(self.open_published_sibling(path, session))
    }

    fn create_sibling(&mut self, filename: &str, save: bool) -> Result<OpenedScene, String> {
        let project = self.startup_project.as_ref().map_err(Clone::clone)?;
        let empty =
            Session::new(Scene::empty(new_scene_id())).map_err(|error| error.to_string())?;
        let scene = ComponentScene::from_legacy(&empty).map_err(|error| error.to_string())?;
        let prepared = project.prepare_scene(filename, &scene)?;
        if save {
            project.verify()?;
            self.save_before_new_scene()?;
        }
        let (path, session) = match prepared.publish() {
            Ok(result) => result,
            Err(error) if save => {
                self.refresh();
                return Err(format!("The current scene was saved. {error}"));
            }
            Err(error) => return Err(error),
        };
        Ok(self.open_published_sibling(path, session))
    }

    fn open_published_sibling(
        &mut self,
        path: std::path::PathBuf,
        session: rot_scene::component_scene::ComponentSession,
    ) -> OpenedScene {
        #[cfg(unix)]
        {
            self.revoke_ai_connection = true;
        }
        OpenedScene::Components(path, Box::new(session))
    }

    fn save_before_new_scene(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        let sequence = self.next_save_sequence().map_err(|(_, message)| message)?;
        let saved = if self.draft_dirty() {
            // Apply Inspector values privately so a failed save preserves the
            // complete authored workspace and its uncommitted draft.
            let mut pending = self.session.clone();
            let edits = self.draft_objects().map_err(|error| error.to_string())?;
            Self::put_draft(&mut pending, edits).map_err(|error| error.to_string())?;
            let saved = pending
                .save_recorded(&self.path)
                .map_err(|error| error.to_string())?;
            self.session = pending;
            saved
        } else {
            self.session
                .save_recorded(&self.path)
                .map_err(|error| error.to_string())?
        };
        #[cfg(unix)]
        self.record_save(sequence, saved);
        #[cfg(not(unix))]
        let _ = saved;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Action;
    use rot_scene::component_scene::ComponentSession;
    use std::{fs, path::Path};

    fn workspace(root: &Path) -> ComponentWorkspace {
        let path = root.join("scene.json");
        let original =
            ComponentScene::from_legacy(&Session::new(Scene::empty("original")).unwrap()).unwrap();
        original.save_new(&path).unwrap();
        ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap())
    }

    #[test]
    fn new_scene_name_preview_and_keyboard_create_an_empty_saved_scene() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = workspace(folder.path());
        for bad in [
            "",
            "..",
            ".secret",
            "a/b",
            "a\\b",
            "aux.scene.json",
            "COM9.level",
            "a:scene",
            "a.",
            "a\u{202e}",
        ] {
            assert!(filename(bad).is_err(), "{bad}");
        }
        let full = format!("{}.scene.json", "a".repeat(120));
        assert_eq!(filename(&full).unwrap(), full);

        let ctx = egui::Context::default();
        let mut opened = None;
        let mut frame = |editor: &mut ComponentWorkspace, events| {
            ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    if let Some(document) = editor.show_new_scene(ui.ctx()) {
                        opened = Some(document);
                    }
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
        editor.command(Action::New);
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.new_scene.is_none());
        assert!(!folder.path().join("Title Card.scene.json").exists());

        editor.command(Action::New);
        frame(&mut editor, vec![]);
        frame(
            &mut editor,
            vec![egui::Event::Text(" Title Card – 夜 ".into())],
        );
        assert_eq!(
            filename(&editor.new_scene.as_ref().unwrap().name).unwrap(),
            "Title Card – 夜.scene.json"
        );
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        let Some(OpenedScene::Components(path, mut session)) = opened else {
            panic!("Enter creates a component sibling");
        };
        assert_eq!(path, folder.path().join("Title Card – 夜.scene.json"));
        assert!(session.scene().objects.is_empty());
        assert_ne!(session.scene().id, editor.session.scene().id);
        assert_eq!(session.revision(), 0);
        assert_eq!(
            session.scene(),
            ComponentSession::open(&path).unwrap().scene()
        );
        assert!(!session.is_dirty());
        assert!(matches!(
            session.undo(0),
            Err(rot_scene::SceneError::NoUndo)
        ));
    }

    #[test]
    fn new_scene_save_conflict_preserves_draft_and_discard_waits_for_publication() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = workspace(folder.path());
        let old_path = editor.path.clone();
        let original = fs::read(&old_path).unwrap();
        editor.command(Action::CreateCube);
        let id = editor.session.scene().objects[0].id.clone();
        editor.select(Some(id.clone()));
        editor.draft.as_mut().unwrap().name = "Pending name".into();
        let before = editor.session.scene().clone();
        let revision = editor.session.revision();
        let side = folder.path().join("Title.scene.json");
        fs::write(&old_path, b"external current scene edit").unwrap();
        assert!(editor.create_sibling("Title.scene.json", true).is_err());
        assert!(!side.exists());
        assert_eq!(editor.session.revision(), revision);
        assert_eq!(editor.session.scene(), &before);
        assert!(editor.selection.contains(&id));
        assert_eq!(editor.draft.as_ref().unwrap().name, "Pending name");
        assert!(editor.draft_dirty());

        fs::write(&side, b"other creator's scene").unwrap();
        assert!(editor.create_sibling("Title.scene.json", false).is_err());
        assert_eq!(fs::read(&side).unwrap(), b"other creator's scene");
        assert_eq!(editor.session.scene(), &before);
        assert!(editor.draft_dirty());
        assert!(matches!(
            editor.create_sibling("Discard Target.scene.json", false),
            Ok(OpenedScene::Components(_, _))
        ));
        assert_eq!(fs::read(&old_path).unwrap(), b"external current scene edit");

        fs::write(&old_path, original).unwrap();
        assert!(matches!(
            editor.create_sibling("Save Target.scene.json", true),
            Ok(OpenedScene::Components(_, _))
        ));
        let saved = ComponentSession::open(old_path).unwrap();
        assert_eq!(saved.scene().objects[0].name, "Pending name");
    }

    #[cfg(unix)]
    #[test]
    fn new_scene_staging_cannot_overwrite_a_racing_file_or_retarget_a_project() {
        use super::super::super::project_startup::ProjectDirectory;
        use std::os::unix::fs::symlink;
        let folder = tempfile::tempdir().unwrap();
        let root = folder.path().join("Project");
        fs::create_dir(&root).unwrap();
        let editor = workspace(&root);
        let scene = editor.session.scene();
        let project = ProjectDirectory::capture(&root).unwrap();
        let target = root.join("Title.scene.json");
        let staged = project.prepare_scene("Title.scene.json", scene).unwrap();
        assert!(!target.exists());
        fs::write(&target, b"winning publication").unwrap();
        assert!(staged.publish().is_err());
        assert_eq!(fs::read(&target).unwrap(), b"winning publication");

        let link = root.join("Alias.scene.json");
        symlink(&target, &link).unwrap();
        assert!(project.prepare_scene("Alias.scene.json", scene).is_err());
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());

        let staged = project.prepare_scene("Late.scene.json", scene).unwrap();
        let moved = folder.path().join("Moved Project");
        fs::rename(&root, &moved).unwrap();
        fs::create_dir(&root).unwrap();
        assert!(staged.publish().is_err());
        assert!(!root.join("Late.scene.json").exists());
        assert!(!moved.join("Late.scene.json").exists());
    }
}
