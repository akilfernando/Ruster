//! CPU proxy for idle-dispatch ownership handoff, not actual UI codec/frame latency.
//! Heavy capture/serialization/restore and destruction happen after the timer.
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use ruster_scene::{BehaviorSource, Scene, Session, MAX_SOURCE_BYTES};
use serde::Serialize;
use serde_json::json;

#[allow(dead_code)]
#[path = "/home/akilf/Projects/Ruster/crates/ruster-editor/src/recovery.rs"]
mod recovery;
use recovery::Snapshot;

const WARMUP: usize = 1;
const SAMPLES: usize = 5;

#[derive(Clone, Serialize)]
struct DraftProxy {
    translation: [String; 3],
    rotation: [String; 3],
    scale: [String; 3],
    name: String,
    selection: Vec<String>,
    pending_source: String,
}
impl DraftProxy {
    fn new(session: &Session, iteration: usize) -> Self {
        Self {
            translation: [
                format!("{}{}", "-".repeat(760), iteration % 2),
                "not-a-number".into(),
                String::new(),
            ],
            rotation: [String::new(), "1e".into(), "0".into()],
            scale: ["1".into(), "-".into(), String::new()],
            name: format!(" unfinished name {} ", "n".repeat(90)),
            selection: session
                .scene()
                .objects
                .iter()
                .map(|o| o.id.clone())
                .collect(),
            pending_source: "return unfinished(".into(),
        }
    }
}
#[derive(Clone)]
struct Origins {
    project: Option<PathBuf>,
    assets: Option<PathBuf>,
}
fn source(text: String) -> BehaviorSource {
    BehaviorSource {
        label: Some("Unfinished code".into()),
        language_id: "unknown-backend".into(),
        host_api_version: 1,
        text,
    }
}
fn make_session(name: &str, parent: &Path, legacy: &Path) -> (Session, PathBuf) {
    let origin = parent.join(format!("{name}.scene.json"));
    match name {
        "small" => {
            let mut session = Session::new(Scene::demo()).unwrap();
            session
                .set_source(0, "unfinished", Some(source("function unfinished(".into())))
                .unwrap();
            session.save(&origin).unwrap();
            (session, origin)
        }
        "legacy-near-cap" => {
            fs::copy(legacy, &origin).unwrap();
            let session = Session::open(&origin).unwrap();
            assert_eq!(session.loaded_format_version(), 4);
            assert_eq!((session.undo_len(), session.redo_len()), (31, 1));
            (session, origin)
        }
        "source-pressure" => {
            let mut session = Session::new(Scene::empty("source-pressure")).unwrap();
            for revision in 0..50 {
                let text = format!("{}{revision:03}", "\u{0001}".repeat(MAX_SOURCE_BYTES - 3));
                session
                    .set_source(revision, "source-1", Some(source(text)))
                    .unwrap();
            }
            assert!(session.history_evictions() > 0);
            session.save(&origin).unwrap();
            (session, origin)
        }
        _ => panic!("unknown fixed fixture"),
    }
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    assert_eq!(
        args.len(),
        2,
        "usage: harness DISPOSABLE_PARENT LEGACY_V4_JSON"
    );
    let parent = PathBuf::from(&args[0]);
    let legacy = PathBuf::from(&args[1]);
    assert!(parent.is_absolute() && legacy.is_absolute());
    let temporary = tempfile::Builder::new()
        .prefix("recovery-handoff-")
        .tempdir_in(&parent)
        .unwrap();
    println!(
        "{}",
        json!({"kind":"configuration", "warmup":WARMUP, "samples":SAMPLES, "storage_parent":parent, "generated_prefix":"recovery-handoff-", "timing":"Session::clone plus representative typed drafts/origins clone plus Box<FnOnce()->Snapshot+Send> allocation", "proxy":true, "draft_shape":"approximately1KiB base numeric/name/source text plus all scene object IDs (up to256)", "not_measured":"actual UI codec, queue/lock/wakeup, worker scheduling, renderer/frame latency, total CPU", "heavy_capture":"after timer, with serialization/restore/oracle and destruction", "quantiles":"median sorted[2], max sorted[4]; no p95"})
    );
    for name in ["small", "legacy-near-cap", "source-pressure"] {
        let (session, origin) = make_session(name, temporary.path(), &legacy);
        let original = fs::read(&origin).unwrap();
        let expected_document = session.snapshot_bytes().unwrap();
        let origins = Origins {
            project: session.path().map(Path::to_path_buf),
            assets: Some(temporary.path().join("asset-origin-placeholder")),
        };
        let mut clone_ns = Vec::new();
        let mut handoff_ns = Vec::new();
        let mut draft_json_bytes = 0;
        for iteration in 0..WARMUP + SAMPLES {
            let drafts = DraftProxy::new(&session, iteration);
            let expected_drafts = serde_json::to_value(&drafts).unwrap();
            draft_json_bytes = serde_json::to_vec(&drafts).unwrap().len();
            let started = Instant::now();
            let owned_session = session.clone();
            let clone_elapsed = started.elapsed();
            let owned_drafts = drafts.clone();
            let owned_origins = origins.clone();
            let job: Box<dyn FnOnce() -> Snapshot + Send> = Box::new(move || {
                let draft_value = serde_json::to_value(owned_drafts).unwrap();
                let snapshot =
                    Snapshot::capture(&owned_session, owned_origins.assets, &draft_value).unwrap();
                assert_eq!(snapshot.origin, owned_origins.project);
                snapshot
            });
            let job = std::hint::black_box(job);
            let handoff_elapsed = started.elapsed();
            if iteration >= WARMUP {
                clone_ns.push(clone_elapsed.as_nanos());
                handoff_ns.push(handoff_elapsed.as_nanos());
            }
            // The payload is deliberately executed after timing, on this harness
            // thread. This proves ownership/copy correctness, not worker latency.
            let snapshot = job();
            assert_eq!(snapshot.drafts().unwrap(), expected_drafts);
            let restored = snapshot.restore().unwrap();
            assert_eq!(restored.snapshot_bytes().unwrap(), expected_document);
            assert!(restored.path().is_none() && restored.is_dirty());
            assert_eq!(
                (restored.undo_len(), restored.redo_len()),
                (session.undo_len(), session.redo_len())
            );
            assert_eq!(fs::read(&origin).unwrap(), original);
            drop(restored);
            drop(snapshot); // Destruction is outside the handoff timer.
        }
        let mut sorted_clone = clone_ns.clone();
        sorted_clone.sort_unstable();
        let mut sorted_handoff = handoff_ns.clone();
        sorted_handoff.sort_unstable();
        println!(
            "{}",
            json!({"kind":"row", "fixture":name, "objects":session.scene().objects.len(), "authored_revision":session.scene().revision, "undo":session.undo_len(), "redo":session.redo_len(), "byte_pressure_evictions":session.history_evictions(), "loaded_format_version":session.loaded_format_version(), "document_json_bytes":expected_document.len(), "origin_file_bytes":original.len(), "proxy_draft_json_bytes":draft_json_bytes, "session_clone_median_ns":sorted_clone[2], "session_clone_max_ns":sorted_clone[4], "handoff_median_ns":sorted_handoff[2], "handoff_max_ns":sorted_handoff[4], "raw_session_clone_ns":clone_ns, "raw_handoff_ns":handoff_ns, "post_timer_capture_restore_verified":true, "origin_unchanged":true})
        );
    }
    let path = temporary.path().to_path_buf();
    temporary.close().unwrap();
    assert!(!path.exists());
}
