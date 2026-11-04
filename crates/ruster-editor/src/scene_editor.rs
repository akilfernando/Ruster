//! Generic scene authoring frontend; logistics rules stay in the legacy sample.
mod behavior_commands;
mod runtime_commands;
use crate::workspace::{Action, Side, Workspace};
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
#[derive(Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    translation: [String; 3],
    rotation: [String; 3],
    scale: [String; 3],
    #[serde(default)]
    rotation_unit: crate::manipulation::AngleUnit,
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct RecoveryDrafts {
    version: u32,
    revision: u64,
    selection: Vec<String>,
    fields: Option<Fields>,
    pending_name: Option<String>,
    manipulation: crate::manipulation::DraftSnapshot,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    behavior: Option<crate::behavior_ui::Drafts>,
}
impl From<&Transform> for Fields {
    fn from(t: &Transform) -> Self {
        Self::with_unit(t, crate::manipulation::AngleUnit::Degrees)
    }
}
impl Fields {
    fn with_unit(t: &Transform, unit: crate::manipulation::AngleUnit) -> Self {
        Self {
            translation: t.translation.map(|v| v.to_string()),
            rotation: t.rotation.map(|v| unit.display(v).to_string()),
            scale: t.scale.map(|v| v.to_string()),
            rotation_unit: unit,
        }
    }
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
            rotation: triple(&self.rotation)?.map(|value| self.rotation_unit.radians(value)),
            scale: triple(&self.scale)?,
        };
        t.validate().map_err(|e| e.to_string())?;
        Ok(t)
    }
}
// A temporary layout constraint never becomes the user's preferred panel size.
fn sidebar_budget(width: f32, hierarchy: Option<f32>, inspector: Option<f32>) -> (bool, f32, f32) {
    let h = hierarchy.unwrap_or(0.0);
    let i = inspector.unwrap_or(0.0);
    let hmin = if hierarchy.is_some() { 120.0 } else { 0.0 };
    let imin = if inspector.is_some() { 240.0 } else { 0.0 };
    let available = (width - 304.0).max(hmin + imin);
    if h + i <= available {
        return (false, h, i);
    }
    let surplus = (available - hmin - imin).max(0.0);
    let requested = (h - hmin).max(0.0) + (i - imin).max(0.0);
    if requested == 0.0 {
        return (true, hmin, imin);
    }
    (
        true,
        hmin + surplus * (h - hmin).max(0.0) / requested,
        imin + surplus * (i - imin).max(0.0) / requested,
    )
}
#[derive(Clone)]
struct AssetDrag {
    catalog_root: PathBuf,
    catalog_revision: u64,
    generation: u64,
    scene_revision: u64,
    asset_id: String,
}
#[derive(Clone, Copy)]
enum ContextAction {
    Create,
    Rename,
    Duplicate,
    Delete,
    Copy,
    Paste,
    Frame,
}
struct HierarchyRow {
    id: String,
    name: String,
    search: String,
    tooltip: String,
}
#[derive(Default)]
struct HierarchySearch {
    revision: Option<u64>,
    query: String,
    rows: Vec<HierarchyRow>,
    visible: Vec<usize>,
}
impl HierarchySearch {
    fn update(&mut self, scene: &Scene, query: &str) {
        let changed = self.revision != Some(scene.revision);
        if changed {
            self.rows = scene
                .objects
                .iter()
                .map(|object| HierarchyRow {
                    id: object.id.clone(),
                    name: object.display_name().to_string(),
                    search: format!("{}\n{}", object.id, object.display_name()).to_lowercase(),
                    tooltip:format!("Stable ID: {}\nRight-click for object actions. Drop a project mesh to assign it to this object only.",object.id),
                })
                .collect();
            self.revision = Some(scene.revision);
        }
        if changed || self.query != query {
            self.query = query.to_string();
            let query = query.to_lowercase();
            self.visible = self
                .rows
                .iter()
                .enumerate()
                .filter_map(|(i, row)| row.search.contains(&query).then_some(i))
                .collect();
        }
    }
}
struct ContentRow {
    info: ruster_assets::AssetInfo,
    label: String,
    search: String,
}
#[derive(Default)]
struct ContentSearch {
    key: Option<(PathBuf, u64)>,
    query: String,
    rows: Vec<ContentRow>,
    visible: Vec<usize>,
}
impl ContentSearch {
    fn update(&mut self, store: Option<&AssetStore>, query: &str) {
        let changed = match (&self.key, store) {
            (Some((root, revision)), Some(store)) => {
                root != store.root() || *revision != store.revision()
            }
            (None, None) => false,
            _ => true,
        };
        if changed {
            self.rows = store
                .map(|store| {
                    store
                        .inspect()
                        .into_iter()
                        .map(|info| {
                            let status = match info.status {
                                ruster_assets::AssetStatus::Ready => "Ready",
                                ruster_assets::AssetStatus::Missing => "Missing",
                                ruster_assets::AssetStatus::Invalid => "Invalid",
                            };
                            let label = format!("{} · {status} · {}", info.id, info.source);
                            ContentRow {
                                search: label.to_lowercase(),
                                info,
                                label,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.key = store.map(|store| (store.root().to_path_buf(), store.revision()));
        }
        if changed || self.query != query {
            self.query = query.to_string();
            let query = query.to_lowercase();
            self.visible = self
                .rows
                .iter()
                .enumerate()
                .filter_map(|(i, row)| row.search.contains(&query).then_some(i))
                .collect();
        }
    }
}

static GENERATION: AtomicU64 = AtomicU64::new(1);
enum RecoveryTransition {
    Replace {
        request: u64,
        next: Box<SceneEditor>,
        clipboard: bool,
        shader: bool,
    },
    Close {
        request: u64,
        cancelled: bool,
    },
}

pub struct SceneEditor {
    session: Session,
    selected: Option<String>,
    manipulation: crate::manipulation::Manipulation,
    behavior: crate::behavior_ui::BehaviorUi,
    runtime: crate::behavior_ui::RuntimeUi,
    external: crate::behavior_external::External,
    external_focused: bool,
    queued_external: Option<(String, crate::behavior_external::Intent)>,
    reset_assets: Option<(crate::behavior_worker::RunKey, BTreeMap<String, Arc<Mesh>>)>,
    draft: Option<Fields>,
    baseline_fields: Option<Fields>,
    workspace: Workspace,
    recovery: crate::recovery_ui::RecoveryUi,
    recovery_transition: Option<RecoveryTransition>,
    close_cleanup_error: Option<String>,
    close_shutdown: bool,
    close_ready_to_exit: bool,
    draft_generation: u64,
    asset_origin: Option<PathBuf>,
    recovered_origin: Option<PathBuf>,
    navigation: ruster_render::NavigationOptions,
    queued_selection: Option<(Option<String>, bool)>,
    queued_action: Option<Action>,
    queued_context: Option<(Option<String>, ContextAction)>,
    queued_drop: Option<(String, AssetDrag)>,
    queued_references: Option<String>,
    queued_refresh: Option<String>,
    hierarchy_query: String,
    hierarchy_focus: bool,
    hierarchy_search: HierarchySearch,
    content_query: String,
    content_focus: bool,
    content_selected: Option<String>,
    content_scroll_to: bool,
    content_search: ContentSearch,
    scale_applied: f32,
    preview: Preview,
    generation: u64,
    error: String,
    notice: String,
    seen_history_evictions: usize,
    path_text: String,
    name_text: String,
    new_scene_pending: bool,
    rename_pending: bool,
    rename_focused: bool,
    path_focused: bool,
    confirm_focused: Option<&'static str>,
    dialog: Option<bool>,
    close_pending: bool,
    close_approved: bool,
    assets: Option<AssetStore>,
    resolved: BTreeMap<String, Arc<Mesh>>,
    preview_assets: BTreeMap<String, Arc<Mesh>>,
    asset_dialog: bool,
    asset_focused: bool,
    asset_source_focus: bool,
    asset_rename_intent: bool,
    asset_id: String,
    asset_source: String,
    render: Option<eframe::egui_wgpu::RenderState>,
    shader_revision: u64,
    shader_notice: String,
}
impl SceneEditor {
    fn new(session: Session) -> Self {
        let mut editor = Self::with_workspace(
            session,
            if cfg!(test) {
                Workspace::detached()
            } else {
                Workspace::load()
            },
        );
        editor.recovery = crate::recovery_ui::RecoveryUi::load();
        editor
    }
    fn with_workspace(session: Session, workspace: Workspace) -> Self {
        let asset_origin = session
            .path()
            .and_then(|path| path.parent())
            .map(std::path::Path::to_path_buf);
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
            .map(|s| s.mesh_snapshots())
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
        let camera = session.path().and_then(|p| workspace.camera_for(p));
        let seen_history_evictions = session.history_evictions();
        Self {
            session,
            selected,
            manipulation,
            behavior: crate::behavior_ui::BehaviorUi::default(),
            runtime: crate::behavior_ui::RuntimeUi::default(),
            external: crate::behavior_external::External::default(),
            external_focused: false,
            queued_external: None,
            reset_assets: None,
            baseline_fields: draft.clone(),
            workspace,
            recovery: crate::recovery_ui::RecoveryUi::disabled(),
            recovery_transition: None,
            close_cleanup_error: None,
            close_shutdown: false,
            close_ready_to_exit: false,
            draft_generation: 0,
            asset_origin,
            recovered_origin: None,
            navigation: ruster_render::NavigationOptions {
                camera,
                reset: camera.is_none(),
                ..Default::default()
            },
            queued_selection: None,
            queued_action: None,
            queued_context: None,
            queued_drop: None,
            queued_references: None,
            queued_refresh: None,
            hierarchy_query: String::new(),
            hierarchy_focus: false,
            hierarchy_search: HierarchySearch::default(),
            content_query: String::new(),
            content_focus: false,
            content_selected: None,
            content_scroll_to: false,
            content_search: ContentSearch::default(),
            scale_applied: 0.0,
            draft,
            preview,
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            error: asset_error,
            notice: "Ready. Right-drag to orbit; scroll to zoom.".into(),
            seen_history_evictions,
            path_text: String::new(),
            name_text,
            new_scene_pending: false,
            rename_pending: false,
            rename_focused: false,
            path_focused: false,
            confirm_focused: None,
            dialog: None,
            close_pending: false,
            close_approved: false,
            assets,
            resolved,
            preview_assets,
            asset_dialog: false,
            asset_focused: false,
            asset_source_focus: false,
            asset_rename_intent: false,
            asset_id: "pyramid".into(),
            asset_source: "meshes/pyramid.obj".into(),
            render: None,
            shader_revision: 0,
            shader_notice: "Built-in scene shader".into(),
        }
    }
    fn drafts(&self) -> bool {
        self.manipulation.precision_pending()
            || self.draft != self.baseline_fields
            || self.behavior.pending()
    }
    fn pending_recovery_drafts(&self) -> bool {
        self.drafts() || self.rename_pending || self.manipulation.offset_pending()
    }
    fn behavior_editable(&self) -> bool {
        !self.manipulation.active()
            && !self.manipulation.precision_pending()
            && self.draft == self.baseline_fields
            && !self.manipulation.offset_pending()
            && self.dialog.is_none()
            && !self.asset_dialog
            && !self.new_scene_pending
            && !self.rename_pending
            && !self.close_pending
            && !self.close_approved
            && !self.workspace.settings_open
            && !self.recovery.open
            && self.recovery_transition.is_none()
    }
    fn behavior_action(&mut self, action: crate::behavior_ui::Action) {
        use crate::behavior_ui::Action;
        if matches!(action, Action::ExternalStatus) {
            self.external.open = true;
            return;
        }
        if let Action::External { source_id, intent } = action {
            self.queued_external = Some((source_id, intent));
            return;
        }
        if !self.behavior_editable() {
            self.error="Resolve transform fields, gestures and modal dialogs before behavior edits; drafts preserved.".into();
            return;
        }
        if matches!(action, Action::Save) {
            self.save();
            self.observe_runtime();
            return;
        }
        let own_draft_action = matches!(
            action,
            Action::ApplySource
                | Action::RevertSource
                | Action::ApplyProperty
                | Action::RevertProperty
                | Action::OpenSource(_)
                | Action::EditProperty(_, _)
        );
        if !own_draft_action {
            if let Err(error) = self.guard() {
                self.behavior.error = error;
                return;
            }
        }
        match self.behavior.execute(action, &mut self.session) {
            Ok(()) => {
                self.sync_draft();
                self.observe_runtime();
                self.error.clear();
                self.notice="Behavior data updated through shared history; Reset runtime to use authored changes.".into();
            }
            Err(error) => {
                self.behavior.error = error.clone();
                self.error = error;
            }
        }
    }
    fn external_action(
        &mut self,
        ctx: &egui::Context,
        source_id: &str,
        intent: crate::behavior_external::Intent,
    ) {
        let result = self.guard().and_then(|()| {
            if self.manipulation.offset_pending() {
                Err("Apply or reset translation offsets before external source operations.".into())
            } else {
                self.external.request(
                    ctx,
                    &self.session,
                    self.generation,
                    source_id,
                    intent,
                    &self.workspace.prefs.external_editor,
                )
            }
        });
        if let Err(error) = result {
            self.external.status = error;
        }
    }
    fn external_frame(&mut self, ctx: &egui::Context) {
        self.external.set_closing(
            self.close_pending
                || self.close_approved
                || self.close_shutdown
                || matches!(
                    self.recovery_transition,
                    Some(RecoveryTransition::Close { .. })
                ),
        );
        let blocked = self.guard().is_err() || self.manipulation.offset_pending();
        if let Some(import) = self.external.poll(
            ctx,
            &self.session,
            self.generation,
            blocked,
            self.behavior.active_source(),
            &self.workspace.prefs.external_editor,
        ) {
            if !blocked {
                match self.session.set_source(
                    self.session.scene().revision,
                    &import.source_id,
                    Some(import.source),
                ) {
                    Ok(_) => {
                        self.external
                            .imported(import.next_base, self.session.scene().revision);
                        self.sync_draft();
                        self.observe_runtime();
                        self.notice="Imported the observed working-file version through scene Undo. Save to keep it.".into();
                        self.error.clear();
                        ctx.request_repaint();
                    }
                    Err(error) => self.external.status = error.to_string(),
                }
            }
        }
        if let Some((source, intent)) =
            self.external
                .ui(ctx, !blocked, self.session.scene().revision)
        {
            self.queued_external = Some((source, intent));
        }
        if let Some((source, intent)) = self.queued_external.take() {
            self.external_action(ctx, &source, intent);
        }
    }
    fn observe_runtime(&mut self) {
        self.runtime.observe(
            self.generation,
            self.session.scene().revision,
            self.assets.as_ref().map(|store| store.revision()),
            self.shader_revision,
        );
    }
    fn poll_runtime(&mut self, ctx: &egui::Context) {
        self.observe_runtime();
        self.runtime.poll(ctx);
        self.accept_runtime_resources();
    }
    fn accept_runtime_resources(&mut self) {
        if let Some(key) = self.runtime.reset_succeeded.take() {
            if self
                .reset_assets
                .as_ref()
                .is_some_and(|(candidate, _)| *candidate == key)
                && self
                    .runtime
                    .frame
                    .as_ref()
                    .is_some_and(|frame| frame.key == key)
            {
                let (_, assets) = self.reset_assets.take().unwrap();
                self.preview_assets = assets;
            }
        }
    }
    #[cfg(test)]
    fn recovery_snapshot(&self) -> Result<crate::recovery::Snapshot, String> {
        let drafts = RecoveryDrafts {
            version: 3,
            revision: self.session.scene().revision,
            selection: self.manipulation.ids.clone(),
            fields: self.draft.clone(),
            pending_name: self.rename_pending.then(|| self.name_text.clone()),
            manipulation: self.manipulation.snapshot_drafts(),
            behavior: Some(self.behavior.snapshot()),
        };
        let mut snapshot = crate::recovery::Snapshot::capture(
            &self.session,
            self.asset_origin.clone(),
            &serde_json::to_value(drafts).map_err(|e| e.to_string())?,
        )?;
        if self.session.path().is_none() {
            snapshot.origin = self.recovered_origin.clone();
        }
        Ok(snapshot)
    }
    fn restore_recovery_drafts(&mut self, value: Value) -> Result<(), String> {
        let behavior_field = value.get("behavior").is_some();
        let version = value.get("version").and_then(Value::as_u64);
        let manipulation_unit = value.pointer("/manipulation/rotation_unit").is_some();
        let fields_unit = value
            .get("fields")
            .filter(|v| !v.is_null())
            .map(|v| v.get("rotation_unit").is_some());
        if !match version {
            Some(1 | 2) => !manipulation_unit && fields_unit != Some(true),
            Some(3) => manipulation_unit && fields_unit != Some(false),
            _ => false,
        } {
            return Err(
                "Recovery drafts have unsupported version or rotation units; checkpoint preserved."
                    .into(),
            );
        }
        let draft: RecoveryDrafts = serde_json::from_value(value)
            .map_err(|e| format!("Unsupported recovery drafts: {e}; checkpoint preserved."))?;
        let scene = self.session.scene();
        if !((draft.version == 1 && !behavior_field)
            || ((draft.version == 2 || draft.version == 3) && draft.behavior.is_some()))
            || draft.revision != scene.revision
            || draft.selection.len() > ruster_scene::MAX_OBJECTS
            || draft.selection.iter().enumerate().any(|(i, id)| {
                draft.selection[..i].contains(id)
                    || !scene.objects.iter().any(|object| &object.id == id)
            })
            || draft.fields.is_some() == draft.selection.is_empty()
            || draft.fields.as_ref().is_some_and(|fields| {
                fields
                    .translation
                    .iter()
                    .chain(&fields.rotation)
                    .chain(&fields.scale)
                    .any(|text| text.chars().count() > 1024)
            })
            || draft
                .pending_name
                .as_ref()
                .is_some_and(|text| text.chars().count() > 128 || draft.selection.is_empty())
        {
            return Err("Recovery drafts have unsupported version, selection, revision or text bounds; checkpoint preserved.".into());
        }
        self.manipulation.ids = draft.selection;
        self.selected = self.manipulation.selected();
        self.sync_draft();
        self.manipulation
            .restore_drafts(self.session.scene(), draft.manipulation)?;
        if let (Some(fields), Some(id)) = (&draft.fields, &self.selected) {
            let transform = &self
                .session
                .scene()
                .objects
                .iter()
                .find(|object| &object.id == id)
                .unwrap()
                .transform;
            let baseline = Fields::with_unit(transform, fields.rotation_unit);
            if fields == &baseline {
                // Clean legacy baselines can use the current degree presentation.
                self.draft = Some(Fields::from(transform));
                self.baseline_fields = self.draft.clone();
            } else {
                self.baseline_fields = Some(baseline);
                self.draft = draft.fields;
            }
        } else {
            self.draft = draft.fields;
        }
        if let Some(behavior) = draft.behavior {
            if behavior.pending()
                && (self.manipulation.precision_pending()
                    || self.draft != self.baseline_fields
                    || self.manipulation.offset_pending())
            {
                return Err("Unsupported compound transform/behavior recovery drafts; original record preserved.".into());
            }
            self.behavior.restore(&self.session, behavior)?;
        }
        if let Some(name) = draft.pending_name {
            self.name_text = name;
            self.rename_pending = true;
        }
        Ok(())
    }
    fn restore_checkpoint(&mut self, id: &str) -> Result<(), String> {
        if self.recovery.boundary_pending() || self.recovery_transition.is_some() {
            return Err(
                "Recovery is finishing the previous boundary; retry once it completes.".into(),
            );
        }
        if self.needs_recovery()
            || self.pending_recovery_drafts()
            || self.manipulation.active()
            || self.dialog.is_some()
            || self.asset_dialog
            || self.new_scene_pending
            || self.close_pending
        {
            return Err("Save your current work or use New scene and resolve its recovery first, then restore this checkpoint. Current work was preserved.".into());
        }
        let (journal, snapshot) = self
            .recovery
            .store
            .as_ref()
            .ok_or("Recovery store unavailable.")?
            .claim(id)?;
        let session = snapshot.restore()?;
        let mut next = Self::with_workspace(session, Workspace::detached());
        next.restore_recovery_drafts(snapshot.drafts()?)?;
        next.asset_origin = snapshot.asset_origin.clone();
        next.recovered_origin = snapshot.origin.clone();
        next.assets = match &next.asset_origin {
            Some(root) => match AssetStore::open(root) {
                Ok(store) => Some(store),
                Err(error) => {
                    next.error = format!("Recovery catalog {} is unavailable: {error}. Restore the original catalog at that path, then Retry recovered content. References were preserved.", root.display());
                    None
                }
            },
            None => None,
        };
        next.asset_snapshot();
        next.preview_assets = next.resolved.clone();
        let render = self.render.clone();
        if let Some(render) = render.as_ref().filter(|_| self.shader_revision > 0) {
            ruster_render::refresh_shader(render, ruster_render::default_shader())?;
        }
        next.workspace.welcome = false;
        next.render = render;
        next.notice = "Recovered as an unsaved copy. Save requests a fresh filename; original project unchanged.".into();
        if let Some(origin) = snapshot.origin.as_ref().or(snapshot.asset_origin.as_ref()) {
            let parent = if snapshot.origin.is_some() {
                origin.parent().unwrap_or(origin)
            } else {
                origin.as_path()
            };
            next.path_text = parent.join("recovered-scene.json").display().to_string();
        }
        self.recovery
            .candidates
            .retain(|candidate| candidate.id != id);
        self.begin_replacement(next, Some(journal), false, true);
        Ok(())
    }
    fn recovery_frame(&mut self, ctx: &egui::Context) {
        if let Some(intent) = self.recovery.ui(ctx) {
            let result = match intent {
                crate::recovery_ui::Intent::Restore(id) => self.restore_checkpoint(&id),
                crate::recovery_ui::Intent::Discard(id) => self
                    .recovery
                    .store
                    .as_ref()
                    .ok_or_else(|| "Recovery store unavailable.".to_string())
                    .and_then(|store| store.discard(&id)),
            };
            match result {
                Ok(()) => self.recovery.refresh(),
                Err(error) => self.recovery.set_warning(error),
            }
        }
        let now = Instant::now();
        let pending = self.needs_recovery() || self.pending_recovery_drafts();
        self.recovery.observe(
            self.session.scene().revision,
            self.manipulation.draft_generation(),
            self.draft_generation
                .wrapping_add(self.behavior.generation()),
            pending,
            now,
        );
        if self.recovery.due(now)
            && pending
            && self.recovery_transition.is_none()
            && !self.close_approved
        {
            let session = self.session.clone();
            let asset_origin = self.asset_origin.clone();
            let recovered_origin = self.recovered_origin.clone();
            let drafts = RecoveryDrafts {
                version: 3,
                revision: session.scene().revision,
                selection: self.manipulation.ids.clone(),
                fields: self.draft.clone(),
                pending_name: self.rename_pending.then(|| self.name_text.clone()),
                manipulation: self.manipulation.snapshot_drafts(),
                behavior: Some(self.behavior.snapshot()),
            };
            let capture = Box::new(move || {
                let mut snapshot = crate::recovery::Snapshot::capture(
                    &session,
                    asset_origin,
                    &serde_json::to_value(drafts).map_err(|e| e.to_string())?,
                )?;
                if session.path().is_none() {
                    snapshot.origin = recovered_origin;
                }
                Ok(snapshot)
            });
            if let Err(error) = self.recovery.dispatch(capture) {
                self.recovery.set_warning(error);
            }
        }
        self.recovery.schedule(ctx, now);
    }
    fn begin_replacement(
        &mut self,
        next: Self,
        adopt: Option<crate::recovery::Journal>,
        clipboard: bool,
        shader: bool,
    ) {
        self.remember_camera();
        self.runtime
            .stop(self.generation, self.session.scene().revision);
        self.manipulation.cancel();
        self.recovery.open = false;
        self.dialog = None;
        self.new_scene_pending = false;
        let request = match adopt {
            Some(journal) => self.recovery.adopt(journal),
            None => self.recovery.retire(),
        };
        self.recovery_transition = Some(RecoveryTransition::Replace {
            request,
            next: Box::new(next),
            clipboard,
            shader,
        });
        self.process_recovery_ack();
    }
    fn process_recovery_ack(&mut self) {
        let Some((id, result)) = self.recovery.take_ack() else {
            return;
        };
        let Some(transition) = self.recovery_transition.take() else {
            return;
        };
        match transition {
            RecoveryTransition::Replace {
                request,
                mut next,
                clipboard,
                shader,
            } if request == id => {
                next.workspace = std::mem::replace(&mut self.workspace, Workspace::detached());
                next.workspace.welcome = clipboard
                    && next.session.path().is_none()
                    && next.session.scene().objects.is_empty()
                    && next.session.authoring().is_empty();
                if let Some(path) = next.session.path() {
                    next.workspace.remember(path);
                }
                if clipboard {
                    next.manipulation.clipboard_from(&mut self.manipulation);
                }
                next.runtime = std::mem::take(&mut self.runtime);
                next.external = std::mem::take(&mut self.external);
                next.external
                    .observe_document(&next.session, next.generation);
                next.runtime.observe(
                    next.generation,
                    next.session.scene().revision,
                    next.assets.as_ref().map(|store| store.revision()),
                    next.shader_revision,
                );
                next.recovery = std::mem::replace(
                    &mut self.recovery,
                    crate::recovery_ui::RecoveryUi::disabled(),
                );
                if let Err(error) = result {
                    next.recovery.set_warning(format!("Previous recovery cleanup failed: {error}. Preserved entry remains in Recovery."));
                }
                *self = *next;
                if shader
                    && self
                        .asset_origin
                        .as_ref()
                        .is_some_and(|root| root.join("scene.wgsl").is_file())
                {
                    if let Err(error) = self.refresh_shader() {
                        self.shader_notice = format!("Recovered shader unavailable: {error}");
                    }
                }
            }
            RecoveryTransition::Close { request, cancelled } if request == id => {
                if cancelled {
                    self.close_approved = false;
                    self.close_pending = false;
                    self.recovery.reschedule();
                } else {
                    match result {
                        Ok(()) => {
                            self.recovery.shutdown();
                            self.runtime.shutdown();
                            self.close_shutdown = true;
                        }
                        Err(error) => self.close_cleanup_error = Some(error),
                    }
                }
            }
            other => self.recovery_transition = Some(other),
        }
    }
    fn close_finalization_ui(&mut self, ctx: &egui::Context) {
        if let Some(error) = self.close_cleanup_error.clone() {
            egui::Modal::new(egui::Id::unique("close_recovery_failed")).show(ctx,|ui| {
                ui.set_width((ctx.content_rect().width()-48.0).clamp(220.0,520.0));
                ui.heading("Recovery cleanup failed");
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-130.0).max(48.0)).show(ui,|ui| {
                    ui.label(&error);
                    ui.label("The preserved checkpoint may reappear next launch. Close anyway, or keep this window open.");
                });
                ui.horizontal_wrapped(|ui| {
                if ui.button("Close anyway").clicked() {self.close_cleanup_error=None;self.recovery.shutdown();self.runtime.shutdown();self.close_shutdown=true;}
                let cancel=ui.button("Cancel closing");
                if self.confirm_focused!=Some("cleanup-error") {cancel.request_focus();self.confirm_focused=Some("cleanup-error");}
                if cancel.clicked() || ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape)) {
                    self.close_cleanup_error=None;self.close_approved=false;self.close_pending=false;self.recovery.reschedule();
                }
                });
            });
        } else if let Some(RecoveryTransition::Close { cancelled, .. }) =
            &mut self.recovery_transition
        {
            if !*cancelled {
                egui::Modal::new(egui::Id::unique("close_recovery_wait")).show(ctx, |ui| {
                    ui.heading("Finishing recovery cleanup…");
                    if ui.button("Cancel closing").clicked()
                        || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                    {
                        *cancelled = true;
                        self.close_approved = false;
                        self.close_pending = false;
                    }
                });
            }
        } else if matches!(
            self.recovery_transition,
            Some(RecoveryTransition::Replace { .. })
        ) {
            egui::Modal::new(egui::Id::unique("recovery_replacement_wait")).show(ctx, |ui| {
                ui.label("Finishing recovery boundary before switching scenes…");
            });
        } else if self.close_approved && !self.close_shutdown && !self.external.finished() {
            egui::Modal::new(egui::Id::unique("external_close_wait")).show(ctx, |ui| {
                ui.label("Finishing the current working-copy job before closing…");
                let cancel = ui.button("Cancel closing");
                cancel.request_focus();
                if cancel.clicked()
                    || ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    })
                {
                    self.close_approved = false;
                    self.close_pending = false;
                    self.external.cancel_close();
                }
            });
        }
    }
    fn start_close_cleanup(&mut self) {
        if self.close_approved
            && self.recovery_transition.is_none()
            && self.close_cleanup_error.is_none()
            && !self.close_shutdown
            && self.external.finished()
        {
            self.close_pending = false;
            let request = self.recovery.retire();
            self.recovery_transition = Some(RecoveryTransition::Close {
                request,
                cancelled: false,
            });
            self.process_recovery_ack();
        }
    }
    #[cfg(test)]
    fn wait_recovery(&mut self) {
        self.recovery.wait_idle();
        self.process_recovery_ack();
    }
    fn reconcile_selection(&mut self) {
        self.manipulation.reconcile(self.session.scene());
        if self.manipulation.ids.is_empty() && self.selected.is_some() {
            self.manipulation
                .select_only(self.session.scene().objects.first().map(|o| o.id.clone()));
        }
        self.selected = self.manipulation.selected();
    }
    fn process_queued_selection(&mut self) {
        let Some((id, additive)) = self.queued_selection.take() else {
            return;
        };
        if self.manipulation.active()
            || self.dialog.is_some()
            || self.rename_pending
            || self.new_scene_pending
            || self.close_pending
            || self.workspace.settings_open
            || self.recovery.open
            || self.recovery_transition.is_some()
        {
            return;
        }
        if self.drafts() && !self.apply() {
            return;
        }
        self.select_object(id, additive);
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
        self.baseline_fields = self.draft.clone();
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
            || self.workspace.settings_open
            || self.recovery.open
            || self.recovery_transition.is_some()
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
        if let Err(error) = self.behavior.apply_all(&mut self.session) {
            self.error = error;
            return false;
        }
        self.observe_runtime();
        if self.manipulation.precision_pending() && self.draft != self.baseline_fields {
            self.error = "Both recovered transform drafts are pending. Revert transform drafts to discard them, or keep this checkpoint for repair; no text was applied.".into();
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
        if self.draft == self.baseline_fields {
            return true;
        }
        let mut draft = match fields.parse() {
            Ok(t) => t,
            Err(e) => {
                self.error = e;
                return false;
            }
        };
        if let Some(baseline) = &self.baseline_fields {
            if let Some(object) = self
                .session
                .scene()
                .objects
                .iter()
                .find(|object| object.id == id)
            {
                for axis in 0..3 {
                    if fields.rotation[axis] == baseline.rotation[axis] {
                        draft.rotation[axis] = object.transform.rotation[axis];
                    }
                }
            }
        }
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
                Ok(()) => {
                    if self.pending_recovery_drafts() {
                        self.error="Scene saved, but unapplied fields remain. Cancel the close/New dialog and apply or reset the translation offset, or explicitly discard it.".into();
                    } else {
                        self.notice = "Scene saved.".into();
                    }
                }
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
        self.remember_camera();
        let render = self.render.clone();
        if let Some(render) = render.as_ref().filter(|_| self.shader_revision > 0) {
            if let Err(error) =
                ruster_render::refresh_shader(render, ruster_render::default_shader())
            {
                self.error = error;
                return;
            }
        }
        let mut next = Self::with_workspace(
            Session::new(Scene::empty("untitled")).expect("empty scene is valid"),
            Workspace::detached(),
        );
        next.render = render;
        self.begin_replacement(next, None, true, false);
    }
    fn clipboard_action(&mut self, paste: bool) {
        if let Err(error) = self.guard() {
            self.error = error;
            return;
        }
        let root = self.asset_origin.clone();
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
    fn tool_options_ui(&mut self, ui: &mut egui::Ui) {
        use crate::manipulation::{Pivot, Tool};
        ui.label("Space");
        egui::ComboBox::from_id_salt("tool_space")
            .width(64.0)
            .selected_text(if self.manipulation.local {
                "Local"
            } else {
                "World"
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.manipulation.local, false, "World");
                ui.selectable_value(&mut self.manipulation.local, true, "Local");
            })
            .response
            .on_hover_text("Local uses the active object's axes");
        ui.label("Pivot");
        egui::ComboBox::from_id_salt("tool_pivot")
            .width(120.0)
            .selected_text(match self.manipulation.pivot {
                Pivot::Individual => "Individual origins",
                Pivot::Center => "Selection center",
                Pivot::Active => "Active object",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.manipulation.pivot,
                    Pivot::Individual,
                    "Individual origins",
                );
                ui.selectable_value(
                    &mut self.manipulation.pivot,
                    Pivot::Center,
                    "Selection center",
                );
                ui.selectable_value(&mut self.manipulation.pivot, Pivot::Active, "Active object");
            })
            .response
            .on_hover_text("Selection center is the mean of object origins");
        ui.checkbox(&mut self.manipulation.snap, "Snap");
        if self.manipulation.snap {
            match self.manipulation.tool {
                Tool::Translate => {
                    ui.add(
                        egui::DragValue::new(&mut self.manipulation.snap_distance)
                            .range(0.001..=1000.0)
                            .speed(0.1)
                            .suffix(" units"),
                    )
                    .on_hover_text("Snap distance in scene units");
                }
                Tool::Rotate => {
                    ui.add(
                        egui::DragValue::new(&mut self.manipulation.snap_degrees)
                            .range(0.1..=180.0)
                            .speed(1.0)
                            .suffix("°"),
                    )
                    .on_hover_text("Snap angle in degrees");
                }
                Tool::Scale => {
                    ui.add(
                        egui::DragValue::new(&mut self.manipulation.snap_scale)
                            .range(0.001..=10.0)
                            .speed(0.05),
                    )
                    .on_hover_text("Scale-factor increment");
                }
            }
        }
        if self.manipulation.tool == Tool::Scale
            && !(self.manipulation.local && self.manipulation.pivot == Pivot::Individual)
        {
            ui.label("Uniform scale").on_hover_text("Choose Local and Individual origins for nonuniform scaling. World nonuniform scaling would require shear.");
        }
    }
    fn manipulation_tools_ui(&mut self, ui: &mut egui::Ui) {
        let editable = self.guard().is_ok() && !self.runtime.visible;
        let compact = ui.available_width() < 560.0;
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal_wrapped(|ui| {
                if compact {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.spacing_mut().button_padding.x = 6.0;
                }
                use crate::manipulation::Tool;
                ui.selectable_value(&mut self.manipulation.tool, Tool::Translate, "Move")
                    .on_hover_text(self.workspace.shortcut_text(Action::Move));
                ui.selectable_value(&mut self.manipulation.tool, Tool::Rotate, "Rotate")
                    .on_hover_text(self.workspace.shortcut_text(Action::Rotate));
                ui.selectable_value(&mut self.manipulation.tool, Tool::Scale, "Scale")
                    .on_hover_text(self.workspace.shortcut_text(Action::Scale));
                if compact {
                    let label = match (self.manipulation.local, self.manipulation.snap) {
                        (true, true) => "Local · Snap",
                        (true, false) => "Local options",
                        (false, true) => "Snap options",
                        _ => "Tool options",
                    };
                    ui.menu_button(label, |ui| self.tool_options_ui(ui))
                        .response
                        .on_hover_text("Tool options · Space, Pivot and Snap");
                } else {
                    ui.separator();
                    self.tool_options_ui(ui);
                }
            });
        });
    }
    fn manipulation_ui(&mut self, ui: &mut egui::Ui) {
        let pending = self.manipulation.offset_pending();
        if pending {
            ui.label("Unapplied translation offset");
        }
        egui::CollapsingHeader::new("Adjust selection").id_salt("selection_adjustments").open(pending.then_some(true)).show(ui,|ui| {
            ui.label("World translation offset");
            ui.add_enabled_ui(!self.manipulation.active() && !self.manipulation.precision_pending(), |ui| {
                let mut enter=false;
                let mut escape=false;
                let group_width=crate::manipulation::component_group_width(ui);
                ui.horizontal_wrapped(|ui| {
                    for (axis,text) in ["X","Y","Z"].into_iter().zip(&mut self.manipulation.delta) {
                        ui.allocate_ui_with_layout(egui::vec2(group_width,ui.spacing().interact_size.y),egui::Layout::left_to_right(egui::Align::Center),|ui| {
                            ui.label(axis);
                            let response=ui.add_sized([ui.available_width().max(48.0),ui.spacing().interact_size.y],egui::TextEdit::singleline(text).id(egui::Id::unique(("selection_offset",axis))).char_limit(1024).interactive(ui.is_enabled()));
                            let owned=ui.is_enabled() && (response.has_focus() || response.lost_focus() || ui.memory(|m|m.had_focus_last_frame(response.id)));
                            enter|=owned && ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Enter));
                            escape|=owned && ui.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape));
                            if response.changed() {self.draft_generation=self.draft_generation.wrapping_add(1);}
                        });
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Reset translation offset").clicked() || escape {
                        self.manipulation.delta=std::array::from_fn(|_|"0".into());
                        self.draft_generation=self.draft_generation.wrapping_add(1);
                    }
                    if ui.add_enabled(!self.manipulation.ids.is_empty(),egui::Button::new("Apply translation offset")).clicked() || (enter && !self.manipulation.ids.is_empty()) {
                        let result=self.guard().and_then(|()|self.manipulation.apply_delta(&mut self.session));
                        match result {
                            Ok(())=>{self.manipulation.delta=std::array::from_fn(|_|"0".into());self.sync_draft();self.error.clear();self.notice="Translation offset applied; one Undo restores the selection.".into();},
                            Err(error)=>self.error=error,
                        }
                    }
                });
            });
        });
    }
    fn asset_snapshot(&mut self) {
        self.content_search.key = None;
        self.resolved = self
            .assets
            .as_ref()
            .map(|store| store.mesh_snapshots())
            .unwrap_or_default();
    }
    fn remember_camera(&mut self) {
        if let (Some(path), Some(camera)) = (self.session.path(), self.navigation.camera) {
            self.workspace.remember_camera(path, camera);
        }
    }
    fn save_scene(&mut self, path: PathBuf) -> Result<(), String> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let old_root = self.asset_origin.clone();
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
        self.asset_origin = Some(parent);
        self.recovered_origin = None;
        self.draft_generation = self.draft_generation.wrapping_add(1);
        if let Some(path) = self.session.path() {
            self.workspace.remember(path);
        }
        self.remember_camera();
        self.workspace.flush();
        self.external.saved(&self.session, self.generation);
        if !self.pending_recovery_drafts() {
            self.recovery.retire();
        } else {
            self.notice="Scene saved; unapplied fields remain. Apply or reset the translation offset before closing, or explicitly discard it.".into();
        }
        Ok(())
    }
    fn assign_mesh(&mut self, mesh: Option<String>) -> Result<u64, String> {
        let id = self.selected.clone().ok_or("Select an object first")?;
        self.assign_mesh_to(&id, mesh)
    }
    fn assign_mesh_to(&mut self, id: &str, mesh: Option<String>) -> Result<u64, String> {
        self.guard().map_err(|error| {
            format!("{error}: finish drafts/gestures or resolve dialogs before assigning a mesh.")
        })?;
        if let Some(asset) = &mesh {
            if !self.resolved.contains_key(asset) {
                return Err(format!(
                    "Mesh {asset} is unavailable; import or repair its source first."
                ));
            }
        }
        let revision = self
            .session
            .set_mesh(self.session.scene().revision, id, mesh)
            .map_err(|e| e.to_string())?;
        self.notice=format!("Mesh assigned to {id} only, revision{revision}; Undo restores the assignment, preview unchanged until Reset.");
        Ok(revision)
    }
    fn asset_payload(&self, id: &str) -> Result<AssetDrag, String> {
        let store = self
            .assets
            .as_ref()
            .ok_or("Open a saved scene with a valid content catalog first.")?;
        if !self.resolved.contains_key(id) {
            return Err(format!("Asset {id} has no valid mesh to assign."));
        }
        Ok(AssetDrag {
            catalog_root: store.root().to_path_buf(),
            catalog_revision: store.revision(),
            generation: self.generation,
            scene_revision: self.session.scene().revision,
            asset_id: id.to_string(),
        })
    }
    fn accept_asset_drop(&mut self, target: &str, payload: &AssetDrag) -> Result<u64, String> {
        self.guard()
            .map_err(|e| format!("{e}: finish drafts or dialogs before dropping content."))?;
        if payload.generation != self.generation {
            return Err(
                "The content drag belongs to an older/different scene session; drag it again."
                    .into(),
            );
        }
        if payload.scene_revision != self.session.scene().revision {
            return Err("The scene changed during the content drag; drag the asset again.".into());
        }
        let store = self
            .assets
            .as_ref()
            .ok_or("Content catalog is unavailable; reopen the scene.")?;
        if payload.catalog_root != store.root() {
            return Err(
                "Content belongs to another project catalog; import it into this project first."
                    .into(),
            );
        }
        if payload.catalog_revision != store.revision() {
            return Err(
                "The catalog changed during the content drag; inspect it and drag again.".into(),
            );
        }
        self.assign_mesh_to(target, Some(payload.asset_id.clone()))
    }
    fn process_content_intents(&mut self) {
        if let Some(id) = self.queued_refresh.take() {
            self.refresh_content_asset(&id);
        }
        if let Some(asset) = self.queued_references.take() {
            if self.drafts() && !self.apply() {
                return;
            }
            if let Err(error) = self.guard() {
                self.error = error;
                return;
            }
            self.manipulation.ids = self
                .session
                .scene()
                .objects
                .iter()
                .filter(|o| o.mesh.as_deref() == Some(asset.as_str()))
                .map(|o| o.id.clone())
                .collect();
            self.selected = self.manipulation.selected();
            self.sync_draft();
        }
        if let Some((target, action)) = self.queued_context.take() {
            if let Some(target) = target {
                if !self.manipulation.ids.contains(&target) {
                    self.queued_selection = Some((Some(target.clone()), false));
                    self.process_queued_selection();
                    if self.selected.as_ref() != Some(&target) {
                        return;
                    }
                }
            }
            if let Err(error) = self.guard() {
                self.error = error;
                return;
            }
            match action {
                ContextAction::Create => self.object_action("create"),
                ContextAction::Rename => {
                    if self.manipulation.ids.len() == 1 {
                        self.rename_pending = true;
                    } else {
                        self.error = "Select one object to rename.".into();
                    }
                }
                ContextAction::Duplicate => self.object_action("duplicate"),
                ContextAction::Delete => self.object_action("delete"),
                ContextAction::Copy => self.clipboard_action(false),
                ContextAction::Paste => self.clipboard_action(true),
                ContextAction::Frame => self.navigation.frame_selected = true,
            }
        }
        if let Some((target, payload)) = self.queued_drop.take() {
            match self.accept_asset_drop(&target, &payload) {
                Ok(_) => {
                    self.error.clear();
                    self.sync_draft();
                }
                Err(error) => self.error = error,
            }
        }
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
        if self.error == self.shader_notice {
            self.error.clear();
        }
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
        self.guard()
            .and_then(|()| {
                if self.manipulation.offset_pending() {
                    Err("drafts_pending".into())
                } else {
                    Ok(())
                }
            })
            .map_err(|code| {
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
            Ok(mut value) => {
                if let Some(result) = value.as_object_mut() {
                    result.insert(
                        "history_evictions".into(),
                        self.session.history_evictions().into(),
                    );
                }
                self.observe_history_evictions();
                json!({"ok":true,"result":value})
            }
            Err((code, message)) => {
                json!({"ok":false,"error":{"code":code,"message":message,"session_token":token,"revision":self.session.scene().revision.to_string()}})
            }
        }
    }
    fn execute(&mut self, token: &str, op: &str, args: &Value) -> Result<Value, (String, String)> {
        if runtime_commands::supported(op) {
            return self.runtime_command(token, op, args);
        }
        if op.starts_with("behavior_") {
            return self.behavior_command(token, op, args);
        }
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
                json!({"session_token":token,"scene_id":scene.id,"revision":scene.revision.to_string(),"objects":objects,"object_count":scene.objects.len(),"unresolved_assets":scene.objects.iter().filter_map(|o|o.mesh.as_ref().filter(|id|!self.resolved.contains_key(*id)).map(|id|json!({"object_id":o.id,"asset_id":id,"action":"Import registered source or repair the project catalog; placeholder cube shown"}))).take(16).collect::<Vec<_>>(),"next_offset":if start+16<scene.objects.len(){Some((start+16).to_string())}else{None},"dirty":self.session.is_dirty(),"drafts_pending":self.drafts()||self.manipulation.active()||self.manipulation.offset_pending(),"recovery_pending":self.dialog.is_some()||self.close_pending||self.close_approved||self.asset_dialog||self.new_scene_pending||self.rename_pending||self.workspace.settings_open||self.recovery.open||self.recovery_transition.is_some(),"preview_tick":self.preview.tick.to_string(),"preview_fingerprint":self.preview.scene.fingerprint(),"asset_revision":self.assets.as_ref().map(|s|s.revision().to_string()),"loaded_format_version":self.session.loaded_format_version(),"shader_revision":self.shader_revision.to_string(),"preview_stale":self.preview.scene!=*scene || self.preview_assets!=self.resolved,"capabilities":["behavior_runtime_inspect","behavior_runtime_reset","behavior_runtime_play","behavior_runtime_pause","behavior_runtime_step","behavior_runtime_stop","behavior_breakpoints","behavior_breakpoint_set","behavior_debug_inspect","behavior_debug_next","behavior_debug_continue","behavior_inspect","behavior_source","behavior_source_set","behavior_source_remove","behavior_create","behavior_binding","behavior_binding_set","behavior_binding_remove","behavior_property","behavior_property_set","scene_inspect","scene_transform","scene_create","scene_rename","scene_duplicate","scene_delete","scene_transform_many","scene_delete_many","scene_duplicate_many","scene_undo","scene_redo","scene_replay","asset_inspect","asset_import","asset_refresh","asset_rename","scene_mesh","shader_refresh"]}),
            );
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(err("stale_session"));
        }
        if op != "scene_replay" {
            self.guard()
                .and_then(|()| {
                    if self.manipulation.offset_pending() {
                        Err("drafts_pending".into())
                    } else {
                        Ok(())
                    }
                })
                .map_err(|e| {
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
    fn hierarchy_contents(&mut self, ui: &mut egui::Ui) {
        ui.heading("Objects");
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none());
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape) && i.modifiers.is_none());
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.hierarchy_query)
                .hint_text("Search objects (name or ID)")
                .char_limit(128)
                .desired_width(f32::INFINITY),
        );
        if self.hierarchy_focus {
            search.request_focus();
            search.scroll_to_me(Some(egui::Align::Min));
            self.hierarchy_focus = false;
        }
        if (search.has_focus() || search.lost_focus()) && escape {
            self.hierarchy_query.clear();
            search.surrender_focus();
        }
        self.hierarchy_search
            .update(self.session.scene(), &self.hierarchy_query);
        if (search.has_focus() || search.lost_focus()) && enter {
            if let Some(index) = self.hierarchy_search.visible.first() {
                self.queued_selection =
                    Some((Some(self.hierarchy_search.rows[*index].id.clone()), false));
                search.surrender_focus();
            } else {
                self.notice = "No objects match the search.".into();
            }
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Create cube").clicked() {
                self.queued_context = Some((None, ContextAction::Create));
            }
            if ui
                .add_enabled(
                    !self.manipulation.ids.is_empty(),
                    egui::Button::new("Duplicate"),
                )
                .clicked()
            {
                self.queued_context = Some((None, ContextAction::Duplicate));
            }
            if ui
                .add_enabled(
                    !self.manipulation.ids.is_empty(),
                    egui::Button::new("Delete"),
                )
                .clicked()
            {
                self.queued_context = Some((None, ContextAction::Delete));
            }
        });
        let hidden = self
            .manipulation
            .ids
            .iter()
            .filter(|id| {
                !self
                    .hierarchy_search
                    .visible
                    .iter()
                    .any(|i| &self.hierarchy_search.rows[*i].id == *id)
            })
            .count();
        ui.small(format!(
            "{} / {} objects · {} selected · {hidden} hidden selected",
            self.hierarchy_search.visible.len(),
            self.hierarchy_search.rows.len(),
            self.manipulation.ids.len()
        ));
        if !self.hierarchy_query.is_empty() && ui.button("Clear object search").clicked() {
            self.hierarchy_query.clear();
            self.hierarchy_search.update(self.session.scene(), "");
        }
        let mut clicked = None;
        let mut context = None;
        let mut dropped = None;
        egui::ScrollArea::vertical()
            .id_salt("scene_object_scroll")
            .show(ui, |ui| {
                for index in &self.hierarchy_search.visible {
                    let row = &self.hierarchy_search.rows[*index];
                    let response = ui
                        .add(
                            egui::Button::selectable(
                                self.manipulation.ids.contains(&row.id),
                                &row.name,
                            )
                            .sense(egui::Sense::click_and_drag()),
                        )
                        .on_hover_text(&row.tooltip);
                    if response.drag_started() {
                        response.dnd_set_drag_payload(crate::behavior_ui::ObjectDrag {
                            generation: self.generation,
                            revision: self.session.scene().revision,
                            id: row.id.clone(),
                        });
                    }
                    if response.clicked() {
                        clicked = Some((
                            Some(row.id.clone()),
                            ui.input(|i| {
                                i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift
                            }),
                        ));
                    }
                    response.context_menu(|ui| {
                        for (label, action) in [
                            ("Create cube", ContextAction::Create),
                            ("Rename object", ContextAction::Rename),
                            ("Duplicate", ContextAction::Duplicate),
                            ("Delete", ContextAction::Delete),
                            ("Copy objects", ContextAction::Copy),
                            ("Paste objects", ContextAction::Paste),
                            ("Frame selected", ContextAction::Frame),
                        ] {
                            if ui.button(label).clicked() {
                                context = Some((Some(row.id.clone()), action));
                                ui.close();
                            }
                        }
                    });
                    if response.dnd_hover_payload::<AssetDrag>().is_some() {
                        ui.painter().rect_stroke(
                            response.rect,
                            3.0,
                            egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if let Some(payload) = response.dnd_release_payload::<AssetDrag>() {
                        dropped = Some((row.id.clone(), (*payload).clone()));
                    }
                }
            });
        if let Some(intent) = clicked {
            self.queued_selection = Some(intent);
        }
        if let Some(intent) = context {
            self.queued_context = Some(intent);
        }
        if let Some(intent) = dropped {
            self.queued_drop = Some(intent);
        }
    }
    fn locate_mesh(&mut self, id: &str) {
        self.workspace.prefs.content_visible = true;
        self.workspace.changed();
        self.content_query.clear();
        self.content_selected = Some(id.into());
        self.content_scroll_to = true;
    }
    fn inspector_contents(&mut self, ui: &mut egui::Ui) {
        let title = if self.manipulation.ids.len() > 1 {
            format!("{} objects", self.manipulation.ids.len())
        } else {
            self.selected
                .as_ref()
                .and_then(|id| self.session.scene().objects.iter().find(|o| &o.id == id))
                .map(|object| object.display_name().to_owned())
                .unwrap_or_else(|| "Inspector".into())
        };
        ui.horizontal_wrapped(|ui| {
            ui.heading(title);
            ui.menu_button("Actions", |ui| {
                if ui
                    .add_enabled(self.selected.is_some(), egui::Button::new("Rename object"))
                    .clicked()
                {
                    match self.guard() {
                        Ok(()) => self.rename_pending = true,
                        Err(error) => self.error = error,
                    };
                    ui.close();
                }
                for (label, action) in [("Duplicate", "duplicate"), ("Delete", "delete")] {
                    if ui.button(label).clicked() {
                        self.object_action(action);
                        ui.close();
                    }
                }
                if ui.button("Copy objects").clicked() {
                    self.clipboard_action(false);
                    ui.close();
                }
                if ui.button("Paste objects").clicked() {
                    self.clipboard_action(true);
                    ui.close();
                }
            });
        });
        if self.selected.is_none() {
            ui.label("Select an object to edit its properties.");
            if self.manipulation.offset_pending() {
                ui.add_enabled_ui(!self.behavior.pending(), |ui| self.manipulation_ui(ui));
            }
            if self.behavior.pending() {
                let editable = self.behavior_editable();
                if let Some(action) = self.behavior.inspector(
                    ui,
                    &self.session,
                    None,
                    self.assets.as_ref(),
                    self.generation,
                    editable,
                ) {
                    self.behavior_action(action);
                }
            }
            return;
        }
        ui.separator();
        ui.strong("Transform");
        if self.draft != self.baseline_fields {
            ui.add_enabled_ui(!self.behavior.pending() && !self.manipulation.active() && self.dialog.is_none() && !self.close_approved && !self.rename_pending && !self.new_scene_pending && !self.close_pending && !self.workspace.settings_open && !self.recovery.open && self.recovery_transition.is_none(), |ui| {
                    ui.label("Recovered transform fields — repair or revert before other precision edits");
                    let mut enter=false;
                    let mut escape=false;
                    if let Some(fields)=&mut self.draft {
                        for (label, values) in [("Position", &mut fields.translation), (fields.rotation_unit.label(), &mut fields.rotation), ("Scale", &mut fields.scale)] {
                            ui.label(label);
                            ui.horizontal_wrapped(|ui| { for (axis,text) in ["X","Y","Z"].into_iter().zip(values) {
                                ui.label(axis);
                                let response=ui.add(egui::TextEdit::singleline(text).desired_width(70.0).char_limit(1024).interactive(ui.is_enabled()));
                                let owned=response.has_focus() || ui.memory(|m|m.had_focus_last_frame(response.id));
                                enter|=owned && ui.input(|i|i.key_pressed(egui::Key::Enter));
                                escape|=owned && ui.input(|i|i.key_pressed(egui::Key::Escape));
                            }});
                        }
                    }
                    if ui.button("Apply recovered transform fields").clicked() || enter {self.apply();}
                    if ui.button("Revert transform drafts").clicked() || escape {self.sync_draft();}
                });
        }
        ui.add_enabled_ui(self.draft == self.baseline_fields && !self.behavior.pending() && !self.manipulation.ids.is_empty(), |ui| {
                match self.manipulation.precision_ui(ui,&mut self.session,self.dialog.is_none()&&!self.close_approved&&!self.rename_pending&&!self.new_scene_pending&&!self.close_pending&&!self.workspace.settings_open && !self.recovery.open && self.recovery_transition.is_none()) {
                    Ok(true)=>{self.sync_draft();self.error.clear();self.notice="Edited transform components applied to the selection; one Undo restores them.".into();},
                    Ok(false)=>{},Err(error)=>self.error=error,
                }
            });
        ui.add_enabled_ui(!self.behavior.pending(), |ui| self.manipulation_ui(ui));
        ui.separator();
        if let Some(object) = self
            .selected
            .as_ref()
            .filter(|_| self.manipulation.ids.len() > 1)
            .and_then(|id| {
                self.session
                    .scene()
                    .objects
                    .iter()
                    .find(|object| &object.id == id)
            })
        {
            ui.label(format!("Active object: {}", object.display_name()));
        }
        ui.strong("Mesh");
        self.mesh_reference_ui(ui);
        let editable = self.behavior_editable();
        if let Some(action) = self.behavior.inspector(
            ui,
            &self.session,
            self.selected.as_deref(),
            self.assets.as_ref(),
            self.generation,
            editable,
        ) {
            self.behavior_action(action);
        }
        ui.separator();
        egui::CollapsingHeader::new("Details")
            .id_salt("object_details")
            .show(ui, |ui| {
                if let Some(id) = &self.selected {
                    ui.label(format!("Stable ID: {id}"));
                }
                if self.manipulation.ids.len() > 1 {
                    ui.label("The last selected object is active.");
                }
                ui.label(format!(
                    "Loaded scene format v{}; Save writes v{}.",
                    self.session.loaded_format_version(),
                    ruster_scene::FORMAT_VERSION
                ));
            });
    }
    fn mesh_reference_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let Some(mesh) = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == id)
            .map(|object| object.mesh.clone())
        else {
            return;
        };
        let response = egui::Frame::group(ui.style())
            .show(ui, |ui| {
                ui.label(mesh.as_deref().unwrap_or("Procedural cube"));
                if let Some(mesh) = &mesh {
                    if ui.button("Locate mesh in Content").clicked() {
                        self.locate_mesh(mesh);
                    }
                }
            })
            .response
            .on_hover_text(
                "Drop a project mesh from Content here to assign it to the active object.",
            );
        response.context_menu(|ui| {
            if ui.button("Use procedural cube").clicked() {
                match self.assign_mesh_to(&id, None) {
                    Ok(_) => self.error.clear(),
                    Err(error) => self.error = error,
                }
                ui.close();
            }
            if let Some(mesh) = &mesh {
                if ui.button("Locate mesh in Content").clicked() {
                    self.locate_mesh(mesh);
                    ui.close();
                }
            }
        });
        if response.dnd_hover_payload::<AssetDrag>().is_some() {
            ui.painter().rect_stroke(
                response.rect,
                3.0,
                egui::Stroke::new(2.0, ui.visuals().selection.bg_fill),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(payload) = response.dnd_release_payload::<AssetDrag>() {
            self.queued_drop = Some((id, (*payload).clone()));
        }
    }
    fn refresh_content_asset(&mut self, id: &str) {
        let result = self.guard().and_then(|()| {
            self.assets
                .as_mut()
                .ok_or_else(|| "Open a saved scene with a valid catalog first.".to_string())
                .and_then(|store| {
                    store
                        .refresh_at(store.revision(), id)
                        .map_err(|e| e.to_string())
                })
        });
        self.asset_snapshot();
        match result {
            Ok(info) => {
                self.error.clear();
                self.notice=format!("Refreshed {} in the immediately saved catalog; scene undo and preview are unchanged.",info.id);
            }
            Err(error) => self.error = error,
        }
    }
    fn browser_contents(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.heading("Content");
            if ui.button("Import / source tools…").clicked() {
                self.asset_dialog = true;
                self.asset_focused = false;
                self.asset_source_focus = false;
                self.asset_rename_intent = false;
            }
            ui.menu_button("Tools", |ui| self.shader_tools_ui(ui));
            if ui.button("Hide Content").clicked() {
                self.workspace.prefs.content_visible = false;
                self.workspace.changed();
            }
        });
        self.content_diagnostics_ui(ui);
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none());
        let escape = ui.input(|i| i.key_pressed(egui::Key::Escape) && i.modifiers.is_none());
        let search = ui.add(
            egui::TextEdit::singleline(&mut self.content_query)
                .hint_text("Search content (ID, source path, status)")
                .char_limit(128)
                .desired_width(f32::INFINITY),
        );
        if self.content_focus {
            search.request_focus();
            search.scroll_to_me(Some(egui::Align::Min));
            self.content_focus = false;
        }
        if (search.has_focus() || search.lost_focus()) && escape {
            self.content_query.clear();
            search.surrender_focus();
        }
        self.content_search
            .update(self.assets.as_ref(), &self.content_query);
        if (search.has_focus() || search.lost_focus()) && enter {
            if let Some(index) = self.content_search.visible.first() {
                self.content_selected = Some(self.content_search.rows[*index].info.id.clone());
                self.content_scroll_to = true;
                search.surrender_focus();
            }
        }
        if self.assets.is_none() {
            if let Some(root) = self.asset_origin.clone() {
                ui.label(format!("Recovered content origin: {}", root.display()));
                if ui.button("Retry recovered content").clicked() {
                    match AssetStore::open(&root) {
                        Ok(store) => { self.assets=Some(store); self.asset_snapshot(); self.error.clear(); }
                        Err(error) => self.error=format!("Cannot reopen {}: {error}. Restore the catalog at this original path; references will not be rebased.",root.display()),
                    }
                }
            }
            ui.label("Save or open a scene to use its project-relative content catalog.");
            return;
        }
        ui.small(format!(
            "{} / {} assets · drag a mesh onto an object or its inspector reference",
            self.content_search.visible.len(),
            self.content_search.rows.len()
        ));
        let mut selected = None;
        let mut assign = None;
        let mut refresh = None;
        let mut edit = None;
        let mut refs = None;
        egui::ScrollArea::vertical()
            .id_salt("content_assets_scroll")
            .show(ui, |ui| {
                for index in &self.content_search.visible {
                    let row = &self.content_search.rows[*index];
                    let available = self.resolved.contains_key(&row.info.id);
                    let response = ui.add(
                        egui::Button::selectable(
                            self.content_selected.as_ref() == Some(&row.info.id),
                            &row.label,
                        )
                        .sense(if available {
                            egui::Sense::click_and_drag()
                        } else {
                            egui::Sense::click()
                        }),
                    );
                    if response.clicked() {
                        selected = Some(row.info.id.clone());
                    }
                    if self.content_scroll_to
                        && self.content_selected.as_ref() == Some(&row.info.id)
                    {
                        response.scroll_to_me(Some(egui::Align::Center));
                    }
                    if response.drag_started() {
                        match self.asset_payload(&row.info.id) {
                            Ok(payload) => response.dnd_set_drag_payload(payload),
                            Err(error) => self.error = error,
                        }
                    }
                    response.context_menu(|ui| {
                        if ui
                            .add_enabled(
                                available && self.selected.is_some(),
                                egui::Button::new("Assign to active object"),
                            )
                            .clicked()
                        {
                            assign = Some(row.info.id.clone());
                            ui.close();
                        }
                        if ui.button("Refresh source").clicked() {
                            refresh = Some(row.info.id.clone());
                            ui.close();
                        }
                        if ui.button("Rename source…").clicked() {
                            edit = Some((row.info.id.clone(), row.info.source.clone()));
                            ui.close();
                        }
                        if ui.button("Select referencing objects").clicked() {
                            refs = Some(row.info.id.clone());
                            ui.close();
                        }
                    });
                    if let Some(error) = &row.info.diagnostic {
                        response.on_hover_text(error);
                    }
                }
            });
        self.content_scroll_to = false;
        if let Some(id) = selected {
            self.content_selected = Some(id);
        }
        if let Some(id) = assign {
            if let Some(target) = self.selected.clone() {
                match self.asset_payload(&id) {
                    Ok(payload) => self.queued_drop = Some((target, payload)),
                    Err(error) => self.error = error,
                }
            }
        }
        if let Some(id) = refresh {
            self.queued_refresh = Some(id);
        }
        if let Some((id, source)) = edit {
            self.asset_id = id;
            self.asset_source = source;
            self.asset_rename_intent = true;
            self.asset_source_focus = true;
            self.asset_focused = false;
            self.asset_dialog = true;
        }
        if let Some(id) = refs {
            self.queued_references = Some(id);
        }
        if let Some(id) = &self.content_selected {
            if let Some(row) = self
                .content_search
                .rows
                .iter()
                .find(|row| &row.info.id == id)
            {
                ui.separator();
                ui.label(&row.label);
                if let Some(error) = &row.info.diagnostic {
                    ui.colored_label(egui::Color32::YELLOW, error);
                }
                if self.resolved.contains_key(id)
                    && row.info.status != ruster_assets::AssetStatus::Ready
                {
                    ui.small(
                        "Last-good mesh remains available; fix and explicitly refresh the source.",
                    );
                }
                ui.small(
                    "Catalog Import/Refresh/Rename persists immediately, outside scene Undo/Save.",
                );
            } else {
                ui.label(format!("Reference {id} is not registered in this catalog; import or repair its source."));
            }
        }
    }

    fn content_diagnostics_ui(&mut self, ui: &mut egui::Ui) {
        let missing = self
            .session
            .scene()
            .objects
            .iter()
            .filter(|object| {
                object
                    .mesh
                    .as_ref()
                    .is_some_and(|id| !self.resolved.contains_key(id))
            })
            .count();
        if missing > 0 {
            ui.horizontal_wrapped(|ui| {
                        ui.colored_label(egui::Color32::YELLOW, format!("{missing} objects have unresolved mesh references; placeholder cubes shown."));
                        if ui.button("Locate missing content").clicked() {
                            let id = self.session.scene().objects.iter().filter_map(|o| o.mesh.as_ref()).find(|id| !self.resolved.contains_key(*id)).cloned();
                            if let Some(id) = id { self.locate_mesh(&id); }
                        }
                    });
        }
        egui::ScrollArea::vertical()
            .id_salt("content_diagnostics")
            .max_height(48.0)
            .show(ui, |ui| {
                for row in self
                    .content_search
                    .rows
                    .iter()
                    .filter(|row| row.info.diagnostic.is_some())
                    .take(3)
                {
                    if let Some(error) = &row.info.diagnostic {
                        ui.colored_label(egui::Color32::YELLOW, error);
                    }
                }
            });
    }
    fn content_ui(&mut self, ctx: &egui::Context) {
        if !self.asset_dialog {
            self.asset_focused = false;
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.asset_dialog = false;
            self.asset_focused = false;
            return;
        }
        self.content_search
            .update(self.assets.as_ref(), &self.content_query);
        egui::Modal::new(egui::Id::unique("asset_dialog")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 520.0));
            ui.heading(if self.asset_rename_intent { "Rename mesh source" } else { "Project mesh assets" });
            egui::ScrollArea::vertical().id_salt("asset_tools_body").max_height((ctx.content_rect().height() - 110.0).max(80.0)).show(ui, |ui| {
            ui.label("Imports and source renames save the asset catalog immediately. Scene Undo reverses assignments only.");
            ui.label("Stable asset ID");
            if self.asset_rename_intent {
                ui.label(&self.asset_id);
            } else {
                let field = ui.text_edit_singleline(&mut self.asset_id);
                if !self.asset_focused && !self.asset_source_focus {
                    field.request_focus(); self.asset_focused = true;
                }
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none());
            ui.label("Project-relative OBJ source / rename destination");
            let source = ui.text_edit_singleline(&mut self.asset_source);
            if !self.asset_focused && self.asset_source_focus {
                source.request_focus(); source.scroll_to_me(Some(egui::Align::Center)); self.asset_focused = true;
            }
            let mut operation = None;
            if self.asset_rename_intent && enter && (source.has_focus() || source.lost_focus()) {
                operation = Some("rename");
            }
            ui.horizontal_wrapped(|ui| {
                if !self.asset_rename_intent {
                    if ui.button("Import mesh asset").clicked() { operation = Some("import"); }
                    if ui.button("Refresh mesh asset").clicked() { operation = Some("refresh"); }
                }
                if ui.button("Rename mesh source").clicked() { operation = Some("rename"); }
            });
            if let Some(op) = operation {
                if self.drafts() || self.manipulation.active() {
                    self.error = "Apply/revert transform drafts and finish gestures before asset operations.".into();
                } else {
                    let id = self.asset_id.clone(); let source = self.asset_source.clone();
                    let result = self.assets.as_mut().ok_or_else(|| "Asset catalog unavailable; reopen a saved project".to_string()).and_then(|store| {
                        match op { "import" => store.import(&id, &source), "refresh" => store.refresh(&id), _ => store.rename(&id, &source) }.map_err(|e| e.to_string())
                    });
                    self.asset_snapshot();
                    self.content_search.update(self.assets.as_ref(), &self.content_query);
                    match result {
                        Ok(info) => {
                            self.notice = format!("Asset {} ready; catalog saved, preview unchanged until Reset.", info.id);
                            self.error.clear();
                            if self.asset_rename_intent { self.asset_dialog = false; self.asset_focused = false; }
                        }
                        Err(error) => self.error = error,
                    }
                }
            }
            if let Some(store) = &self.assets {
                ui.label(format!("Catalog revision {}", store.revision()));
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height() * 0.25).min(180.0)).show(ui, |ui| {
                    for row in &self.content_search.rows {
                        ui.label(&row.label);
                        if let Some(error) = &row.info.diagnostic { ui.colored_label(egui::Color32::LIGHT_RED, error); }
                    }
                });
            }
            if !self.error.is_empty() { ui.add(egui::Label::new(egui::RichText::new(&self.error).color(egui::Color32::LIGHT_RED)).truncate()).on_hover_text(&self.error); }
            });
            if ui.button("Close assets").clicked() { self.asset_dialog = false; self.asset_focused = false; }
        });
    }
    fn rename_dialog(&mut self, ctx: &egui::Context) {
        if !self.rename_pending {
            self.rename_focused = false;
            return;
        }
        egui::Modal::new(egui::Id::unique("rename_object_dialog")).show(ctx, |ui| {
            ui.heading("Rename object");
            ui.label("Display name; the stable object ID stays unchanged.");
            let enter_pressed =
                ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none());
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape) && i.modifiers.is_none());
            let edit = ui.add(egui::TextEdit::singleline(&mut self.name_text).char_limit(128));
            if edit.changed() {
                self.draft_generation = self.draft_generation.wrapping_add(1);
            }
            if !self.rename_focused {
                edit.request_focus();
                self.rename_focused = true;
            }
            let enter = (edit.has_focus() || edit.lost_focus()) && enter_pressed;
            if ui.button("Apply name").clicked() || enter {
                self.rename_pending = false;
                self.object_action("rename");
                if !self.error.is_empty() {
                    self.rename_pending = true;
                }
            }
            if ui.button("Cancel rename").clicked() || escape {
                self.rename_pending = false;
                self.sync_draft();
            }
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
            }
        });
        if !self.rename_pending {
            self.rename_focused = false;
        }
    }
    fn observe_history_evictions(&mut self) -> bool {
        let count = self.session.history_evictions();
        let removed = count.saturating_sub(self.seen_history_evictions);
        self.seen_history_evictions = count;
        if removed == 0 {
            return false;
        }
        self.notice.push_str(&format!(
            " Undo budget: {removed} older step(s) removed; {} pressure removals in this session.",
            count
        ));
        true
    }
    fn needs_recovery(&self) -> bool {
        self.drafts()
            || self.manipulation.offset_pending()
            || (self.session.is_dirty()
                && !(self.session.path().is_none()
                    && self.session.scene().objects.is_empty()
                    && self.session.authoring().is_empty()
                    && self.session.undo_len() == 0
                    && self.session.redo_len() == 0))
    }
    fn request_close(&mut self) {
        self.runtime
            .stop(self.generation, self.session.scene().revision);
        if self.recovery_transition.is_some() {
            self.error =
                "Recovery cleanup is still finishing; retry Close once it completes.".into();
            return;
        }
        self.manipulation.cancel();
        self.remember_camera();
        self.workspace.flush();
        if self.recovery.open {
            self.recovery.open = false;
        }
        if self.new_scene_pending {
            self.error = "Resolve or cancel New scene before closing.".into();
        } else if self.rename_pending {
            self.error = "Apply or cancel the name edit before closing.".into();
        } else if self.needs_recovery() {
            self.close_pending = true;
        } else {
            self.close_approved = true;
        }
    }
    fn request_new(&mut self) {
        if self.needs_recovery() {
            self.new_scene_pending = true;
        } else {
            self.replace_with_empty();
        }
    }
    fn route_shortcuts(&mut self, ctx: &egui::Context) {
        let blocked = self.manipulation.active()
            || self.dialog.is_some()
            || self.close_pending
            || self.close_approved
            || self.rename_pending
            || self.new_scene_pending
            || self.asset_dialog
            || self.recovery.open
            || self.recovery_transition.is_some()
            || ctx.input(|i| i.pointer.button_down(egui::PointerButton::Secondary));
        let Some(action) = self.workspace.shortcuts(ctx, blocked) else {
            return;
        };
        self.queued_action = Some(action);
    }
    fn process_queued_action(&mut self) {
        let Some(action) = self.queued_action.take() else {
            return;
        };
        if self.close_approved
            || self.workspace.settings_open
            || self.recovery.open
            || self.recovery_transition.is_some()
            || self.rename_pending
            || self.asset_dialog
            || self.close_pending
            || self.dialog.is_some()
            || self.new_scene_pending
        {
            return;
        }
        match action {
            Action::New => self.request_new(),
            Action::Quit => self.request_close(),
            Action::Open => self.dialog = Some(true),
            Action::Save => self.save(),
            Action::SaveAs => {
                if self.apply() {
                    self.dialog = Some(false);
                }
            }
            Action::Undo => self.history(false),
            Action::Redo => self.history(true),
            Action::Copy => self.clipboard_action(false),
            Action::Paste => self.clipboard_action(true),
            Action::Duplicate => self.object_action("duplicate"),
            Action::Delete => self.object_action("delete"),
            Action::Move | Action::Rotate | Action::Scale => {
                if self.guard().is_ok() {
                    self.manipulation.tool = match action {
                        Action::Move => crate::manipulation::Tool::Translate,
                        Action::Rotate => crate::manipulation::Tool::Rotate,
                        _ => crate::manipulation::Tool::Scale,
                    };
                }
            }
            Action::FindObjects => {
                self.workspace.prefs.hierarchy_visible = true;
                self.workspace.changed();
                self.hierarchy_focus = true;
            }
            Action::FindContent => {
                self.workspace.prefs.content_visible = true;
                self.workspace.changed();
                self.content_focus = true;
            }
            Action::Rename => {
                if self.guard().is_ok() && self.selected.is_some() {
                    self.rename_pending = true;
                }
            }
            Action::SelectAll => {
                if self.manipulation.active() {
                    return;
                }
                if self.drafts() && !self.apply() {
                    return;
                }
                self.manipulation.ids = self
                    .session
                    .scene()
                    .objects
                    .iter()
                    .map(|object| object.id.clone())
                    .collect();
                self.selected = self.manipulation.selected();
                self.sync_draft();
            }
            Action::Frame => {
                if self.guard().is_ok() {
                    self.navigation.frame_selected = true;
                }
            }
            Action::Projection => {
                if self.guard().is_ok() {
                    self.navigation.toggle_projection = true;
                }
            }
            Action::Front
            | Action::Back
            | Action::Left
            | Action::Right
            | Action::Top
            | Action::Bottom
            | Action::Isometric
            | Action::FreeView => {
                if self.guard().is_ok() {
                    self.navigation.view_preset = Some(match action {
                        Action::Front => ruster_render::ViewPreset::Front,
                        Action::Back => ruster_render::ViewPreset::Back,
                        Action::Left => ruster_render::ViewPreset::Left,
                        Action::Right => ruster_render::ViewPreset::Right,
                        Action::Top => ruster_render::ViewPreset::Top,
                        Action::Bottom => ruster_render::ViewPreset::Bottom,
                        Action::Isometric => ruster_render::ViewPreset::Isometric,
                        _ => ruster_render::ViewPreset::Free,
                    });
                }
            }
        }
    }
    fn command_menu_item(&mut self, ui: &mut egui::Ui, label: &str, action: Action) {
        if ui
            .add(egui::Button::new(label).shortcut_text(self.workspace.shortcut_text(action)))
            .clicked()
        {
            self.queued_action = Some(action);
            ui.close();
        }
    }
    fn shader_tools_ui(&mut self, ui: &mut egui::Ui) {
        if ui.button("Create shader source").clicked() {
            use std::io::Write;
            let result = self
                .assets
                .as_ref()
                .ok_or_else(|| "Open a saved project first".to_string())
                .and_then(|store| {
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(store.root().join("scene.wgsl"))
                        .and_then(|mut file| {
                            file.write_all(ruster_render::default_shader().as_bytes())
                        })
                        .map_err(|error| error.to_string())
                });
            match result {
                Ok(()) => {
                    self.notice = "Created scene.wgsl; edit it, then Refresh scene shader.".into()
                }
                Err(error) => self.error = error,
            }
            ui.close();
        }
        if ui.button("Refresh scene shader").clicked() {
            if self.drafts() || self.manipulation.active() {
                self.error =
                    "Apply/revert transform drafts and finish the gesture before shader refresh."
                        .into();
            } else if let Err(error) = self.refresh_shader() {
                self.shader_notice = format!(
                    "Shader rejected; last good pipeline retained: {}",
                    error.chars().take(256).collect::<String>()
                );
                self.error = self.shader_notice.clone();
            }
            ui.close();
        }
        ui.label(&self.shader_notice);
    }
    fn status_problem_ui(ui: &mut egui::Ui, error: &str, shader: Option<&str>) {
        let primary = if error.is_empty() {
            shader
        } else {
            Some(error)
        };
        let Some(primary) = primary else {
            return;
        };
        let secondary = shader.filter(|message| *message != primary);
        let summary = primary
            .lines()
            .find(|line| !line.trim().is_empty())
            .unwrap_or("Operation failed")
            .trim();
        // Allocate the action first from the right so even a long summary
        // cannot displace access to the retained diagnostic.
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), ui.spacing().interact_size.y),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.menu_button("Details", |ui| {
                    ui.set_max_width((ui.ctx().content_rect().width() - 48.0).clamp(160.0, 600.0));
                    egui::ScrollArea::vertical()
                        .id_salt("status_problem_details")
                        .max_height((ui.ctx().content_rect().height() * 0.5).clamp(96.0, 320.0))
                        .show(ui, |ui| {
                            ui.add(egui::Label::new(primary).wrap());
                            if let Some(secondary) = secondary {
                                ui.separator();
                                ui.add(egui::Label::new(secondary).wrap());
                            }
                        });
                });
                ui.add(
                    egui::Label::new(egui::RichText::new(summary).color(egui::Color32::LIGHT_RED))
                        .truncate(),
                );
            },
        );
        if let Some(secondary) = secondary {
            let summary = secondary
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Operation failed")
                .trim();
            ui.add(
                egui::Label::new(egui::RichText::new(summary).color(egui::Color32::YELLOW))
                    .truncate(),
            );
        }
    }
    fn scene_menus(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.menu_button("File", |ui| {
                for (label, action) in [
                    ("New scene", Action::New),
                    ("Open scene…", Action::Open),
                    ("Save", Action::Save),
                    ("Save as…", Action::SaveAs),
                ] {
                    self.command_menu_item(ui, label, action);
                }
                ui.menu_button("Recent scenes", |ui| {
                    if self.workspace.prefs.recent.is_empty() {
                        ui.label("No recent scenes.");
                    }
                    for path in self.workspace.prefs.recent.clone() {
                        if ui.button(path.display().to_string()).clicked() {
                            self.path_text = path.display().to_string();
                            self.queued_action = Some(Action::Open);
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button("Recovery checkpoints…").clicked() {
                    self.recovery.show_picker();
                    self.manipulation.cancel();
                    ui.close();
                }
                if ui.button("Workspace and shortcuts…").clicked() {
                    self.workspace.settings_open = true;
                    ui.close();
                }
                self.command_menu_item(ui, "Close window", Action::Quit);
            });
            ui.menu_button("Edit", |ui| {
                for action in [
                    Action::Undo,
                    Action::Redo,
                    Action::Copy,
                    Action::Paste,
                    Action::Duplicate,
                    Action::Delete,
                    Action::Rename,
                    Action::SelectAll,
                ] {
                    self.command_menu_item(ui, action.name(), action);
                }
                ui.menu_button("Tools", |ui| {
                    for action in [Action::Move, Action::Rotate, Action::Scale] {
                        self.command_menu_item(ui, action.name(), action);
                    }
                });
                ui.separator();
                if ui.button("Create cube").clicked() {
                    self.queued_context = Some((None, ContextAction::Create));
                    ui.close();
                }
                ui.menu_button("Assign mesh…", |ui| {
                    if ui.button("Procedural cube").clicked() {
                        if let Err(error) = self.assign_mesh(None) {
                            self.error = error;
                        }
                        ui.close();
                    }
                    for id in self
                        .assets
                        .as_ref()
                        .map(|store| {
                            store
                                .inspect()
                                .into_iter()
                                .map(|asset| asset.id)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default()
                    {
                        if ui.button(&id).clicked() {
                            if let Err(error) = self.assign_mesh(Some(id.clone())) {
                                self.error = error;
                            }
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button("View", |ui| {
                let mut visibility_changed = false;
                for (label, visible) in [
                    ("Objects", &mut self.workspace.prefs.hierarchy_visible),
                    ("Inspector", &mut self.workspace.prefs.inspector_visible),
                    ("Content browser", &mut self.workspace.prefs.content_visible),
                ] {
                    visibility_changed |= ui.checkbox(visible, label).changed();
                }
                if visibility_changed {
                    self.workspace.changed();
                }
                ui.separator();
                for action in [
                    Action::Frame,
                    Action::Projection,
                    Action::FindObjects,
                    Action::FindContent,
                ] {
                    self.command_menu_item(ui, action.name(), action);
                }
                ui.menu_button("View presets", |ui| {
                    for action in [
                        Action::Front,
                        Action::Back,
                        Action::Left,
                        Action::Right,
                        Action::Top,
                        Action::Bottom,
                        Action::Isometric,
                        Action::FreeView,
                    ] {
                        self.command_menu_item(ui, action.name(), action);
                    }
                });
                if ui.button("Reset camera").clicked() {
                    self.navigation.reset = true;
                    ui.close();
                }
            });
            ui.menu_button("Content", |ui| {
                if ui.button("Assets / import…").clicked() {
                    self.asset_dialog = true;
                    self.asset_rename_intent = false;
                    self.asset_source_focus = false;
                    self.asset_focused = false;
                    ui.close();
                }
                self.shader_tools_ui(ui);
            });
            ui.separator();
            let dirty = self.session.is_dirty() || self.pending_recovery_drafts();
            let mode = if self.runtime.frame.is_some() {
                if self.runtime.visible {
                    if self.runtime.stale {
                        "Runtime (stale) · "
                    } else {
                        "Runtime · "
                    }
                } else if self.runtime.stale {
                    "Authored / stale runtime · "
                } else {
                    "Authored · "
                }
            } else {
                ""
            };
            let title = format!(
                "{}{}{}",
                if dirty { "* " } else { "" },
                mode,
                self.session.scene().id
            );
            let width = (ui.available_width() - 64.0).clamp(48.0, 240.0);
            ui.add_sized(
                [width, ui.spacing().interact_size.y],
                egui::Label::new(&title).truncate(),
            )
            .on_hover_text(format!(
                "{} · {}",
                self.session.scene().id,
                if dirty { "Unsaved changes" } else { "Saved" }
            ));
            if ui
                .button("Save")
                .on_hover_text(self.workspace.shortcut_text(Action::Save))
                .clicked()
            {
                self.queued_action = Some(Action::Save);
            }
        });
    }
    fn scene_sidebars(&mut self, ui: &mut egui::Ui) {
        let (constrained, hierarchy_width, inspector_width) = sidebar_budget(
            ui.available_width(),
            self.workspace
                .prefs
                .hierarchy_visible
                .then_some(self.workspace.prefs.hierarchy_width),
            self.workspace
                .prefs
                .inspector_visible
                .then_some(self.workspace.prefs.inspector_width),
        );
        if self.workspace.prefs.hierarchy_visible {
            let panel = match (constrained, self.workspace.prefs.hierarchy_side) {
                (true, Side::Left) => egui::Panel::left("scene_hierarchy_constrained"),
                (true, Side::Right) => egui::Panel::right("scene_hierarchy_constrained"),
                (false, Side::Left) => {
                    egui::Panel::left(("scene_hierarchy", self.workspace.layout_generation))
                }
                (false, Side::Right) => {
                    egui::Panel::right(("scene_hierarchy", self.workspace.layout_generation))
                }
            };
            let panel = if constrained {
                panel.resizable(false).exact_size(hierarchy_width)
            } else {
                panel
                    .default_size(hierarchy_width)
                    .resizable(true)
                    .size_range(120.0..=800.0)
            };
            let output = panel.show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("hierarchy_panel_scroll")
                    .show(ui, |ui| self.hierarchy_contents(ui));
            });
            let width = output.response.rect.width().clamp(120.0, 800.0);
            if !constrained && (width - self.workspace.prefs.hierarchy_width).abs() > 0.5 {
                self.workspace.prefs.hierarchy_width = width;
                self.workspace.changed();
            }
        }
        if self.workspace.prefs.inspector_visible {
            let panel = match (constrained, self.workspace.prefs.inspector_side) {
                (true, Side::Left) => egui::Panel::left("scene_inspector_constrained"),
                (true, Side::Right) => egui::Panel::right("scene_inspector_constrained"),
                (false, Side::Left) => {
                    egui::Panel::left(("scene_inspector", self.workspace.layout_generation))
                }
                (false, Side::Right) => {
                    egui::Panel::right(("scene_inspector", self.workspace.layout_generation))
                }
            };
            let panel = if constrained {
                panel.resizable(false).exact_size(inspector_width)
            } else {
                panel
                    .default_size(inspector_width)
                    .resizable(true)
                    .size_range(240.0..=1000.0)
            };
            let output = panel.show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.inspector_contents(ui));
            });
            let width = output.response.rect.width().clamp(240.0, 1000.0);
            if !constrained && (width - self.workspace.prefs.inspector_width).abs() > 0.5 {
                self.workspace.prefs.inspector_width = width;
                self.workspace.changed();
            }
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let focused = ctx.input(|input| input.focused);
        if focused && !self.external_focused {
            self.external.focus_returned();
        }
        self.external_focused = focused;
        self.recovery.poll(&ctx);
        self.process_recovery_ack();
        self.poll_runtime(&ctx);
        self.content_search
            .update(self.assets.as_ref(), &self.content_query);
        if self.scale_applied != self.workspace.prefs.scale {
            ctx.set_zoom_factor(self.workspace.prefs.scale);
            self.scale_applied = self.workspace.prefs.scale;
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_ready_to_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.close_approved {
                self.request_close();
            }
        }
        if self.manipulation.active()
            && (ctx.input(|i| !i.focused || i.key_pressed(egui::Key::Escape))
                || self.dialog.is_some()
                || self.asset_dialog
                || self.close_pending
                || self.close_approved
                || self.new_scene_pending
                || self.rename_pending
                || self.recovery.open)
        {
            self.manipulation.cancel();
            self.notice = "Transform cancelled; authored state unchanged.".into();
        }
        self.route_shortcuts(&ctx);
        egui::Panel::top("scene_top").show(ui, |ui| self.scene_menus(ui));
        egui::Panel::bottom("scene_status").show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("global_status_scroll")
                .max_height(96.0)
                .show(ui, |ui| {
                    let shader = (self.shader_notice.contains("rejected")
                        || self.shader_notice.contains("unavailable"))
                    .then_some(self.shader_notice.as_str());
                    Self::status_problem_ui(ui, &self.error, shader);
                    self.recovery.warning_ui(ui);
                    if !self.workspace.error.is_empty() {
                        ui.colored_label(egui::Color32::YELLOW, &self.workspace.error);
                        if ui.button("Recover workspace preferences…").clicked() {
                            self.workspace.settings_open = true;
                        }
                    }

                    if !self.runtime.status.is_empty() {
                        ui.add(egui::Label::new(&self.runtime.status).truncate())
                            .on_hover_text(&self.runtime.status);
                    }
                    if !self.notice.is_empty() {
                        ui.add(egui::Label::new(&self.notice).truncate())
                            .on_hover_text(&self.notice);
                    }
                    if !self.external.status.is_empty() {
                        ui.add(egui::Label::new(&self.external.status).truncate())
                            .on_hover_text(&self.external.status);
                        if ui.button("Working-copy details").clicked() {
                            self.external.open = true;
                        }
                    }
                });
        });
        if self.workspace.prefs.content_visible {
            let remaining = ui.available_height();
            // Reserve 160 points for compact tools/runtime/view chrome and
            // about 180 for the viewport, after global status/menu bars.
            let max_height = (remaining - 340.0).clamp(32.0, 200.0);
            let default_height = (ctx.content_rect().height() * 0.25).clamp(80.0, 200.0);
            let preferred_height = egui::containers::panel::PanelState::load(
                &ctx,
                ui.scope_id().with("content_browser"),
            )
            .map(|state| state.size().y)
            .unwrap_or(default_height);
            // Never let a temporary cap overwrite the normal panel's remembered
            // divider height. The constrained instance has one stable identity.
            let panel = if preferred_height > max_height {
                egui::Panel::bottom("content_browser_constrained")
                    .resizable(false)
                    .exact_size(max_height)
            } else {
                egui::Panel::bottom("content_browser")
                    .default_size(preferred_height)
                    .resizable(true)
                    .size_range(32.0..=max_height)
            };
            panel.show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("content_panel_scroll")
                    .min_scrolled_height(0.0)
                    .max_height(ui.available_height())
                    .show(ui, |ui| self.browser_contents(ui));
            });
        }
        self.scene_sidebars(ui);
        self.process_queued_action();
        self.process_queued_selection();
        self.process_content_intents();
        self.runtime.observe(
            self.generation,
            self.session.scene().revision,
            self.assets.as_ref().map(|store| store.revision()),
            self.shader_revision,
        );
        egui::CentralPanel::default().show(ui, |ui| {
            if self.workspace.welcome && self.session.scene().objects.is_empty() {
                ui.heading("Start creating");ui.label("Create a cube in the hierarchy, open a scene, or choose a recent scene. Your last project is never opened automatically.");
                ui.horizontal(|ui| {if ui.button("Open a scene…").clicked(){self.dialog=Some(true);}
                if ui.button("Workspace settings…").clicked(){self.workspace.settings_open=true;}});
            }
            self.manipulation_tools_ui(ui);
            let blocked=self.guard().is_err() || self.manipulation.offset_pending();
            if let Some(key)=self.runtime.controls(ui,&self.session,self.generation,blocked) {self.reset_assets=Some((key,self.resolved.clone()));}
            if let Some(diagnostic)=&self.runtime.diagnostic {
                ui.horizontal(|ui| {
                    if ui.button("Open location").clicked() {if let Err(error)=self.behavior.navigate_diagnostic(&self.session,diagnostic,self.runtime.captured.as_deref()){self.error=error;}}
                    ui.add(egui::Label::new(egui::RichText::new(format!("{}: {}",if self.runtime.stale {"Stale runtime"}else{"Runtime error"},diagnostic.message)).color(egui::Color32::LIGHT_RED)).truncate()).on_hover_text(&diagnostic.message);
                });
            }
            self.runtime.observe(self.generation,self.session.scene().revision,self.assets.as_ref().map(|store|store.revision()),self.shader_revision);
            let transient = if !self.runtime.visible {
                self.manipulation.transient_scene(self.session.scene())
            } else {
                None
            };
            let selected: Vec<_> = self.manipulation.ids.iter().map(String::as_str).collect();
            self.navigation.enabled=!self.close_approved && !self.manipulation.active() && !self.workspace.settings_open && !self.recovery.open && self.recovery_transition.is_none() && !self.asset_dialog && self.dialog.is_none() && !self.rename_pending && !self.new_scene_pending && !self.close_pending;
            self.navigation.keyboard_enabled=self.navigation.enabled && !ctx.egui_wants_keyboard_input() && !self.drafts();
            self.navigation.movement_speed=self.workspace.prefs.movement_speed;
            let viewport = ruster_render::viewport_with_navigation(
                ui,
                if self.runtime.visible {
                    self.runtime.frame.as_ref().map(|frame|frame.scene.as_ref()).unwrap_or(self.session.scene())
                } else {
                    transient.as_ref().unwrap_or(self.session.scene())
                },
                &selected,
                if self.runtime.visible {
                    &self.preview_assets
                } else {
                    &self.resolved
                },
                &mut self.navigation,
            );
            let may_draw = !self.runtime.visible
                && !self.drafts()
                && self.dialog.is_none()
                && !self.asset_dialog
                && !self.close_pending
                && !self.close_approved
                && !self.new_scene_pending
                && !self.rename_pending && !self.workspace.settings_open && !self.recovery.open && self.recovery_transition.is_none();
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
            if viewport.response.clicked() && !overlay_owns && !self.manipulation.active() && !self.workspace.settings_open && !self.recovery.open && self.recovery_transition.is_none() && !self.asset_dialog && self.dialog.is_none() && !self.rename_pending && !self.new_scene_pending && !self.close_pending {
                let id = viewport
                    .picked
                    .filter(|id| self.session.scene().objects.iter().any(|o| &o.id == id));
                let additive =
                    ui.input(|i| i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift);
                self.queued_selection=Some((id,additive));
            }
        });
        self.process_queued_selection();
        self.workspace.settings_ui(&ctx);
        if ctx.input(|i| i.pointer.any_released()) {
            self.remember_camera();
            self.workspace.flush();
        }
        self.rename_dialog(&ctx);
        if self.dialog.is_none() {
            self.path_focused = false;
        }
        if !self.new_scene_pending && !self.close_pending {
            self.confirm_focused = None;
        }
        if self.new_scene_pending && self.dialog.is_none() {
            egui::Modal::new(egui::Id::unique("new_scene_dialog")).show(&ctx, |ui| {
                ui.heading("New scene — unsaved work");
                ui.label("Save your current scene, discard it, or cancel.");
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                    self.new_scene_pending = false;
                    self.confirm_focused = None;
                    return;
                }
                let save = ui.button("Save before new scene");
                if self.confirm_focused != Some("new") {
                    save.request_focus();
                    self.confirm_focused = Some("new");
                }
                if save.clicked() {
                    self.save();
                    if !self.session.is_dirty() && !self.pending_recovery_drafts() {
                        self.replace_with_empty();
                    }
                }
                if ui.button("Discard and create new scene").clicked() {
                    self.replace_with_empty();
                }
                if ui.button("Cancel new scene").clicked() {
                    self.new_scene_pending = false;
                }
                if !self.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
                }
            });
        }
        if let Some(open) = self.dialog {
            egui::Modal::new(egui::Id::unique("scene_path_dialog")).show(&ctx,|ui| {
                ui.heading(if open {"Open scene"}else{"Save scene as"});
                self.confirm_focused=None;
                ui.label("Absolute scene path");let enter_pressed=ui.input(|i|i.key_pressed(egui::Key::Enter)&&i.modifiers.is_none());let cancel=ui.input(|i|i.key_pressed(egui::Key::Escape)&&i.modifiers.is_none());let edit=ui.text_edit_singleline(&mut self.path_text);
                if !self.path_focused {edit.request_focus();self.path_focused=true;}
                let submit=(edit.has_focus()||edit.lost_focus())&&enter_pressed;
                if open && (self.session.is_dirty()||self.drafts()) {
                    ui.colored_label(egui::Color32::YELLOW,"Opening discards current unsaved scene and drafts. Cancel to preserve them.");
                }
                ui.horizontal(|ui| {
                    if ui.button(if open {"Open and discard current"}else{"Save to new path"}).clicked()||submit {
                        let path=PathBuf::from(self.path_text.trim());
                        if open {
                            match Session::open(path) {
                                Ok(session)=> {
                                    let render=self.render.clone();
                                    let reset=if self.shader_revision>0 {render.as_ref().map(|r|ruster_render::refresh_shader(r,ruster_render::default_shader())).unwrap_or(Ok(()))} else {Ok(())};
                                    match reset {Ok(())=>{let mut next=Self::with_workspace(session,Workspace::detached());next.render=render;next.workspace.welcome=false;self.begin_replacement(next,None,true,false);},Err(error)=>self.error=error}
                                },
                                Err(error)=>self.error=error.to_string()
                            }
                        } else {
                            match self.save_scene(path) {
                                Ok(())=>{self.dialog=None;if self.close_pending && !self.pending_recovery_drafts() {self.close_approved=true;}self.notice="Scene saved.".into();if self.new_scene_pending && !self.pending_recovery_drafts() {self.replace_with_empty();}},
                                Err(e)=>self.error=e.to_string()
                            }
                        }
                    }
                    if ui.button("Cancel scene dialog").clicked()||cancel {self.dialog=None;self.path_focused=false;}
                });
                if !self.error.is_empty(){ui.colored_label(egui::Color32::LIGHT_RED,&self.error);}
            });
        }
        self.content_ui(&ctx);
        let editable = self.behavior_editable();
        if let Some(action) = self.behavior.source_window(&ctx, editable) {
            self.behavior_action(action);
        }
        self.behavior.captured_window(&ctx, editable);
        let blocked = self.guard().is_err() || self.manipulation.offset_pending();
        if let Some(diagnostic) = self.runtime.debug_window(&ctx, &self.session, blocked) {
            if let Err(error) = self.behavior.navigate_diagnostic(
                &self.session,
                &diagnostic,
                self.runtime.captured.as_deref(),
            ) {
                self.error = error;
            }
        }
        if self.close_pending && self.dialog.is_none() && !self.asset_dialog && !self.rename_pending
        {
            egui::Modal::new(egui::Id::unique("scene_close_dialog")).show(&ctx, |ui| {
                ui.label("Save, discard or cancel before closing.");
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                    self.close_pending = false;
                    self.confirm_focused = None;
                    return;
                }
                let save = ui.button("Save and close scene");
                if self.confirm_focused != Some("close") {
                    save.request_focus();
                    self.confirm_focused = Some("close");
                }
                if save.clicked() {
                    self.save();
                    if !self.session.is_dirty() && !self.pending_recovery_drafts() {
                        self.close_approved = true;
                    }
                }
                if ui.button("Discard and close scene").clicked() {
                    self.close_approved = true;
                }
                if ui.button("Cancel closing scene").clicked() {
                    self.close_pending = false;
                }
                if !self.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
                }
            });
        }
        if !self.close_approved {
            self.recovery_frame(&ctx);
        }
        self.external_frame(&ctx);
        if self.hierarchy_focus || self.content_focus {
            ctx.request_repaint();
        }
        if self.observe_history_evictions() {
            ctx.request_repaint();
        }
        self.close_finalization_ui(&ctx);
        self.start_close_cleanup();
        if self.close_shutdown {
            if self.recovery.shutdown_finished()
                && self.runtime.finished()
                && self.external.finished()
            {
                self.close_ready_to_exit = true;
                self.remember_camera();
                self.workspace.flush();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                ctx.request_repaint_after(Duration::from_millis(16));
            }
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
pub fn run_empty() -> Result<(), String> {
    run_session(Session::new(Scene::empty("untitled")).map_err(|e| e.to_string())?)
}
pub fn run(path: PathBuf) -> Result<(), String> {
    let session = Session::open(path).map_err(|e| e.to_string())?;
    run_session(session)
}
fn run_session(session: Session) -> Result<(), String> {
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
            crate::editor_theme::configure(&cc.egui_ctx);
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
                    if let Some(path) = editor.session.path() {
                        editor.workspace.remember(path);
                        editor.workspace.welcome = false;
                    }
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
    // A small window must constrain only its temporary panel instances, not
    // erase preferred widths or leave the actual viewport behind the chrome.
    #[test]
    fn compact_layout_preserves_viewport_and_restores_preferred_panels() {
        let project = tempfile::tempdir().unwrap();
        let mut store = AssetStore::open(project.path()).unwrap();
        for (id, path) in [
            ("triangle", "triangle.obj"),
            ("triangle_2", "triangle_2.obj"),
            ("triangle_3", "triangle_3.obj"),
        ] {
            std::fs::write(
                project.path().join(path),
                "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
            )
            .unwrap();
            store.import(id, path).unwrap();
        }
        drop(store);
        for swapped in [false, true] {
            let ctx = egui::Context::default();
            crate::editor_theme::configure(&ctx);
            let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
            editor.assets = Some(AssetStore::open(project.path()).unwrap());
            editor.asset_snapshot();
            editor.workspace.prefs.hierarchy_width = 320.0;
            editor.workspace.prefs.inspector_width = 400.0;
            editor.workspace.prefs.hierarchy_side = if swapped { Side::Right } else { Side::Left };
            editor.workspace.prefs.inspector_side = if swapped { Side::Left } else { Side::Right };
            let preferred = (320.0, 400.0);
            let events = std::cell::RefCell::new(Vec::new());
            let details_center = std::cell::Cell::new(None);
            let full_problem_count = std::cell::Cell::new(0);
            let content_rect = std::cell::Cell::new(None);
            let asset_row_visible = std::cell::Cell::new(false);
            let mut time = 0.0;
            let mut frame = |editor: &mut SceneEditor, size| {
                time += 0.1;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        events: std::mem::take(&mut *events.borrow_mut()),
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..Default::default()
                    },
                    |ui| {
                        editor.ui(ui);
                        content_rect.set(
                            egui::containers::panel::PanelState::load(
                                ui.ctx(),
                                ui.scope_id().with("content_browser"),
                            )
                            .map(|state| state.outer_rect),
                        );
                    },
                );
                output.textures_delta.clear();
                let viewport = output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Callback(callback) => {
                        Some(callback.rect.intersect(shape.clip_rect))
                    }
                    _ => None,
                });
                if editor.runtime.frame.is_some() {
                    let mode = if editor.runtime.visible {
                        "Runtime"
                    } else {
                        "Authored"
                    };
                    assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::epaint::Shape::Text(text) if text.galley.job.text.contains(mode) && text.galley.job.text.contains(&editor.session.scene().id))),"rendered scene label must identify the displayed view");
                }
                details_center.set(output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text)
                        if text.galley.job.text == "Details"
                            && text.pos.y > ctx.content_rect().center().y =>
                    {
                        Some(text.pos + text.galley.size() / 2.0)
                    }
                    _ => None,
                }));
                if !editor.error.is_empty() && editor.shader_notice.contains("rejected") {
                    for (index, message) in [&editor.error, &editor.shader_notice]
                        .into_iter()
                        .enumerate()
                    {
                        if index == 1 && editor.error == editor.shader_notice {
                            continue;
                        }
                        let first_line = message.lines().next().unwrap();
                        assert_eq!(output.shapes.iter().filter(|shape|matches!(&shape.shape,egui::epaint::Shape::Text(text) if text.galley.job.text==first_line)).count(),1,"each distinct problem must retain a concise visible summary");
                    }
                }
                if editor.error.contains('\n') && editor.error == editor.shader_notice {
                    let summary = editor.error.lines().next().unwrap();
                    let summary_count=output.shapes.iter().filter(|shape|matches!(&shape.shape,egui::epaint::Shape::Text(text) if text.galley.job.text==summary)).count();
                    assert_eq!(summary_count,1,"identical multiline error/shader messages must share one concise footer summary");
                    full_problem_count.set(output.shapes.iter().filter(|shape|matches!(&shape.shape,egui::epaint::Shape::Text(text) if text.galley.job.text==editor.error)).count());
                }
                asset_row_visible.set(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::epaint::Shape::Text(text) if text.galley.job.text.starts_with("triangle · Ready") && egui::Rect::from_min_size(text.pos,text.galley.size()).intersect(shape.clip_rect).height()>=text.galley.size().y-0.5)));
                viewport.expect("production viewport callback must remain present")
            };
            let normal_size = egui::vec2(1280.0, 800.0);
            frame(&mut editor, normal_size);
            frame(&mut editor, normal_size);
            assert!(
                asset_row_visible.get(),
                "real asset row must be visible before the resize journey"
            );
            let before = content_rect.get().expect("normal Content panel state");
            let divider = egui::pos2(before.center().x, before.top());
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            events.borrow_mut().push(egui::Event::PointerMoved(divider));
            frame(&mut editor, normal_size);
            events.borrow_mut().push(pointer(divider, true));
            frame(&mut editor, normal_size);
            let dragged = divider + egui::vec2(0.0, 10.0);
            events.borrow_mut().push(egui::Event::PointerMoved(dragged));
            frame(&mut editor, normal_size);
            events.borrow_mut().push(pointer(dragged, false));
            frame(&mut editor, normal_size);
            frame(&mut editor, normal_size);
            let preferred_content_height = content_rect.get().unwrap().height();
            assert!((preferred_content_height-(before.height()-10.0)).abs()<2.0,"real divider drag must establish a non-default preferred height: {preferred_content_height}");
            let mut original: Option<egui::Rect> = None;
            for size in [
                egui::vec2(1280.0, 800.0),
                egui::vec2(700.0, 500.0),
                egui::vec2(640.0, 480.0),
                egui::vec2(1280.0, 800.0),
            ] {
                frame(&mut editor, size);
                let viewport = frame(&mut editor, size);
                assert!(
                    viewport.width() >= 256.0,
                    "viewport width {} at {size:?}",
                    viewport.width()
                );
                assert!(
                    viewport.height() >= 180.0,
                    "viewport height {} at {size:?}",
                    viewport.height()
                );
                assert_eq!(
                    (
                        editor.workspace.prefs.hierarchy_width,
                        editor.workspace.prefs.inspector_width
                    ),
                    preferred,
                    "temporary constraints must not become saved widths"
                );
                if size.x == 1280.0 {
                    assert!(
                        (content_rect.get().unwrap().height() - preferred_content_height).abs()
                            < 1.0,
                        "expansion must restore the dragged Content height"
                    );
                    assert!(
                        asset_row_visible.get(),
                        "asset row must be visible again after restoring the normal window"
                    );
                    if let Some(before) = original {
                        assert!(
                            (viewport.left() - before.left()).abs() < 1.0
                                && (viewport.right() - before.right()).abs() < 1.0,
                            "expansion must restore actual preferred panel instances"
                        );
                    } else {
                        original = Some(viewport);
                    }
                }
            }
            editor
                .runtime
                .reset(&ctx, &editor.session, editor.generation)
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while editor.runtime.frame.is_none() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "runtime reset must publish a committed frame"
                );
                frame(&mut editor, egui::vec2(640.0, 480.0));
                std::thread::yield_now();
            }
            editor.error =
                "Repair or revert the unfinished value; entered text is preserved.".into();
            editor.shader_notice = "Shader rejected; last good pipeline retained.".into();
            editor.runtime.stale = true;
            editor.runtime.diagnostic = Some(ruster_behavior::Diagnostic {
                code: ruster_behavior::DiagnosticCode::Runtime,
                source_id: Some("script".into()),
                source_hash: Some("hash".into()),
                binding_id: None,
                object_id: None,
                phase: ruster_behavior::Phase::Update,
                message: "Runtime error: repair the source and Reset.".into(),
                line: Some(3),
            });
            editor.manipulation.local = true;
            editor.manipulation.snap = true;
            frame(&mut editor, egui::vec2(640.0, 480.0));
            let viewport = frame(&mut editor, egui::vec2(640.0, 480.0));
            assert!(
                viewport.height() >= 180.0,
                "errors and stale status must not displace the viewport: {}",
                viewport.height()
            );
            editor.error=concat!("Shader rejected; last good pipeline retained: scene shader rejected; previous pipeline retained:\n",
                "Validation Error\n\nCaused by:\n  In Device::create_render_pipeline\n    Shader validation failed in fragment entry point\n    Missing output at location 0\n",
                "The last good pipeline is still active.").into();
            editor.shader_notice = editor.error.clone();
            let retained = editor.error.clone();
            frame(&mut editor, egui::vec2(640.0, 480.0));
            let viewport = frame(&mut editor, egui::vec2(640.0, 480.0));
            assert!(
                viewport.height() >= 180.0,
                "multiline diagnostics must not consume the viewport"
            );
            let details = details_center
                .get()
                .expect("error Details action must remain visible");
            assert!(ctx.content_rect().contains(details));
            let pointer = |pressed| egui::Event::PointerButton {
                pos: details,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            events
                .borrow_mut()
                .extend([egui::Event::PointerMoved(details), pointer(true)]);
            frame(&mut editor, egui::vec2(640.0, 480.0));
            events.borrow_mut().push(pointer(false));
            frame(&mut editor, egui::vec2(640.0, 480.0));
            // egui deliberately hides a popup's first sizing pass.
            frame(&mut editor, egui::vec2(640.0, 480.0));
            assert_eq!(
                full_problem_count.get(),
                1,
                "Details must expose the full retained message once"
            );
            assert_eq!(editor.error, retained);
            assert_eq!(editor.shader_notice, retained);
            events.borrow_mut().push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
            frame(&mut editor, egui::vec2(640.0, 480.0));
            editor.runtime.visible = false;
            frame(&mut editor, egui::vec2(640.0, 480.0));
            editor.workspace.prefs.hierarchy_visible = false;
            editor.workspace.prefs.inspector_visible = false;
            let viewport = frame(&mut editor, egui::vec2(640.0, 480.0));
            assert!(
                viewport.width() > 600.0,
                "hidden panels must release their budget"
            );
        }
    }
    // Folding advanced controls must never hide recovered/invalid input, and
    // its keyboard actions must preserve invalid data until explicit cancellation.
    #[test]
    fn pending_offset_forces_advanced_section_open_and_owns_enter_escape() {
        for scene in [Scene::demo(), Scene::empty("source-only")] {
            let mut editor = SceneEditor::new(Session::new(scene).unwrap());
            let ctx = egui::Context::default();
            let id = egui::Id::unique(("selection_offset", "X"));
            let mut time = 0.0;
            let mut frame = |editor: &mut SceneEditor, events| {
                time += 0.2;
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        time: Some(time),
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(480.0, 320.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| editor.inspector_contents(ui),
                );
                output.textures_delta.clear();
            };
            frame(&mut editor, vec![]);
            assert!(
                ctx.read_response(id).is_none(),
                "clean advanced section starts folded"
            );
            editor.manipulation.delta[0] = "-".into();
            let snapshot = editor.recovery_snapshot().unwrap();
            let mut restored = SceneEditor::new(snapshot.restore().unwrap());
            restored
                .restore_recovery_drafts(snapshot.drafts().unwrap())
                .unwrap();
            frame(&mut restored, vec![]);
            frame(&mut restored, vec![]);
            let field = ctx
                .read_response(id)
                .expect("recovered offset must force open its field");
            assert!(field.rect.is_positive());
            ctx.memory_mut(|memory| memory.request_focus(id));
            let key = |key| egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            };
            let before = restored.session.snapshot_bytes().unwrap();
            frame(&mut restored, vec![key(egui::Key::Enter)]);
            assert_eq!(restored.manipulation.delta[0], "-");
            if restored.selected.is_some() {
                assert!(!restored.error.is_empty());
            } else {
                assert!(
                    restored.error.is_empty(),
                    "Apply is unavailable without a selection"
                );
                restored.manipulation.delta[0] = "2.5".into();
                ctx.memory_mut(|memory| memory.request_focus(id));
                frame(&mut restored, vec![]);
                frame(&mut restored, vec![key(egui::Key::Enter)]);
                assert_eq!(
                    restored.manipulation.delta[0], "2.5",
                    "Enter cannot clear a valid pending offset when Apply is disabled"
                );
            }
            assert_eq!(restored.session.snapshot_bytes().unwrap(), before);
            frame(&mut restored, vec![key(egui::Key::Escape)]);
            assert!(!restored.manipulation.offset_pending());
            assert_eq!(restored.session.snapshot_bytes().unwrap(), before);
        }
    }
    #[test]
    fn degree_fields_roundtrip_and_legacy_recovery_keep_original_units() {
        let mut scene = Scene::demo();
        scene.objects[0].transform.rotation = [0.2, 0.3, 0.4];
        let original = scene.objects[0].transform.clone();
        let fields = Fields::from(&original);
        assert_eq!(fields.rotation[0], 0.2_f32.to_degrees().to_string());
        for (actual, expected) in fields
            .parse()
            .unwrap()
            .rotation
            .into_iter()
            .zip(original.rotation)
        {
            assert!((actual - expected).abs() < 1e-6);
        }
        let mut e = SceneEditor::new(Session::new(scene).unwrap());
        assert!(e.apply());
        assert_eq!(e.session.undo_len(), 0);
        assert_eq!(e.session.scene().objects[0].transform, original);
        for version in [1, 2] {
            let mut payload = e.recovery_snapshot().unwrap().drafts().unwrap();
            payload["version"] = version.into();
            if version == 1 {
                payload.as_object_mut().unwrap().remove("behavior");
            }
            payload["fields"] = serde_json::to_value(Fields::with_unit(
                &original,
                crate::manipulation::AngleUnit::Radians,
            ))
            .unwrap();
            payload["fields"]
                .as_object_mut()
                .unwrap()
                .remove("rotation_unit");
            payload["fields"]["rotation"][0] = "-".into();
            let mut legacy = crate::manipulation::Manipulation::default();
            legacy.ids = e.manipulation.ids.clone();
            // The genuine old clean precision baseline stores radians.
            let mut manipulation = serde_json::to_value(legacy.snapshot_drafts()).unwrap();
            for (i, value) in original
                .translation
                .into_iter()
                .chain(original.rotation)
                .chain(original.scale)
                .enumerate()
            {
                manipulation["precision"][i] = value.to_string().into();
            }
            manipulation
                .as_object_mut()
                .unwrap()
                .remove("rotation_unit");
            payload["manipulation"] = manipulation;
            e.restore_recovery_drafts(payload).unwrap();
            assert_eq!(e.draft.as_ref().unwrap().rotation[0], "-");
            assert!(!e.apply());
            assert_eq!(
                e.recovery_snapshot().unwrap().drafts().unwrap()["fields"]["rotation_unit"],
                "radians"
            );
            e.draft.as_mut().unwrap().rotation[0] = "1".into();
            assert!(e.apply(), "{}", e.error);
            assert_eq!(
                e.session.scene().objects[0].transform.rotation,
                [1.0, 0.3, 0.4]
            );
            e.session.undo(e.session.scene().revision).unwrap();
            e.sync_draft();
        }
    }
    #[test]
    fn remappable_rename_and_select_all_respect_text_focus_and_keep_active_identity() {
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let ctx = egui::Context::default();
        let frame = |editor: &mut SceneEditor,
                     key: egui::Key,
                     modifiers: egui::Modifiers,
                     text_focus: bool| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: vec![
                        egui::Event::ModifiersChanged(modifiers),
                        egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    if text_focus {
                        let mut text = "text-owned shortcut".to_string();
                        ui.text_edit_singleline(&mut text).request_focus();
                    }
                    editor.route_shortcuts(&ctx);
                    editor.process_queued_action();
                },
            );
            output.textures_delta.clear();
        };
        frame(&mut editor, egui::Key::A, egui::Modifiers::CTRL, true);
        assert_eq!(editor.manipulation.ids.len(), 1);
        ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
        frame(&mut editor, egui::Key::A, egui::Modifiers::CTRL, false);
        let selection = editor.manipulation.ids.clone();
        assert_eq!(selection.len(), 3);
        assert_eq!(editor.selected.as_deref(), Some("right"));
        frame(&mut editor, egui::Key::F2, egui::Modifiers::NONE, false);
        assert!(editor.rename_pending);
        assert_eq!(editor.manipulation.ids, selection);
        editor.name_text = "Active name draft".into();
        let snapshot = editor.recovery_snapshot().unwrap();
        let mut restored = SceneEditor::new(snapshot.restore().unwrap());
        restored
            .restore_recovery_drafts(snapshot.drafts().unwrap())
            .unwrap();
        assert_eq!(restored.manipulation.ids, selection);
        assert_eq!(restored.selected.as_deref(), Some("right"));
        assert!(restored.rename_pending);
        assert_eq!(restored.name_text, "Active name draft");
    }
    #[test]
    fn runtime_content_snapshot_changes_only_for_matching_published_run() {
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let first = crate::behavior_worker::RunKey {
            generation: editor.generation,
            reset_epoch: 1,
            authored_revision: 0,
        };
        let second = crate::behavior_worker::RunKey {
            reset_epoch: 2,
            ..first
        };
        let old = Arc::new(Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        });
        let new = Arc::new(Mesh {
            vertices: Vec::new(),
            indices: Vec::new(),
        });
        editor.preview_assets = BTreeMap::from([("old-mesh".into(), old.clone())]);
        editor.reset_assets = Some((second, BTreeMap::from([("new-mesh".into(), new.clone())])));
        editor.runtime.frame = Some(crate::behavior_worker::Frame {
            key: first,
            tick: 0,
            scene: Arc::new(Scene::demo()),
        });
        editor.runtime.reset_succeeded = Some(first);
        editor.accept_runtime_resources();
        assert!(Arc::ptr_eq(&editor.preview_assets["old-mesh"], &old));
        assert!(
            editor.reset_assets.is_some(),
            "superseded run must not consume newer resource snapshot"
        );
        editor.runtime.reset_succeeded = Some(second);
        editor.accept_runtime_resources();
        assert!(
            Arc::ptr_eq(&editor.preview_assets["old-mesh"], &old),
            "acknowledgement without matching display frame must not replace geometry resources"
        );
        editor.runtime.frame = Some(crate::behavior_worker::Frame {
            key: second,
            tick: 0,
            scene: Arc::new(Scene::demo()),
        });
        editor.runtime.reset_succeeded = Some(second);
        editor.accept_runtime_resources();
        assert!(Arc::ptr_eq(&editor.preview_assets["new-mesh"], &new));
        assert!(editor.reset_assets.is_none());
    }
    #[test]
    fn embedded_source_save_and_recovery_keep_unfinished_work_and_guard_agents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        Session::new(Scene::demo()).unwrap().save(&path).unwrap();
        let mut editor = SceneEditor::new(Session::open(&path).unwrap());
        editor.behavior_action(crate::behavior_ui::Action::Create("left".into()));
        assert_eq!(editor.session.undo_len(), 1);
        let ctx = egui::Context::default();
        let frame = |editor: &mut SceneEditor, events: Vec<egui::Event>| {
            let mut action = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1100.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |_| action = editor.behavior.source_window(&ctx, true),
            );
            output.textures_delta.clear();
            if let Some(action) = action {
                editor.behavior_action(action);
            }
        };
        frame(&mut editor, vec![]);
        frame(
            &mut editor,
            vec![egui::Event::Text("\nreturn {update=function(".into())],
        );
        assert!(editor.behavior.pending());
        let inspection = editor.command("test", "scene_inspect", json!({}))["result"].clone();
        assert_eq!(inspection["drafts_pending"], true);
        let rejected=editor.command("test","scene_create",json!({"expected_session_token":inspection["session_token"],"expected_revision":inspection["revision"],"object_id":"agent-object","name":"Must not overwrite drafts"}));
        assert_eq!(rejected["error"]["code"], "drafts_pending");
        let snapshot = editor.recovery_snapshot().unwrap();
        let payload = snapshot.drafts().unwrap();
        assert_eq!(payload["version"], 3);
        let mut recovered = SceneEditor::new(snapshot.restore().unwrap());
        recovered.restore_recovery_drafts(payload).unwrap();
        assert!(recovered.behavior.pending());
        assert!(recovered.behavior.source_open);
        assert!(!recovered.runtime.is_playing());
        assert!(recovered.runtime.frame.is_none());
        frame(
            &mut editor,
            vec![
                egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
                egui::Event::Key {
                    key: egui::Key::S,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL,
                },
            ],
        );
        assert!(!editor.behavior.pending(), "{}", editor.error);
        assert!(!editor.session.is_dirty());
        let reopened = Session::open(path).unwrap();
        assert_eq!(reopened.authoring(), editor.session.authoring());
        assert!(reopened
            .authoring()
            .sources
            .values()
            .next()
            .unwrap()
            .text
            .contains("return {update=function("));
        assert_eq!(
            reopened.undo_len(),
            2,
            "source Apply/Save is one edit after atomic attachment, not one per key"
        );
        // A source can outlive all scene objects. Recover, hide its unfinished
        // code, then reopen it through the actual empty-selection inspector.
        let ids = recovered
            .session
            .scene()
            .objects
            .iter()
            .map(|object| object.id.clone())
            .collect::<Vec<_>>();
        recovered
            .session
            .delete_many(recovered.session.scene().revision, &ids)
            .unwrap();
        recovered.manipulation.ids.clear();
        recovered.selected = None;
        recovered.sync_draft();
        let source_only = recovered.recovery_snapshot().unwrap();
        let mut orphan = SceneEditor::new(source_only.restore().unwrap());
        orphan
            .restore_recovery_drafts(source_only.drafts().unwrap())
            .unwrap();
        assert!(orphan.session.scene().objects.is_empty() && orphan.selected.is_none());
        let exact = serde_json::to_value(orphan.behavior.snapshot()).unwrap();
        let authored = orphan.session.snapshot_bytes().unwrap();
        frame(&mut orphan, vec![]);
        frame(
            &mut orphan,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(!orphan.behavior.source_open && orphan.behavior.pending());
        let inspector_ctx = egui::Context::default();
        let inspector_frame = |orphan: &mut SceneEditor, events| {
            let mut output = inspector_ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 320.0),
                    )),
                    ..Default::default()
                },
                |ui| orphan.inspector_contents(ui),
            );
            output.textures_delta.clear();
            output
        };
        let output = inspector_frame(&mut orphan, vec![]);
        let button = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text)
                    if text.galley.job.text == "Show unapplied source draft" =>
                {
                    Some(text.pos + text.galley.size() / 2.0)
                }
                _ => None,
            })
            .expect("empty inspector must expose the hidden unfinished source");
        let pointer = |pressed| egui::Event::PointerButton {
            pos: button,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        inspector_frame(
            &mut orphan,
            vec![egui::Event::PointerMoved(button), pointer(true)],
        );
        inspector_frame(&mut orphan, vec![pointer(false)]);
        assert!(orphan.behavior.source_open && orphan.behavior.pending());
        assert_eq!(
            serde_json::to_value(orphan.behavior.snapshot()).unwrap(),
            exact
        );
        assert_eq!(orphan.session.snapshot_bytes().unwrap(), authored);
    }
    // Recovery must preserve unapplied invalid text and asset provenance without
    // granting overwrite authority to the original project, even if it changed.
    #[test]
    fn recovery_restores_invalid_drafts_and_assets_as_a_leased_unsaved_copy() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let path = project.join("scene.json");
        std::fs::write(
            project.join("triangle.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        Session::new(Scene::demo()).unwrap().save(&path).unwrap();
        let store = crate::recovery::Store::open(dir.path().join("recovery")).unwrap();
        let mut e = SceneEditor::new(Session::open(&path).unwrap());
        assert!(
            e.recovery.store.is_none(),
            "test constructors never open user recovery storage"
        );
        e.recovery = crate::recovery_ui::RecoveryUi::at(store.clone());
        e.assets
            .as_mut()
            .unwrap()
            .import("triangle", "triangle.obj")
            .unwrap();
        e.asset_snapshot();
        e.assign_mesh(Some("triangle".into())).unwrap();
        e.sync_draft();
        let mut draft = serde_json::to_value(RecoveryDrafts {
            version: 1,
            revision: e.session.scene().revision,
            selection: e.manipulation.ids.clone(),
            fields: e.draft.clone(),
            pending_name: Some("Unapplied name".into()),
            manipulation: e.manipulation.snapshot_drafts(),
            behavior: None,
        })
        .unwrap();
        draft["manipulation"]
            .as_object_mut()
            .unwrap()
            .remove("rotation_unit");
        draft["fields"]
            .as_object_mut()
            .unwrap()
            .remove("rotation_unit");
        draft["manipulation"]["precision"][0] = "-".into();
        draft["manipulation"]["edited"][0] = true.into();
        draft["manipulation"]["revision"] = e.session.scene().revision.into();
        draft["manipulation"]["delta"][1] = "-".into();
        e.restore_recovery_drafts(draft).unwrap();
        let snapshot = e.recovery_snapshot();
        e.recovery.checkpoint(snapshot);
        assert!(e.recovery.warning.is_empty(), "{}", e.recovery.warning);
        assert!(
            store.scan().unwrap().is_empty(),
            "live window retains the lease"
        );
        e.recovery.disconnect_wait();
        drop(e); // Simulate a crash: do not retire the journal.
        let id = store.scan().unwrap().remove(0).id;
        std::fs::write(&path, "external project replacement").unwrap();
        let mut restored = SceneEditor::new(Session::new(Scene::empty("untitled")).unwrap());
        restored.recovery = crate::recovery_ui::RecoveryUi::at(store.clone());
        restored.restore_checkpoint(&id).unwrap();
        restored.wait_recovery();
        assert!(
            store.scan().unwrap().is_empty(),
            "adoption keeps the claimed lease"
        );
        assert!(restored.session.path().is_none());
        assert_eq!(restored.session.undo_len(), 1);
        assert_eq!(restored.asset_origin.as_deref(), Some(project.as_path()));
        assert!(restored.resolved.contains_key("triangle"));
        assert!(restored.rename_pending);
        assert_eq!(restored.name_text, "Unapplied name");
        let recovered = serde_json::to_value(restored.manipulation.snapshot_drafts()).unwrap();
        assert_eq!(recovered["precision"][0], "-");
        assert_eq!(recovered["delta"][1], "-");
        assert_eq!(
            restored.session.scene().objects[0].transform.translation[0],
            -2.0
        );
        assert!(!restored.runtime.is_playing());
        assert!(!restored.manipulation.active());
        assert!(restored.save_scene(path.clone()).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "external project replacement"
        );
        assert!(restored
            .save_scene(dir.path().join("rebased.json"))
            .is_err());
        // Save committed scene data while drafts remain: the recovery lease stays.
        restored
            .save_scene(project.join("partial-save.json"))
            .unwrap();
        assert!(store.scan().unwrap().is_empty());
        restored.recovery.disconnect_wait();
        drop(restored);
        assert_eq!(
            store.scan().unwrap().len(),
            1,
            "saving with drafts does not retire recovery"
        );
        let mut final_copy = SceneEditor::new(Session::new(Scene::empty("untitled")).unwrap());
        final_copy.recovery = crate::recovery_ui::RecoveryUi::at(store.clone());
        final_copy.restore_checkpoint(&id).unwrap();
        final_copy.wait_recovery();
        final_copy.rename_pending = false;
        final_copy.sync_draft();
        final_copy.manipulation.delta = std::array::from_fn(|_| "0".into());
        final_copy
            .save_scene(project.join("recovered.json"))
            .unwrap();
        final_copy.recovery.disconnect_wait();
        drop(final_copy);
        assert!(
            store.scan().unwrap().is_empty(),
            "successful draft-free save retires recovery"
        );
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "external project replacement"
        );
    }

    #[test]
    fn recovery_rejects_unsupported_drafts_before_replacing_current_work() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::recovery::Store::open(dir.path().join("recovery")).unwrap();
        let mut source = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let mut drafts = serde_json::to_value(RecoveryDrafts {
            version: 1,
            revision: source.session.scene().revision,
            selection: source.manipulation.ids.clone(),
            fields: source.draft.clone(),
            pending_name: None,
            manipulation: source.manipulation.snapshot_drafts(),
            behavior: None,
        })
        .unwrap();
        drafts["version"] = 99.into();
        let mut journal = store.allocate().unwrap();
        journal
            .checkpoint(crate::recovery::Snapshot::capture(&source.session, None, &drafts).unwrap())
            .unwrap();
        drop(journal);
        let id = store.scan().unwrap().remove(0).id;
        source.recovery = crate::recovery_ui::RecoveryUi::at(store.clone());
        let before = source.session.scene().clone();
        assert!(source
            .restore_checkpoint(&id)
            .unwrap_err()
            .contains("Save your current work"));
        assert_eq!(source.session.scene(), &before);
        let (started, begin) = std::sync::mpsc::channel();
        let (release, gate) = std::sync::mpsc::channel();
        let snapshot = source.recovery_snapshot();
        source
            .recovery
            .dispatch(Box::new(move || {
                started.send(()).unwrap();
                gate.recv_timeout(Duration::from_secs(5)).unwrap();
                snapshot
            }))
            .unwrap();
        begin.recv_timeout(Duration::from_secs(5)).unwrap();
        source.replace_with_empty();
        assert_eq!(
            source.session.scene(),
            &before,
            "prepared New must wait for the recovery boundary acknowledgement"
        );
        release.send(()).unwrap();
        source.wait_recovery();
        let before = source.session.scene().clone();
        assert!(source
            .restore_checkpoint(&id)
            .unwrap_err()
            .contains("unsupported version"));
        assert_eq!(source.session.scene(), &before);
        assert_eq!(
            store.scan().unwrap().len(),
            1,
            "unsupported drafts preserve the checkpoint and release its temporary claim"
        );
    }
    #[test]
    fn close_cleanup_conflict_keeps_window_visible_and_escape_cancels() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        Session::new(Scene::demo()).unwrap().save(&path).unwrap();
        let recovery_dir = dir.path().join("recovery");
        let store = crate::recovery::Store::open(recovery_dir.clone()).unwrap();
        let mut e = SceneEditor::new(Session::open(path).unwrap());
        e.recovery = crate::recovery_ui::RecoveryUi::at(store.clone());
        let snapshot = e.recovery_snapshot();
        e.recovery.checkpoint(snapshot);
        let record = std::fs::read_dir(recovery_dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("window-")
            })
            .unwrap()
            .join("checkpoint.json");
        std::fs::write(&record, "external replacement").unwrap();
        e.request_close();
        assert!(e.close_approved);
        e.start_close_cleanup();
        e.wait_recovery();
        assert!(e
            .close_cleanup_error
            .as_ref()
            .unwrap()
            .contains("changed outside"));
        assert!(!e.close_shutdown);
        assert!(!e.close_ready_to_exit);
        assert!(!e.close_pending);
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                events: vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }],
                ..Default::default()
            },
            |_| e.close_finalization_ui(&ctx),
        );
        output.textures_delta.clear();
        assert!(!e.close_approved);
        assert!(e.close_cleanup_error.is_none());
        assert_eq!(
            std::fs::read_to_string(record).unwrap(),
            "external replacement"
        );
        e.recovery.disconnect_wait();
        assert_eq!(store.scan().unwrap().len(), 1);
    }
    // Search is view state: filtering must not edit or silently discard hidden
    // selection, while Enter selection still observes the normal draft guard.
    #[test]
    fn hierarchy_search_keyboard_and_context_actions_preserve_invalid_drafts() {
        let mut e = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let count = e.session.scene().objects.len();
        let ctx = egui::Context::default();
        let frame = |e: &mut SceneEditor, key: Option<egui::Key>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: key
                        .into_iter()
                        .map(|key| egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        })
                        .collect(),
                    ..Default::default()
                },
                |ui| e.hierarchy_contents(ui),
            );
            output.textures_delta.clear();
            e.process_queued_selection();
        };
        e.hierarchy_query = "RIGHT".into();
        e.hierarchy_focus = true;
        frame(&mut e, None);
        assert_eq!(e.hierarchy_search.visible.len(), 1);
        assert_eq!(e.selected.as_deref(), Some("left"));
        e.draft.as_mut().unwrap().translation[0] = "-".into();
        frame(&mut e, Some(egui::Key::Enter));
        assert_eq!(e.selected.as_deref(), Some("left"));
        e.queued_context = Some((Some("right".into()), ContextAction::Delete));
        e.process_content_intents();
        assert_eq!(e.session.scene().objects.len(), count);
        assert_eq!(e.draft.as_ref().unwrap().translation[0], "-");
        e.sync_draft();
        e.hierarchy_focus = true;
        frame(&mut e, None);
        frame(&mut e, Some(egui::Key::Enter));
        assert_eq!(e.selected.as_deref(), Some("right"));
        e.hierarchy_focus = true;
        frame(&mut e, None);
        frame(&mut e, Some(egui::Key::Escape));
        assert!(e.hierarchy_query.is_empty());
        assert_eq!(e.hierarchy_search.visible.len(), count);
        assert_eq!(e.selected.as_deref(), Some("right"));
        e.queued_context = Some((Some("right".into()), ContextAction::Duplicate));
        e.process_content_intents();
        assert_eq!(e.session.scene().objects.len(), count + 1);
        assert_eq!(e.session.undo_len(), 1);
    }

    // Exercise egui's typed release path, not just the assignment helper. A
    // drag freezes both catalog and scene provenance and edits only its target.
    #[test]
    fn internal_mesh_drop_is_single_target_undoable_and_rejects_stale_provenance() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        std::fs::write(
            dir.path().join("triangle.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        Session::new(Scene::demo()).unwrap().save(&path).unwrap();
        let mut e = SceneEditor::new(Session::open(&path).unwrap());
        e.assets
            .as_mut()
            .unwrap()
            .import("triangle", "triangle.obj")
            .unwrap();
        e.asset_snapshot();
        let payload = e.asset_payload("triangle").unwrap();
        let ctx = egui::Context::default();
        let mut rect = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            e.mesh_reference_ui(ui);
            rect = ui.min_rect();
        });
        output.textures_delta.clear();
        let pos = rect.center();
        for pressed in [true, false] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: vec![
                        egui::Event::PointerMoved(pos),
                        egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    egui::DragAndDrop::set_payload(&ctx, payload.clone());
                    e.mesh_reference_ui(ui);
                },
            );
            output.textures_delta.clear();
        }
        assert!(
            e.queued_drop.is_some(),
            "the inspector must receive the typed egui drop"
        );
        e.process_content_intents();
        assert!(e.error.is_empty(), "{}", e.error);
        assert_eq!(
            e.session.scene().objects[0].mesh.as_deref(),
            Some("triangle")
        );
        assert_eq!(e.session.scene().objects[1].mesh, None);
        assert_eq!(e.session.undo_len(), 1);
        assert!(e.preview.scene.objects.iter().all(|o| o.mesh.is_none()));
        assert!(e
            .accept_asset_drop("right", &payload)
            .unwrap_err()
            .contains("scene changed"));
        let fresh = e.asset_payload("triangle").unwrap();
        let unchanged = e.session.scene().clone();
        let mut old_session = fresh.clone();
        old_session.generation += 1;
        assert!(e
            .accept_asset_drop("right", &old_session)
            .unwrap_err()
            .contains("scene session"));
        let mut foreign = fresh.clone();
        foreign.catalog_root = dir.path().join("other-project");
        assert!(e
            .accept_asset_drop("right", &foreign)
            .unwrap_err()
            .contains("another project"));
        let mut obsolete = fresh.clone();
        obsolete.catalog_revision += 1;
        assert!(e
            .accept_asset_drop("right", &obsolete)
            .unwrap_err()
            .contains("catalog changed"));
        e.draft.as_mut().unwrap().scale[0] = "-".into();
        assert!(e.accept_asset_drop("right", &fresh).is_err());
        assert_eq!(e.draft.as_ref().unwrap().scale[0], "-");
        assert_eq!(e.session.scene(), &unchanged);
        assert_eq!(e.session.undo_len(), 1);
        e.sync_draft();
        e.history(false);
        assert!(e.session.scene().objects.iter().all(|o| o.mesh.is_none()));
    }
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

    #[test]
    fn asset_dialog_focus_and_escape_preserve_scene_and_enter_does_not_import() {
        let mut e = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let before = e.session.scene().clone();
        e.asset_dialog = true;
        let ctx = egui::Context::default();
        let frame = |editor: &mut SceneEditor, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |_| editor.content_ui(&ctx),
            );
            output.textures_delta.clear();
        };
        frame(&mut e, vec![]);
        assert!(ctx.memory(|m| m.focused().is_some()));
        let key = |key| {
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        frame(&mut e, key(egui::Key::Enter));
        assert!(e.asset_dialog);
        assert_eq!(e.session.scene(), &before);
        assert!(e.assets.is_none());
        frame(&mut e, key(egui::Key::Escape));
        assert!(!e.asset_dialog);
        assert!(!e.asset_focused);
        assert_eq!(e.session.scene(), &before);
    }

    #[test]
    fn close_shortcut_uses_the_same_dirty_and_name_recovery_as_native_close() {
        let ctx = egui::Context::default();
        let frame = |editor: &mut SceneEditor| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: vec![
                        egui::Event::ModifiersChanged(egui::Modifiers::CTRL),
                        egui::Event::Key {
                            key: egui::Key::Q,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::CTRL,
                        },
                    ],
                    ..Default::default()
                },
                |_| {
                    editor.route_shortcuts(&ctx);
                    editor.process_queued_action();
                },
            );
            output.textures_delta.clear();
        };
        let mut dirty = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        dirty.draft.as_mut().unwrap().translation[0] = "-".into();
        let before = dirty.session.scene().clone();
        frame(&mut dirty);
        assert!(dirty.close_pending);
        assert!(!dirty.close_approved);
        assert_eq!(dirty.session.scene(), &before);
        assert_eq!(dirty.draft.as_ref().unwrap().translation[0], "-");
        let mut clean = SceneEditor::new(Session::new(Scene::empty("untitled")).unwrap());
        frame(&mut clean);
        assert!(clean.close_approved);
        assert!(!clean.close_pending);
        let mut named = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        named.rename_pending = true;
        named.name_text = "Unapplied".into();
        named.request_close();
        assert!(!named.close_approved);
        assert!(!named.close_pending);
        assert!(named.rename_pending);
        assert_eq!(named.name_text, "Unapplied");
        named.rename_pending = false;
        named.new_scene_pending = true;
        named.request_close();
        assert!(!named.close_approved);
        assert!(!named.close_pending);
        assert!(named.new_scene_pending);
    }

    #[test]
    fn rename_dialog_accepts_enter_tab_and_escape_with_real_focus() {
        let mut e = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let ctx = egui::Context::default();
        let frame = |editor: &mut SceneEditor, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |_| editor.rename_dialog(&ctx),
            );
            output.textures_delta.clear();
        };
        let key = |key| {
            vec![egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }]
        };
        e.rename_pending = true;
        e.name_text = "Entered name".into();
        frame(&mut e, vec![]);
        frame(&mut e, key(egui::Key::Enter));
        assert!(!e.rename_pending);
        assert_eq!(e.session.scene().objects[0].display_name(), "Entered name");
        e.rename_pending = true;
        e.name_text = "Discard name".into();
        frame(&mut e, vec![]);
        frame(&mut e, key(egui::Key::Escape));
        assert!(!e.rename_pending);
        assert_eq!(e.session.scene().objects[0].display_name(), "Entered name");
        assert_eq!(e.name_text, "Entered name");
        e.rename_pending = true;
        e.name_text = "Tabbed name".into();
        frame(&mut e, vec![]);
        frame(&mut e, key(egui::Key::Tab));
        frame(&mut e, key(egui::Key::Enter));
        assert!(!e.rename_pending);
        assert_eq!(e.session.scene().objects[0].display_name(), "Tabbed name");
    }

    #[test]
    fn fresh_editor_state_is_detached_and_human_selection_commits_valid_drafts() {
        let mut e = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        assert!(
            e.workspace.state_path().is_none(),
            "unit editor constructors never use the user's preference path"
        );
        e.draft.as_mut().unwrap().translation[0] = "5.5".into();
        e.queued_selection = Some((Some("right".into()), false));
        e.process_queued_selection();
        assert_eq!(e.selected.as_deref(), Some("right"));
        assert_eq!(e.session.scene().objects[0].transform.translation[0], 5.5);
        assert_eq!(e.session.undo_len(), 1);
        e.draft.as_mut().unwrap().translation[0] = "-".into();
        e.queued_selection = Some((Some("left".into()), false));
        e.process_queued_selection();
        assert_eq!(e.selected.as_deref(), Some("right"));
        assert_eq!(e.draft.as_ref().unwrap().translation[0], "-");
        assert_eq!(e.session.undo_len(), 1);
        let empty = SceneEditor::new(Session::new(Scene::empty("untitled")).unwrap());
        assert!(
            !empty.needs_recovery(),
            "untouched welcome scene has no work to discard"
        );
        let source_only = json!({"format":"ruster-scene","format_version":5,"scene":Scene::empty("source-only"),"authoring":{"sources":{"script":{"label":null,"language_id":"unknown-backend","host_api_version":1,"text":"unfinished ("}},"bindings":{}},"undo":[],"redo":[]});
        let source_only = SceneEditor::new(
            Session::from_snapshot_bytes(&serde_json::to_vec(&source_only).unwrap()).unwrap(),
        );
        assert_eq!(source_only.session.undo_len(), 0);
        assert!(
            source_only.needs_recovery(),
            "a detached source-only document is authored work, even with zero history/objects"
        );
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
        e.content_search.update(e.assets.as_ref(), "Ready");
        assert_eq!(e.content_search.visible.len(), 1);
        std::fs::write(&source, "broken mesh").unwrap();
        args["expected_asset_revision"] = "2".into();
        assert_eq!(
            e.command("test", "asset_refresh", args)["error"]["code"],
            "asset_error"
        );
        assert!(Arc::ptr_eq(&e.resolved["triangle"], &last_good));
        assert_eq!(e.assets.as_ref().unwrap().revision(), 2);
        // Failed refresh changes diagnostics without advancing catalog revision.
        e.content_search.update(e.assets.as_ref(), "Invalid");
        assert_eq!(e.content_search.visible.len(), 1);
        assert!(e.content_search.rows[0].info.diagnostic.is_some());
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
        // Contextual Rename has a single Enter intent; generic asset tools above
        // deliberately keep their three operations explicit.
        e.asset_dialog = true;
        e.asset_rename_intent = true;
        e.asset_source_focus = true;
        e.asset_id = "triangle".into();
        e.asset_source = "renamed.obj".into();
        let ctx = egui::Context::default();
        let frame = |editor: &mut SceneEditor, key: Option<egui::Key>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: key
                        .into_iter()
                        .map(|key| egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        })
                        .collect(),
                    ..Default::default()
                },
                |_| editor.content_ui(&ctx),
            );
            output.textures_delta.clear();
        };
        // Repair the source first: rename validates source content too.
        std::fs::write(&source, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        frame(&mut e, None);
        assert!(ctx.memory(|memory| memory.focused().is_some()));
        frame(&mut e, Some(egui::Key::Escape));
        assert!(!e.asset_dialog);
        assert!(source.exists());
        e.asset_dialog = true;
        frame(&mut e, None);
        frame(&mut e, Some(egui::Key::Enter));
        assert!(!e.asset_dialog, "{}", e.error);
        assert!(dir.path().join("renamed.obj").exists());
        assert!(!source.exists());
        assert_eq!(e.assets.as_ref().unwrap().revision(), 3);
    }

    #[test]
    fn missing_content_summary_is_bounded_at_minimum_two_times_scale() {
        let mut scene = Scene::demo();
        let prototype = scene.objects[0].clone();
        scene.objects = (0..256)
            .map(|i| {
                let mut object = prototype.clone();
                object.id = format!("object-{i}");
                object.mesh = Some(format!("missing-{i}"));
                object
            })
            .collect();
        let mut e = SceneEditor::new(Session::new(scene).unwrap());
        let ctx = egui::Context::default();
        let mut bounds = egui::Rect::NOTHING;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(480.0, 320.0),
                )),
                ..Default::default()
            },
            |ui| {
                e.content_diagnostics_ui(ui);
                bounds = ui.min_rect();
            },
        );
        output.textures_delta.clear();
        assert!(
            bounds.height() < 128.0,
            "unresolved references must not consume the viewport: {bounds:?}"
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
        editor.manipulation.delta[0] = "-".into();
        let unchanged = editor.session.snapshot_bytes().unwrap();
        let inspection = editor.command("host", "scene_inspect", json!({}));
        assert_eq!(inspection["result"]["drafts_pending"], true);
        assert_eq!(
            editor.command("host", "scene_transform", args.clone())["error"]["code"],
            "drafts_pending"
        );
        assert_eq!(editor.command("host","asset_import",json!({"expected_session_token":before["session_token"],"expected_revision":before["revision"],"expected_asset_revision":"0","asset_id":"triangle","source":"triangle.obj"}))["error"]["code"],"drafts_pending");
        assert_eq!(editor.session.snapshot_bytes().unwrap(), unchanged);
        assert_eq!(editor.session.undo_len(), 0);
        assert_eq!(editor.manipulation.delta[0], "-");
        editor.close_approved = true;
        assert_eq!(
            editor.command("host", "scene_inspect", json!({}))["result"]["recovery_pending"],
            true
        );
        editor.close_approved = false;
        editor.manipulation.delta[0] = "0".into();
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
