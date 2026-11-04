//! Generic scene authoring frontend; logistics rules stay in the legacy sample.
use eframe::{
    egui,
    egui_wgpu::{WgpuSetup, WgpuSetupCreateNew},
};
use ruster_assets::{AssetStore, Mesh};
use ruster_scene::{Object, Preview, Scene, Session, Transform};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc};
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
    manipulation: crate::manipulation::Manipulation,
    draft: Option<Fields>,
    preview: Preview,
    show_preview: bool,
    running: bool,
    last_tick: Instant,
    generation: u64,
    error: String,
    notice: String,
    path_text: String,
    name_text: String,
    new_scene_pending: bool,
    rename_pending: bool,
    dialog: Option<bool>,
    close_pending: bool,
    close_approved: bool,
    assets: Option<AssetStore>,
    resolved: BTreeMap<String, Arc<Mesh>>,
    preview_assets: BTreeMap<String, Arc<Mesh>>,
    asset_dialog: bool,
    asset_id: String,
    asset_source: String,
    render: Option<eframe::egui_wgpu::RenderState>,
    shader_revision: u64,
    shader_notice: String,
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
        let (assets, asset_error) = match session.path().and_then(|p| p.parent()) {
            Some(root) => match AssetStore::open(root) {
                Ok(s) => (Some(s), String::new()),
                Err(e) => (None, e.to_string()),
            },
            None => (None, String::new()),
        };
        let resolved: BTreeMap<String, Arc<Mesh>> = assets
            .as_ref()
            .map(|s| {
                s.meshes()
                    .into_iter()
                    .map(|(k, v)| (k, Arc::new(v)))
                    .collect()
            })
            .unwrap_or_default();
        let preview_assets = resolved.clone();
        let mut manipulation = crate::manipulation::Manipulation::default();
        manipulation.select_only(selected.clone());
        manipulation.reset_precision(session.scene());
        let name_text = session
            .scene()
            .objects
            .first()
            .map(|o| o.display_name().to_string())
            .unwrap_or_default();
        Self {
            session,
            selected,
            manipulation,
            draft,
            preview,
            show_preview: false,
            running: false,
            last_tick: Instant::now(),
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            error: asset_error,
            notice: "Ready. Right-drag to orbit; scroll to zoom.".into(),
            path_text: String::new(),
            name_text,
            new_scene_pending: false,
            rename_pending: false,
            dialog: None,
            close_pending: false,
            close_approved: false,
            assets,
            resolved,
            preview_assets,
            asset_dialog: false,
            asset_id: "pyramid".into(),
            asset_source: "meshes/pyramid.obj".into(),
            render: None,
            shader_revision: 0,
            shader_notice: "Built-in scene shader".into(),
        }
    }
    fn drafts(&self) -> bool {
        self.manipulation.precision_pending()
            || self
                .selected
                .as_ref()
                .and_then(|id| self.session.scene().objects.iter().find(|o| &o.id == id))
                .map(|o| Some(Fields::from(&o.transform)).as_ref() != self.draft.as_ref())
                .unwrap_or(false)
    }
    fn reconcile_selection(&mut self) {
        self.manipulation.reconcile(self.session.scene());
        if self.manipulation.ids.is_empty() && self.selected.is_some() {
            self.manipulation
                .select_only(self.session.scene().objects.first().map(|o| o.id.clone()));
        }
        self.selected = self.manipulation.selected();
    }
    fn select_object(&mut self, id: Option<String>, additive: bool) {
        self.manipulation.select(id, additive);
        self.selected = self.manipulation.selected();
        self.sync_draft();
    }
    fn sync_draft(&mut self) {
        self.reconcile_selection();
        self.manipulation.reset_precision(self.session.scene());
        self.name_text = self
            .selected
            .as_ref()
            .and_then(|id| self.session.scene().objects.iter().find(|o| &o.id == id))
            .map(|o| o.display_name().to_string())
            .unwrap_or_default();
        self.draft = self
            .selected
            .as_ref()
            .and_then(|id| self.session.scene().objects.iter().find(|o| &o.id == id))
            .map(|o| Fields::from(&o.transform));
    }
    fn guard(&self) -> Result<(), String> {
        if self.drafts() || self.manipulation.active() {
            Err("drafts_pending".into())
        } else if self.dialog.is_some()
            || self.asset_dialog
            || self.new_scene_pending
            || self.rename_pending
            || self.close_pending
            || self.close_approved
        {
            Err("recovery_pending".into())
        } else {
            Ok(())
        }
    }
    fn apply(&mut self) -> bool {
        if self.manipulation.active() {
            self.error = "Finish or cancel the transform gesture first.".into();
            return false;
        }
        if self.manipulation.precision_pending() {
            match self.manipulation.apply_precision(&mut self.session) {
                Ok(()) => {
                    self.sync_draft();
                    self.error.clear();
                    return true;
                }
                Err(error) => {
                    self.error = error;
                    return false;
                }
            }
        }
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
            match self.save_scene(path) {
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
    fn object_action(&mut self, action: &str) {
        if let Err(error) = self.guard() {
            self.error = error;
            return;
        }
        let revision = self.session.scene().revision;
        let selected = self.selected.clone();
        let new_id = if action == "create" {
            match crate::manipulation::Manipulation::fresh_ids(&self.session, 1) {
                Ok(mut ids) => ids.remove(0),
                Err(error) => {
                    self.error = error;
                    return;
                }
            }
        } else {
            String::new()
        };
        let result = match action {
            "create" => self
                .session
                .create(revision, Object::cube(new_id.clone(), Some("Cube".into()))),
            "duplicate" => {
                let new_ids = match crate::manipulation::Manipulation::fresh_ids(
                    &self.session,
                    self.manipulation.ids.len(),
                ) {
                    Ok(ids) => ids,
                    Err(error) => {
                        self.error = error;
                        return;
                    }
                };
                let pairs: Vec<_> = self
                    .manipulation
                    .ids
                    .iter()
                    .cloned()
                    .zip(new_ids.iter().cloned())
                    .collect();
                let result = self.session.duplicate_many(revision, &pairs);
                if result.is_ok() {
                    self.manipulation.ids = new_ids;
                    self.selected = self.manipulation.selected();
                }
                result
            }
            "delete" => self.session.delete_many(revision, &self.manipulation.ids),
            "rename" => selected
                .as_deref()
                .ok_or_else(|| ruster_scene::SceneError::Invalid("Select an object first.".into()))
                .and_then(|id| {
                    self.session
                        .rename(revision, id, Some(self.name_text.clone()))
                }),
            _ => unreachable!(),
        };
        match result {
            Ok(_) => {
                if action == "create" {
                    self.manipulation.select_only(Some(new_id));
                    self.selected = self.manipulation.selected();
                }
                self.sync_draft();
                self.error.clear();
                self.notice = format!("Authored {action}; preview unchanged until Reset.");
            }
            Err(error) => self.error = error.to_string(),
        }
    }
    fn replace_with_empty(&mut self) {
        let render = self.render.clone();
        if let Some(render) = render.as_ref().filter(|_| self.shader_revision > 0) {
            if let Err(error) =
                ruster_render::refresh_shader(render, ruster_render::default_shader())
            {
                self.error = error;
                return;
            }
        }
        let mut next =
            Self::new(Session::new(Scene::empty("untitled")).expect("empty scene is valid"));
        next.manipulation.clipboard_from(&mut self.manipulation);
        *self = next;
        self.render = render;
    }
    fn clipboard_action(&mut self, paste: bool) {
        if let Err(error) = self.guard() {
            self.error = error;
            return;
        }
        let root = self.assets.as_ref().map(|s| s.root().to_path_buf());
        let result = if paste {
            self.manipulation.paste(&mut self.session, root.as_deref())
        } else {
            self.manipulation.copy(&self.session, root.as_deref())
        };
        match result {
            Ok(()) => {
                self.selected = self.manipulation.selected();
                self.sync_draft();
                self.error.clear();
                self.notice = if paste {
                    "Pasted objects; one Undo restores the scene."
                } else {
                    "Copied objects inside this editor window."
                }
                .into();
            }
            Err(error) => self.error = error,
        }
    }
    fn manipulation_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(format!(
            "{} selected · last selected is active",
            self.manipulation.ids.len()
        ));
        ui.horizontal(|ui| {
            if ui.button("Copy objects").clicked() {
                self.clipboard_action(false);
            }
            if ui.button("Paste objects").clicked() {
                self.clipboard_action(true);
            }
        });
        ui.small(
            "Object clipboard stays in this window. Text fields keep normal clipboard behavior.",
        );
        ui.add_enabled_ui(!self.manipulation.active() && !self.manipulation.precision_pending(), |ui| {
            use crate::manipulation::{Tool,Pivot};
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.manipulation.tool, Tool::Translate, "Move");
                ui.selectable_value(&mut self.manipulation.tool, Tool::Rotate, "Rotate");
                ui.selectable_value(&mut self.manipulation.tool, Tool::Scale, "Scale");
            });
            ui.checkbox(&mut self.manipulation.local,"Local axes (active object)");
            egui::ComboBox::from_label("Pivot").selected_text(match self.manipulation.pivot {Pivot::Individual=>"Individual origins",Pivot::Center=>"Selection center (mean origins)",Pivot::Active=>"Active object"}).show_ui(ui,|ui| {
                ui.selectable_value(&mut self.manipulation.pivot,Pivot::Individual,"Individual origins");
                ui.selectable_value(&mut self.manipulation.pivot,Pivot::Center,"Selection center (mean origins)");
                ui.selectable_value(&mut self.manipulation.pivot,Pivot::Active,"Active object");
            });
            if self.manipulation.tool==Tool::Scale {
                ui.small(if self.manipulation.local && self.manipulation.pivot==Pivot::Individual {"Local axis scaling at each object's origin."} else {"Uniform group scaling. For nonuniform scaling, choose Local + Individual origins. World nonuniform/shear is unsupported."});
            }
            ui.checkbox(&mut self.manipulation.snap,"Snapping");
            ui.horizontal(|ui| {
                match self.manipulation.tool {
                    Tool::Translate=>{ui.label("Distance (scene units)");ui.add(egui::DragValue::new(&mut self.manipulation.snap_distance).range(0.001..=1000.0).speed(0.1));},
                    Tool::Rotate=>{ui.label("Angle (degrees)");ui.add(egui::DragValue::new(&mut self.manipulation.snap_degrees).range(0.1..=180.0).speed(1.0));},
                    Tool::Scale=>{ui.label("Scale factor increment");ui.add(egui::DragValue::new(&mut self.manipulation.snap_scale).range(0.001..=10.0).speed(0.05));},
                }
            });
            ui.label("World translation offset for selected objects");
            ui.horizontal(|ui| {
                for (axis, text) in ["X", "Y", "Z"]
                    .into_iter()
                    .zip(&mut self.manipulation.delta)
                {
                    ui.label(axis);
                    ui.add(egui::TextEdit::singleline(text).desired_width(50.0));
                }
            });
            if ui
                .add_enabled(
                    !self.manipulation.ids.is_empty(),
                    egui::Button::new("Apply translation offset"),
                )
                .clicked()
            {
                let result = self
                    .guard()
                    .and_then(|()| self.manipulation.apply_delta(&mut self.session));
                match result {
                    Ok(()) => {
                        self.sync_draft();
                        self.error.clear();
                        self.notice =
                            "Translation offset applied; one Undo restores all selected objects."
                                .into();
                    }
                    Err(error) => self.error = error,
                }
            }
        });
        if self.manipulation.active() {
            ui.label("Transform preview — release to commit, Escape to cancel.");
        }
    }
    fn asset_snapshot(&mut self) {
        let meshes = self
            .assets
            .as_ref()
            .map(|store| store.meshes())
            .unwrap_or_default();
        self.resolved = meshes
            .into_iter()
            .map(|(id, mesh)| {
                let shared = self
                    .resolved
                    .get(&id)
                    .filter(|old| old.as_ref() == &mesh)
                    .cloned()
                    .unwrap_or_else(|| Arc::new(mesh));
                (id, shared)
            })
            .collect();
    }
    fn save_scene(&mut self, path: PathBuf) -> Result<(), String> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let old_root = self.assets.as_ref().map(|s| s.root().to_path_buf());
        if self.session.has_asset_references() && old_root.as_ref() != Some(&parent) {
            return Err("Save As across asset roots is unavailable while scene or undo/redo history references meshes; save beside the existing project.".into());
        }
        let destination_store = if old_root.as_ref() != Some(&parent) {
            Some(AssetStore::open(&parent).map_err(|e| e.to_string())?)
        } else {
            None
        };
        self.session.save(path).map_err(|e| e.to_string())?;
        if old_root.as_ref() != Some(&parent) {
            self.assets = destination_store;
            self.asset_snapshot();
            if let Some(render) = &self.render {
                ruster_render::refresh_shader(render, ruster_render::default_shader())?;
            }
            self.shader_revision = 0;
            self.shader_notice = "Built-in scene shader".into();
        }
        Ok(())
    }
    fn assign_mesh(&mut self, mesh: Option<String>) -> Result<u64, String> {
        if self.drafts() || self.manipulation.active() {
            return Err(
                "Apply or revert transform fields and finish the gesture before assigning a mesh."
                    .into(),
            );
        }
        if let Some(id) = &mesh {
            if !self.resolved.contains_key(id) {
                return Err(format!(
                    "Mesh {id} is unavailable; import or repair its source first."
                ));
            }
        }
        let id = self.selected.as_deref().ok_or("Select an object first")?;
        let revision = self
            .session
            .set_mesh(self.session.scene().revision, id, mesh)
            .map_err(|e| e.to_string())?;
        self.notice =
            format!("Mesh assigned at revision {revision}; preview unchanged until Reset.");
        Ok(revision)
    }
    fn refresh_shader(&mut self) -> Result<(), String> {
        let root = self
            .assets
            .as_ref()
            .ok_or("Open a saved project first")?
            .root();
        let path = root.join("scene.wgsl");
        if std::fs::symlink_metadata(&path)
            .map_err(|e| format!("scene.wgsl: {e}"))?
            .file_type()
            .is_symlink()
        {
            return Err("scene.wgsl must not be a symlink".into());
        }
        if !std::fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .is_file()
        {
            return Err("scene.wgsl must be a regular file".into());
        }
        let bytes = ruster_assets::read_regular_file(&path, 65536).map_err(|e| e.to_string())?;
        let source = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let render = self.render.as_ref().ok_or("GPU renderer unavailable")?;
        let next_revision = self
            .shader_revision
            .checked_add(1)
            .ok_or("shader revision exhausted")?;
        ruster_render::refresh_shader(render, &source)?;
        self.shader_revision = next_revision;
        self.shader_notice = format!("Project shader revision {} active", self.shader_revision);
        Ok(())
    }
    fn content_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value, (String, String)> {
        let err = |code: &str, message: String| (code.to_string(), message);
        let allowed: &[&str] = match op {
            "asset_inspect" => &["offset"],
            "asset_import" | "asset_rename" => &[
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
                "asset_id",
                "source",
            ],
            "asset_refresh" => &[
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
                "asset_id",
            ],
            "scene_mesh" => &[
                "expected_session_token",
                "expected_revision",
                "expected_asset_revision",
                "object_id",
                "asset_id",
            ],
            "shader_refresh" => &[
                "expected_session_token",
                "expected_revision",
                "expected_shader_revision",
            ],
            _ => return Err(err("unsupported_operation", op.into())),
        };
        let object = args
            .as_object()
            .ok_or_else(|| err("invalid_arguments", "Expected object arguments".into()))?;
        if object.keys().any(|k| !allowed.contains(&k.as_str()))
            || (op != "asset_inspect" && allowed.iter().any(|key| !object.contains_key(*key)))
        {
            return Err(err("invalid_arguments", "Unknown argument".into()));
        }
        let number = |key: &str| -> Result<u64, (String, String)> {
            let value = args[key].as_str().ok_or_else(|| {
                err(
                    "invalid_arguments",
                    format!("{key} must be canonical decimal string"),
                )
            })?;
            if value.is_empty()
                || (value != "0" && value.starts_with('0'))
                || !value.bytes().all(|b| b.is_ascii_digit())
            {
                return Err(err("invalid_arguments", format!("Invalid {key}")));
            }
            value
                .parse()
                .map_err(|_| err("invalid_arguments", format!("Invalid {key}")))
        };
        if op == "asset_inspect" {
            let store = self
                .assets
                .as_ref()
                .ok_or_else(|| err("assets_unavailable", self.error.clone()))?;
            let all = store.inspect();
            let offset = if args.get("offset").is_some() {
                number("offset")?
            } else {
                0
            };
            let offset = usize::try_from(offset)
                .map_err(|_| err("invalid_arguments", "offset exceeds range".into()))?;
            if offset > all.len() {
                return Err(err("invalid_arguments", "offset exceeds count".into()));
            }
            let records: Vec<_> = all.iter().skip(offset).take(16).collect();
            return Ok(
                json!({"session_token":token,"revision":self.session.scene().revision.to_string(),"asset_revision":store.revision().to_string(),"assets":records,"asset_count":all.len(),"next_offset":if offset+16<all.len(){Some((offset+16).to_string())}else{None},"shader_revision":self.shader_revision.to_string(),"shader_status":self.shader_notice,"shader_source":"scene.wgsl"}),
            );
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(err(
                "stale_session",
                "Inspect current session before retrying".into(),
            ));
        }
        self.guard().map_err(|code| {
            err(
                &code,
                "Apply/revert drafts or resolve dialogs before content operations".into(),
            )
        })?;
        if number("expected_revision")? != self.session.scene().revision {
            return Err(err("stale_revision", "Inspect authored scene again".into()));
        }
        if op == "shader_refresh" {
            if number("expected_shader_revision")? != self.shader_revision {
                return Err(err(
                    "stale_shader_revision",
                    "Inspect shader status again".into(),
                ));
            }
            self.refresh_shader().map_err(|e| {
                self.shader_notice = format!(
                    "Refresh rejected; last good pipeline retained: {}",
                    e.chars().take(512).collect::<String>()
                );
                err("shader_error", self.shader_notice.clone())
            })?;
            return Ok(
                json!({"shader_revision":self.shader_revision.to_string(),"status":self.shader_notice}),
            );
        }
        let expected_asset = number("expected_asset_revision")?;
        let store = self.assets.as_mut().ok_or_else(|| {
            err(
                "assets_unavailable",
                "Open a project with a valid catalog".into(),
            )
        })?;
        if expected_asset != store.revision() {
            return Err(err(
                "stale_asset_revision",
                "Inspect asset catalog again".into(),
            ));
        }
        if op == "scene_mesh" {
            let id = args["object_id"]
                .as_str()
                .ok_or_else(|| err("invalid_arguments", "object_id required".into()))?;
            let mesh = if args["asset_id"].is_null() {
                None
            } else {
                Some(
                    args["asset_id"]
                        .as_str()
                        .ok_or_else(|| {
                            err(
                                "invalid_arguments",
                                "asset_id must be string or null".into(),
                            )
                        })?
                        .to_owned(),
                )
            };
            if let Some(id) = &mesh {
                if !self.resolved.contains_key(id) {
                    return Err(err(
                        "asset_unavailable",
                        format!("{id} has no valid imported mesh"),
                    ));
                }
            }
            let previous = self
                .session
                .scene()
                .objects
                .iter()
                .find(|o| o.id == id)
                .ok_or_else(|| err("object_not_found", id.into()))?
                .mesh
                .clone();
            let revision = self
                .session
                .set_mesh(self.session.scene().revision, id, mesh.clone())
                .map_err(|e| err("invalid_arguments", e.to_string()))?;
            self.notice =
                format!("Agent assigned mesh to {id}, revision{revision}; preview unchanged.");
            return Ok(
                json!({"revision":revision.to_string(),"asset_revision":expected_asset.to_string(),"object_id":id,"before":previous,"after":mesh,"dirty":self.session.is_dirty()}),
            );
        }
        let id = args["asset_id"]
            .as_str()
            .ok_or_else(|| err("invalid_arguments", "asset_id required".into()))?;
        let result = match op {
            "asset_import" => store.import_at(
                expected_asset,
                id,
                args["source"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments", "source required".into()))?,
            ),
            "asset_refresh" => store.refresh_at(expected_asset, id),
            "asset_rename" => store.rename_at(
                expected_asset,
                id,
                args["source"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments", "source required".into()))?,
            ),
            _ => unreachable!(),
        };
        let revision = store.revision();
        self.asset_snapshot();
        match result {
            Ok(info) => {
                self.notice =
                    format!("Agent {op}: catalog revision{revision}; scene/preview unchanged.");
                Ok(
                    json!({"asset":info,"asset_revision":revision.to_string(),"scene_revision":self.session.scene().revision.to_string()}),
                )
            }
            Err(e) => Err(err("asset_error", e.to_string())),
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
        if matches!(
            op,
            "asset_inspect"
                | "asset_import"
                | "asset_refresh"
                | "asset_rename"
                | "scene_mesh"
                | "shader_refresh"
        ) {
            return self.content_command(token, op, args);
        }
        let err = |s: &str| (s.to_string(), s.to_string());
        let keys: &[&str] = match op {
            "scene_inspect" => &["offset"],
            "scene_transform" => &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                "transform",
            ],
            "scene_create" | "scene_rename" => &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                "name",
            ],
            "scene_duplicate" => &[
                "expected_session_token",
                "expected_revision",
                "object_id",
                "new_object_id",
                "name",
            ],
            "scene_delete" => &["expected_session_token", "expected_revision", "object_id"],
            "scene_transform_many" => {
                &["expected_session_token", "expected_revision", "transforms"]
            }
            "scene_delete_many" => &["expected_session_token", "expected_revision", "object_ids"],
            "scene_duplicate_many" => &["expected_session_token", "expected_revision", "objects"],
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
                json!({"session_token":token,"scene_id":scene.id,"revision":scene.revision.to_string(),"objects":objects,"object_count":scene.objects.len(),"unresolved_assets":scene.objects.iter().filter_map(|o|o.mesh.as_ref().filter(|id|!self.resolved.contains_key(*id)).map(|id|json!({"object_id":o.id,"asset_id":id,"action":"Import registered source or repair the project catalog; placeholder cube shown"}))).take(16).collect::<Vec<_>>(),"next_offset":if start+16<scene.objects.len(){Some((start+16).to_string())}else{None},"dirty":self.session.is_dirty(),"drafts_pending":self.drafts()||self.manipulation.active(),"recovery_pending":self.dialog.is_some()||self.close_pending||self.asset_dialog||self.new_scene_pending||self.rename_pending,"preview_tick":self.preview.tick.to_string(),"preview_fingerprint":self.preview.scene.fingerprint(),"asset_revision":self.assets.as_ref().map(|s|s.revision().to_string()),"loaded_format_version":self.session.loaded_format_version(),"shader_revision":self.shader_revision.to_string(),"preview_stale":self.preview.scene!=*scene || self.preview_assets!=self.resolved,"capabilities":["scene_inspect","scene_transform","scene_create","scene_rename","scene_duplicate","scene_delete","scene_transform_many","scene_delete_many","scene_duplicate_many","scene_undo","scene_redo","scene_replay","asset_inspect","asset_import","asset_refresh","asset_rename","scene_mesh","shader_refresh"]}),
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
            "scene_create" => {
                let id = args["object_id"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                self.session
                    .create(revision, Object::cube(id, Some(name.into())))
            }
            "scene_rename" => {
                let id = args["object_id"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                self.session.rename(revision, id, Some(name.into()))
            }
            "scene_duplicate" => {
                let id = args["object_id"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                let new_id = args["new_object_id"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                self.session
                    .duplicate(revision, id, new_id, Some(name.into()))
            }
            "scene_delete" => {
                let id = args["object_id"]
                    .as_str()
                    .ok_or_else(|| err("invalid_arguments"))?;
                self.session.delete(revision, id)
            }
            "scene_transform_many" => {
                let values = args["transforms"]
                    .as_array()
                    .filter(|v| v.len() <= ruster_scene::MAX_OBJECTS)
                    .ok_or_else(|| err("invalid_arguments"))?;
                let edits: Vec<_> = values
                    .iter()
                    .map(|value| {
                        let entry = value.as_object().ok_or_else(|| err("invalid_arguments"))?;
                        if entry.len() != 2
                            || entry
                                .keys()
                                .any(|k| !["object_id", "transform"].contains(&k.as_str()))
                        {
                            return Err(err("invalid_arguments"));
                        }
                        let id = value["object_id"]
                            .as_str()
                            .ok_or_else(|| err("invalid_arguments"))?;
                        let transform: Transform =
                            serde_json::from_value(value["transform"].clone())
                                .map_err(|_| err("invalid_arguments"))?;
                        Ok((id.to_string(), transform))
                    })
                    .collect::<Result<_, (String, String)>>()?;
                self.session.edit_transforms(revision, &edits)
            }
            "scene_delete_many" => {
                let values = args["object_ids"]
                    .as_array()
                    .filter(|v| v.len() <= ruster_scene::MAX_OBJECTS)
                    .ok_or_else(|| err("invalid_arguments"))?;
                let ids: Vec<_> = values
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_string)
                            .ok_or_else(|| err("invalid_arguments"))
                    })
                    .collect::<Result<_, _>>()?;
                self.session.delete_many(revision, &ids)
            }
            "scene_duplicate_many" => {
                let values = args["objects"]
                    .as_array()
                    .filter(|v| v.len() <= ruster_scene::MAX_OBJECTS)
                    .ok_or_else(|| err("invalid_arguments"))?;
                let pairs: Vec<_> = values
                    .iter()
                    .map(|value| {
                        let entry = value.as_object().ok_or_else(|| err("invalid_arguments"))?;
                        if entry.len() != 2
                            || entry
                                .keys()
                                .any(|k| !["object_id", "new_object_id"].contains(&k.as_str()))
                        {
                            return Err(err("invalid_arguments"));
                        }
                        Ok((
                            value["object_id"]
                                .as_str()
                                .ok_or_else(|| err("invalid_arguments"))?
                                .to_string(),
                            value["new_object_id"]
                                .as_str()
                                .ok_or_else(|| err("invalid_arguments"))?
                                .to_string(),
                        ))
                    })
                    .collect::<Result<_, (String, String)>>()?;
                let result = self.session.duplicate_many(revision, &pairs);
                if result.is_ok() {
                    self.manipulation.ids = pairs.into_iter().map(|(_, id)| id).collect();
                    self.selected = self.manipulation.selected();
                }
                result
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
        if matches!(op, "scene_create" | "scene_duplicate") {
            self.selected = args[if op == "scene_create" {
                "object_id"
            } else {
                "new_object_id"
            }]
            .as_str()
            .map(str::to_string);
        }
        if matches!(op, "scene_create" | "scene_duplicate") {
            self.manipulation.select_only(self.selected.clone());
        }
        self.sync_draft();
        let changed = before != *self.session.scene();
        let mut ids: std::collections::BTreeSet<_> =
            before.objects.iter().map(|o| o.id.as_str()).collect();
        ids.extend(self.session.scene().objects.iter().map(|o| o.id.as_str()));
        let mut diff = Vec::with_capacity(16);
        let mut diff_count = 0;
        for id in ids {
            let old = before.objects.iter().find(|o| o.id == id);
            let new = self.session.scene().objects.iter().find(|o| o.id == id);
            if old != new {
                diff_count += 1;
                if diff.len() < 16 {
                    diff.push(json!({"object_id":id,
                        "before":old.map(|o| &o.transform),"after":new.map(|o| &o.transform),
                        "before_mesh":old.and_then(|o| o.mesh.as_ref()),"after_mesh":new.and_then(|o| o.mesh.as_ref()),
                        "before_object":old,"after_object":new}));
                }
            }
        }
        let diff_truncated = diff_count > diff.len();
        self.notice = format!(
            "Agent {op}: revision {revision}; {} authored object(s) changed. Preview unchanged.",
            diff_count
        );
        Ok(
            json!({"session_token":token,"revision":revision.to_string(),"changed":changed,"diff":diff,"diff_count":diff_count,"diff_truncated":diff_truncated,"dirty":self.session.is_dirty()}),
        )
    }
    fn content_ui(&mut self, ctx: &egui::Context) {
        if !self.asset_dialog {
            return;
        }
        egui::Modal::new(egui::Id::unique("asset_dialog")).show(ctx,|ui| {
            ui.heading("Project mesh assets");
            ui.label("Imports and source renames save the asset catalog immediately. Scene Undo reverses assignments only.");
            ui.label("Stable asset ID");ui.text_edit_singleline(&mut self.asset_id);
            ui.label("Project-relative OBJ source / rename destination");ui.text_edit_singleline(&mut self.asset_source);
            let mut operation=None;
            ui.horizontal(|ui| {
                if ui.button("Import mesh asset").clicked(){operation=Some("import");}
                if ui.button("Refresh mesh asset").clicked(){operation=Some("refresh");}
                if ui.button("Rename mesh source").clicked(){operation=Some("rename");}
            });
            if let Some(op)=operation {
                if self.drafts(){self.error="Apply/revert transform drafts before asset operations.".into();}
                else {
                    let id=self.asset_id.clone();let source=self.asset_source.clone();
                    let result=self.assets.as_mut().ok_or_else(||"Asset catalog unavailable; reopen a saved project".to_string()).and_then(|store| {
                        match op {"import"=>store.import(&id,&source),"refresh"=>store.refresh(&id),_=>store.rename(&id,&source)}.map_err(|e|e.to_string())
                    });
                    self.asset_snapshot();
                    match result {Ok(info)=>{self.notice=format!("Asset {} ready; preview unchanged until Reset.",info.id);self.error.clear();},Err(e)=>self.error=e}
                }
            }
            if let Some(store)=&self.assets {
                ui.label(format!("Catalog revision {}",store.revision()));
                egui::ScrollArea::vertical().max_height(180.).show(ui,|ui| {for info in store.inspect(){ui.label(format!("{} · {} · {:?}",info.id,info.source,info.status));if let Some(error)=info.diagnostic {ui.colored_label(egui::Color32::LIGHT_RED,error);}}});
            }
            if !self.error.is_empty(){ui.colored_label(egui::Color32::LIGHT_RED,&self.error);}
            if ui.button("Close assets").clicked(){self.asset_dialog=false;}
        });
    }
    fn route_shortcuts(&mut self, ctx: &egui::Context) {
        if self.manipulation.active() {
            return;
        }
        if !ctx.egui_wants_keyboard_input()
            && self.dialog.is_none()
            && !self.close_pending
            && !self.rename_pending
            && !self.new_scene_pending
            && !self.asset_dialog
        {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S)) {
                self.save();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::C)) {
                self.clipboard_action(false);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::V)) {
                self.clipboard_action(true);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::D)) {
                self.object_action("duplicate");
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Delete)) {
                self.object_action("delete");
            }
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, egui::Key::Z)
            }) {
                self.history(true);
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::Z)) {
                self.history(false);
            }
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_approved {
            self.manipulation.cancel();
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.new_scene_pending {
                self.error = "Resolve or cancel New scene before closing.".into();
                self.running = false;
            } else if self.rename_pending {
                self.error = "Apply or cancel the name edit before closing.".into();
                self.running = false;
            } else if self.session.is_dirty() || self.drafts() {
                self.close_pending = true;
                self.running = false;
            } else {
                self.close_approved = true;
            }
        }
        if self.manipulation.active()
            && (ctx.input(|i| !i.focused || i.key_pressed(egui::Key::Escape))
                || self.dialog.is_some()
                || self.asset_dialog
                || self.close_pending
                || self.close_approved
                || self.new_scene_pending
                || self.rename_pending)
        {
            self.manipulation.cancel();
            self.notice = "Transform cancelled; authored state unchanged.".into();
        }
        self.route_shortcuts(&ctx);
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
            ui.horizontal_wrapped(|ui| {
                ui.heading("Ruster Scene Editor");
                if ui.button("New scene").clicked() {
                    if self.session.is_dirty() || self.drafts() {
                        self.new_scene_pending = true;
                        self.running = false;
                    } else {
                        self.replace_with_empty();
                    }
                }
                if ui.button("Assets…").clicked() {
                    self.asset_dialog = true;
                }
                ui.menu_button("Assign mesh…", |ui| {
                    let ids: Vec<_> = self
                        .assets
                        .as_ref()
                        .map(|s| s.inspect().into_iter().map(|i| i.id).collect())
                        .unwrap_or_default();
                    if ui.button("Procedural cube").clicked() {
                        if let Err(e) = self.assign_mesh(None) {
                            self.error = e;
                        }
                        ui.close();
                    }
                    for id in ids {
                        if ui.button(&id).clicked() {
                            if let Err(e) = self.assign_mesh(Some(id)) {
                                self.error = e;
                            }
                            ui.close();
                        }
                    }
                });
                if ui.button("Open scene…").clicked() {
                    self.dialog = Some(true);
                }
                if ui.button("Save scene").clicked() {
                    self.save();
                }
                if ui.button("Save scene as…").clicked() && self.apply() {
                    self.dialog = Some(false);
                }
                if ui.button("Undo authored edit").clicked() {
                    self.history(false)
                }
                if ui.button("Redo authored edit").clicked() {
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
                if ui.button("Create cube").clicked() {
                    self.object_action("create");
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.selected.is_some(), egui::Button::new("Duplicate"))
                        .clicked()
                    {
                        self.object_action("duplicate");
                    }
                    if ui
                        .add_enabled(self.selected.is_some(), egui::Button::new("Delete"))
                        .clicked()
                    {
                        self.object_action("delete");
                    }
                });
                let ids: Vec<_> = self
                    .session
                    .scene()
                    .objects
                    .iter()
                    .map(|o| (o.id.clone(), o.display_name().to_string()))
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("scene_object_scroll")
                    .show(ui, |ui| {
                        for (id, name) in ids {
                            if ui
                                .selectable_label(self.manipulation.ids.contains(&id), name)
                                .clicked()
                            {
                                if self.guard().is_err() {
                                    self.error =
                                        "Apply or revert transform before changing selection."
                                            .into();
                                } else {
                                    let additive = ui.input(|i| {
                                        i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift
                                    });
                                    self.select_object(Some(id), additive);
                                }
                            }
                        }
                    });
            });
        egui::Panel::right("scene_inspector").default_size(320.).resizable(true).show(ui,|ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Object");
            if let Some(id) = &self.selected { ui.small(format!("Stable ID: {id}")); }
            if let Some(object) = self.selected.as_ref().and_then(|id| self.session.scene().objects.iter().find(|o| &o.id == id)) { ui.label(object.display_name()); }
            if ui.add_enabled(self.manipulation.ids.len() == 1, egui::Button::new("Rename object")).clicked() {
                match self.guard() { Ok(()) => self.rename_pending = true, Err(error) => self.error = error }
            }
            self.manipulation_ui(ui);
            ui.heading("Transform");
            if let Some(object)=self.selected.as_ref().and_then(|id|self.session.scene().objects.iter().find(|o|&o.id==id)) {ui.label(format!("Mesh: {}",object.mesh.as_deref().unwrap_or("Procedural cube")));}
            ui.label(format!("Loaded scene format v{}; Save writes v{}.",self.session.loaded_format_version(),ruster_scene::FORMAT_VERSION));
            ui.add_enabled_ui(!self.manipulation.ids.is_empty() && !self.manipulation.active(), |ui| {
                match self.manipulation.precision_ui(ui,&mut self.session) {
                    Ok(true)=>{self.sync_draft();self.error.clear();self.notice="Edited transform components applied to the selection; one Undo restores them.".into();},
                    Ok(false)=>{},Err(error)=>self.error=error,
                }
            });
            ui.separator();ui.label(format!("Preview tick {}",self.preview.tick));
            if self.preview.scene!=*self.session.scene(){ui.colored_label(egui::Color32::YELLOW,"Scene or imported content changed; Reset preview.");}
            ui.horizontal(|ui| {
                if ui.button(if self.running{"Pause preview"}else{"Run preview"}).clicked(){self.running = !self.running;}
                if ui.button("Step preview").clicked(){if let Err(e)=self.preview.step(){self.error=e.to_string();}}
            });
            if ui.button("Reset preview").clicked(){self.running=false;self.preview.reset(self.session.scene());self.preview_assets=self.resolved.clone();}
            ui.label("Static arrangement: ticks advance lifecycle only; no sample gameplay is implied.");
            });
        });
        if !self.asset_dialog && self.dialog.is_none() && !self.close_pending {
            egui::Panel::bottom("content_status").show(ui,|ui| {
                ui.horizontal(|ui| {
                    if ui.button("Create shader source").clicked(){
                        use std::io::Write;
                        let result=self.assets.as_ref().ok_or_else(||"Open a saved project first".to_string()).and_then(|store| {
                            std::fs::OpenOptions::new().write(true).create_new(true).open(store.root().join("scene.wgsl")).and_then(|mut f|f.write_all(ruster_render::default_shader().as_bytes())).map_err(|e|e.to_string())
                        });
                        match result {Ok(())=>self.notice="Created scene.wgsl; edit it, then Refresh scene shader.".into(),Err(e)=>self.error=e}
                    }
                    if ui.button("Refresh scene shader").clicked(){
                        if self.drafts() || self.manipulation.active(){self.error="Apply/revert transform drafts and finish the gesture before shader refresh.".into();}
                        else if let Err(e)=self.refresh_shader(){self.shader_notice=format!("Shader rejected; last good pipeline retained: {}",e.chars().take(256).collect::<String>());self.error=self.shader_notice.clone();}
                    }
                    ui.label(&self.shader_notice);
                });
                for object in &self.session.scene().objects {if let Some(id)=&object.mesh {if !self.resolved.contains_key(id){ui.colored_label(egui::Color32::YELLOW,format!("Missing mesh {id} for {}; placeholder cube shown. Import or repair its source.",object.id));}}}
                if let Some(store)=&self.assets {for info in store.inspect().into_iter().filter(|i|i.diagnostic.is_some()).take(3){ui.colored_label(egui::Color32::YELLOW,format!("{}: {}",info.id,info.diagnostic.unwrap_or_default()));}}
            });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled(
                    !self.manipulation.active(),
                    egui::Checkbox::new(&mut self.show_preview, "Show preview snapshot"),
                );
                ui.label("Right-drag orbit · scroll zoom");
            });
            let transient = if !self.show_preview {
                self.manipulation.transient_scene(self.session.scene())
            } else {
                None
            };
            let selected: Vec<_> = self.manipulation.ids.iter().map(String::as_str).collect();
            let viewport = ruster_render::viewport_interactive(
                ui,
                if self.show_preview {
                    &self.preview.scene
                } else {
                    transient.as_ref().unwrap_or(self.session.scene())
                },
                &selected,
                if self.show_preview {
                    &self.preview_assets
                } else {
                    &self.resolved
                },
                !self.manipulation.active(),
            );
            let may_draw = !self.show_preview
                && !self.drafts()
                && self.dialog.is_none()
                && !self.asset_dialog
                && !self.close_pending
                && !self.close_approved
                && !self.new_scene_pending
                && !self.rename_pending;
            let overlay_owns = if may_draw {
                let before = self.session.scene().revision;
                let result = self.manipulation.draw(ui, &viewport, &mut self.session);
                if before != self.session.scene().revision {
                    self.sync_draft();
                    self.notice = "Transform committed; Undo restores the whole selection.".into();
                }
                match result {
                    Ok(owns) => owns,
                    Err(error) => {
                        self.error = error;
                        true
                    }
                }
            } else {
                false
            };
            if viewport.response.clicked() && !overlay_owns && self.guard().is_ok() {
                let id = viewport
                    .picked
                    .filter(|id| self.session.scene().objects.iter().any(|o| &o.id == id));
                let additive =
                    ui.input(|i| i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift);
                self.select_object(id, additive);
            }
        });
        if self.rename_pending {
            egui::Modal::new(egui::Id::unique("rename_object_dialog")).show(&ctx, |ui| {
                ui.heading("Rename object");
                ui.label("Display name; the stable object ID stays unchanged.");
                ui.text_edit_singleline(&mut self.name_text);
                if ui.button("Apply name").clicked() {
                    self.rename_pending = false;
                    self.object_action("rename");
                    if !self.error.is_empty() {
                        self.rename_pending = true;
                    }
                }
                if ui.button("Cancel rename").clicked() {
                    self.rename_pending = false;
                    self.sync_draft();
                }
                if !self.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
                }
            });
        }
        if self.new_scene_pending && self.dialog.is_none() {
            egui::Modal::new(egui::Id::unique("new_scene_dialog")).show(&ctx, |ui| {
                ui.heading("New scene — unsaved work");
                ui.label("Save your current scene, discard it, or cancel.");
                if ui.button("Save before new scene").clicked() {
                    self.save();
                    if !self.session.is_dirty() && !self.drafts() {
                        self.replace_with_empty();
                    }
                }
                if ui.button("Discard and create new scene").clicked() {
                    self.replace_with_empty();
                }
                if ui.button("Cancel new scene").clicked() {
                    self.new_scene_pending = false;
                }
            });
        }
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
                            match Session::open(path) {
                                Ok(session)=> {
                                    let render=self.render.clone();
                                    let reset=render.as_ref().map(|r|ruster_render::refresh_shader(r,ruster_render::default_shader())).unwrap_or(Ok(()));
                                    match reset {Ok(())=>{let mut next=Self::new(session);next.manipulation.clipboard_from(&mut self.manipulation);*self=next;self.render=render;},Err(error)=>self.error=error}
                                },
                                Err(error)=>self.error=error.to_string()
                            }
                        } else {
                            match self.save_scene(path) {
                                Ok(())=>{self.dialog=None;if self.close_pending {self.close_approved=true;}self.notice="Scene saved.".into();if self.new_scene_pending {self.replace_with_empty();}},
                                Err(e)=>self.error=e.to_string()
                            }
                        }
                    }
                    if ui.button("Cancel scene dialog").clicked() {self.dialog=None;}
                });
            });
        }
        self.content_ui(&ctx);
        if self.close_pending && self.dialog.is_none() && !self.asset_dialog && !self.rename_pending
        {
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
                editor: {
                    let mut editor = SceneEditor::new(session);
                    editor.render = cc.wgpu_render_state.clone();
                    editor
                },
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
    // Unique UI boundary risk: an unfocused name field in a modal must not let
    // Ctrl+S apply/sync another draft and destroy the pending text.
    #[test]
    fn modal_shortcuts_preserve_pending_name_and_recovery_intent() {
        let ctx = egui::Context::default();
        for modal in 0..4 {
            let mut e = SceneEditor::new(Session::new(Scene::demo()).unwrap());
            e.name_text = "Unapplied name".into();
            match modal {
                0 => e.rename_pending = true,
                1 => e.new_scene_pending = true,
                2 => e.asset_dialog = true,
                _ => e.dialog = Some(true),
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: vec![
                        egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
                        egui::Event::Key {
                            key: egui::Key::S,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::CTRL,
                        },
                    ],
                    ..Default::default()
                },
                |_| e.route_shortcuts(&ctx),
            );
            output.textures_delta.clear();
            assert_eq!(e.name_text, "Unapplied name");
            assert_eq!(e.session.scene().revision, 0);
            assert!(e.session.path().is_none());
            assert_eq!(e.dialog, if modal == 3 { Some(true) } else { None });
        }
    }

    // Unique integration risk: object membership changes need ID-based diffs,
    // shared human/agent history and selection repair rather than zipped vectors.
    #[test]
    fn object_commands_share_history_guards_and_preserve_preview() {
        let mut e = SceneEditor::new(Session::new(Scene::empty("created")).unwrap());
        let initial = e.command("test", "scene_inspect", json!({}))["result"].clone();
        let token = initial["session_token"].clone();
        let args =
            |revision: &str| json!({"expected_session_token":token,"expected_revision":revision});
        let mut create = args("0");
        create["object_id"] = "stable".into();
        create["name"] = "My object".into();
        e.rename_pending = true;
        assert_eq!(
            e.command("test", "scene_create", create.clone())["error"]["code"],
            "recovery_pending"
        );
        e.rename_pending = false;
        let result = e.command("test", "scene_create", create.clone());
        assert_eq!(result["ok"], true);
        assert!(result["result"]["diff"][0]["before"].is_null());
        assert_eq!(result["result"]["diff"][0]["after_object"]["id"], "stable");
        assert_eq!(e.selected.as_deref(), Some("stable"));
        assert!(e.preview.scene.objects.is_empty());
        assert_eq!(
            e.command("test", "scene_create", create)["error"]["code"],
            "stale_revision"
        );
        let mut rename = args("1");
        rename["object_id"] = "stable".into();
        rename["name"] = "Renamed".into();
        e.draft.as_mut().unwrap().translation[0] = "-".into();
        assert_eq!(
            e.command("test", "scene_rename", rename.clone())["error"]["code"],
            "drafts_pending"
        );
        e.sync_draft();
        assert_eq!(e.command("test", "scene_rename", rename)["ok"], true);
        assert_eq!(e.session.scene().objects[0].id, "stable");
        assert_eq!(e.session.scene().objects[0].display_name(), "Renamed");
        let mut duplicate = args("2");
        duplicate["object_id"] = "stable".into();
        duplicate["new_object_id"] = "copy".into();
        duplicate["name"] = "Copy".into();
        assert_eq!(e.command("test", "scene_duplicate", duplicate)["ok"], true);
        assert_eq!(e.selected.as_deref(), Some("copy"));
        let mut delete = args("3");
        delete["object_id"] = "copy".into();
        let result = e.command("test", "scene_delete", delete);
        assert!(result["result"]["diff"][0]["after"].is_null());
        assert_eq!(e.selected.as_deref(), Some("stable"));
        e.history(false);
        assert_eq!(e.session.scene().objects.len(), 2);
        assert!(e.preview.scene.objects.is_empty());
        e.history(true);
        assert_eq!(e.session.scene().objects.len(), 1);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("created.json");
        e.save_scene(path.clone()).unwrap();
        assert_eq!(Session::open(path).unwrap().scene(), e.session.scene());
        let old_token =
            e.command("test", "scene_inspect", json!({}))["result"]["session_token"].clone();
        e.replace_with_empty();
        assert!(e.session.scene().objects.is_empty());
        assert!(e.selected.is_none());
        assert!(e.preview.scene.objects.is_empty());
        assert_ne!(
            e.command("test", "scene_inspect", json!({}))["result"]["session_token"],
            old_token
        );
    }

    #[test]
    fn content_commands_preserve_preview_and_last_good_assets_across_refresh_failures() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let source = dir.path().join("triangle.obj");
        std::fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        Session::new(ruster_scene::Scene::demo())
            .unwrap()
            .save(&path)
            .unwrap();
        let mut e = SceneEditor::new(Session::open(&path).unwrap());
        let inspection = e.command("test", "scene_inspect", json!({}))["result"].clone();
        let expected = json!({"expected_session_token":inspection["session_token"],"expected_revision":"0","expected_asset_revision":"0","asset_id":"triangle","source":"triangle.obj"});
        assert_eq!(
            e.command("test", "asset_import", expected.clone())["ok"],
            true
        );
        assert_eq!(
            e.command("test", "asset_import", expected)["error"]["code"],
            "stale_asset_revision"
        );
        let mut args = json!({"expected_session_token":inspection["session_token"],"expected_revision":"0","expected_asset_revision":"1","object_id":"left","asset_id":"triangle"});
        assert_eq!(e.command("test", "scene_mesh", args.clone())["ok"], true);
        assert!(e.preview_assets.is_empty());
        assert_eq!(e.preview.scene.objects[0].mesh, None);
        e.preview.reset(e.session.scene());
        e.preview_assets = e.resolved.clone();
        let frozen = e.preview_assets["triangle"].clone();
        std::fs::write(&source, "v 0 0 0\nv 2 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        args["expected_revision"] = "1".into();
        args.as_object_mut().unwrap().remove("object_id");
        assert_eq!(e.command("test", "asset_refresh", args.clone())["ok"], true);
        assert!(!Arc::ptr_eq(&e.resolved["triangle"], &frozen));
        assert!(Arc::ptr_eq(&e.preview_assets["triangle"], &frozen));
        let last_good = e.resolved["triangle"].clone();
        std::fs::write(&source, "broken mesh").unwrap();
        args["expected_asset_revision"] = "2".into();
        assert_eq!(
            e.command("test", "asset_refresh", args)["error"]["code"],
            "asset_error"
        );
        assert!(Arc::ptr_eq(&e.resolved["triangle"], &last_good));
        assert_eq!(e.assets.as_ref().unwrap().revision(), 2);
        e.history(false);
        assert_eq!(e.session.scene().objects[0].mesh, None);
        assert!(e.assets.as_ref().unwrap().mesh("triangle").is_some());
        assert!(e.session.has_asset_references());
        let elsewhere = tempfile::tempdir().unwrap();
        let destination = elsewhere.path().join("moved.json");
        let original_path = e.session.path().unwrap().to_owned();
        assert!(e.save_scene(destination.clone()).is_err());
        assert!(!destination.exists());
        assert_eq!(e.session.path(), Some(original_path.as_path()));
        let missing_asset = json!({"expected_session_token":inspection["session_token"],"expected_revision":e.session.scene().revision.to_string(),"expected_asset_revision":"2","object_id":"left"});
        assert_eq!(
            e.command("test", "scene_mesh", missing_asset)["error"]["code"],
            "invalid_arguments"
        );
    }

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
        let transforms: Vec<_> = editor
            .session
            .scene()
            .objects
            .iter()
            .map(|object| {
                let mut transform = object.transform.clone();
                transform.translation[0] += 1.0;
                json!({"object_id":object.id,"transform":transform})
            })
            .collect();
        let result = editor.command("host", "scene_transform_many", json!({"expected_session_token":result["result"]["session_token"],"expected_revision":"0","transforms":transforms}));
        assert_eq!(result["ok"], true);
        assert_eq!(result["result"]["diff_count"], 256);
        assert_eq!(result["result"]["diff_truncated"], true);
        assert_eq!(result["result"]["diff"].as_array().unwrap().len(), 16);
        assert!(serde_json::to_vec(&result).unwrap().len() < 65536);
        assert_eq!(editor.session.undo_len(), 1);
        let revision = editor.session.scene().revision;
        let malformed = editor.command("host", "scene_transform_many", json!({"expected_session_token":result["result"]["session_token"],"expected_revision":revision.to_string(),"transforms":[{"object_id":"object-0","transform":editor.session.scene().objects[0].transform,"unknown":true}]}));
        assert_eq!(malformed["error"]["code"], "invalid_arguments");
        assert_eq!(editor.session.scene().revision, revision);
        editor.close_pending = true;
        assert_eq!(editor.command("host","scene_undo",json!({"expected_session_token":result["result"]["session_token"],"expected_revision":"0"}))["error"]["code"],"recovery_pending");
    }
}
