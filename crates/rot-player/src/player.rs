//! Runtime host application for packaged v23 creations.

use std::{
    collections::BTreeMap,
    sync::{mpsc, Arc},
    time::Duration,
};

use rot_app::audio::{Output as AudioOutput, OutputStatus};
use rot_app::creation::StartupTransition;
use rot_app::hud::{paint_hud, HudPresentationMode};
use rot_app::ResourceRoot;
use rot_app::{egui, Application, Content, MeshMap, Step, TextureMap};
use rot_behavior::{Diagnostic, Engine, Limits, Runtime};
use rot_scene::component_scene::{Component, ComponentScene, ObjectHierarchy};
use rot_scene::hud::{resolve_hud, HudCache, ResolvedHud};
use rot_scene::input::{
    ActionFrame, ActionKind, ActionMap, ActionValue, InputKey, KeyEventAccumulator, PhysicalInput,
};
use rot_scene::{Authoring, ComponentDrawPlan};

mod commands;
mod hud_controls;
use commands::CommandUi;
pub use commands::{dispatch_command, CommandRequest};
use hud_controls::HudControls;

struct StartTarget {
    scene: Box<ComponentScene>,
    authoring: Authoring,
    content: Content,
    sounds: rot_app::audio::Sounds,
    clips: rot_behavior::Clips,
    title: String,
}

impl StartTarget {
    fn load(root: &ResourceRoot, route: &StartupTransition, dt: Duration) -> Result<Self, String> {
        let (resources, manifest, runtime_scene) = route.load_destination(root)?;
        if manifest.tick_duration() != dt {
            return Err("Start Transition scenes require the same fixed tick duration.".into());
        }
        let meshes = manifest.load_meshes(&resources)?;
        let textures = manifest.load_textures(&resources)?;
        let sounds = manifest.load_sounds(&resources)?;
        let clips = rot_behavior::decode_clips(&manifest.load_animations(&resources)?)?;
        let scene = runtime_scene.into_scene();
        let hierarchy = ObjectHierarchy::build(&scene.objects).map_err(|e| e.to_string())?;
        let authoring = scene.authoring_snapshot(&hierarchy);
        let frame = ComponentDrawPlan::build_initial_frame(&scene, 0).map_err(|e| e.to_string())?;
        let content = Content::from_component_frame(frame, meshes, textures)?;
        Ok(Self {
            scene,
            authoring,
            content,
            sounds,
            clips,
            title: manifest.title,
        })
    }
}

/// Standalone runtime application driving a packaged v23 scene.
pub struct PlayerApp {
    title: String,
    scene: Box<ComponentScene>,
    authoring: Authoring,
    content: Content,
    fixed_dt: Duration,
    engine: Option<Engine>,
    runtime: Option<Runtime>,
    needs_reset: bool,
    diagnostic: Option<Diagnostic>,
    action_keys: KeyEventAccumulator,
    physical_input_owned: bool,
    pending_input: Option<ActionFrame>,
    last_input: Option<(u64, ActionFrame, &'static str)>,
    run_epoch: u64,
    command_ui: CommandUi,
    command_receipt: Option<rot_behavior::CommandReceipt>,
    hud_cache: HudCache,
    hud_controls: HudControls,
    sounds: BTreeMap<String, Arc<rot_assets::AudioClip>>,
    audio: AudioOutput,
    clips: rot_behavior::Clips,
    start_transition: Option<(ResourceRoot, StartupTransition)>,
    asynchronous_start: bool,
    pending_start: Option<mpsc::Receiver<Result<StartTarget, String>>>,
    host_tick_origin: u64,
    scene_switched: bool,
    scene_switch_pending_presentation: bool,
}

impl PlayerApp {
    /// Load authored v23 scene and resources without executing scripts until `reset`.
    pub fn from_component_scene(
        title: String,
        scene: Box<ComponentScene>,
        meshes: MeshMap,
        textures: TextureMap,
        fixed_dt: Duration,
    ) -> Result<Self, String> {
        let hierarchy = ObjectHierarchy::build(&scene.objects).map_err(|e| e.to_string())?;
        let authoring = scene.authoring_snapshot(&hierarchy);
        let frame = ComponentDrawPlan::build_initial_frame(&scene, 0).map_err(|e| e.to_string())?;
        let content = Content::from_component_frame(frame, meshes, textures)?;
        Self::with_content(title, scene, authoring, content, fixed_dt)
    }

    fn with_content(
        title: String,
        scene: Box<ComponentScene>,
        authoring: Authoring,
        content: Content,
        fixed_dt: Duration,
    ) -> Result<Self, String> {
        if !(Duration::from_millis(1)..=Duration::from_secs(1)).contains(&fixed_dt) {
            return Err("Creation fixed tick duration must be 1 ms..=1 s".into());
        }
        let action_keys =
            KeyEventAccumulator::new(scene.actions.clone()).map_err(|e| e.to_string())?;
        Ok(Self {
            title,
            scene,
            authoring,
            content,
            fixed_dt,
            engine: None,
            runtime: None,
            needs_reset: true,
            diagnostic: None,
            action_keys,
            physical_input_owned: false,
            pending_input: None,
            last_input: None,
            run_epoch: 0,
            command_ui: CommandUi::default(),
            command_receipt: None,
            hud_cache: HudCache::default(),
            hud_controls: HudControls::default(),
            sounds: BTreeMap::new(),
            audio: AudioOutput::default(),
            clips: rot_behavior::Clips::new(),
            start_transition: None,
            asynchronous_start: false,
            pending_start: None,
            host_tick_origin: 0,
            scene_switched: false,
            scene_switch_pending_presentation: false,
        })
    }

    pub fn set_start_transition(&mut self, root: ResourceRoot, route: StartupTransition) {
        self.start_transition = Some((root, route));
    }

    pub fn has_start_transition(&self) -> bool {
        self.start_transition.is_some()
    }

    pub fn prepare_start_on_worker(&mut self) {
        self.asynchronous_start = true;
    }

    pub fn start_transition_pending(&self) -> bool {
        self.pending_start.is_some()
    }

    pub fn switched_scene(&self) -> bool {
        self.scene_switched
    }

    pub fn is_static_component(&self) -> bool {
        if let Some(runtime) = &self.runtime {
            return runtime.is_static_component();
        }
        let Ok(hierarchy) = ObjectHierarchy::build(&self.scene.objects) else {
            return false;
        };
        let has_script = self
            .scene
            .objects
            .iter()
            .enumerate()
            .filter(|(index, _)| hierarchy.effectively_enabled[*index])
            .any(|(_, object)| {
                object.components.iter().any(|component| {
                    component.enabled && matches!(component.kind, Component::Script { .. })
                })
            });
        let has_animation = !self.scene.active_animation_clip_ids(&hierarchy).is_empty();
        !has_script && !has_animation
    }

    pub fn authored_actions(&self) -> &ActionMap {
        &self.scene.actions
    }

    pub fn authored_hud(&self) -> &rot_scene::hud::Hud {
        &self.scene.hud
    }

    pub fn scene_id(&self) -> &str {
        &self.scene.id
    }

    pub fn authored_scene_json(&self) -> Result<serde_json::Value, String> {
        serde_json::to_value(&self.scene).map_err(|e| e.to_string())
    }

    pub fn runtime_scene_json(&self) -> Result<serde_json::Value, String> {
        if let Some(updated) = self
            .runtime
            .as_ref()
            .and_then(Runtime::updated_component_scene)
        {
            serde_json::to_value(updated).map_err(|e| e.to_string())
        } else {
            serde_json::to_value(&self.scene).map_err(|e| e.to_string())
        }
    }

    pub fn world_matrices_json(&self) -> Option<serde_json::Value> {
        let runtime = self.runtime.as_ref()?;
        let scene = runtime.component_scene()?;
        let matrices = runtime.world_matrices()?;
        let map = scene
            .objects
            .iter()
            .zip(matrices)
            .map(|(object, matrix)| (&object.id, matrix.to_cols_array_2d()))
            .collect::<BTreeMap<_, _>>();
        serde_json::to_value(map).ok()
    }

    fn start_error(reason: String) -> String {
        format!("Start Transition failed. {reason} Restore the destination package and Reset.")
    }

    fn start_transition(&mut self, host_tick: u64) -> Result<(), String> {
        let (root, route) = self
            .start_transition
            .as_ref()
            .expect("transition press matched");
        if !self.asynchronous_start {
            let target =
                StartTarget::load(root, route, self.fixed_dt).map_err(Self::start_error)?;
            return self
                .commit_start_transition(target, host_tick)
                .map_err(Self::start_error);
        }
        let (send, receive) = mpsc::sync_channel(1);
        let (root, route, dt) = (root.clone(), route.clone(), self.fixed_dt);
        std::thread::Builder::new()
            .name("rot-start-transition".into())
            .spawn(move || {
                let _ = send.send(StartTarget::load(&root, &route, dt));
            })
            .map_err(|error| Self::start_error(error.to_string()))?;
        self.pending_start = Some(receive);
        Ok(())
    }

    fn poll_start_transition(&mut self, host_tick: u64) -> Result<(), String> {
        let result = match self
            .pending_start
            .as_ref()
            .expect("pending target")
            .try_recv()
        {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return Ok(()),
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("The destination could not be prepared.".into())
            }
        };
        self.pending_start = None;
        result
            .and_then(|target| self.commit_start_transition(target, host_tick))
            .map_err(Self::start_error)
    }

    fn commit_start_transition(
        &mut self,
        target: StartTarget,
        host_tick: u64,
    ) -> Result<(), String> {
        let next_epoch = self
            .run_epoch
            .checked_add(1)
            .ok_or("Runtime identity exhausted")?;
        let mut candidate = Self::with_content(
            target.title,
            target.scene,
            target.authoring,
            target.content,
            self.fixed_dt,
        )?;
        candidate.set_sounds(target.sounds);
        candidate.set_clips(target.clips);
        candidate.reset()?;
        candidate.content.validate()?;
        candidate.physical_input_owned = self.physical_input_owned;
        let next_keys = KeyEventAccumulator::new(candidate.scene.actions.clone())
            .expect("destination map validated");
        self.action_keys.switch_scene(next_keys);
        std::mem::swap(&mut candidate.action_keys, &mut self.action_keys);
        candidate.run_epoch = next_epoch;
        candidate.host_tick_origin = host_tick;
        candidate.scene_switched = true;
        candidate.scene_switch_pending_presentation = true;
        *self = candidate;
        Ok(())
    }

    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        self.diagnostic.as_ref()
    }

    pub fn runtime_tick(&self) -> Option<u64> {
        self.runtime.as_ref().map(Runtime::tick)
    }

    pub fn runtime_state_report(
        &self,
    ) -> std::collections::BTreeMap<&str, &rot_scene::StateFields> {
        self.runtime
            .as_ref()
            .map(Runtime::state_report)
            .unwrap_or_default()
    }

    pub fn runtime_commit_revision(&self) -> Option<u64> {
        self.runtime.as_ref().map(Runtime::commit_revision)
    }

    pub fn last_step_commit_revision(&self) -> Option<u64> {
        self.runtime
            .as_ref()
            .and_then(Runtime::last_step_commit_revision)
    }

    pub fn command_report(&self) -> serde_json::Value {
        self.runtime.as_ref().map_or_else(
            || serde_json::json!([]),
            |runtime| {
                let rows: Vec<_> = runtime
                    .authoring_snapshot()
                    .bindings
                    .iter()
                    .filter(|(_, binding)| binding.target == rot_scene::BindingTarget::Application)
                    .flat_map(|(binding_id, binding)| {
                        binding.commands.iter().map(move |(id, definition)| {
                            serde_json::json!({"binding_id":binding_id,"command_id":id,
                        "source_id":binding.source_id,"enabled":binding.enabled,
                        "schema_hash":runtime.command_schema_hash(binding_id),
                        "argument_count":definition.arguments.len()})
                        })
                    })
                    .take(rot_scene::MAX_COMMANDS)
                    .collect();
                serde_json::json!(rows)
            },
        )
    }

    pub fn command_receipt_report(&self) -> serde_json::Value {
        self.command_receipt
            .as_ref()
            .map(commands::receipt_wire)
            .unwrap_or(serde_json::Value::Null)
    }

    pub fn query_observations(&self) -> &[rot_behavior::QueryObservation] {
        self.runtime
            .as_ref()
            .map_or(&[], Runtime::query_observations)
    }

    /// Bounded terminal/headless observation.
    pub fn hud_snapshot(&self) -> ResolvedHud {
        if let Some(runtime) = &self.runtime {
            let authoring = runtime.authoring_snapshot();
            resolve_hud(&authoring.hud, authoring, |binding, field| {
                runtime.state().binding(binding)?.get(field)
            })
        } else {
            resolve_hud(&self.authoring.hud, &self.authoring, |binding, field| {
                self.authoring
                    .bindings
                    .get(binding)?
                    .state_defaults
                    .get(field)
            })
        }
    }

    pub fn hud_presentation_mode(&self) -> HudPresentationMode {
        match (&self.runtime, self.needs_reset) {
            (Some(_), true) => HudPresentationMode::Stale,
            (Some(_), false) => HudPresentationMode::Committed,
            (None, _) => HudPresentationMode::AuthoredPreview,
        }
    }

    pub fn fixed_dt(&self) -> Duration {
        self.fixed_dt
    }

    fn supply_next_input(&mut self, frame: ActionFrame) -> Result<(), String> {
        if self.needs_reset || self.runtime_tick().is_none() {
            return Err("Reset before supplying input".into());
        }
        if self.physical_input_owned || self.pending_input.is_some() {
            return Err("Input control or a supplied frame already owns the next step".into());
        }
        let frame = frame
            .validate_against(&self.scene.actions)
            .map_err(|e| e.to_string())?
            .into_frame();
        self.action_keys.cancel();
        self.pending_input = Some(frame);
        Ok(())
    }

    pub fn input_observation(&self) -> Option<(u64, &ActionFrame, &'static str)> {
        self.last_input
            .as_ref()
            .map(|(tick, frame, origin)| (*tick, frame, *origin))
    }

    fn latch_physical_input(
        &mut self,
        events: impl IntoIterator<Item = (PhysicalInput, bool)>,
    ) -> Result<(), String> {
        if self.needs_reset || self.pending_input.is_some() {
            self.cancel_action_input();
            return Err("Reset or finish the supplied step before claiming input".into());
        }
        self.physical_input_owned = true;
        for (input, pressed) in events {
            self.action_keys.physical_event(input, pressed);
        }
        Ok(())
    }

    fn action_help(&self, ui: &mut egui::Ui) {
        let actions = &self.scene.actions;
        if actions.is_empty() {
            return;
        }
        ui.heading("Controls");
        ui.label(if self.physical_input_owned {
            "Scene controls active · Esc to release"
        } else {
            "Run, then choose Control runtime"
        });
        let keys = |values: &[InputKey]| {
            if values.is_empty() {
                "Unbound".to_owned()
            } else {
                values
                    .iter()
                    .map(|key| key.logical_name())
                    .collect::<Vec<_>>()
                    .join(" / ")
            }
        };
        egui::ScrollArea::vertical()
            .id_salt("authored-action-help")
            .max_height(240.0)
            .show(ui, |ui| {
                for (id, definition) in actions {
                    let hint = match &definition.kind {
                        ActionKind::Button {
                            keys: bound,
                            primary_mouse,
                        } => match (bound.is_empty(), primary_mouse) {
                            (true, true) => "Primary Mouse".into(),
                            (false, true) => format!("{} / Primary Mouse", keys(bound)),
                            (_, false) => keys(bound),
                        },
                        ActionKind::Axis1d { negative, positive } => {
                            format!("− {}   + {}", keys(negative), keys(positive))
                        }
                    };
                    ui.strong(definition.label.as_deref().unwrap_or(id));
                    ui.label(hint);
                    if let Some((_, frame, _)) = &self.last_input {
                        if let Some(value) = frame.0.get(id) {
                            let text = match value {
                                ActionValue::Button(value) => format!(
                                    "Held {} · Pressed {} · Released {}",
                                    value.held, value.pressed, value.released
                                ),
                                ActionValue::Axis1d { value } => format!("Value {value}"),
                            };
                            ui.label(text);
                        }
                    }
                }
            });
        if let Some((tick, _, origin)) = &self.last_input {
            ui.label(format!("Last committed input: tick {tick} · {origin}"));
        }
        ui.separator();
        let authoring = self.runtime.as_ref().map_or(&self.authoring, |runtime| {
            runtime.authoring_snapshot().as_ref()
        });
        for binding in authoring
            .bindings
            .values()
            .filter(|binding| binding.enabled)
            .take(16)
        {
            if let Some(object) = self
                .content
                .scene()
                .objects
                .iter()
                .find(|object| Some(object.id.as_str()) == binding.object_id())
            {
                ui.strong(object.display_name());
                ui.label(format!("Position {:.3?}", object.transform.translation));
                ui.label(format!("Rotation {:.3?}", object.transform.rotation));
            }
        }
        ui.separator();
    }

    pub fn set_sounds(&mut self, sounds: BTreeMap<String, Arc<rot_assets::AudioClip>>) {
        self.sounds = sounds;
        self.audio.stop();
    }

    pub fn set_clips(&mut self, clips: rot_behavior::Clips) {
        self.clips = clips;
        self.needs_reset = true;
    }

    pub fn audio_batch(&self) -> Option<&rot_behavior::AudioBatch> {
        self.runtime.as_ref()?.audio_batch()
    }

    fn report<T>(&mut self, result: Result<T, Diagnostic>) -> Result<T, String> {
        result.map_err(|error| {
            let message = serde_json::to_string(&error).expect("diagnostic serializes");
            self.diagnostic = Some(error);
            message
        })
    }
}

fn emit_script_logs(runtime: &Runtime) {
    for entry in runtime.script_logs() {
        match entry.level {
            rot_behavior::ScriptLogLevel::Info => {
                println!("[Script][{}] {}", entry.object_id, entry.message);
            }
            rot_behavior::ScriptLogLevel::Warning => {
                eprintln!("[Script][{}] {}", entry.object_id, entry.message);
            }
        }
    }
}

/// Explicit supplied-frame execution with outer-clock authority.
pub fn step_with_input(
    runtime: &mut rot_app::Runtime<PlayerApp>,
    expected_tick: u64,
    frame: ActionFrame,
) -> Result<(), String> {
    if runtime.is_running() || runtime.tick() != expected_tick {
        return Err(
            "Supplied input requires paused execution at the expected committed tick".into(),
        );
    }
    runtime.app_mut().supply_next_input(frame)?;
    runtime.step()
}

impl Application for PlayerApp {
    fn content(&self) -> &Content {
        &self.content
    }

    fn reset(&mut self) -> Result<(), String> {
        self.host_tick_origin = 0;
        self.scene_switch_pending_presentation = false;
        self.pending_start = None;
        self.audio.stop();
        self.cancel_action_input();
        self.cancel_hud_input();
        self.hud_controls.reset();
        self.needs_reset = true;
        self.last_input = None;
        self.command_ui = CommandUi::default();
        let next_epoch = self
            .run_epoch
            .checked_add(1)
            .ok_or("Runtime identity exhausted")?;
        let keys =
            KeyEventAccumulator::new(self.scene.actions.clone()).map_err(|e| e.to_string())?;
        if self.engine.is_none() {
            let result = Engine::new(Limits {
                fixed_dt: self.fixed_dt.as_secs_f64(),
                ..Limits::default()
            });
            self.engine = Some(self.report(result)?);
        }
        let engine = self.engine.as_mut().expect("initialized engine");
        engine.set_clips(self.clips.clone());
        let result = engine.reset_component(&self.scene, 0);
        let candidate = self.report(result)?;
        emit_script_logs(&candidate);
        self.content.set_behavior_draw_frame(candidate.draw_frame());
        self.hud_cache.clear();
        self.runtime = Some(candidate);
        self.audio.reset(self.sounds.clone());
        self.audio.pause(true);
        self.run_epoch = next_epoch;
        self.command_receipt = None;
        self.action_keys = keys;
        self.needs_reset = false;
        self.diagnostic = None;
        Ok(())
    }

    fn take_scene_switch(&mut self) -> bool {
        std::mem::take(&mut self.scene_switch_pending_presentation)
    }

    fn playback_changed(&mut self, playing: bool) {
        self.audio.pause(!playing);
    }

    fn live_step_committed(&mut self) {
        if self.pending_start.is_some() {
            return;
        }
        if let Some(batch) = self.runtime.as_ref().and_then(Runtime::audio_batch) {
            self.audio.consume_spatial(
                batch.commit_revision,
                batch
                    .cues
                    .iter()
                    .map(|cue| (cue.asset_id.as_str(), cue.gain, cue.pan)),
            );
        }
    }

    fn buffered_viewport_input(&self) -> bool {
        self.needs_hud_input_phase()
    }

    fn viewport_frame_begin(&mut self, ctx: &egui::Context) -> Result<(), String> {
        self.begin_hud_frame(ctx);
        Ok(())
    }

    fn viewport_interaction(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
    ) -> Result<rot_app::ViewportInputOwnership, String> {
        Ok(self.interact_hud(ui, rect, None))
    }

    fn viewport_interaction_with_game_focus(
        &mut self,
        ui: &mut egui::Ui,
        rect: egui::Rect,
        game_focus: Option<egui::Id>,
    ) -> Result<rot_app::ViewportInputOwnership, String> {
        Ok(self.interact_hud(ui, rect, game_focus))
    }

    fn cancel_viewport_input(&mut self) {
        self.cancel_hud_input();
        self.cancel_action_input();
    }

    fn viewport_overlay(
        &mut self,
        painter: &egui::Painter,
        rect: egui::Rect,
    ) -> Result<(), String> {
        if self.paint_prepared_controls(painter) {
            return Ok(());
        }
        let mode = self.hud_presentation_mode();
        if let Some(runtime) = &self.runtime {
            let authoring = runtime.authoring_snapshot();
            let resolved = self
                .hud_cache
                .prepare(&authoring.hud, authoring, |binding, field| {
                    runtime.state().binding(binding)?.get(field)
                });
            paint_hud(painter, rect, &authoring.hud, resolved, mode);
        } else {
            let resolved =
                self.hud_cache
                    .prepare(&self.authoring.hud, &self.authoring, |binding, field| {
                        self.authoring
                            .bindings
                            .get(binding)?
                            .state_defaults
                            .get(field)
                    });
            paint_hud(painter, rect, &self.authoring.hud, resolved, mode);
        }
        Ok(())
    }

    fn keyboard_actions(&self) -> Option<&ActionMap> {
        (!self.scene.actions.is_empty()).then_some(&self.scene.actions)
    }

    fn action_input(&mut self, owned: bool, events: &[(InputKey, bool)]) -> Result<(), String> {
        if !owned {
            self.cancel_action_input();
            return Ok(());
        }
        self.latch_physical_input(
            events
                .iter()
                .map(|&(key, pressed)| (PhysicalInput::Key(key), pressed)),
        )
    }

    fn action_physical_input(&mut self, events: &[(PhysicalInput, bool)]) -> Result<(), String> {
        self.latch_physical_input(events.iter().copied())
    }

    fn cancel_action_input(&mut self) {
        self.action_keys.cancel();
        self.physical_input_owned = false;
        self.pending_input = None;
    }

    fn tick(&mut self, step: Step) -> Result<(), String> {
        let supplied = self.pending_input.take();
        if self.needs_reset {
            return Err("Reset before running this packaged scene".into());
        }
        if step.dt != self.fixed_dt {
            return Err("Host Step.dt differs from the packaged fixed tick".into());
        }
        if self.pending_start.is_some() {
            return self.poll_start_transition(step.tick);
        }
        let local_tick = step
            .tick
            .checked_sub(self.host_tick_origin)
            .ok_or("Invalid host tick")?;
        if self.runtime_tick().and_then(|tick| tick.checked_add(1)) != Some(local_tick) {
            return Err("Host and behavior tick differ; Reset through the host Runtime".into());
        }
        let origin = if supplied.is_some() {
            "programmatic"
        } else if self.physical_input_owned {
            "native"
        } else {
            "neutral"
        };
        let mut input = supplied.unwrap_or_else(|| self.action_keys.take_snapshot().into_frame());
        if let Some(runtime) = &mut self.runtime {
            runtime.merge_hud_action(&mut input);
        }
        if self.start_transition.as_ref().is_some_and(|(_, route)| {
            matches!(input.0.get(&route.action_id), Some(ActionValue::Button(state)) if state.pressed)
        }) {
            return self.start_transition(step.tick);
        }
        let runtime = self.runtime.as_mut().ok_or("Reset before running")?;
        let result = self
            .engine
            .as_mut()
            .expect("Reset initialized engine")
            .step_with_input(runtime, &input);
        if result.is_err() {
            self.cancel_action_input();
            self.audio.stop();
            self.needs_reset = true;
        }
        self.report(result)?;
        self.last_input = Some((local_tick, input, origin));
        let runtime = self.runtime.as_ref().expect("active runtime");
        emit_script_logs(runtime);
        self.content.set_behavior_draw_frame(runtime.draw_frame());
        self.diagnostic = None;
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        self.hud_action_status(ui);
        let audio_notice = match self.audio.status() {
            OutputStatus::Unavailable => Some("Audio output unavailable."),
            OutputStatus::MissingAsset => Some("A sound cue's WAV is unavailable."),
            OutputStatus::VoiceLimit => Some("Audio voice limit reached. A cue was dropped."),
            _ => None,
        };
        if let Some(notice) = audio_notice {
            ui.colored_label(ui.visuals().warn_fg_color, notice);
        }
        ui.heading(&self.title);
        if self.start_transition_pending() {
            ui.label("Preparing destination…");
        }
        self.action_help(ui);
        if let Some(error) = &self.diagnostic {
            ui.colored_label(ui.visuals().error_fg_color, error.to_string());
        }
        Ok(())
    }

    fn ui_with_playback(&mut self, ui: &mut egui::Ui, playing: bool) -> Result<(), String> {
        self.commands_ui(ui, playing);
        self.ui(ui)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_app::creation::{STARTUP_TARGET_DIRECTORY, STARTUP_TRANSITION_NAME};
    use rot_scene::component_scene::ComponentScene;
    use rot_scene::input::ActionDefinition;
    use rot_scene::{Scene, Session};

    #[test]
    fn native_start_keeps_source_until_ready_and_presents_target_tick_zero() {
        let temp = tempfile::tempdir().unwrap();
        let root = ResourceRoot::new(temp.path()).unwrap();
        let target_dir = temp.path().join(STARTUP_TARGET_DIRECTORY);
        std::fs::create_dir(&target_dir).unwrap();
        let session = |id| Session::new(Scene::empty(id)).unwrap();
        let mut title = ComponentScene::from_legacy(&session("title")).unwrap();
        title.actions.insert(
            "start".into(),
            ActionDefinition {
                label: None,
                kind: ActionKind::Button {
                    keys: vec![InputKey::Enter],
                    primary_mouse: false,
                },
            },
        );
        let mut target = ComponentScene::from_legacy(&session("target")).unwrap();
        target.actions = title.actions.clone();
        target.save_new(target_dir.join("scene.json")).unwrap();
        std::fs::write(
            target_dir.join("app.json"),
            serde_json::to_vec(&serde_json::json!({
                "format":"rot-application", "version":5, "title":"target", "scene":"scene.json",
                "meshes":{}, "textures":{}, "texture_roles":{},
                "tick_nanos":rot_behavior::DEFAULT_FIXED_TICK.as_nanos() as u64,
            }))
            .unwrap(),
        )
        .unwrap();
        let route = StartupTransition::new("title".into(), "start".into(), "target".into());
        std::fs::write(
            temp.path().join(STARTUP_TRANSITION_NAME),
            serde_json::to_vec(&route).unwrap(),
        )
        .unwrap();

        let dt = rot_behavior::DEFAULT_FIXED_TICK;
        let mut app = PlayerApp::from_component_scene(
            "Title".into(),
            Box::new(title),
            MeshMap::new(),
            TextureMap::new(),
            dt,
        )
        .unwrap();
        app.set_start_transition(root, route);
        app.prepare_start_on_worker();
        let mut runtime = rot_app::Runtime::new(app, dt).unwrap();
        runtime.reset().unwrap();
        runtime.set_running(true);
        runtime
            .app_mut()
            .action_input(true, &[(InputKey::Enter, true)])
            .unwrap();
        runtime.advance(dt).unwrap();
        assert!(runtime.app().start_transition_pending());
        assert_eq!(runtime.app().content().scene().id, "title");
        assert_eq!(runtime.app().runtime_tick(), Some(0));

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while runtime.app().start_transition_pending() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
            runtime.advance(dt * 4).unwrap();
        }
        assert_eq!(runtime.app().content().scene().id, "target");
        assert_eq!(runtime.app().runtime_tick(), Some(0));
        assert!(runtime.app().physical_input_owned);
        runtime
            .app_mut()
            .action_input(true, &[(InputKey::Enter, false)])
            .unwrap();
        runtime.advance(dt).unwrap();
        assert_eq!(runtime.app().runtime_tick(), Some(1));
        assert!(
            matches!(runtime.app().input_observation().unwrap().1.0["start"],
            ActionValue::Button(state) if state == Default::default())
        );
    }

    #[test]
    fn player_steps_dynamic_rigid_body_under_gravity() {
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("phys")).unwrap()).unwrap();
        scene.objects.push(rot_scene::component_scene::ObjectNode {
            id: "crate".into(),
            name: "Crate".into(),
            enabled: true,
            parent: None,
            transform: rot_scene::Transform {
                translation: [0.0, 5.0, 0.0],
                ..Default::default()
            },
            components: vec![
                rot_scene::component_scene::ComponentInstance {
                    id: "crate-mesh".into(),
                    enabled: true,
                    kind: rot_scene::component_scene::Component::MeshRenderer {
                        geometry: rot_scene::component_scene::Geometry::Cube,
                        tint: [1.0; 4],
                        material: None,
                    },
                },
                rot_scene::component_scene::ComponentInstance {
                    id: "crate-col".into(),
                    enabled: true,
                    kind: rot_scene::component_scene::Component::Collider {
                        role: rot_scene::ColliderRole::Blocker,
                        offset: [0.0; 3],
                        shape: rot_scene::ColliderShape::Box {
                            half_extents: [0.5; 3],
                        },
                    },
                },
                rot_scene::component_scene::ComponentInstance {
                    id: "crate-rb".into(),
                    enabled: true,
                    kind: rot_scene::component_scene::Component::RigidBody {
                        body_type: rot_scene::RigidBodyType::Dynamic,
                        mass: 1.0,
                        linear_damping: 0.0,
                        angular_damping: 0.05,
                        gravity_scale: 1.0,
                        lock_translation: [false; 3],
                        lock_rotation: [false; 3],
                        ccd: false,
                    },
                },
            ],
        });
        let dt = rot_behavior::DEFAULT_FIXED_TICK;
        let app = PlayerApp::from_component_scene(
            "Physics".into(),
            Box::new(scene),
            MeshMap::new(),
            TextureMap::new(),
            dt,
        )
        .unwrap();
        let mut runtime = rot_app::Runtime::new(app, dt).unwrap();
        runtime.reset().unwrap();
        runtime.set_running(true);
        for _ in 0..30 {
            runtime.advance(dt).unwrap();
        }
        assert_eq!(runtime.app().runtime_tick(), Some(30));
        let y = runtime
            .app()
            .runtime
            .as_ref()
            .unwrap()
            .object_translation("crate")
            .unwrap()[1];
        assert!(
            y < 4.0,
            "expected rigid body to fall below y=4.0 after 30 ticks, got y={y}"
        );
    }

    #[test]
    fn player_steps_joint_and_character_controller_scene() {
        let mut scene =
            ComponentScene::from_legacy(&Session::new(Scene::empty("joint-cc")).unwrap()).unwrap();
        scene.objects.push(rot_scene::component_scene::ObjectNode {
            id: "floor".into(),
            name: "Floor".into(),
            enabled: true,
            parent: None,
            transform: rot_scene::Transform {
                translation: [0.0, -0.5, 0.0],
                ..Default::default()
            },
            components: vec![rot_scene::component_scene::ComponentInstance {
                id: "floor-col".into(),
                enabled: true,
                kind: rot_scene::component_scene::Component::Collider {
                    role: rot_scene::ColliderRole::Blocker,
                    offset: [0.0; 3],
                    shape: rot_scene::ColliderShape::Box {
                        half_extents: [10.0, 0.5, 10.0],
                    },
                },
            }],
        });
        scene.objects.push(rot_scene::component_scene::ObjectNode {
            id: "anchored".into(),
            name: "Anchored".into(),
            enabled: true,
            parent: None,
            transform: rot_scene::Transform {
                translation: [3.0, 4.0, 0.0],
                ..Default::default()
            },
            components: vec![
                rot_scene::component_scene::ComponentInstance {
                    id: "anchored-col".into(),
                    enabled: true,
                    kind: rot_scene::component_scene::Component::Collider {
                        role: rot_scene::ColliderRole::Blocker,
                        offset: [0.0; 3],
                        shape: rot_scene::ColliderShape::Box {
                            half_extents: [0.5; 3],
                        },
                    },
                },
                rot_scene::component_scene::ComponentInstance {
                    id: "anchored-rb".into(),
                    enabled: true,
                    kind: rot_scene::component_scene::Component::RigidBody {
                        body_type: rot_scene::RigidBodyType::Dynamic,
                        mass: 1.0,
                        linear_damping: 0.0,
                        angular_damping: 0.05,
                        gravity_scale: 1.0,
                        lock_translation: [false; 3],
                        lock_rotation: [false; 3],
                        ccd: false,
                    },
                },
                rot_scene::component_scene::ComponentInstance {
                    id: "anchored-joint".into(),
                    enabled: true,
                    kind: rot_scene::component_scene::Component::Joint {
                        kind: rot_scene::component_scene::JointKind::Fixed,
                        connected_body: None,
                        anchor: [0.0; 3],
                        connected_anchor: [3.0, 4.0, 0.0],
                        axis: [0.0, 1.0, 0.0],
                        limits: None,
                        rest_length: 1.0,
                        stiffness: 100.0,
                        damping: 5.0,
                        collide_connected: false,
                    },
                },
            ],
        });
        scene.script_files.insert(
            "move-cc".into(),
            rot_scene::component_scene::ScriptFile {
                asset_id: "move-cc".into(),
                path: "Scripts/move_cc.lua".into(),
                text: r#"return {
    on_update = function(o, props, dt, host)
        host.self:component("CharacterController"):move({3.0, -1.0, 0.0})
    end,
}"#
                .into(),
            },
        );
        scene.objects.push(rot_scene::component_scene::ObjectNode {
            id: "walker".into(),
            name: "Walker".into(),
            enabled: true,
            parent: None,
            transform: rot_scene::Transform::default(),
            components: vec![
                rot_scene::component_scene::ComponentInstance {
                    id: "walker-cc".into(),
                    enabled: true,
                    kind: rot_scene::component_scene::Component::CharacterController {
                        radius: 0.4,
                        height: 1.8,
                        offset: [0.0, 0.9, 0.0],
                        slope_limit_degrees: 45.0,
                        step_height: 0.3,
                        skin_width: 0.02,
                        snap_to_ground: 0.1,
                    },
                },
                rot_scene::component_scene::ComponentInstance {
                    id: "walker-script".into(),
                    enabled: true,
                    kind: rot_scene::component_scene::Component::Script {
                        binding: rot_scene::BehaviorBinding {
                            target: rot_scene::BindingTarget::Object("walker".into()),
                            source_id: "move-cc".into(),
                            enabled: true,
                            properties: std::collections::BTreeMap::new(),
                            state_defaults: std::collections::BTreeMap::new(),
                            commands: std::collections::BTreeMap::new(),
                        },
                        source_kind: None,
                        movement: rot_scene::component_scene::ScriptMovement::Ordinary,
                    },
                },
            ],
        });
        let dt = rot_behavior::DEFAULT_FIXED_TICK;
        let app = PlayerApp::from_component_scene(
            "JointAndCC".into(),
            Box::new(scene),
            MeshMap::new(),
            TextureMap::new(),
            dt,
        )
        .unwrap();
        let mut runtime = rot_app::Runtime::new(app, dt).unwrap();
        runtime.reset().unwrap();
        runtime.set_running(true);
        for _ in 0..30 {
            runtime.advance(dt).unwrap();
        }
        assert_eq!(runtime.app().runtime_tick(), Some(30));
        let anchored_y = runtime
            .app()
            .runtime
            .as_ref()
            .unwrap()
            .object_translation("anchored")
            .unwrap()[1];
        assert!((anchored_y - 4.0).abs() < 0.05);
        let walker_x = runtime
            .app()
            .runtime
            .as_ref()
            .unwrap()
            .object_translation("walker")
            .unwrap()[0];
        assert!(
            walker_x > 1.0,
            "expected walker to advance +X, got {walker_x}"
        );
    }
}
