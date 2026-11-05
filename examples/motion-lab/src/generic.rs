//! Read-only Scene5 consumer. Generic editor owns source/property editing and persistence.

use std::{path::Path, time::Duration};

use ruster_app::{egui, Application, Content, MeshMap, Step};
use ruster_behavior::{Diagnostic, Engine, Limits, Runtime};
use ruster_scene::{PropertyValue, Session};

pub const DEMO: &[u8] = include_bytes!("../resources/generic.scene.json");
pub const FIXED_DT: Duration = Duration::from_millis(100);

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
}

impl GenericMotionLab {
    /// Load complete authored data, including unfinished sources and history, without running it.
    /// Engine is created lazily by Reset on this application's owning thread.
    pub fn new(session: Session, meshes: MeshMap, fixed_dt: Duration) -> Result<Self, String> {
        if !(Duration::from_millis(1)..=Duration::from_secs(1)).contains(&fixed_dt) {
            return Err("Motion comparison tick duration must be 1 ms..=1 s".into());
        }
        let content = Content::new(session.scene().clone(), meshes)?;
        let selected = session.scene().objects.first().map(|o| o.id.clone());
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
        Content::new(session.scene().clone(), self.content.meshes().clone())?;
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

impl Application for GenericMotionLab {
    fn content(&self) -> &Content {
        &self.content
    }

    fn reset(&mut self) -> Result<(), String> {
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
        self.needs_reset = false;
        self.diagnostic = None;
        Ok(())
    }

    fn tick(&mut self, step: Step) -> Result<(), String> {
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
        let result = self
            .engine
            .as_mut()
            .expect("Reset initialized engine")
            .step(runtime);
        self.report(result)?;
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
            if let Some(error) = &self.diagnostic {
                ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            }
            return Ok(());
        }
        ui.heading("Motion comparison");
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
