//! Manual and MCP entry points for the same guarded creator export operation.
use super::SceneEditor;
use crate::creation_export::{Identity, Request};
use eframe::egui;
use serde_json::{json, Value};
use std::path::PathBuf;

type Result<T> = std::result::Result<T, (String, String)>;
fn fail(code: &str, message: impl Into<String>) -> (String, String) {
    (code.into(), message.into().chars().take(4096).collect())
}
fn number(value: &Value) -> Option<u64> {
    let s = value.as_str()?;
    if s.is_empty() || (s != "0" && s.starts_with('0')) || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}
impl SceneEditor {
    fn export_identity(&self) -> Identity {
        Identity {
            generation: self.generation,
            revision: self.session.scene().revision,
            persistence_token: self.persistence_token(),
            asset_revision: self.assets.as_ref().map(|s| s.revision()),
            shader_revision: self.shader_revision,
        }
    }
    fn export_ready(&self, own_dialog: bool) -> std::result::Result<(), String> {
        let guard = if own_dialog {
            self.guard_except_export_dialog()
        } else {
            self.guard()
        };
        guard.map_err(|code| match code.as_str() {
            "drafts_pending" => "Finish or revert unapplied edits before exporting.".to_string(),
            _ => "Resolve the open dialog or recovery choice before exporting.".to_string(),
        })?;
        if self.manipulation.offset_pending() {
            return Err("Apply or reset the translation offset before exporting.".into());
        }
        if self.session.path().is_none()
            || self.session.is_dirty()
            || self.session.loaded_format_version() != ruster_scene::FORMAT_VERSION
        {
            return Err("Save the scene before exporting.".into());
        }
        if !self.shader_builtin {
            return Err("Custom shaders cannot be exported yet. Choose the built-in appearance for this session.".into());
        }
        if self.creation_export.busy() {
            return Err("An export is already running. Wait or cancel it first.".into());
        }
        Ok(())
    }
    fn start_creation_export(
        &mut self,
        title: String,
        destination: PathBuf,
        mcp_scope: bool,
        initial_camera: Option<ruster_render::CameraState>,
    ) -> std::result::Result<Value, String> {
        self.export_ready(false)?;
        if title.trim().is_empty()
            || title.trim() != title
            || title.len() > 128
            || title.chars().any(char::is_control)
        {
            return Err(
                "Use a title of 1–128 UTF-8 bytes without controls or surrounding whitespace."
                    .into(),
            );
        }
        let (mesh_ids, texture_ids) =
            crate::creation_export::scene_dependencies(self.session.scene())?;
        let ids = mesh_ids.union(&texture_ids).cloned().collect();
        let assets = match self.assets.as_ref() {
            Some(store) => {
                for id in &mesh_ids {
                    if store.mesh(id).is_none() {
                        return Err(format!("Mesh {id} has no accepted mesh content; repair its source and Refresh before exporting."));
                    }
                }
                for id in &texture_ids {
                    if store.texture(id).is_none() {
                        return Err(format!("Texture {id} has no accepted texture content; repair its source and Refresh before exporting."));
                    }
                }
                Some(store.export_snapshot(&ids).map_err(|e| e.to_string())?)
            }
            None if mesh_ids.is_empty() && texture_ids.is_empty() => None,
            None => {
                return Err(
                    "Repair the content catalog before exporting imported models or textures."
                        .into(),
                )
            }
        };
        let ctx = self
            .export_context
            .as_ref()
            .ok_or("The editor is still starting; retry after its first frame.")?
            .clone();
        let runtime_kit = self
            .creation_export
            .runtime_kit()
            .ok_or("Choose a prepared Linux runtime kit before exporting.")?
            .to_path_buf();
        let request = Request {
            identity: self.export_identity(),
            title,
            scene_path: self.session.path().unwrap().to_path_buf(),
            scene_bytes: self.session.snapshot_bytes().map_err(|e| e.to_string())?,
            assets,
            destination,
            runtime_kit,
            mcp_scope,
            initial_camera,
        };
        let accepted = self.creation_export.start(&ctx, request)?;
        self.notice =
            "Exporting application… You can keep editing; changed work cancels this export.".into();
        // MCP already receives the operation/receipt through inspection. Do not
        // open an unsolicited window over its subsequent viewport captures.
        // Existing human-opened status windows remain under human ownership.
        if !mcp_scope {
            self.export_status_open = true;
        }
        Ok(accepted)
    }
    pub(super) fn export_command(&mut self, token: &str, op: &str, args: &Value) -> Result<Value> {
        let map = args
            .as_object()
            .ok_or_else(|| fail("invalid_arguments", "Expected argument object"))?;
        let (required, optional): (&[&str], &[&str]) = match op {
            "creation_export_inspect" => (&[], &["operation_id"]),
            "creation_export_start" => (
                &[
                    "expected_session_token",
                    "expected_revision",
                    "expected_persistence_token",
                    "expected_asset_revision",
                    "expected_shader_revision",
                    "expected_export_token",
                    "title",
                    "filename",
                ],
                &["initial_camera"],
            ),
            "creation_export_cancel" => (&["expected_session_token", "operation_id"], &[]),
            _ => {
                return Err(fail(
                    "unsupported_operation",
                    "Unknown creation export operation",
                ));
            }
        };
        if required.iter().any(|key| !map.contains_key(*key))
            || map
                .keys()
                .any(|key| !required.contains(&key.as_str()) && !optional.contains(&key.as_str()))
        {
            return Err(fail(
                "invalid_arguments",
                "Arguments must match the export tool schema",
            ));
        }
        let operation = args
            .get("operation_id")
            .map(|v| {
                number(v).filter(|n| *n > 0).ok_or_else(|| {
                    fail(
                        "invalid_arguments",
                        "operation_id must be a positive decimal u64 string",
                    )
                })
            })
            .transpose()?;
        if op == "creation_export_inspect" {
            let mut value = self
                .creation_export
                .inspect(operation)
                .map_err(|e| fail("unknown_operation", e))?;
            let block = self
                .export_ready(false)
                .err()
                .or_else(|| {
                    self.creation_export
                        .runtime_kit()
                        .is_none()
                        .then(|| "Configure a prepared Linux export template.".to_string())
                })
                .or_else(|| {
                    self.creation_export
                        .output_directory()
                        .is_none()
                        .then(|| "Grant an MCP export directory at startup.".to_string())
                });
            value["session_token"] = json!(token);
            value["revision"] = json!(self.session.scene().revision.to_string());
            value["persistence_token"] = json!(self.persistence_token());
            value["asset_revision"] = json!(self.assets.as_ref().map(|s| s.revision().to_string()));
            value["shader_revision"] = json!(self.shader_revision.to_string());
            value["builtin_appearance"] = json!(self.shader_builtin);
            value["can_start"] = json!(
                block.is_none()
                    && self.creation_export.runtime_kit().is_some()
                    && self.creation_export.output_directory().is_some()
            );
            value["blocked_reason"] = json!(block);
            return Ok(value);
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(fail(
                "stale_session",
                "Inspect the current editor scene again",
            ));
        }
        if op == "creation_export_cancel" {
            self.creation_export
                .inspect(operation)
                .map_err(|e| fail("unknown_operation", e))?;
            let current = self
                .creation_export
                .inspect(None)
                .map_err(|e| fail("unknown_operation", e))?;
            if self.creation_export.busy()
                && current["operation"]["operation_id"] == args["operation_id"]
            {
                self.creation_export.cancel();
            }
            return self
                .creation_export
                .inspect(operation)
                .map_err(|e| fail("unknown_operation", e));
        }
        let revision = number(&args["expected_revision"]).ok_or_else(|| {
            fail(
                "invalid_arguments",
                "expected_revision must be a decimal u64 string",
            )
        })?;
        let shader = number(&args["expected_shader_revision"]).ok_or_else(|| {
            fail(
                "invalid_arguments",
                "expected_shader_revision must be a decimal u64 string",
            )
        })?;
        let assets = if args["expected_asset_revision"].is_null() {
            None
        } else {
            Some(number(&args["expected_asset_revision"]).ok_or_else(|| {
                fail(
                    "invalid_arguments",
                    "expected_asset_revision must be null or a decimal u64 string",
                )
            })?)
        };
        if revision != self.session.scene().revision {
            return Err(fail("stale_revision", "Inspect current authored state"));
        }
        if args["expected_persistence_token"].as_str() != Some(self.persistence_token().as_str()) {
            return Err(fail(
                "stale_persistence",
                "Inspect the save association again",
            ));
        }
        if assets != self.assets.as_ref().map(|s| s.revision()) {
            return Err(fail("stale_assets", "Inspect the content catalog again"));
        }
        if shader != self.shader_revision {
            return Err(fail("stale_shader", "Inspect the current appearance again"));
        }
        if args["expected_export_token"].as_str()
            != Some(self.creation_export.configuration_token().as_str())
        {
            return Err(fail(
                "stale_export_configuration",
                "Inspect the configured runtime again",
            ));
        }
        let title = args["title"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or_else(|| fail("invalid_arguments", "title must be a bounded string"))?
            .to_string();
        let filename = args["filename"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 128)
            .ok_or_else(|| fail("invalid_arguments", "filename must be a bounded string"))?;
        self.guard().map_err(|code| {
            fail(
                &code,
                "Finish or revert human edits and resolve dialogs before exporting through MCP",
            )
        })?;
        if self.manipulation.offset_pending() {
            return Err(fail(
                "drafts_pending",
                "Apply or reset the translation offset before exporting through MCP",
            ));
        }
        let destination = self
            .creation_export
            .mcp_destination(filename)
            .map_err(|e| fail("export_permission", e))?;
        let initial_camera = crate::creation_export::parse_initial_camera(&args["initial_camera"])
            .map_err(|e| fail("invalid_arguments", e))?;
        self.start_creation_export(title, destination, true, initial_camera)
            .map_err(|e| fail("export_blocked", e))
    }
    pub(super) fn open_export_dialog(&mut self) {
        if self.creation_export.busy() {
            self.export_status_open = true;
            return;
        }
        if let Err(e) = self.guard() {
            self.error = format!("Finish current fields or dialogs before exporting ({e}).");
            return;
        }
        if self.export_title.is_empty() {
            self.export_title = self
                .session
                .path()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.session.scene().id.clone());
        }
        if self.export_destination.is_empty() {
            self.export_destination = self
                .session
                .path()
                .and_then(|p| p.parent())
                .unwrap_or(std::path::Path::new("."))
                .join(format!("{}.tar.gz", self.session.scene().id))
                .display()
                .to_string();
        }
        self.export_runtime_kit = self
            .creation_export
            .runtime_kit()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        self.export_dialog = true;
        self.export_use_current_view = false;
        self.export_focused = false;
        self.export_error.clear();
    }
    pub(super) fn use_builtin_export_appearance(&mut self) -> std::result::Result<(), String> {
        self.guard_except_export_dialog()?;
        if self.manipulation.offset_pending() {
            return Err("Apply or reset the translation offset first.".into());
        }
        let revision = self
            .shader_revision
            .checked_add(1)
            .ok_or("Appearance revision exhausted")?;
        let render = self.render.as_ref().ok_or("Renderer unavailable")?;
        ruster_render::refresh_shader(render, ruster_render::default_shader())?;
        if self.error == self.shader_notice {
            self.error.clear();
        }
        self.shader_revision = revision;
        self.shader_builtin = true;
        self.shader_notice = "Built-in scene shader".into();
        Ok(())
    }
    /// Guarded owner-thread completion/publication, independent of painting.
    /// The host must defer this until fresh human input/close has priority.
    pub(super) fn export_maintenance(&mut self) {
        // Draft intent need not advance authored revision. It still invalidates
        // publication, as do dialogs/gestures/project changes.
        if self.guard().is_err() || self.manipulation.offset_pending() {
            self.creation_export.cancel();
        }
        let current_identity = self.export_identity();
        if self.creation_export.poll(&current_identity) {
            if let Ok(state) = self.creation_export.inspect(None) {
                match state["operation"]["state"].as_str() {
                    Some("committed") => self.notice =
                        "Application exported. The archive contains the captured saved creation."
                            .into(),
                    Some("cancelled") => {
                        self.notice =
                            "Export cancelled; existing files and current work were preserved."
                                .into()
                    }
                    Some("failed") => {
                        self.error = state["operation"]["error"]
                            .as_str()
                            .unwrap_or("Export failed")
                            .chars()
                            .take(4096)
                            .collect()
                    }
                    _ => (),
                }
            }
        }
    }
    pub(super) fn export_frame(&mut self, ctx: &egui::Context) {
        self.export_maintenance();
        if self.export_dialog {
            egui::Modal::new(egui::Id::unique("creator_export_dialog")).show(ctx, |ui| {
                ui.set_max_width((ctx.content_rect().width() - 64.0).clamp(200.0, 560.0));
                // Observe form keys before a text field consumes them. Let buttons
                // receive keyboard activation before choosing the form action.
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                ui.heading("Export application");
                ui.label("Title");
                let title = ui.add(
                    egui::TextEdit::singleline(&mut self.export_title)
                        .desired_width(ui.available_width()),
                );
                if !self.export_focused {
                    title.request_focus();
                    self.export_focused = true;
                }
                ui.label("Linux export template");
                ui.add(
                    egui::TextEdit::singleline(&mut self.export_runtime_kit)
                        .desired_width(ui.available_width()),
                );
                ui.label("Archive (.tar.gz)");
                ui.add(
                    egui::TextEdit::singleline(&mut self.export_destination)
                        .desired_width(ui.available_width()),
                );
                ui.checkbox(&mut self.export_use_current_view, "Use current view")
                    .on_hover_text("Start the exported application with this camera and projection. Requires a current Linux export template.");
                let blocked = self.export_ready(true).err();
                if let Some(reason) = &blocked {
                    ui.label(reason);
                }
                if (self.session.path().is_none()
                    || self.session.is_dirty()
                    || self.session.loaded_format_version() != ruster_scene::FORMAT_VERSION)
                    && ui.button("Save scene").clicked()
                {
                    self.export_dialog = false;
                    self.save();
                    return;
                }
                let mut appearance_activated = false;
                if !self.shader_builtin
                    && ui
                        .button("Use built-in appearance for this session")
                        .clicked()
                {
                    appearance_activated = true;
                    if let Err(error) = self.use_builtin_export_appearance() {
                        self.export_error = error;
                    }
                }
                let (submit, cancel) = ui
                    .horizontal(|ui| {
                        let submit = ui.add_enabled(
                            blocked.is_none(),
                            egui::Button::new("Export application"),
                        );
                        let cancel = ui.button("Cancel");
                        (submit, cancel)
                    })
                    .inner;
                if cancel.clicked() || escape || (enter && cancel.has_focus()) {
                    self.export_dialog = false;
                } else if (submit.clicked() || (enter && !appearance_activated))
                    && blocked.is_none()
                {
                    let runtime = PathBuf::from(self.export_runtime_kit.trim());
                    let destination = PathBuf::from(self.export_destination.trim());
                    let configured = if self.export_runtime_kit.trim().is_empty() {
                        Err("Choose a prepared Linux runtime kit.".into())
                    } else {
                        self.creation_export.configure_runtime_kit(&runtime)
                    };
                    let result = configured.and_then(|()| {
                        if self.export_destination.trim().is_empty() {
                            return Err("Choose a fresh archive path.".into());
                        }
                        let destination = if destination.is_absolute() {
                            destination
                        } else {
                            self.session
                                .path()
                                .and_then(|p| p.parent())
                                .unwrap_or(std::path::Path::new("."))
                                .join(destination)
                        };
                        self.export_dialog = false;
                        let initial_camera = if self.export_use_current_view {
                            Some(self.navigation.camera.filter(|camera| camera.is_valid()).ok_or("The current viewport camera is not ready; close this dialog and frame the view first.")?)
                        } else { None };
                        self.start_creation_export(self.export_title.clone(), destination, false, initial_camera)
                            .map(|_| ())
                    });
                    if let Err(error) = result {
                        self.export_dialog = true;
                        self.export_error = error;
                    }
                }
                ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Enter);
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                });
                if !self.export_error.is_empty() {
                    ui.colored_label(ui.visuals().error_fg_color, &self.export_error);
                }
            });
        }
        if self.export_status_open {
            let mut open = true;
            egui::Window::new("Application export")
                .open(&mut open)
                .resizable(true)
                .show(ctx, |ui| {
                    if let Ok(state) = self.creation_export.inspect(None) {
                        match state["operation"]["state"].as_str() {
                            Some("running") => {
                                ui.label("Preparing your application…");
                            }
                            Some("cancelling") => {
                                ui.label("Cancelling export…");
                            }
                            Some("committed") => {
                                ui.heading("Export ready");
                                ui.label(
                                    state["operation"]["receipt"]["archive"]
                                        .as_str()
                                        .unwrap_or("Archive created"),
                                );
                            }
                            Some("cancelled") => {
                                ui.label("Export cancelled. Your work is unchanged.");
                            }
                            Some("failed") => {
                                ui.colored_label(
                                    ui.visuals().error_fg_color,
                                    state["operation"]["error"]
                                        .as_str()
                                        .unwrap_or("Export failed"),
                                );
                            }
                            _ => {
                                ui.label("No export is running.");
                            }
                        }
                    }
                    if self.creation_export.busy() {
                        if ui.button("Cancel export").clicked() {
                            self.creation_export.cancel();
                        }
                    } else if ui.button("Close export status").clicked() {
                        self.export_status_open = false;
                    }
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                        self.creation_export.cancel();
                        self.export_status_open = false;
                    }
                });
            if !open {
                self.creation_export.cancel();
                self.export_status_open = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::{Scene, Session};
    fn frame(
        editor: &mut SceneEditor,
        ctx: &egui::Context,
        key: Option<egui::Key>,
        modifiers: egui::Modifiers,
    ) {
        let events = std::iter::once(egui::Event::ModifiersChanged(modifiers))
            .chain(key.into_iter().flat_map(|key| {
                [true, false].map(move |pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers,
                })
            }))
            .collect();
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                events,
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            },
            |ui| {
                editor.export_context = Some(ctx.clone());
                editor.route_shortcuts(ui.ctx());
                editor.process_queued_action();
                editor.export_frame(ctx);
            },
        );
        output.textures_delta.clear();
    }
    #[test]
    fn export_dialog_keyboard_and_shared_preconditions_preserve_scene() {
        let dir = tempfile::tempdir().unwrap();
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        editor.save_scene(dir.path().join("scene.json")).unwrap();
        let before = editor.session.snapshot_bytes().unwrap();
        let ctx = egui::Context::default();
        frame(&mut editor, &ctx, None, egui::Modifiers::NONE);
        frame(
            &mut editor,
            &ctx,
            Some(egui::Key::E),
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..Default::default()
            },
        );
        assert!(
            editor.export_dialog,
            "discoverable remappable shortcut opens export"
        );
        frame(&mut editor, &ctx, None, egui::Modifiers::NONE);
        frame(
            &mut editor,
            &ctx,
            Some(egui::Key::Enter),
            egui::Modifiers::NONE,
        );
        assert!(
            editor.export_dialog && editor.export_error.contains("Choose"),
            "Enter in the title field must submit the form, then explain the missing template: {}",
            editor.export_error
        );
        assert!(!editor.creation_export.busy());
        frame(
            &mut editor,
            &ctx,
            Some(egui::Key::Escape),
            egui::Modifiers::NONE,
        );
        assert!(!editor.export_dialog);
        let inspected =
            editor.command("test", "creation_export_inspect", json!({}))["result"].clone();
        let mut args = json!({"expected_session_token":inspected["session_token"],"expected_revision":inspected["revision"],"expected_persistence_token":inspected["persistence_token"],"expected_asset_revision":inspected["asset_revision"],"expected_shader_revision":inspected["shader_revision"],"expected_export_token":inspected["export_token"],"title":"Creator keyboard","filename":"creator.tar.gz"});
        editor.save_scene(dir.path().join("scene.json")).unwrap();
        assert_eq!(
            editor.command("test", "creation_export_start", args.clone())["error"]["code"],
            "stale_persistence"
        );
        args["expected_persistence_token"] = json!(editor.persistence_token());
        editor.draft.as_mut().unwrap().translation[0] = "-".into();
        assert_eq!(
            editor.command("test", "creation_export_start", args)["error"]["code"],
            "drafts_pending"
        );
        editor.sync_draft();
        editor.open_export_dialog();
        editor.export_runtime_kit = dir.path().display().to_string();
        frame(&mut editor, &ctx, None, egui::Modifiers::NONE);
        for _ in 0..5 {
            frame(
                &mut editor,
                &ctx,
                Some(egui::Key::Tab),
                egui::Modifiers::NONE,
            );
        }
        frame(
            &mut editor,
            &ctx,
            Some(egui::Key::Enter),
            egui::Modifiers::NONE,
        );
        assert!(
            !editor.export_dialog && !editor.creation_export.busy(),
            "Enter on the traversed Cancel button must not start a worker"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        assert!(!editor.session.is_dirty());
        assert!(!dir.path().join("creator.tar.gz").exists());
    }
}
