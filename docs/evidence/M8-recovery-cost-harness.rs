//! Production recovery public API, synchronous capture/fsync baseline.
//! All fixture construction, JSON metadata and correctness oracles are untimed.
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use ruster_scene::{BehaviorSource, Scene, Session, MAX_SOURCE_BYTES};
use serde_json::{json, Value};

#[allow(dead_code)]
#[path = "/home/akilf/Projects/Ruster/crates/ruster-editor/src/recovery.rs"]
mod recovery;
use recovery::{Snapshot, Store};

const WARMUP: usize = 1;
const SAMPLES: usize = 5;

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
fn drafts(iteration: usize) -> Value {
    // Invalid numeric/JSON editor text is kept as text; values are built off-timer.
    json!({"numeric": format!("{}{}", "-".repeat(1000), iteration % 2), "source": "return unfinished(", "pending_json": "{\"incomplete\":"})
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
    // Fresh unique prefix; TempDir removes only this harness-owned tree on exit.
    let temporary = tempfile::Builder::new()
        .prefix("recovery-cost-")
        .tempdir_in(&parent)
        .unwrap();
    println!(
        "{}",
        json!({"kind":"configuration", "warmup":WARMUP, "samples":SAMPLES, "storage_parent":parent, "generated_prefix":"recovery-cost-", "drafts":"~1KiB invalid numeric/source/JSON field strings toggled off-timer", "timing":"full Snapshot::capture then Journal::checkpoint; capture/write/combined split from same operation", "quantiles":"median sorted[2], max sorted[4]; no p95", "forced_disk_cache_eviction":false})
    );
    for name in ["small", "legacy-near-cap", "source-pressure"] {
        let (session, origin) = make_session(name, temporary.path(), &legacy);
        let original = fs::read(&origin).unwrap();
        let expected_document = session.snapshot_bytes().unwrap();
        let expected_revision = session.scene().revision;
        let (undo, redo) = (session.undo_len(), session.redo_len());
        let store_path = temporary.path().join(format!("{name}-recovery"));
        let store = Store::open(store_path.clone()).unwrap();
        let mut journal = store.allocate().unwrap();
        let mut capture_ns = Vec::new();
        let mut write_ns = Vec::new();
        let mut combined_ns = Vec::new();
        for iteration in 0..WARMUP + SAMPLES {
            let draft = drafts(iteration);
            let started = Instant::now();
            let captured = Snapshot::capture(&session, None, &draft).unwrap();
            let capture_elapsed = started.elapsed();
            let write_started = Instant::now();
            journal.checkpoint(captured).unwrap();
            let write_elapsed = write_started.elapsed();
            let combined_elapsed = started.elapsed();
            if iteration >= WARMUP {
                capture_ns.push(capture_elapsed.as_nanos());
                write_ns.push(write_elapsed.as_nanos());
                combined_ns.push(combined_elapsed.as_nanos());
            }
        }
        // A dropped/crashed lease is scanned and explicitly claimed via production APIs.
        drop(journal);
        let candidates = store.scan().unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].error.is_none());
        let candidate = &candidates[0];
        let (claimed, captured) = store.claim(&candidate.id).unwrap();
        assert_eq!(captured.origin.as_deref(), session.path());
        assert_eq!(captured.drafts().unwrap(), drafts(WARMUP + SAMPLES - 1));
        let snapshot_bytes = serde_json::to_vec(&captured).unwrap().len();
        let record_bytes = fs::metadata(store_path.join(&candidate.id).join("checkpoint.json"))
            .unwrap()
            .len();
        let restored = captured.restore().unwrap();
        assert_eq!(restored.snapshot_bytes().unwrap(), expected_document);
        assert_eq!(restored.scene().revision, expected_revision);
        assert_eq!((restored.undo_len(), restored.redo_len()), (undo, redo));
        assert!(restored.path().is_none() && restored.is_dirty());
        assert_eq!(
            fs::read(&origin).unwrap(),
            original,
            "recovery must not save/upgrade the origin"
        );
        let mut sorted_capture = capture_ns.clone();
        sorted_capture.sort_unstable();
        let mut sorted_write = write_ns.clone();
        sorted_write.sort_unstable();
        let mut sorted_combined = combined_ns.clone();
        sorted_combined.sort_unstable();
        println!(
            "{}",
            json!({"kind":"row", "fixture":name, "objects":session.scene().objects.len(), "authored_revision":expected_revision, "undo":undo, "redo":redo, "byte_pressure_evictions":session.history_evictions(), "loaded_format_version":session.loaded_format_version(), "origin_file_bytes":original.len(), "document_json_bytes":expected_document.len(), "snapshot_json_bytes":snapshot_bytes, "checkpoint_record_bytes":record_bytes, "draft_json_bytes":serde_json::to_vec(&drafts(0)).unwrap().len(), "capture_median_ns":sorted_capture[2], "capture_max_ns":sorted_capture[4], "write_median_ns":sorted_write[2], "write_max_ns":sorted_write[4], "combined_median_ns":sorted_combined[2], "combined_max_ns":sorted_combined[4], "raw_capture_ns":capture_ns, "raw_write_ns":write_ns, "raw_combined_ns":combined_ns, "claim_restore_verified":true, "origin_unchanged":true})
        );
        claimed.retire().unwrap();
    }
    // Explicit Drop cleanup is included after, never inside, any API timing.
    let path = temporary.path().to_path_buf();
    temporary.close().unwrap();
    assert!(!path.exists());
}
