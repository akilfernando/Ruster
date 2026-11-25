//! Bounded, isolated fixed-step checks against a captured authored session.
use crate::{Diagnostic, DiagnosticCode, Engine, Limits, Runtime};
use rot_scene::{input::ActionFrame, Session, StateValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

pub const FORMAT: &str = "rot-play-check";
pub const VERSION: u32 = 1;
pub const PACKAGED_PATH: &str = "resources/play-check.json";
pub const MAX_CASE_BYTES: usize = 64 * 1024;
pub const MAX_FRAMES: usize = 120;
pub const MAX_EXPECTATIONS: usize = 8;
pub const MAX_EXPECTED_TEXT_BYTES: usize = 256;
pub const MAX_RUN_TIME: Duration = Duration::from_secs(5);

pub fn sidecar_path(scene: &Path) -> PathBuf {
    scene.with_extension("play-check.json")
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CheckIdentity {
    pub scene_sha256: String,
    pub action_map_sha256: String,
    pub resources_sha256: String,
    pub tick_nanos: u64,
}

impl CheckIdentity {
    pub fn capture(
        session: &Session,
        resources_sha256: String,
        tick_nanos: u64,
    ) -> Result<Self, String> {
        let bytes = session
            .snapshot_bytes()
            .map_err(|error| error.to_string())?;
        Self::for_saved_scene(
            hash(&bytes),
            &session.authoring().actions,
            resources_sha256,
            tick_nanos,
        )
    }

    pub fn for_saved_scene(
        scene_sha256: String,
        actions: &rot_scene::input::ActionMap,
        resources_sha256: String,
        tick_nanos: u64,
    ) -> Result<Self, String> {
        if !is_hash(&scene_sha256)
            || !is_hash(&resources_sha256)
            || !(1_000_000..=1_000_000_000).contains(&tick_nanos)
        {
            return Err(
                "play check requires scene and resource digests and fixed tick of 1ms..=1s".into(),
            );
        }
        let actions = serde_json::to_vec(actions).map_err(|error| error.to_string())?;
        Ok(Self {
            scene_sha256,
            action_map_sha256: hash(&actions),
            resources_sha256,
            tick_nanos,
        })
    }

    pub fn stale_fields(&self, current: &Self) -> Vec<&'static str> {
        [
            (self.scene_sha256 != current.scene_sha256, "scene/source"),
            (
                self.action_map_sha256 != current.action_map_sha256,
                "input map",
            ),
            (
                self.resources_sha256 != current.resources_sha256,
                "resources",
            ),
            (self.tick_nanos != current.tick_nanos, "fixed tick"),
        ]
        .into_iter()
        .filter_map(|(different, label)| different.then_some(label))
        .collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayCheck {
    pub format: String,
    pub version: u32,
    pub name: String,
    pub identity: CheckIdentity,
    pub frames: Vec<ActionFrame>,
    pub expectations: Vec<Expectation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    pub select: Selection,
    pub comparison: Comparison,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Selection {
    Tick,
    Translation { object_id: String },
    State { binding_id: String, field: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Comparison {
    Equals {
        expected: StateValue,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tolerance: Option<f64>,
    },
    Range {
        min: f64,
        max: f64,
    },
}

impl PlayCheck {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || self.version != VERSION {
            return Err("expected rot-play-check version 1".into());
        }
        if self.name.is_empty()
            || self.name.len() > 128
            || self.name.trim() != self.name
            || self.name.chars().any(char::is_control)
        {
            return Err("play check name requires 1..128 UTF-8 bytes without controls or surrounding whitespace".into());
        }
        if self.frames.is_empty()
            || self.frames.len() > MAX_FRAMES
            || self.expectations.is_empty()
            || self.expectations.len() > MAX_EXPECTATIONS
        {
            return Err("play check requires 1..120 frames and 1..8 expectations".into());
        }
        if ![
            &self.identity.scene_sha256,
            &self.identity.action_map_sha256,
            &self.identity.resources_sha256,
        ]
        .into_iter()
        .all(|digest| is_hash(digest))
            || !(1_000_000..=1_000_000_000).contains(&self.identity.tick_nanos)
        {
            return Err("invalid play check scene, action, resource or fixed-tick identity".into());
        }
        for (index, expectation) in self.expectations.iter().enumerate() {
            expectation
                .validate()
                .map_err(|error| format!("expectation {}: {error}", index + 1))?;
        }
        Ok(())
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_CASE_BYTES {
            return Err("play check exceeds 64 KiB".into());
        }
        let case: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        case.validate()?;
        Ok(case)
    }

    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|error| error.to_string())?;
        if bytes.len() > MAX_CASE_BYTES {
            return Err("play check exceeds 64 KiB".into());
        }
        Ok(bytes)
    }
}

impl Expectation {
    fn validate(&self) -> Result<(), String> {
        let id = |value: &str| rot_scene::validate_id(value).map_err(|error| error.to_string());
        match &self.select {
            Selection::Tick => (),
            Selection::Translation { object_id } => id(object_id)?,
            Selection::State { binding_id, field } => {
                id(binding_id)?;
                id(field)?;
            }
        }
        match &self.comparison {
            Comparison::Equals {
                expected,
                tolerance,
            } => {
                expected.validate().map_err(|error| error.to_string())?;
                if matches!(expected, StateValue::Text(value) if value.len() > MAX_EXPECTED_TEXT_BYTES)
                {
                    return Err("expected state text exceeds 256 UTF-8 bytes".into());
                }
                if tolerance.is_some_and(|n| !n.is_finite() || n < 0.)
                    || tolerance.is_some()
                        && !matches!(expected, StateValue::Number(_) | StateValue::Vec3(_))
                {
                    return Err(
                        "optional nonnegative finite tolerance requires a number or vec3".into(),
                    );
                }
                match (&self.select, expected) {
                    (Selection::Tick, StateValue::Number(value))
                        if (0.0..=MAX_FRAMES as f64).contains(value)
                            && value.fract() == 0.
                            && tolerance.is_none() => {}
                    (Selection::Tick, _) => {
                        return Err("tick requires an exact integer expectation in 0..=120".into())
                    }
                    (Selection::Translation { .. }, StateValue::Vec3(_))
                    | (Selection::State { .. }, _) => (),
                    (Selection::Translation { .. }, _) => {
                        return Err("translation requires a vec3 expectation".into())
                    }
                }
            }
            Comparison::Range { min, max } => {
                if !min.is_finite() || !max.is_finite() || min > max {
                    return Err("range requires finite ordered bounds".into());
                }
                if !matches!(self.select, Selection::State { .. }) {
                    return Err("range requires a numeric state selector".into());
                }
            }
        }
        Ok(())
    }

    fn observe(
        &self,
        scene: &rot_scene::Scene,
        tick: u64,
        runtime: Option<&Runtime>,
    ) -> Option<StateValue> {
        match &self.select {
            Selection::Tick => Some(StateValue::Number(tick as f64)),
            Selection::Translation { object_id } => scene
                .objects
                .iter()
                .find(|object| object.id == *object_id)
                .map(|object| StateValue::Vec3(object.transform.translation.map(f64::from))),
            Selection::State { binding_id, field } => {
                runtime?.state().binding(binding_id)?.get(field).cloned()
            }
        }
    }

    fn matches(&self, actual: &StateValue) -> bool {
        match (&self.comparison, actual) {
            (
                Comparison::Equals {
                    expected: StateValue::Number(expected),
                    tolerance,
                },
                StateValue::Number(actual),
            ) => expected == actual || (*expected - *actual).abs() <= tolerance.unwrap_or(0.),
            (
                Comparison::Equals {
                    expected: StateValue::Vec3(expected),
                    tolerance,
                },
                StateValue::Vec3(actual),
            ) => expected
                .iter()
                .zip(actual)
                .all(|(e, a)| e == a || (*e - *a).abs() <= tolerance.unwrap_or(0.)),
            (Comparison::Equals { expected, .. }, actual) => expected == actual,
            (Comparison::Range { min, max }, StateValue::Number(actual)) => {
                (min..=max).contains(&actual)
            }
            (Comparison::Range { .. }, _) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Passed,
    AssertionFailed,
    InvalidCase,
    Stale,
    ScriptError,
    Cancelled,
    TimedOut,
}

#[derive(Clone, Debug, Serialize)]
pub struct AssertionResult {
    pub index: usize,
    pub expectation: Expectation,
    pub actual: Option<StateValue>,
    pub actual_truncated: bool,
    pub passed: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub status: Status,
    pub name: String,
    pub identity: CheckIdentity,
    pub case_sha256: String,
    pub completed_tick: u64,
    pub results: Vec<AssertionResult>,
    pub diagnostic: Option<String>,
}

impl Report {
    fn finish(mut self, status: Status, diagnostic: impl Into<String>) -> Self {
        self.status = status;
        self.diagnostic = Some(diagnostic.into().chars().take(512).collect());
        self
    }
    fn script_error(self, failure: Diagnostic) -> Self {
        let status = if failure.code == DiagnosticCode::Cancelled {
            Status::Cancelled
        } else {
            Status::ScriptError
        };
        self.finish(status, failure.to_string())
    }
}

/// Run a private snapshot with a fresh Lua VM. Cancellation uses the existing
/// instruction hook. The elapsed budget is observed between bounded runtime
/// transactions, so it can overrun by at most one admitted Reset or Step.
pub fn run(
    session: &Session,
    case: &PlayCheck,
    resources_sha256: &str,
    tick_nanos: u64,
    clips: &crate::Clips,
    cancel: Arc<AtomicBool>,
) -> Report {
    let identity = CheckIdentity::capture(session, resources_sha256.into(), tick_nanos);
    run_with_identity(session, case, identity, clips, cancel)
}

pub fn run_with_identity(
    session: &Session,
    case: &PlayCheck,
    identity: Result<CheckIdentity, String>,
    clips: &crate::Clips,
    cancel: Arc<AtomicBool>,
) -> Report {
    run_with_source(
        CheckSource::Behavior(session, clips),
        case,
        identity,
        cancel,
    )
}

/// Check a behavior session projected from an authored v23 scene, including
/// any hierarchy admitted by the same runtime projection as ordinary Play.
pub fn run_projected(
    session: &Session,
    case: &PlayCheck,
    identity: CheckIdentity,
    clips: &crate::Clips,
    cancel: Arc<AtomicBool>,
) -> Report {
    run_with_source(
        CheckSource::Projected(session, clips),
        case,
        Ok(identity),
        cancel,
    )
}

/// Check the immutable scene used by ordinary v23 static Play. Static Steps
/// advance only the tick and do not initialize or run a Lua VM.
pub fn run_static(
    scene: &rot_scene::Scene,
    case: &PlayCheck,
    identity: CheckIdentity,
    cancel: Arc<AtomicBool>,
) -> Report {
    run_with_source(CheckSource::Static(scene), case, Ok(identity), cancel)
}

enum CheckSource<'a> {
    Behavior(&'a Session, &'a crate::Clips),
    Projected(&'a Session, &'a crate::Clips),
    Static(&'a rot_scene::Scene),
}

fn run_with_source(
    source: CheckSource<'_>,
    case: &PlayCheck,
    identity: Result<CheckIdentity, String>,
    cancel: Arc<AtomicBool>,
) -> Report {
    let deadline = Instant::now() + MAX_RUN_TIME;
    let mut report = Report {
        status: Status::InvalidCase,
        name: case.name.chars().take(128).collect(),
        identity: CheckIdentity {
            scene_sha256: case.identity.scene_sha256.chars().take(64).collect(),
            action_map_sha256: case.identity.action_map_sha256.chars().take(64).collect(),
            resources_sha256: case.identity.resources_sha256.chars().take(64).collect(),
            tick_nanos: case.identity.tick_nanos,
        },
        case_sha256: String::new(),
        completed_tick: 0,
        results: Vec::new(),
        diagnostic: None,
    };
    let bytes = match case.encode() {
        Ok(bytes) => bytes,
        Err(error) => return report.finish(Status::InvalidCase, error),
    };
    report.case_sha256 = hash(&bytes);
    let current = match identity {
        Ok(identity) => identity,
        Err(error) => return report.finish(Status::InvalidCase, error),
    };
    let stale = case.identity.stale_fields(&current);
    if !stale.is_empty() {
        return report.finish(
            Status::Stale,
            format!(
                "Changed {}; re-pin the case explicitly without replacing its expectations.",
                stale.join(", ")
            ),
        );
    }
    if matches!(&source, CheckSource::Behavior(session, _) if !session.scene().hierarchy.is_empty())
    {
        return report.finish(
            Status::InvalidCase,
            "Play checks currently require a flat scene.",
        );
    }
    let empty_actions = rot_scene::input::ActionMap::new();
    let actions = match &source {
        CheckSource::Behavior(session, _) | CheckSource::Projected(session, _) => {
            &session.authoring().actions
        }
        CheckSource::Static(_) => &empty_actions,
    };
    for (index, frame) in case.frames.iter().enumerate() {
        if let Err(error) = frame.validate_only_against(actions) {
            return report.finish(
                Status::InvalidCase,
                format!("input frame {}: {error}", index + 1),
            );
        }
    }
    let stopped = || {
        if cancel.load(Ordering::Acquire) {
            Some((Status::Cancelled, "Play check cancelled."))
        } else if Instant::now() >= deadline {
            Some((
                Status::TimedOut,
                "Play check exceeded the five-second runtime budget.",
            ))
        } else {
            None
        }
    };
    if let Some((status, message)) = stopped() {
        return report.finish(status, message);
    }
    let (scene, runtime) = match source {
        CheckSource::Static(scene) => {
            report.completed_tick = case.frames.len() as u64;
            (scene, None)
        }
        CheckSource::Behavior(session, clips) | CheckSource::Projected(session, clips) => {
            let limits = Limits {
                fixed_dt: Duration::from_nanos(current.tick_nanos).as_secs_f64(),
                ..Limits::default()
            };
            let mut engine = match Engine::with_cancellation(limits, cancel.clone()) {
                Ok(engine) => engine,
                Err(failure) => return report.script_error(failure),
            };
            engine.set_clips(clips.clone());
            let mut runtime = match engine.reset_projected(
                session.scene(),
                session.authoring(),
                session.transform_camera_binding().cloned(),
                session.runtime_projection(),
            ) {
                Ok(runtime) => runtime,
                Err(failure) => return report.script_error(failure),
            };
            for frame in &case.frames {
                if let Some((status, message)) = stopped() {
                    return report.finish(status, message);
                }
                if let Err(failure) = engine.step_with_input(&mut runtime, frame) {
                    return report.script_error(failure);
                }
                report.completed_tick = runtime.tick();
            }
            (session.scene(), Some(runtime))
        }
    };
    let scene = runtime.as_ref().map_or(scene, Runtime::scene);
    if let Some((status, message)) = stopped() {
        return report.finish(status, message);
    }
    report.results = case
        .expectations
        .iter()
        .enumerate()
        .map(|(index, expectation)| {
            let mut actual = expectation.observe(scene, report.completed_tick, runtime.as_ref());
            let passed = actual
                .as_ref()
                .is_some_and(|actual| expectation.matches(actual));
            let mut actual_truncated = false;
            if let Some(StateValue::Text(text)) = actual.as_mut() {
                if text.len() > MAX_EXPECTED_TEXT_BYTES {
                    text.truncate(text.floor_char_boundary(MAX_EXPECTED_TEXT_BYTES));
                    actual_truncated = true;
                }
            }
            AssertionResult {
                index,
                expectation: expectation.clone(),
                actual,
                actual_truncated,
                passed,
            }
        })
        .collect();
    report.status = if report.results.iter().all(|result| result.passed) {
        Status::Passed
    } else {
        Status::AssertionFailed
    };
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{input::ActionValue, Scene};

    const RESOURCE_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const TICK: u64 = 16_666_667;

    fn fixture(session: &Session, frames: Vec<ActionFrame>) -> PlayCheck {
        PlayCheck {
            format: FORMAT.into(),
            version: VERSION,
            name: "Move twice".into(),
            identity: CheckIdentity::capture(session, RESOURCE_HASH.into(), TICK).unwrap(),
            frames,
            expectations: vec![Expectation {
                select: Selection::Tick,
                comparison: Comparison::Equals {
                    expected: StateValue::Number(2.),
                    tolerance: None,
                },
            }],
        }
    }

    #[test]
    fn final_reports_distinguish_mismatch_stale_invalid_and_cancellation() {
        let session = Session::new(Scene::empty("play-check")).unwrap();
        let mut case = fixture(&session, vec![ActionFrame::default(); 2]);
        let cancel = Arc::new(AtomicBool::new(false));
        let run_case = |case: &PlayCheck| {
            run(
                &session,
                case,
                RESOURCE_HASH,
                TICK,
                &Default::default(),
                cancel.clone(),
            )
        };
        let bytes = case.encode().unwrap();
        let pass = run_case(&PlayCheck::decode(&bytes).unwrap());
        assert_eq!(pass.status, Status::Passed);
        assert_eq!(pass.results[0].actual, Some(StateValue::Number(2.)));
        assert!(pass.results[0].passed);
        assert_eq!(pass.completed_tick, 2);
        assert_eq!(
            run(
                &session,
                &case,
                &"2".repeat(64),
                TICK,
                &Default::default(),
                cancel.clone()
            )
            .status,
            Status::Stale
        );

        case.expectations.push(Expectation {
            select: Selection::State {
                binding_id: "missing".into(),
                field: "total".into(),
            },
            comparison: Comparison::Range { min: 0., max: 2. },
        });
        let fail = run_case(&case);
        assert_eq!(fail.status, Status::AssertionFailed);
        assert_eq!(fail.results[1].actual, None);
        assert!(!fail.results[1].passed);
        case.frames[1]
            .0
            .insert("missing-action".into(), ActionValue::Axis1d { value: 1 });
        let invalid = run_case(&case);
        assert_eq!(invalid.status, Status::InvalidCase);
        assert_eq!(invalid.completed_tick, 0);
        assert!(invalid.diagnostic.unwrap().contains("input frame 2"));
        case.frames[1].0.clear();
        cancel.store(true, Ordering::Release);
        assert_eq!(run_case(&case).status, Status::Cancelled);
        assert_eq!(session.scene().revision, 0);
    }

    #[test]
    #[ignore = "real saved Scene21 fixture and named action frames"]
    fn saved_typed_check_fails_after_speed_change_passes_after_repair_and_keeps_preview() {
        use rot_scene::PropertyValue;
        let directory = PathBuf::from(std::env::var_os("ROT_TEST_PLAY_CHECK_FIXTURE").unwrap());
        let path = directory.join("scene.json");
        let original = std::fs::read(&path).unwrap();
        let frames: Vec<ActionFrame> =
            serde_json::from_slice(&std::fs::read(directory.join("input.json")).unwrap()).unwrap();
        let mut session = Session::from_snapshot_bytes(&original).unwrap();
        let mut case = fixture(&session, frames);
        case.expectations.extend([
            Expectation {
                select: Selection::Translation {
                    object_id: "subject".into(),
                },
                comparison: Comparison::Equals {
                    expected: StateValue::Vec3([2., 0., 0.]),
                    tolerance: None,
                },
            },
            Expectation {
                select: Selection::State {
                    binding_id: "subject-movement".into(),
                    field: "total".into(),
                },
                comparison: Comparison::Range { min: 2., max: 2. },
            },
        ]);

        let mut engine = Engine::new(Limits {
            fixed_dt: Duration::from_nanos(TICK).as_secs_f64(),
            ..Limits::default()
        })
        .unwrap();
        let mut preview = engine.reset(session.scene(), session.authoring()).unwrap();
        engine
            .step_with_input(&mut preview, &case.frames[0])
            .unwrap();
        let run_case = |session: &Session, case: &PlayCheck| {
            run(
                session,
                case,
                RESOURCE_HASH,
                TICK,
                &Default::default(),
                Arc::new(AtomicBool::new(false)),
            )
        };
        assert_eq!(run_case(&session, &case).status, Status::Passed);
        let mut changed = session.binding("subject-movement").unwrap().clone();
        changed
            .properties
            .insert("speed".into(), PropertyValue::Number(2.));
        session
            .set_binding(session.scene().revision, "subject-movement", Some(changed))
            .unwrap();
        assert_eq!(run_case(&session, &case).status, Status::Stale);
        case.identity = CheckIdentity::capture(&session, RESOURCE_HASH.into(), TICK).unwrap();
        let before = session.snapshot_bytes().unwrap();
        let fail = run_case(&session, &case);
        assert_eq!(fail.status, Status::AssertionFailed);
        assert_eq!(fail.results[1].actual, Some(StateValue::Vec3([4., 0., 0.])));
        assert_eq!(fail.results[2].actual, Some(StateValue::Number(4.)));
        assert_eq!(before, session.snapshot_bytes().unwrap());
        session.undo(session.scene().revision).unwrap();
        case.identity = CheckIdentity::capture(&session, RESOURCE_HASH.into(), TICK).unwrap();
        let after = run_case(&session, &case);
        assert_eq!(after.status, Status::Passed);
        assert_eq!(preview.tick(), 1);
        assert_eq!(
            preview.state().binding("subject-movement").unwrap()["total"],
            StateValue::Number(1.)
        );
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
}
