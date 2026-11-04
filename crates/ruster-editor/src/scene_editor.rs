//! Generic scene authoring frontend; logistics rules stay in the legacy sample.
use eframe::{
    egui,
    egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
};
use ruster_scene::{Preview, Session, Transform};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
#[derive(Clone, PartialEq)]
struct Fields {
    translation: [String; 3],
    rotation: [String; 3],
    scale: [String; 3],
}
impl From<&Transform> for Fields {
    fn from(t: &Transform) -> Self {
        Self {
            translation: t.translation.map(|v| v.to_string()),
            rotation: t.rotation.map(|v| v.to_string()),
            scale: t.scale.map(|v| v.to_string()),
        }
    }
}
impl Fields {
    fn parse(&self) -> Result<Transform, String> {
        fn triple(values: &[String; 3]) -> Result<[f32; 3], String> {
            let mut result = [0.; 3];
            for (i, s) in values.iter().enumerate() {
                result[i] = s.trim().parse().map_err(|_| {
                    "Enter finite decimal transform values; your fields are preserved.".to_string()
                })?;
            }
            Ok(result)
        }
        let t = Transform {
            translation: triple(&self.translation)?,
            rotation: triple(&self.rotation)?,
            scale: triple(&self.scale)?,
        };
        t.validate().map_err(|e| e.to_string())?;
        Ok(t)
    }
}
static GENERATION: AtomicU64 = AtomicU64::new(1);

pub struct SceneEditor {
    session: Session,
    selected: Option<String>,
    draft: Option<Fields>,
    preview: Preview,
    show_preview: bool,
    running: bool,
    last_tick: Instant,
    generation: u64,
    error: String,
    notice: String,
    path_text: String,
    dialog: Option<bool>,
    close_pending: bool,
    close_approved: bool,
}
impl SceneEditor {
    fn new(session: Session) -> Self {
        let selected = session.scene().objects.first().map(|o| o.id.clone());
        let draft = session
            .scene()
            .objects
            .first()
            .map(|o| Fields::from(&o.transform));
        let preview = Preview::new(session.scene());
        Self {
            session,
            selected,
            draft,
            preview,
            show_preview: false,
            running: false,
            last_tick: Instant::now(),
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            error: String::new(),
            notice: "Ready. Right-drag to orbit; scroll to zoom.".into(),
            path_text: String::new(),
            dialog: None,
            close_pending: false,
            close_approved: false,
        }
    }
    fn drafts(&self) -> bool {
        self.selected
            .as_ref()
            .and_then(|id| self.session.scene().objects.iter().find(|o| &o.id == id))
            .map(|o| Some(Fields::from(&o.transform)).as_ref() != self.draft.as_ref())
            .unwrap_or(false)
    }
    fn sync_draft(&mut self) {
        self.draft = self
            .selected
            .as_ref()
            .and_then(|id| self.session.scene().objects.iter().find(|o| &o.id == id))
            .map(|o| Fields::from(&o.transform));
    }
    fn guard(&self) -> Result<(), String> {
        if self.drafts() {
            Err("drafts_pending".into())
        } else if self.dialog.is_some() || self.close_pending || self.close_approved {
            Err("recovery_pending".into())
        } else {
            Ok(())
        }
    }
    fn apply(&mut self) -> bool {
        let Some(id) = self.selected.clone() else {
            return true;
        };
        let Some(fields) = self.draft.clone() else {
            return true;
        };
        let draft = match fields.parse() {
            Ok(t) => t,
            Err(e) => {
                self.error = e;
                return false;
            }
        };
        match self
            .session
            .edit(self.session.scene().revision, &id, &draft)
        {
            Ok(_) => {
                self.sync_draft();
                self.error.clear();
                self.notice = "Authored transform applied. Reset preview to use it.".into();
                true
            }
            Err(e) => {
                self.error = e.to_string();
                false
            }
        }
    }
    fn save(&mut self) {
        if !self.apply() {
            return;
        }
        let path = self.session.path().map(|p| p.to_path_buf());
        if let Some(path) = path {
            match self.session.save(path) {
                Ok(()) => self.notice = "Scene saved.".into(),
                Err(e) => self.error = e.to_string(),
            }
        } else {
            self.dialog = Some(false);
        }
    }
    fn history(&mut self, redo: bool) {
        if let Err(e) = self.guard() {
            self.error = e;
            return;
        }
        let r = self.session.scene().revision;
        let result = if redo {
            self.session.redo(r)
        } else {
            self.session.undo(r)
        };
        match result {
            Ok(_) => {
                self.sync_draft();
                self.notice = "Authored history changed; preview unchanged.".into()
            }
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn command(&mut self, nonce: &str, op: &str, args: Value) -> Value {
        let token = format!("{nonce}:scene:{}", self.generation);
        match self.execute(&token, op, &args) {
            Ok(v) => json!({"ok":true,"result":v}),
            Err((code, message)) => {
                json!({"ok":false,"error":{"code":code,"message":message,"session_token":token,"revision":self.session.scene().revision.to_string()}})
            }
        }
    }
    fn execute(&mut self, token: &str, op: &str, args: &Value) -> Result<Value, (String, String)> {
        let err = |s: &str| (s.to_string(), s.to_string());
        let keys: &[&str] = match op {
            "scene_inspect" => &["offset"],
            "scene_transform" => &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                "transform",
            ],
            "scene_undo" | "scene_redo" => &["expected_session_token", "expected_revision"],
            "scene_replay" => &["expected_session_token", "expected_revision", "ticks"],
            _ => return Err(err("unsupported_operation")),
        };
        let map = args.as_object().ok_or_else(|| err("invalid_arguments"))?;
        if map.keys().any(|k| !keys.contains(&k.as_str())) {
            return Err(err("invalid_arguments"));
        }
        let number = |v: &Value| -> Result<u64, (String, String)> {
            let s = v.as_str().ok_or_else(|| err("invalid_arguments"))?;
            if s.is_empty()
                || (s != "0" && s.starts_with('0'))
                || !s.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(err("invalid_arguments"));
            }
            s.parse().map_err(|_| err("invalid_arguments"))
        };
        if op == "scene_inspect" {
            let offset = if args.get("offset").is_some() {
                number(&args["offset"])?
            } else {
                0
            };
            let start = usize::try_from(offset).map_err(|_| err("invalid_arguments"))?;
            let scene = self.session.scene();
            if start > scene.objects.len() {
                return Err(err("invalid_arguments"));
            }
            let objects: Vec<_> = scene.objects.iter().skip(start).take(16).collect();
            return Ok(
                json!({"session_token":token,"scene_id":scene.id,"revision":scene.revision.to_string(),"objects":objects,"object_count":scene.objects.len(),"next_offset":if start+16<scene.objects.len(){Some((start+16).to_string())}else{None},"dirty":self.session.is_dirty(),"drafts_pending":self.drafts(),"recovery_pending":self.dialog.is_some()||self.close_pending,"preview_tick":self.preview.tick.to_string(),"preview_fingerprint":self.preview.scene.fingerprint(),"preview_stale":self.preview.scene!=*scene,"capabilities":["scene_inspect","scene_transform","scene_undo","scene_redo","scene_replay"]}),
            );
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(err("stale_session"));
        }
        if op != "scene_replay" {
            self.guard().map_err(|e| {
                (
                    e.clone(),
                    format!("{e}: apply/revert drafts or resolve dialog, then inspect again"),
                )
            })?;
        }
        let revision = number(&args["expected_revision"])?;
        if revision != self.session.scene().revision {
            return Err(err("stale_revision"));
        }
        if op == "scene_replay" {
            let ticks = number(&args["ticks"])?;
            if ticks > 10000 {
                return Err(err("work_limit"));
            }
            let mut preview = Preview::new(self.session.scene());
            for _ in 0..ticks {
                preview
                    .step()
                    .map_err(|e| ("runtime_error".into(), e.to_string()))?;
            }
            return Ok(
                json!({"session_token":token,"revision":revision.to_string(),"ticks":preview.tick.to_string(),"scene_id":preview.scene.id,"object_count":preview.scene.objects.len(),"fingerprint":preview.scene.fingerprint()}),
            );
        }
        let before = self.session.scene().clone();
        let result = match op {
            "scene_transform" => {
                let id = args["object_id"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                let transform: Transform = serde_json::from_value(args["transform"].clone())
                    .map_err(|_| err("invalid_arguments"))?;
                self.session.edit(revision, id, &transform)
            }
            "scene_undo" => self.session.undo(revision),
            "scene_redo" => self.session.redo(revision),
            _ => unreachable!(),
        };
        let revision = result.map_err(|e| {
            let code = match &e {
                ruster_scene::SceneError::NoUndo => "no_undo",
                ruster_scene::SceneError::NoRedo => "no_redo",
                ruster_scene::SceneError::RevisionOverflow => "revision_exhausted",
                ruster_scene::SceneError::ObjectNotFound(_) => "object_not_found",
                ruster_scene::SceneError::StaleRevision { .. } => "stale_revision",
                _ => "invalid_arguments",
            };
            (code.into(), e.to_string())
        })?;
        self.sync_draft();
        let changed = before != *self.session.scene();
        let diff: Vec<_> = before
            .objects
            .iter()
            .zip(&self.session.scene().objects)
            .filter(|(a, b)| a.transform != b.transform)
            .map(|(a, b)| json!({"object_id":a.id,"before":a.transform,"after":b.transform}))
            .collect();
        self.notice = format!(
            "Agent {op}: revision {revision}; {} transform(s) changed. Preview unchanged.",
            diff.len()
        );
        Ok(
            json!({"session_token":token,"revision":revision.to_string(),"changed":changed,"diff":diff,"dirty":self.session.is_dirty()}),
        )
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_approved {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.session.is_dirty() || self.drafts() {
                self.close_pending = true;
                self.running = false;
            } else {
                self.close_approved = true;
            }
        }
        if !ctx.egui_wants_keyboard_input() && self.dialog.is_none() && !self.close_pending {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
                self.save();
            }
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
            }) {
                self.history(true);
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
                self.history(false);
            }
        }
        if self.running && self.last_tick.elapsed() >= Duration::from_millis(250) {
            if let Err(e) = self.preview.step() {
                self.error = e.to_string();
                self.running = false;
            }
            self.last_tick = Instant::now();
        }
        if self.running {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        egui::Panel::top("scene_top").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Ruster Scene Editor");
                if ui.button("Open scene…").clicked() {
                    self.dialog = Some(true);
                }
                if ui.button("Save scene").clicked() {
                    self.save();
                }
                if ui.button("Save scene as…").clicked() && self.apply() {
                    self.dialog = Some(false);
                }
                if ui.button("Undo transform").clicked() {
                    self.history(false)
                }
                if ui.button("Redo transform").clicked() {
                    self.history(true)
                }
            });
            ui.label(format!(
                "{} · revision {} · {}",
                self.session.scene().id,
                self.session.scene().revision,
                if self.session.is_dirty() || self.drafts() {
                    "UNSAVED"
                } else {
                    "Saved"
                }
            ));
        });
        egui::Panel::bottom("scene_status").show(ui, |ui| {
            ui.label(&self.notice);
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
            }
        });
        egui::Panel::left("scene_hierarchy")
            .default_size(180.)
            .show(ui, |ui| {
                ui.heading("Objects");
                let ids: Vec<_> = self
                    .session
                    .scene()
                    .objects
                    .iter()
                    .map(|o| o.id.clone())
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("scene_object_scroll")
                    .show(ui, |ui| {
                        for id in ids {
                            if ui
                                .selectable_label(self.selected.as_ref() == Some(&id), &id)
                                .clicked()
                            {
                                if self.drafts() {
                                    self.error =
                                        "Apply or revert transform before changing selection."
                                            .into();
                                } else {
                                    self.selected = Some(id);
                                    self.sync_draft();
                                }
                            }
                        }
                    });
            });
        egui::Panel::right("scene_inspector").exact_size(290.).show(ui,|ui| {
            ui.heading("Transform");
            if let Some(t)=&mut self.draft {
                for (label,values) in [("Position",&mut t.translation),("Rotation (radians)",&mut t.rotation),("Scale",&mut t.scale)] {
                    ui.label(label);
                    ui.horizontal(|ui| { for (axis,value) in ["X","Y","Z"].into_iter().zip(values) {
                        ui.label(axis);ui.add(egui::TextEdit::singleline(value).desired_width(52.));
                    }});
                }
                ui.horizontal(|ui| {
                    if ui.button("Apply transform").clicked(){self.apply();}
                    if ui.button("Revert transform").clicked(){self.sync_draft();self.error.clear();}
                });
            }
            ui.separator();ui.label(format!("Preview tick {}",self.preview.tick));
            if self.preview.scene!=*self.session.scene(){ui.colored_label(egui::Color32::YELLOW,"Authored scene changed; Reset preview.");}
            ui.horizontal(|ui| {
                if ui.button(if self.running{"Pause preview"}else{"Run preview"}).clicked(){self.running = !self.running;}
                if ui.button("Step preview").clicked(){if let Err(e)=self.preview.step(){self.error=e.to_string();}}
            });
            if ui.button("Reset preview").clicked(){self.running=false;self.preview.reset(self.session.scene());}
            ui.label("Static arrangement: ticks advance lifecycle only; no sample gameplay is implied.");
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.show_preview, "Show preview snapshot");
                ui.label("Right-drag orbit · scroll zoom");
            });
            if let Some(id) = ruster_render::viewport(
                ui,
                if self.show_preview {
                    &self.preview.scene
                } else {
                    self.session.scene()
                },
                self.selected.as_deref(),
            ) {
                if !self.drafts() {
                    self.selected = Some(id);
                    self.sync_draft();
                }
            }
        });
        if let Some(open) = self.dialog {
            egui::Modal::new(egui::Id::unique("scene_path_dialog")).show(&ctx,|ui| {
                ui.heading(if open {"Open scene"}else{"Save scene as"});
                ui.label("Absolute scene path");ui.text_edit_singleline(&mut self.path_text);
                if open && (self.session.is_dirty()||self.drafts()) {
                    ui.colored_label(egui::Color32::YELLOW,"Opening discards current unsaved scene and drafts. Cancel to preserve them.");
                }
                ui.horizontal(|ui| {
                    if ui.button(if open {"Open and discard current"}else{"Save to new path"}).clicked() {
                        let path=PathBuf::from(self.path_text.trim());
                        if open {
                            match Session::open(path) {Ok(session)=>*self=Self::new(session),Err(e)=>self.error=e.to_string()}
                        } else {
                            match self.session.save(path) {
                                Ok(())=>{self.dialog=None;if self.close_pending {self.close_approved=true;}self.notice="Scene saved.".into();},
                                Err(e)=>self.error=e.to_string()
                            }
                        }
                    }
                    if ui.button("Cancel scene dialog").clicked() {self.dialog=None;}
                });
            });
        }
        if self.close_pending && self.dialog.is_none() {
            egui::Modal::new(egui::Id::unique("scene_close_dialog")).show(&ctx, |ui| {
                ui.label("Save, discard or cancel before closing.");
                if ui.button("Save and close scene").clicked() {
                    self.save();
                    if !self.session.is_dirty() && !self.drafts() {
                        self.close_approved = true;
                    }
                }
                if ui.button("Discard and close scene").clicked() {
                    self.close_approved = true;
                }
                if ui.button("Cancel closing scene").clicked() {
                    self.close_pending = false;
                }
            });
        }
        if self.close_approved {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}
struct Host {
    editor: SceneEditor,
    #[cfg(unix)]
    bridge: Option<crate::bridge::Bridge>,
}
impl eframe::App for Host {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.editor.ui(ui);
        #[cfg(unix)]
        if let Some(bridge) = &mut self.bridge {
            if bridge.dispatch_command(|nonce, op, args| self.editor.command(nonce, op, args)) {
                ui.ctx().request_repaint();
            }
        }
    }
}
pub fn run(path: PathBuf) -> Result<(), String> {
    let session = Session::open(path).map_err(|e| e.to_string())?;
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = wgpu::Backends::VULKAN;
    ruster_render::configure_setup(&mut setup);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280., 800.])
            .with_app_id("org.ruster.editor"),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "Ruster Editor",
        options,
        Box::new(move |cc| {
            crate::ui::configure(&cc.egui_ctx);
            if let Some(render) = cc.wgpu_render_state.as_ref() {
                ruster_render::install(render);
                eprintln!("Scene editor adapter: {:?}", render.adapter.get_info());
            }
            #[cfg(unix)]
            let bridge = if std::env::var("RUSTER_MCP_ENABLE").as_deref() == Ok("1") {
                Some(crate::bridge::Bridge::start(cc.egui_ctx.clone())?)
            } else {
                None
            };
            Ok(Box::new(Host {
                editor: SceneEditor::new(session),
                #[cfg(unix)]
                bridge,
            }))
        }),
    )
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_text_is_preserved_and_successful_apply_canonicalizes_drafts() {
        let mut e = SceneEditor::new(Session::new(ruster_scene::Scene::demo()).unwrap());
        let initial = e.session.scene().clone();
        e.draft.as_mut().unwrap().scale[0] = "-".into();
        assert!(e.drafts());
        assert!(!e.apply());
        assert_eq!(e.session.scene(), &initial);
        assert_eq!(e.draft.as_ref().unwrap().scale[0], "-");
        e.draft.as_mut().unwrap().scale[0] = " 1e0 ".into();
        assert!(e.apply());
        assert!(!e.drafts());
        assert!(e.error.is_empty());
        assert_eq!(e.session.scene(), &initial);
    }

    #[test]
    fn live_scene_commands_guard_drafts_conflicts_and_share_undo_without_changing_preview() {
        let mut editor = SceneEditor::new(Session::new(ruster_scene::Scene::demo()).unwrap());
        let before = editor.command("host", "scene_inspect", json!({}))["result"].clone();
        let id = before["objects"][0]["id"].as_str().unwrap();
        let mut transform = editor.session.scene().objects[0].transform.clone();
        transform.translation[0] += 1.;
        let args = json!({"expected_session_token":before["session_token"],"expected_revision":before["revision"],"object_id":id,"transform":transform});
        editor.draft.as_mut().unwrap().translation[1] = "-".into();
        assert_eq!(
            editor.command("host", "scene_transform", args.clone())["error"]["code"],
            "drafts_pending"
        );
        editor.sync_draft();
        let preview = editor.preview.scene.clone();
        assert_eq!(
            editor.command("host", "scene_transform", args.clone())["ok"],
            true
        );
        assert_eq!(
            editor.command("host", "scene_transform", args)["error"]["code"],
            "stale_revision"
        );
        assert_eq!(editor.preview.scene, preview);
        assert!(editor.session.is_dirty());
        editor.history(false);
        assert_eq!(editor.session.scene().objects, preview.objects);
        let old = editor.command("host", "scene_inspect", json!({}))["result"].clone();
        editor = SceneEditor::new(Session::new(ruster_scene::Scene::demo()).unwrap());
        assert_eq!(editor.command("host","scene_undo",json!({"expected_session_token":old["session_token"],"expected_revision":old["revision"]}))["error"]["code"],"stale_session");
    }
    #[test]
    fn paginated_inspection_and_isolated_replay_stay_bounded() {
        let mut scene = ruster_scene::Scene::demo();
        let template = scene.objects[0].clone();
        scene.objects = (0..256)
            .map(|i| {
                let mut o = template.clone();
                o.id = format!("object-{i}");
                o
            })
            .collect();
        let mut editor = SceneEditor::new(Session::new(scene).unwrap());
        let result = editor.command("host", "scene_inspect", json!({}));
        assert_eq!(result["result"]["objects"].as_array().unwrap().len(), 16);
        assert!(serde_json::to_vec(&result).unwrap().len() < 65536);
        let args = json!({"expected_session_token":result["result"]["session_token"],"expected_revision":"0","ticks":"10000"});
        let replay = editor.command("host", "scene_replay", args);
        assert_eq!(replay["ok"], true);
        assert_eq!(replay["result"]["ticks"], "10000");
        assert_eq!(editor.preview.tick, 0);
        assert!(serde_json::to_vec(&replay).unwrap().len() < 65536);
        editor.close_pending = true;
        assert_eq!(editor.command("host","scene_undo",json!({"expected_session_token":result["result"]["session_token"],"expected_revision":"0"}))["error"]["code"],"recovery_pending");
    }
}
