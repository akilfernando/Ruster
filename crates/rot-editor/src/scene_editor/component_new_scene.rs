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
    focused: bool,
    error: Option<String>,
}

fn filename(name: &str) -> Result<String, &'static str> {
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

fn new_scene_id() -> String {
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
        #[cfg(unix)]
        if self.pending_save.is_some() {
            return;
        }
        if self.ai_transition_pending() || self.surface_edit_pending() {
            return;
        }
        self.new_scene = Some(NewScene::default());
        self.error = None;
    }

    pub(super) fn show_new_scene(&mut self, ctx: &egui::Context) -> Option<OpenedScene> {
        let mut draft = self.new_scene.take()?;
        let dirty = self.session.is_dirty() || self.draft_dirty();
        let enter = ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let response = egui::Modal::new(egui::Id::unique("component_new_scene"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(440.0));
                ui.heading(Text::LabelNew.plain());
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
                if dirty {
                    ui.weak(Text::NewSceneUnsaved.plain());
                }
                if let Some(error) = draft.error.as_ref() {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                let (cancel, create) = ui
                    .horizontal_wrapped(|ui| {
                        let mut create = None;
                        let ready = path.is_ok();
                        let action = if dirty {
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
                if path.is_err() && !draft.name.trim().is_empty() && draft.error.is_none() {
                    ui.colored_label(
                        editor_theme::colors(ui).error,
                        path.as_ref().unwrap_err().to_string(),
                    );
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
            match self.create_sibling(&filename, save) {
                Ok(document) => return Some(document),
                Err(error) => draft.error = Some(error),
            }
        }
        self.new_scene = Some(draft);
        None
    }

    fn create_sibling(&mut self, filename: &str, save: bool) -> Result<OpenedScene, String> {
        let project = self.startup_project.as_ref().map_err(Clone::clone)?;
        let empty =
            Session::new(Scene::empty(new_scene_id())).map_err(|error| error.to_string())?;
        let scene = ComponentScene::from_legacy(&empty).map_err(|error| error.to_string())?;
        let prepared = project.prepare_scene(filename, &scene)?;
        // Commit pending Inspector values only in an isolated copy. A failed
        // old-scene save must leave the active draft and history intact.
        let saved = if save {
            let mut pending = self.session.clone();
            if self.draft_dirty() {
                let edits = self.draft_objects().map_err(|error| error.to_string())?;
                Self::put_draft(&mut pending, edits).map_err(|error| error.to_string())?;
            }
            project.verify()?;
            pending
                .save_recorded(&self.path)
                .map_err(|error| error.to_string())?;
            Some(pending)
        } else {
            None
        };
        let (path, session) = match prepared.publish() {
            Ok(result) => result,
            Err(error) => {
                if let Some(saved) = saved {
                    self.session = saved;
                    self.refresh();
                    return Err(format!("The current scene was saved. {error}"));
                }
                return Err(error);
            }
        };
        #[cfg(unix)]
        {
            self.revoke_ai_connection = true;
        }
        Ok(OpenedScene::Components(path, Box::new(session)))
    }
}
