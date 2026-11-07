//! Read-only authored-scene consumer. The generic editor owns editing and persistence.

use std::{path::Path, time::Duration};

use ruster_app::{egui, Application, Content, MeshMap, Step, TextureMap};
use ruster_behavior::{Diagnostic, Engine, Limits, Runtime};
use ruster_scene::input::{
    ActionFrame, ActionKind, ActionMap, ActionValue, InputKey, KeyEventAccumulator,
};
use ruster_scene::{PropertyValue, Session};

pub const DEMO: &[u8] = include_bytes!("../resources/generic.scene.json");
pub const FIXED_DT: Duration = Duration::from_millis(100);
/// Old snapshots keep the original viewer quantum. Current authored content
/// shares the editor/export duration instead of a sample-specific default.
pub fn session_tick_duration(session: &Session) -> Duration {
    if session.loaded_format_version() >= 11 {
        ruster_behavior::DEFAULT_FIXED_TICK
    } else {
        FIXED_DT
    }
}

/// No lock, registry, cache or authored-file writes on this resource path.
pub fn read_session(path: &Path) -> Result<Session, String> {
    let bytes = ruster_assets::read_regular_file(path, ruster_scene::MAX_FILE_BYTES as usize)
        .map_err(|e| format!("Scene resource {}: {e}", path.display()))?;
    Session::from_snapshot_bytes(&bytes)
        .map_err(|e| format!("Scene resource {}: {e}", path.display()))
}

pub struct GenericMotionLab {
    session: Session,
    content: Content,
    fixed_dt: Duration,
    engine: Option<Engine>,
    runtime: Option<Runtime>,
    speed_override: Option<f64>,
    needs_reset: bool,
    diagnostic: Option<Diagnostic>,
    selected: Option<String>,
    creation_title: Option<String>,
    action_keys: KeyEventAccumulator,
    physical_input_owned: bool,
    pending_input: Option<ActionFrame>,
    last_input: Option<(u64, ActionFrame, &'static str)>,
}

impl GenericMotionLab {
    /// Load complete authored data, including unfinished sources and history, without running it.
    /// Engine is created lazily by Reset on this application's owning thread.
    pub fn new(session: Session, meshes: MeshMap, fixed_dt: Duration) -> Result<Self, String> {
        Self::with_textures(session, meshes, TextureMap::new(), fixed_dt)
    }

    pub fn with_textures(
        session: Session,
        meshes: MeshMap,
        textures: TextureMap,
        fixed_dt: Duration,
    ) -> Result<Self, String> {
        if !(Duration::from_millis(1)..=Duration::from_secs(1)).contains(&fixed_dt) {
            return Err("Motion comparison tick duration must be 1 ms..=1 s".into());
        }
        let content = Content::with_textures(session.scene().clone(), meshes, textures)?;
        let selected = session.scene().objects.first().map(|o| o.id.clone());
        let action_keys = KeyEventAccumulator::new(session.authoring().actions.clone())
            .map_err(|e| e.to_string())?;
        Ok(Self {
            session,
            content,
            fixed_dt,
            engine: None,
            runtime: None,
            speed_override: None,
            needs_reset: true,
            diagnostic: None,
            selected,
            creation_title: None,
            action_keys,
            physical_input_owned: false,
            pending_input: None,
            last_input: None,
        })
    }

    pub fn session(&self) -> &Session {
        &self.session
    }
    pub fn diagnostic(&self) -> Option<&Diagnostic> {
        self.diagnostic.as_ref()
    }
    pub fn runtime_tick(&self) -> Option<u64> {
        self.runtime.as_ref().map(Runtime::tick)
    }
    pub fn fixed_dt(&self) -> Duration {
        self.fixed_dt
    }

    /// One explicit headless/programmatic frame. The caller must own paused
    /// stepping; this never claims native focus or leaves synthetic held state.
    fn supply_next_input(&mut self, frame: ActionFrame) -> Result<(), String> {
        if self.needs_reset || self.runtime.is_none() {
            return Err("Reset before supplying input".into());
        }
        if self.physical_input_owned || self.pending_input.is_some() {
            return Err("Input control or a supplied frame already owns the next step".into());
        }
        let frame = frame
            .validate_against(&self.session.authoring().actions)
            .map_err(|e| e.to_string())?
            .into_frame();
        self.action_keys.cancel();
        self.pending_input = Some(frame);
        Ok(())
    }
    /// Last successfully committed tick only, not current physical keyboard state.
    pub fn input_observation(&self) -> Option<(u64, &ActionFrame, &'static str)> {
        self.last_input
            .as_ref()
            .map(|(tick, frame, origin)| (*tick, frame, *origin))
    }

    fn action_help(&self, ui: &mut egui::Ui) {
        if self.session.authoring().actions.is_empty() {
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
                for (id, definition) in &self.session.authoring().actions {
                    let hint = match &definition.kind {
                        ActionKind::Button { keys: bound } => keys(bound),
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
        for binding in self
            .session
            .authoring()
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
                .find(|object| object.id == binding.object_id)
            {
                ui.strong(object.display_name());
                ui.label(format!("Position {:.3?}", object.transform.translation));
                ui.label(format!("Rotation {:.3?}", object.transform.rotation));
            }
        }
        ui.separator();
    }

    /// Present packaged content using its own identity over the same behavior/runtime path.
    pub fn set_creation_presentation(&mut self, title: String) {
        self.creation_title = Some(title);
        self.selected = None;
    }

    /// Application configuration only: never changes embedded properties, history or Save bytes.
    /// Applies to enabled bindings exposing a numeric `speed`; requires at least one match.
    pub fn set_speed_override(&mut self, speed: Option<f64>) -> Result<(), String> {
        if speed.is_some_and(|n| !n.is_finite()) {
            return Err("Runtime speed override must be finite".into());
        }
        if speed.is_some()
            && !self.session.authoring().bindings.values().any(|binding| {
                binding.enabled
                    && matches!(
                        binding.properties.get("speed"),
                        Some(PropertyValue::Number(_))
                    )
            })
        {
            return Err("No enabled binding exposes a numeric speed property".into());
        }
        if self.speed_override != speed {
            self.speed_override = speed;
            self.needs_reset = true;
        }
        Ok(())
    }

    /// Explicit caller reload; retained runtime remains visible but stale until successful Reset.
    /// The generic editor remains the authoring surface.
    pub fn replace_session(&mut self, session: Session) -> Result<(), String> {
        Content::with_textures(
            session.scene().clone(),
            self.content.meshes().clone(),
            self.content.textures().clone(),
        )?;
        let keys = KeyEventAccumulator::new(session.authoring().actions.clone())
            .map_err(|e| e.to_string())?;
        self.cancel_action_input();
        self.action_keys = keys;
        self.last_input = None;
        self.session = session;
        self.needs_reset = true;
        self.diagnostic = None;
        if !self
            .session
            .scene()
            .objects
            .iter()
            .any(|o| Some(&o.id) == self.selected.as_ref())
        {
            self.selected = self.session.scene().objects.first().map(|o| o.id.clone());
        }
        Ok(())
    }

    fn report<T>(&mut self, result: Result<T, Diagnostic>) -> Result<T, String> {
        result.map_err(|error| {
            // Application's string boundary still retains every structured diagnostic field.
            let message = serde_json::to_string(&error).expect("diagnostic serializes");
            self.diagnostic = Some(error);
            message
        })
    }
}

/// Explicit supplied-frame execution with outer-clock authority. It never
/// pauses a playing application to make an otherwise invalid request succeed.
pub fn step_with_input(
    runtime: &mut ruster_app::Runtime<GenericMotionLab>,
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

impl Application for GenericMotionLab {
    fn content(&self) -> &Content {
        &self.content
    }

    fn reset(&mut self) -> Result<(), String> {
        self.cancel_action_input();
        self.last_input = None;
        let keys = KeyEventAccumulator::new(self.session.authoring().actions.clone())
            .map_err(|e| e.to_string())?;
        let mut authoring = self.session.authoring().clone();
        if let Some(speed) = self.speed_override {
            let mut matched = false;
            for binding in authoring.bindings.values_mut().filter(|b| b.enabled) {
                if let Some(PropertyValue::Number(value)) = binding.properties.get_mut("speed") {
                    *value = speed;
                    matched = true;
                }
            }
            if !matched {
                return Err("No enabled binding exposes a numeric speed property; clear the runtime override".into());
            }
        }
        if self.engine.is_none() {
            let result = Engine::new(Limits {
                fixed_dt: self.fixed_dt.as_secs_f64(),
                ..Limits::default()
            });
            self.engine = Some(self.report(result)?);
        }
        let result = self
            .engine
            .as_mut()
            .expect("initialized engine")
            .reset(self.session.scene(), &authoring);
        let candidate = self.report(result)?;
        // Engine validated transforms; meshes remain the same immutable Content resources.
        *self.content.scene_mut() = candidate.scene().clone();
        self.runtime = Some(candidate);
        self.action_keys = keys;
        self.needs_reset = false;
        self.diagnostic = None;
        Ok(())
    }

    fn keyboard_actions(&self) -> Option<&ActionMap> {
        (!self.session.authoring().actions.is_empty()).then_some(&self.session.authoring().actions)
    }
    fn action_input(&mut self, owned: bool, events: &[(InputKey, bool)]) -> Result<(), String> {
        if !owned {
            self.cancel_action_input();
            return Ok(());
        }
        if self.needs_reset || self.pending_input.is_some() {
            self.cancel_action_input();
            return Err("Reset or finish the supplied step before claiming input".into());
        }
        self.physical_input_owned = true;
        for &(key, pressed) in events {
            self.action_keys.key_event(key, pressed);
        }
        Ok(())
    }
    fn cancel_action_input(&mut self) {
        self.action_keys.cancel();
        self.physical_input_owned = false;
        self.pending_input = None;
    }
    fn tick(&mut self, step: Step) -> Result<(), String> {
        let supplied = self.pending_input.take();
        if self.needs_reset {
            return Err("Reset before running this authored snapshot or runtime override".into());
        }
        if step.dt != self.fixed_dt {
            return Err("Host Step.dt differs from Motion Lab's configured fixed tick".into());
        }
        let runtime = self.runtime.as_mut().ok_or("Reset before running")?;
        if runtime.tick().checked_add(1) != Some(step.tick) {
            return Err("Host and behavior tick differ; Reset through the host Runtime".into());
        }
        let origin = if supplied.is_some() {
            "programmatic"
        } else if self.physical_input_owned {
            "native"
        } else {
            "neutral"
        };
        let input = supplied.unwrap_or_else(|| self.action_keys.take_snapshot().into_frame());
        let result = self
            .engine
            .as_mut()
            .expect("Reset initialized engine")
            .step_with_input(runtime, &input);
        if result.is_err() {
            self.cancel_action_input();
        }
        self.report(result)?;
        self.last_input = Some((step.tick, input, origin));
        *self.content.scene_mut() = self
            .runtime
            .as_ref()
            .expect("active runtime")
            .scene()
            .clone();
        self.diagnostic = None;
        Ok(())
    }

    fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }
    fn select(&mut self, id: &str) {
        if self.creation_title.is_none() && self.session.scene().objects.iter().any(|o| o.id == id)
        {
            self.selected = Some(id.into());
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui) -> Result<(), String> {
        if let Some(title) = &self.creation_title {
            ui.heading(title);
            self.action_help(ui);
            if let Some(error) = &self.diagnostic {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            }
            return Ok(());
        }
        self.action_help(ui);
        ui.heading(if self.session.authoring().actions.is_empty() {
            "Motion comparison"
        } else {
            "Runtime preview"
        });
        ui.label("Compare authored positions with fixed-step motion.");
        ui.small("Author sources, properties and history in the generic Ruster editor. This viewer reads a snapshot; reopen after saving changes.");
        ui.label(format!(
            "Fixed tick: {:.3} s · committed ticks: {}",
            self.fixed_dt.as_secs_f64(),
            self.runtime_tick().unwrap_or(0)
        ));
        if let Some(speed) = self.speed_override {
            ui.label(format!(
                "Runtime speed override: {speed} (authored values preserved)"
            ));
        }
        if self.needs_reset {
            ui.strong("Snapshot requires Reset before Run or Step.");
        }
        ui.separator();
        for authored in &self.session.scene().objects {
            if ui
                .selectable_label(
                    self.selected.as_deref() == Some(&authored.id),
                    authored.display_name(),
                )
                .clicked()
            {
                self.selected = Some(authored.id.clone());
            }
        }
        if let Some(object) = self
            .content
            .scene()
            .objects
            .iter()
            .find(|o| Some(&o.id) == self.selected.as_ref())
        {
            ui.separator();
            ui.strong(object.display_name());
            ui.label(format!(
                "Runtime position: {:.3?}",
                object.transform.translation
            ));
            ui.label(format!(
                "Runtime rotation (rad): {:.3?}",
                object.transform.rotation
            ));
            if let Some(authored) = self
                .session
                .scene()
                .objects
                .iter()
                .find(|o| o.id == object.id)
            {
                ui.label(format!(
                    "Authored position: {:.3?}",
                    authored.transform.translation
                ));
                let distance = object
                    .transform
                    .translation
                    .iter()
                    .zip(authored.transform.translation)
                    .map(|(runtime, authored)| f64::from(*runtime - authored).powi(2))
                    .sum::<f64>()
                    .sqrt();
                ui.label(format!("Displacement from authored: {distance:.3}"));
            }
        }
        if let Some(error) = &self.diagnostic {
            ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            ui.small(format!(
                "source={} hash={} binding={} object={} phase={:?}",
                error.source_id.as_deref().unwrap_or("—"),
                error.source_hash.as_deref().unwrap_or("—"),
                error.binding_id.as_deref().unwrap_or("—"),
                error.object_id.as_deref().unwrap_or("—"),
                error.phase
            ));
            ui.label("Repair in the generic editor, save and reopen this viewer. Last committed motion remains visible.");
        }
        Ok(())
    }
}
