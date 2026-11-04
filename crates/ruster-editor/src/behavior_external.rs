//! One explicit off-UI working-copy job; embedded source retains save authority.
use crate::behavior_files::{self as files, Context, LaunchSpec, SyncBase, SyncOutcome};
use eframe::egui;
use ruster_scene::{Authoring, BehaviorSource, Session};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    process::Child,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread::JoinHandle,
};
fn files_error(error: files::FilesError) -> String {
    let kind = match error.kind {
        files::ErrorKind::Unsaved => "Save required",
        files::ErrorKind::Invalid => "Invalid working-copy data",
        files::ErrorKind::Busy => "Working-copy context busy",
        files::ErrorKind::Conflict => "Working-copy conflict",
        files::ErrorKind::Missing => "Working copy missing",
        files::ErrorKind::TooLarge => "Working-copy limit exceeded",
        files::ErrorKind::Io => "Working-copy I/O failed",
    };
    format!("{kind}: {}", error.message)
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Intent {
    Export,
    Open { line: u32 },
    OpenExisting { line: u32 },
    Refresh,
    ImportObserved,
    OverwriteObserved,
    ExportNewLanguage,
}
#[derive(Clone, Debug)]
struct Key {
    generation: u64,
    revision: u64,
    scene: PathBuf,
    source_id: Option<String>,
    source_hash: Option<String>,
    launch_epoch: u64,
}
impl Key {
    fn matches(&self, session: &Session, generation: u64) -> bool {
        self.generation == generation
            && self.revision == session.scene().revision
            && session.path() == Some(self.scene.as_path())
            && self.source_id.as_ref().is_none_or(|id| {
                session.source(id).is_some_and(|source| {
                    Some(ruster_behavior::source_hash(&source.text)) == self.source_hash
                })
            })
    }
}
#[derive(Clone)]
struct View {
    source: String,
    revision: u64,
    path: PathBuf,
    embedded: Option<String>,
    observed: Option<String>,
    language_changed: bool,
}
struct Input {
    key: Key,
    intent: Option<Intent>,
    source: Option<BehaviorSource>,
    authoring: Option<Arc<Authoring>>,
    live: BTreeMap<String, SyncBase>,
    launch: LaunchSpec,
    observed: Option<String>,
}
struct Completion {
    key: Key,
    result: Result<Output, String>,
}
struct Output {
    outcome: Option<SyncOutcome>,
    child: Option<Child>,
    warning: String,
    promotion: Option<String>,
    launch: Option<LaunchPlan>,
}
struct LaunchPlan {
    context: Context,
    id: String,
    source: BehaviorSource,
    path: PathBuf,
    expected: String,
    config: LaunchSpec,
    line: u32,
}
pub(super) struct Import {
    pub source_id: String,
    pub source: BehaviorSource,
    pub next_base: SyncBase,
}
#[derive(Default)]
pub(super) struct External {
    document: Option<(u64, Option<PathBuf>)>,
    job: Option<JoinHandle<Completion>>,
    live: BTreeMap<String, SyncBase>,
    view: Option<View>,
    children: Vec<Child>,
    refresh_pending: bool,
    promote_pending: bool,
    closing: bool,
    launch_epoch: Arc<AtomicU64>,
    guard_state: Option<(u64, bool)>,
    #[cfg(test)]
    prepare_gate: Option<(
        std::sync::mpsc::SyncSender<()>,
        std::sync::mpsc::Receiver<()>,
    )>,
    #[cfg(test)]
    return_gate: Option<(
        std::sync::mpsc::SyncSender<()>,
        std::sync::mpsc::Receiver<()>,
    )>,
    pub status: String,
    pub open: bool,
}
impl External {
    pub fn observe_document(&mut self, session: &Session, generation: u64) {
        if self
            .document
            .as_ref()
            .is_some_and(|(old, path)| *old == generation && path.as_deref() == session.path())
        {
            return;
        }
        self.document = Some((generation, session.path().map(std::path::Path::to_path_buf)));
        self.launch_epoch.fetch_add(1, Ordering::SeqCst);
        self.guard_state = None;
        self.live.clear();
        self.view = None;
        self.refresh_pending = false;
        self.promote_pending = false;
        self.open = false;
        self.status = if self.job.is_some() {
            "Previous document's working-copy job is finishing; it cannot import into this document.".into()
        } else {
            String::new()
        };
    }
    pub fn saved(&mut self, session: &Session, generation: u64) {
        self.observe_document(session, generation);
        self.promote_pending = !self.live.is_empty();
    }
    pub fn focus_returned(&mut self) {
        if !self.closing {
            self.refresh_pending = true;
            self.reap();
        }
    }
    pub fn closing(&mut self) {
        self.closing = true;
        self.launch_epoch.fetch_add(1, Ordering::SeqCst);
        self.refresh_pending = false;
        self.reap();
    }
    pub fn cancel_close(&mut self) {
        self.closing = false;
        self.launch_epoch.fetch_add(1, Ordering::SeqCst);
    }
    pub fn set_closing(&mut self, closing: bool) {
        if self.closing != closing {
            if closing {
                self.closing();
            } else {
                self.cancel_close();
            }
        }
    }
    pub fn finished(&self) -> bool {
        self.job.is_none() && !self.promote_pending
    }
    fn reap(&mut self) {
        self.children
            .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
    }
    fn observe_guard(&mut self, revision: u64, blocked: bool) {
        if self.guard_state != Some((revision, blocked)) {
            self.guard_state = Some((revision, blocked));
            self.launch_epoch.fetch_add(1, Ordering::SeqCst);
        }
    }
    pub fn request(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        generation: u64,
        source_id: &str,
        intent: Intent,
        launch: &LaunchSpec,
    ) -> Result<(), String> {
        self.observe_document(session, generation);
        self.live.retain(|id, _| session.source(id).is_some());
        self.reap();
        if self.closing {
            return Err("Finish or cancel closing before external editor operations.".into());
        }
        if self.job.is_some() {
            return Err("A working-copy job is still finishing; retry after its result.".into());
        }
        if matches!(intent, Intent::Open { .. } | Intent::OpenExisting { .. })
            && self.children.len() >= 8
        {
            return Err("Eight editor launcher processes are still active; retry after one exits. Ruster will not kill your editor.".into());
        }
        let source = session
            .source(source_id)
            .ok_or("Source is missing; no working copy was changed.")?;
        let scene = session
            .path()
            .ok_or("Save this scene before creating or checking external working copies.")?
            .to_path_buf();
        let observed = self
            .view
            .as_ref()
            .filter(|view| view.source == source_id)
            .and_then(|view| view.observed.clone());
        if matches!(
            intent,
            Intent::ImportObserved | Intent::OverwriteObserved | Intent::OpenExisting { .. }
        ) && observed.is_none()
        {
            return Err(
                "Refresh and inspect an observed file version before resolving a conflict.".into(),
            );
        }
        if matches!(
            intent,
            Intent::ImportObserved | Intent::OverwriteObserved | Intent::OpenExisting { .. }
        ) && self
            .view
            .as_ref()
            .is_none_or(|view| view.revision != session.scene().revision)
        {
            return Err("Embedded state changed after the observed file comparison; Refresh before resolving versions.".into());
        }
        if self
            .view
            .as_ref()
            .is_some_and(|view| view.source != source_id)
        {
            self.view = None;
        }
        self.observe_guard(session.scene().revision, false);
        let key = Key {
            generation,
            revision: session.scene().revision,
            scene,
            source_id: Some(source_id.into()),
            source_hash: Some(ruster_behavior::source_hash(&source.text)),
            launch_epoch: self.launch_epoch.load(Ordering::SeqCst),
        };
        let authoring = matches!(
            intent,
            Intent::Export | Intent::Open { .. } | Intent::ExportNewLanguage
        )
        .then(|| Arc::new(session.authoring().clone()));
        let input = Input {
            key,
            intent: Some(intent),
            source: Some(source.clone()),
            authoring,
            live: self.live.clone(),
            launch: launch.clone(),
            observed,
        };
        self.start(ctx, input)
    }
    fn start(&mut self, ctx: &egui::Context, input: Input) -> Result<(), String> {
        let ctx = ctx.clone();
        self.status = "Working-copy job running…".into();
        #[cfg(test)]
        let gate = self.prepare_gate.take();
        #[cfg(test)]
        let return_gate = self.return_gate.take();
        self.job = Some(
            std::thread::Builder::new()
                .name("ruster-working-copy".into())
                .spawn(move || {
                    let key = input.key.clone();
                    let result = run(input);
                    #[cfg(test)]
                    if let Some((reached, release)) = gate {
                        reached.send(()).unwrap();
                        release
                            .recv_timeout(std::time::Duration::from_secs(5))
                            .unwrap();
                    }
                    ctx.request_repaint();
                    #[cfg(test)]
                    if let Some((notified, release)) = return_gate {
                        notified.send(()).unwrap();
                        release
                            .recv_timeout(std::time::Duration::from_secs(5))
                            .unwrap();
                    }
                    Completion { key, result }
                })
                .map_err(|error| error.to_string())?,
        );
        Ok(())
    }
    fn start_launch(
        &mut self,
        ctx: &egui::Context,
        key: Key,
        plan: LaunchPlan,
    ) -> Result<(), String> {
        let ctx = ctx.clone();
        let epoch = self.launch_epoch.clone();
        self.status = "Launching the configured editor for the validated working copy…".into();
        self.job=Some(std::thread::Builder::new().name("ruster-editor-launch".into()).spawn(move|| {
            let result=(|| {
                let valid=||epoch.load(Ordering::SeqCst)==key.launch_epoch;
                if !valid() {return Err("Editor launch cancelled before process creation; files preserved.".into());}
                let observed=files::import_observed(&plan.context,&plan.id,&plan.source,&plan.expected).map_err(files_error)?;
                let SyncOutcome::ImportCandidate{path,..}=observed else {return Err("Observed working-copy validation returned no source version; editor was not launched.".into());};
                if path!=plan.path {return Err("Controlled working-copy path changed; editor was not launched.".into());}
                let mut command=plan.config.command(&plan.context,&path,plan.line).map_err(files_error)?;
                // This is the launch commit point: a queued/new document, draft
                // or close cancels up to this final check, without blocking UI.
                if !valid() {return Err("Editor launch cancelled before process creation; files preserved.".into());}
                let child=command.spawn().map_err(|error|format!("Editor process launch failed: {error}"))?;
                Ok(Output{outcome:None,child:Some(child),warning:String::new(),promotion:Some(format!("Opened observed working copy {} at line {}. Embedded and external versions were preserved.",path.display(),plan.line)),launch:None})
            })();
            ctx.request_repaint();Completion{key,result}
        }).map_err(|error|error.to_string())?);
        Ok(())
    }
    pub fn poll(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        generation: u64,
        blocked: bool,
        active_source: Option<&str>,
        launch: &LaunchSpec,
    ) -> Option<Import> {
        self.observe_document(session, generation);
        self.observe_guard(session.scene().revision, blocked);
        let mut import = None;
        if self.job.as_ref().is_some_and(JoinHandle::is_finished) {
            let completed = self.job.take().unwrap().join();
            match completed {
                Err(_)=>self.status="Working-copy job stopped unexpectedly; embedded source and existing files were preserved. Retry explicitly.".into(),
                Ok(completed)=> {
                    match completed.result {
                        Err(error)=>self.status=error,
                        Ok(mut output)=> {
                            if let Some(child)=output.child.take() {self.children.push(child);}
                            if !completed.key.matches(session,generation) {self.status="Scene/source changed during the working-copy job. Embedded data was preserved; Refresh again to compare current versions.".into();}
                            else {
                                if let Some(outcome)=output.outcome {import=self.accept(completed.key.source_id.as_deref().unwrap(),session,outcome,blocked);}
                                else if let Some(status)=output.promotion {self.status=status;}
                                if !output.warning.is_empty() {self.status.push(' ');self.status.push_str(&output.warning);}
                                if let Some(plan)=output.launch {
                                    if !blocked && !self.closing && completed.key.launch_epoch==self.launch_epoch.load(Ordering::SeqCst) {
                                        if let Err(error)=self.start_launch(ctx,completed.key,plan) {self.status=error;}
                                    } else {self.status="Editor launch cancelled because document/drafts/closing changed; working files were preserved.".into();}
                                }
                            }
                        }
                    }
                }
            }
            self.reap();
        }
        if self.job.is_none() {
            if self.promote_pending {
                self.promote_pending = false;
                if let Some(scene) = session.path() {
                    let input = Input {
                        key: Key {
                            generation,
                            revision: session.scene().revision,
                            scene: scene.to_path_buf(),
                            source_id: None,
                            source_hash: None,
                            launch_epoch: self.launch_epoch.load(Ordering::SeqCst),
                        },
                        intent: None,
                        source: None,
                        authoring: None,
                        live: self.live.clone(),
                        launch: launch.clone(),
                        observed: None,
                    };
                    if let Err(error) = self.start(ctx, input) {
                        self.status = error;
                    }
                }
            } else if self.refresh_pending && !blocked && !self.closing {
                self.refresh_pending = false;
                if let Some(id) = active_source {
                    if self.live.contains_key(id)
                        || self.view.as_ref().is_some_and(|view| view.source == id)
                    {
                        if let Err(error) =
                            self.request(ctx, session, generation, id, Intent::Refresh, launch)
                        {
                            self.status = error;
                        }
                    }
                }
            }
        }
        // A completion notifier can wake the UI just before JoinHandle flips
        // to finished. Poll only while work exists, so that wake cannot be lost.
        if self.job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }
        import
    }
    fn accept(
        &mut self,
        id: &str,
        session: &Session,
        outcome: SyncOutcome,
        blocked: bool,
    ) -> Option<Import> {
        match outcome {
            SyncOutcome::Exported { path, base } | SyncOutcome::Unchanged { path, base } => {
                self.live.insert(id.into(), base.clone());
                self.view = Some(View {
                    source: id.into(),
                    revision: session.scene().revision,
                    path: path.clone(),
                    embedded: Some(base.embedded_sha256),
                    observed: Some(base.file_sha256),
                    language_changed: false,
                });
                self.status=format!("Working copy ready: {}. Embedded source remains authoritative; project Save retains matching synchronization bases.",path.display());
            }
            SyncOutcome::ImportCandidate {
                path,
                text,
                expected_embedded_sha256,
                observed_file_sha256,
                next_base,
                ..
            } => {
                self.view = Some(View {
                    source: id.into(),
                    revision: session.scene().revision,
                    path,
                    embedded: Some(ruster_behavior::source_hash(
                        &session.source(id).unwrap().text,
                    )),
                    observed: Some(observed_file_sha256),
                    language_changed: false,
                });
                if blocked
                    || ruster_behavior::source_hash(&session.source(id).unwrap().text)
                        != expected_embedded_sha256
                {
                    self.status="External file changed while embedded drafts/dialogs were active. Both versions preserved; resolve drafts then import the observed version or Refresh.".into();
                    self.open = true;
                } else {
                    let mut source = session.source(id).unwrap().clone();
                    source.text = text;
                    return Some(Import {
                        source_id: id.into(),
                        source,
                        next_base,
                    });
                }
            }
            SyncOutcome::Conflict {
                path,
                embedded_sha256,
                file_sha256,
            } => {
                self.view = Some(View {
                    source: id.into(),
                    revision: session.scene().revision,
                    path,
                    embedded: Some(embedded_sha256),
                    observed: Some(file_sha256),
                    language_changed: false,
                });
                self.status="Embedded and external versions conflict. Both were preserved. Choose the observed file version or explicitly replace that exact file version.".into();
                self.open = true;
            }
            SyncOutcome::Missing { path } => {
                self.view = Some(View {
                    source: id.into(),
                    revision: session.scene().revision,
                    path,
                    embedded: None,
                    observed: None,
                    language_changed: false,
                });
                self.status="Working file is missing. Refresh does not recreate it; Export explicitly to create a copy.".into();
                self.open = true;
            }
            SyncOutcome::LanguageChanged {
                previous_path,
                current_path,
            } => {
                self.view = Some(View {
                    source: id.into(),
                    revision: session.scene().revision,
                    path: current_path,
                    embedded: None,
                    observed: None,
                    language_changed: true,
                });
                self.status=format!("Language/API changed. Existing file {} was preserved; explicitly export the new language/API working copy.",previous_path.display());
                self.open = true;
            }
        }
        None
    }
    pub fn imported(&mut self, base: SyncBase, revision: u64) {
        if let Some(view) = self
            .view
            .as_mut()
            .filter(|view| view.source == base.source_id)
        {
            view.revision = revision;
            view.embedded = Some(base.embedded_sha256.clone());
            view.observed = Some(base.file_sha256.clone());
            view.language_changed = false;
        }
        self.status=format!("Imported observed external version for {} through scene Undo. Save the project to retain it.",base.source_id);
        self.live.insert(base.source_id.clone(), base);
    }
    pub fn ui(
        &mut self,
        ctx: &egui::Context,
        enabled: bool,
        revision: u64,
    ) -> Option<(String, Intent)> {
        if !self.open {
            return None;
        }
        let mut opened = true;
        let mut close = false;
        let mut request = None;
        egui::Window::new("External working copy").open(&mut opened).default_width(520.0).max_width((ctx.content_rect().width()-48.0).max(220.0)).show(ctx,|ui| {
            egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-230.0).max(32.0)).show(ui,|ui| {
                ui.label(&self.status);
                if let Some(view)=&self.view {
                    ui.label(format!("Source {}",view.source));
                    ui.add(egui::Label::new(view.path.display().to_string()).truncate()).on_hover_text(view.path.display().to_string());
                    if let Some(hash)=&view.embedded {ui.add(egui::Label::new(format!("Embedded {hash}")).truncate()).on_hover_text(hash);}
                    if let Some(hash)=&view.observed {ui.add(egui::Label::new(format!("Observed file {hash}")).truncate()).on_hover_text(hash);}
                    if view.revision!=revision {ui.label("Embedded state changed since this observation; Refresh before choosing versions.");}
                }
            });
            if let Some(view)=&self.view {
                ui.add_enabled_ui(enabled && self.job.is_none(),|ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Refresh working copy").clicked() {request=Some((view.source.clone(),Intent::Refresh));}
                        ui.add_enabled_ui(view.revision==revision,|ui| {
                            if view.language_changed && ui.button("Export new language/API copy").clicked() {request=Some((view.source.clone(),Intent::ExportNewLanguage));}
                            if view.observed.is_some() {
                                if ui.button("Open observed file in Zed (keep both versions)").clicked() {request=Some((view.source.clone(),Intent::OpenExisting{line:1}));}
                                if ui.button("Import observed file through Undo").clicked() {request=Some((view.source.clone(),Intent::ImportObserved));}
                                if ui.button("Replace observed file with embedded source").clicked() {request=Some((view.source.clone(),Intent::OverwriteObserved));}
                            } else if !view.language_changed && ui.button("Export working copy").clicked() {request=Some((view.source.clone(),Intent::Export));}
                        });
                    });
                });
            }
            close=ui.button("Close working-copy status").clicked() || (enabled && ui.input_mut(|input|input.consume_key(egui::Modifiers::NONE,egui::Key::Escape)));
        });
        if !opened || close {
            self.open = false;
        }
        request
    }
}
fn run(input: Input) -> Result<Output, String> {
    let context = Context::from_saved_scene(Some(&input.key.scene)).map_err(files_error)?;
    let mut manifest = files::load_manifest(&context).map_err(files_error)?;
    let Some(intent) = input.intent else {
        let promotion =
            files::promote_saved_bases(&context, &manifest, &input.live).map_err(files_error)?;
        return Ok(Output{outcome:None,child:None,warning:String::new(),launch:None,promotion:Some(format!("Saved synchronization bases: {} promoted, {} still unsaved, {} obsolete metadata entries pruned. Working files preserved.",promotion.promoted.len(),promotion.not_saved.len(),promotion.pruned.len()))});
    };
    let id = input.key.source_id.as_deref().unwrap();
    let source = input.source.as_ref().unwrap();
    if let Intent::OpenExisting { line } = intent {
        let expected = input.observed.as_deref().unwrap();
        let observed =
            files::import_observed(&context, id, source, expected).map_err(files_error)?;
        let SyncOutcome::ImportCandidate { path, .. } = observed else {
            return Err(
                "Observed file has no importable UTF-8 source version; no editor launched.".into(),
            );
        };
        return Ok(Output {
            outcome: None,
            child: None,
            warning: String::new(),
            promotion: Some(
                "Observed working file prepared without modifying either version.".into(),
            ),
            launch: Some(LaunchPlan {
                context: context.clone(),
                id: id.into(),
                source: source.clone(),
                path,
                expected: expected.into(),
                config: input.launch,
                line,
            }),
        });
    }
    let durable = manifest.source(id);
    let previous = input.live.get(id).or(durable.as_ref());
    let outcome = match intent {
        Intent::OpenExisting { .. } => unreachable!(),
        Intent::Refresh => files::refresh(&context, id, source, previous),
        Intent::ImportObserved => {
            files::import_observed(&context, id, source, input.observed.as_deref().unwrap())
        }
        Intent::OverwriteObserved => {
            files::overwrite_observed(&context, id, source, input.observed.as_deref().unwrap())
        }
        Intent::ExportNewLanguage => files::export(&context, id, source, None),
        Intent::Export | Intent::Open { .. } => files::export(&context, id, source, previous),
    }
    .map_err(files_error)?;
    let mut warning = String::new();
    let mut launch = None;
    if let Some(authoring) = &input.authoring {
        let tooling_previous = manifest.tooling();
        match files::tooling(&context, authoring, tooling_previous.as_ref()) {
            Ok(output) => {
                let config = output.config;
                let declarations = output.annotation_hints.len();
                match files::persist_tooling(&context, &manifest, &output.base) {
                    Ok(updated) => {
                        manifest = updated;
                        warning=format!("Generated LuaLS workspace {} with {declarations} source declarations; configuration {}.",context.root().display(),config.display());
                    }
                    Err(error) => {
                        warning = format!(
                            "Language tooling ownership was not retained: {}",
                            files_error(error)
                        )
                    }
                }
            }
            Err(error) => {
                warning = format!("Language tooling was not updated: {}", files_error(error))
            }
        }
    }

    if let SyncOutcome::Exported { base, .. } | SyncOutcome::Unchanged { base, .. } = &outcome {
        let mut candidates = input.live;
        candidates.insert(id.into(), base.clone());
        if let Err(error) = files::promote_saved_bases(&context, &manifest, &candidates) {
            warning.push_str(&format!(
                " Saved-base promotion preserved previous manifest: {error}"
            ));
        }
    }
    if let Intent::Open { line } = intent {
        if let SyncOutcome::Exported { path, base } | SyncOutcome::Unchanged { path, base } =
            &outcome
        {
            launch = Some(LaunchPlan {
                context: context.clone(),
                id: id.into(),
                source: source.clone(),
                path: path.clone(),
                expected: base.file_sha256.clone(),
                config: input.launch,
                line,
            });
        }
    }
    Ok(Output {
        outcome: Some(outcome),
        child: None,
        warning,
        promotion: None,
        launch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    fn fixture() -> (tempfile::TempDir, Session, LaunchSpec) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut session = Session::new(ruster_scene::Scene::demo()).unwrap();
        session
            .set_source(
                0,
                "script",
                Some(BehaviorSource {
                    label: Some("Working copy".into()),
                    language_id: "lua54".into(),
                    host_api_version: 1,
                    text: "return {update=function(o) return o end} -- A".into(),
                }),
            )
            .unwrap();
        session.save(path).unwrap();
        (
            dir,
            session,
            crate::workspace::Preferences::default().external_editor,
        )
    }
    fn finish(
        external: &mut External,
        ctx: &egui::Context,
        session: &Session,
        generation: u64,
        blocked: bool,
        launch: &LaunchSpec,
    ) -> Option<Import> {
        let until = Instant::now() + Duration::from_secs(5);
        while external.job.as_ref().is_some_and(|job| !job.is_finished()) {
            assert!(Instant::now() < until, "working-copy completion deadline");
            std::thread::park_timeout(Duration::from_millis(1));
        }
        external.poll(ctx, session, generation, blocked, Some("script"), launch)
    }
    fn replace(session: &mut Session, text: &str) {
        let mut source = session.source("script").unwrap().clone();
        source.text = text.into();
        session
            .set_source(session.scene().revision, "script", Some(source))
            .unwrap();
    }
    fn marker_launch(marker: &std::path::Path) -> LaunchSpec {
        let path = serde_json::to_string(marker.to_str().unwrap()).unwrap();
        LaunchSpec {
            executable: PathBuf::from("python3"),
            args: vec![
                "-c".into(),
                format!("import pathlib,sys;pathlib.Path({path}).write_text(sys.argv[1])"),
                "{file}".into(),
            ],
        }
    }
    #[test]
    fn completion_wake_before_thread_return_keeps_polling_then_returns_to_idle() {
        use std::sync::mpsc;
        let (_dir, session, launch) = fixture();
        let ctx = egui::Context::default();
        let mut external = External::default();
        let (notified, wake) = mpsc::sync_channel(1);
        let (resume, release) = mpsc::sync_channel(1);
        external.return_gate = Some((notified, release));
        external
            .request(&ctx, &session, 1, "script", Intent::Export, &launch)
            .unwrap();
        wake.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            !external.job.as_ref().unwrap().is_finished(),
            "the worker has notified before returning its completion"
        );
        let frame = |external: &mut External| {
            let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
                external.poll(&ctx, &session, 1, false, Some("script"), &launch);
            });
            let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
            output.textures_delta.clear();
            delay
        };
        let mut delay = Duration::MAX;
        // Drain initial egui repaint debt while the sole worker wake is consumed.
        for _ in 0..5 {
            delay = frame(&mut external);
        }
        assert!(delay<=Duration::from_millis(50),"pending job must retain a bounded completion poll even after the early wake: {delay:?}");
        resume.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while !external.job.as_ref().unwrap().is_finished() {
            assert!(Instant::now() < until, "thread completion deadline");
            std::thread::park_timeout(Duration::from_millis(1));
        }
        frame(&mut external);
        assert!(external.finished());
        assert!(external.status.contains("Working copy ready"));
        for _ in 0..5 {
            delay = frame(&mut external);
        }
        assert_eq!(
            delay,
            Duration::MAX,
            "completed/idle controllers must not retain a poll timer"
        );
    }
    fn finish_jobs(
        external: &mut External,
        ctx: &egui::Context,
        session: &Session,
        generation: u64,
        blocked: bool,
        launch: &LaunchSpec,
    ) {
        for _ in 0..3 {
            if external.job.is_none() {
                break;
            }
            assert!(finish(external, ctx, session, generation, blocked, launch).is_none());
        }
        assert!(
            external.job.is_none(),
            "one preparation and one bounded launch stage must finish"
        );
        let until = Instant::now() + Duration::from_secs(5);
        while !external.children.is_empty() {
            external.reap();
            assert!(Instant::now() < until, "test marker launcher did not exit");
            if !external.children.is_empty() {
                std::thread::park_timeout(Duration::from_millis(1));
            }
        }
    }
    #[test]
    fn stale_or_closing_preparation_never_launches_and_conflict_can_open_without_overwrite() {
        use std::sync::mpsc;
        for cause in ["document", "close", "draft"] {
            let (dir, session, _) = fixture();
            let marker = dir.path().join("launched");
            let launch = marker_launch(&marker);
            let ctx = egui::Context::default();
            let mut external = External::default();
            let (reached, ready) = mpsc::sync_channel(1);
            let (resume, release) = mpsc::sync_channel(1);
            external.prepare_gate = Some((reached, release));
            external
                .request(
                    &ctx,
                    &session,
                    1,
                    "script",
                    Intent::Open { line: 1 },
                    &launch,
                )
                .unwrap();
            ready.recv_timeout(Duration::from_secs(5)).unwrap();
            let generation = if cause == "document" {
                external.observe_document(&session, 2);
                2
            } else {
                1
            };
            if cause == "close" {
                external.set_closing(true);
            }
            resume.send(()).unwrap();
            finish_jobs(
                &mut external,
                &ctx,
                &session,
                generation,
                cause == "draft",
                &launch,
            );
            assert!(!marker.exists(),"{cause} must reject launch authorization, not merely ignore its already-launched result");
        }
        let (dir, mut session, _) = fixture();
        let marker = dir.path().join("opened-conflict");
        let launch = marker_launch(&marker);
        let ctx = egui::Context::default();
        let mut external = External::default();
        external
            .request(&ctx, &session, 1, "script", Intent::Export, &launch)
            .unwrap();
        finish_jobs(&mut external, &ctx, &session, 1, false, &launch);
        let file = external.view.as_ref().unwrap().path.clone();
        std::fs::write(&file, "external version to inspect").unwrap();
        replace(&mut session, "embedded version to preserve");
        external
            .request(&ctx, &session, 1, "script", Intent::Refresh, &launch)
            .unwrap();
        finish_jobs(&mut external, &ctx, &session, 1, false, &launch);
        assert!(external.status.contains("conflict"));
        let before = session.snapshot_bytes().unwrap();
        external
            .request(
                &ctx,
                &session,
                1,
                "script",
                Intent::OpenExisting { line: 2 },
                &launch,
            )
            .unwrap();
        finish_jobs(&mut external, &ctx, &session, 1, false, &launch);
        assert_eq!(
            std::fs::read_to_string(marker).unwrap(),
            file.display().to_string()
        );
        assert_eq!(
            std::fs::read_to_string(file).unwrap(),
            "external version to inspect"
        );
        assert_eq!(
            session.snapshot_bytes().unwrap(),
            before,
            "opening a conflicting file must not choose either source version"
        );
    }
    #[test]
    fn unsaved_export_keeps_durable_saved_base_and_restart_refresh_imports_observed_file() {
        let (_dir, mut session, launch) = fixture();
        let ctx = egui::Context::default();
        let mut external = External::default();
        external
            .request(&ctx, &session, 1, "script", Intent::Export, &launch)
            .unwrap();
        assert!(finish(&mut external, &ctx, &session, 1, false, &launch).is_none());
        let context = Context::from_saved_scene(session.path()).unwrap();
        let original = ruster_behavior::source_hash(&session.source("script").unwrap().text);
        assert_eq!(
            files::load_manifest(&context)
                .unwrap()
                .source("script")
                .unwrap()
                .embedded_sha256,
            original
        );
        replace(&mut session, "unfinished ( -- B");
        external
            .request(&ctx, &session, 1, "script", Intent::Export, &launch)
            .unwrap();
        finish(&mut external, &ctx, &session, 1, false, &launch);
        assert_eq!(
            files::load_manifest(&context)
                .unwrap()
                .source("script")
                .unwrap()
                .embedded_sha256,
            original,
            "unsaved export must not promote B over saved A"
        );
        let path = session.path().unwrap().to_path_buf();
        drop(external);
        let mut reopened = Session::open(&path).unwrap();
        let mut external = External::default();
        external
            .request(&ctx, &reopened, 2, "script", Intent::Refresh, &launch)
            .unwrap();
        let import = finish(&mut external, &ctx, &reopened, 2, false, &launch)
            .expect("restart should offer file-only B instead of overwriting it with saved A");
        assert_eq!(import.source.text, "unfinished ( -- B");
        reopened
            .set_source(
                reopened.scene().revision,
                &import.source_id,
                Some(import.source),
            )
            .unwrap();
        external.imported(import.next_base, reopened.scene().revision);
        reopened.save(path).unwrap();
        external.saved(&reopened, 2);
        external.poll(&ctx, &reopened, 2, false, Some("script"), &launch);
        finish(&mut external, &ctx, &reopened, 2, false, &launch);
        assert_eq!(
            files::load_manifest(&context)
                .unwrap()
                .source("script")
                .unwrap()
                .embedded_sha256,
            ruster_behavior::source_hash("unfinished ( -- B")
        );
    }
    #[test]
    fn stale_jobs_drafts_and_explicit_observed_conflicts_preserve_authoring() {
        let (_dir, mut session, launch) = fixture();
        let ctx = egui::Context::default();
        let mut external = External::default();
        external
            .request(&ctx, &session, 1, "script", Intent::Export, &launch)
            .unwrap();
        finish(&mut external, &ctx, &session, 1, false, &launch);
        let path = external.view.as_ref().unwrap().path.clone();
        std::fs::write(&path, "external B").unwrap();
        external
            .request(&ctx, &session, 1, "script", Intent::Refresh, &launch)
            .unwrap();
        session
            .rename(
                session.scene().revision,
                "left",
                Some("Unrelated authored edit".into()),
            )
            .unwrap();
        let before = session.snapshot_bytes().unwrap();
        assert!(finish(&mut external, &ctx, &session, 1, false, &launch).is_none());
        assert_eq!(session.snapshot_bytes().unwrap(), before);
        assert!(external.status.contains("changed during"));
        external
            .request(&ctx, &session, 1, "script", Intent::Refresh, &launch)
            .unwrap();
        assert!(finish(&mut external, &ctx, &session, 1, true, &launch).is_none());
        assert!(external.status.contains("drafts"));
        assert_eq!(session.snapshot_bytes().unwrap(), before);
        replace(&mut session, "embedded D");
        assert!(external
            .request(&ctx, &session, 1, "script", Intent::ImportObserved, &launch)
            .unwrap_err()
            .contains("Refresh before resolving"));
        external
            .request(&ctx, &session, 1, "script", Intent::Refresh, &launch)
            .unwrap();
        finish(&mut external, &ctx, &session, 1, false, &launch);
        assert!(external.status.contains("conflict"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external B");
        assert_eq!(session.source("script").unwrap().text, "embedded D");
        std::fs::write(&path, "external C after observation").unwrap();
        external
            .request(&ctx, &session, 1, "script", Intent::ImportObserved, &launch)
            .unwrap();
        assert!(finish(&mut external, &ctx, &session, 1, false, &launch).is_none());
        assert_eq!(session.source("script").unwrap().text, "embedded D");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "external C after observation"
        );
        external
            .request(&ctx, &session, 1, "script", Intent::Refresh, &launch)
            .unwrap();
        finish(&mut external, &ctx, &session, 1, false, &launch);
        let before = session.snapshot_bytes().unwrap();
        external
            .request(
                &ctx,
                &session,
                1,
                "script",
                Intent::OverwriteObserved,
                &launch,
            )
            .unwrap();
        finish(&mut external, &ctx, &session, 1, false, &launch);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "embedded D");
        assert_eq!(
            session.snapshot_bytes().unwrap(),
            before,
            "explicit file replacement must not create an authored edit"
        );
    }
}
