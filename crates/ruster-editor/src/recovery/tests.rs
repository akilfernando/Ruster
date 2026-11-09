use super::*;
use ruster_scene::{BehaviorBinding, BehaviorSource, PropertyValue, Scene, Session};
use std::collections::BTreeMap;

fn drafts() -> Value {
    serde_json::json!({
        "precision": {"x": "-", "y": "not a number"},
        "source": "return function unfinished(",
        "pending_json": "{\"unfinished\":",
        "rename": "a pending name"
    })
}
fn snapshot(session: &Session) -> Snapshot {
    Snapshot::capture(session, None, &drafts()).unwrap()
}
fn id(journal: &Journal) -> String {
    journal.path.file_name().unwrap().to_str().unwrap().into()
}
fn authored_session() -> Session {
    let mut session = Session::new(Scene::demo()).unwrap();
    session
        .set_source(
            0,
            "unfinished-source",
            Some(BehaviorSource {
                label: Some("Unfinished code".into()),
                language_id: "unknown-backend".into(),
                host_api_version: 1,
                text: "function unfinished(".into(),
            }),
        )
        .unwrap();
    session
        .set_binding(
            1,
            "binding-left",
            Some(BehaviorBinding {
                target: ruster_scene::BindingTarget::Object("left".into()),
                state_defaults: Default::default(),
                source_id: "unfinished-source".into(),
                enabled: true,
                properties: BTreeMap::from([(
                    "orphan".into(),
                    PropertyValue::Text("repair me".into()),
                )]),
            }),
        )
        .unwrap();
    let mut source = session.source("unfinished-source").unwrap().clone();
    source.text = "another unfinished source {".into();
    session
        .set_source(2, "unfinished-source", Some(source))
        .unwrap();
    session.undo(3).unwrap();
    session
}

// Unique risk: recovery must preserve unfinished field text and unknown/invalid
// authored behavior/history, while origin metadata must not restore save authority.
#[test]
fn snapshot_preserves_unfinished_work_and_restores_only_an_unsaved_copy() {
    let temp = tempfile::tempdir().unwrap();
    let origin = temp.path().join("creator.scene.json");
    let mut session = authored_session();
    session.save(&origin).unwrap();
    let original_bytes = fs::read(&origin).unwrap();
    let mut source = session.source("unfinished-source").unwrap().clone();
    source.text = "new invalid draft {".into();
    session
        .set_source(4, "unfinished-source", Some(source))
        .unwrap();
    session.undo(5).unwrap();
    assert_eq!((session.undo_len(), session.redo_len()), (2, 1));
    let asset_origin = temp.path().join("asset-catalog");
    let captured = Snapshot::capture(&session, Some(asset_origin.clone()), &drafts()).unwrap();
    assert_eq!(captured.origin.as_deref(), session.path());
    assert_eq!(captured.asset_origin, Some(asset_origin));
    assert_eq!(captured.drafts().unwrap(), drafts());
    let mut restored = captured.restore().unwrap();
    assert_eq!(restored.scene(), session.scene());
    assert_eq!(restored.authoring(), session.authoring());
    assert_eq!((restored.undo_len(), restored.redo_len()), (2, 1));
    assert!(restored.path().is_none());
    assert!(restored.is_dirty());
    assert!(restored.save(&origin).is_err());
    assert_eq!(fs::read(&origin).unwrap(), original_bytes);
    assert!(restored.path().is_none());
    assert!(restored.is_dirty());
    restored.redo(6).unwrap();
    assert_eq!(
        restored.source("unfinished-source").unwrap().text,
        "new invalid draft {"
    );
    restored.undo(7).unwrap();
    assert_eq!(restored.authoring(), session.authoring());
    let copy = temp.path().join("restored-copy.scene.json");
    restored.save(&copy).unwrap();
    assert_eq!(
        Session::open(&copy).unwrap().authoring(),
        session.authoring()
    );
    assert_eq!(fs::read(&origin).unwrap(), original_bytes);
}

// Unique risk: dropped/crashed windows become recoverable without stealing live
// work; quota pressure cannot evict any record, and duplicated FDs cannot prolong
// a lease after its owner drops. Explicit cleanup is the only slot reclamation.
#[test]
fn leases_quota_and_explicit_discard_preserve_other_windows() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("recovery")).unwrap();
    let session = Session::new(Scene::demo()).unwrap();
    let mut journals = Vec::new();
    for _ in 0..MAX_RECORDS {
        let mut journal = store.allocate().unwrap();
        journal.checkpoint(snapshot(&session)).unwrap();
        journals.push(journal);
    }
    let originals: Vec<_> = journals
        .iter()
        .map(|j| (j.path.clone(), fs::read(j.path.join(RECORD_FILE)).unwrap()))
        .collect();
    let first_id = id(&journals[0]);
    assert!(store.scan().unwrap().is_empty(), "live windows are omitted");
    assert!(store.claim(&first_id).err().unwrap().contains("busy"));
    assert!(store.discard(&first_id).err().unwrap().contains("busy"));
    assert!(store.allocate().err().unwrap().contains("16"));
    for (path, bytes) in &originals {
        assert_eq!(fs::read(path.join(RECORD_FILE)).unwrap(), *bytes);
    }
    #[cfg(unix)]
    let retained_fd = journals[0].owner.0.try_clone().unwrap();
    drop(journals.remove(0)); // Simulates a process/window ending without retiring its record.
    let candidates = store.scan().unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].id, first_id);
    assert!(candidates[0].error.is_none());
    assert_eq!(candidates[0].scene_id.as_deref(), Some("procedural-demo"));
    let (claimed, captured) = store.claim(&first_id).unwrap();
    assert!(captured.restore().unwrap().path().is_none());
    assert!(store.claim(&first_id).err().unwrap().contains("busy"));
    assert!(store.discard(&first_id).err().unwrap().contains("busy"));
    #[cfg(unix)]
    drop(retained_fd); // Successful claim above proves Drop explicitly ended the old lease.
    drop(claimed);
    store.discard(&first_id).unwrap();
    assert!(!originals[0].0.exists());
    let replacement = store.allocate().unwrap();
    replacement.retire().unwrap();
    for (path, bytes) in &originals[1..] {
        assert_eq!(fs::read(path.join(RECORD_FILE)).unwrap(), *bytes);
    }
}

// Unique risk: corrupt/untrusted local records, special paths or stale external
// replacements must remain visible and must not overwrite or silently delete work.
#[test]
fn corrupt_nonregular_and_external_changes_fail_visibly_without_replacing_records() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path().join("recovery")).unwrap();
    let session = authored_session();
    let mut journal = store.allocate().unwrap();
    journal.checkpoint(snapshot(&session)).unwrap();
    let record_path = journal.path.join(RECORD_FILE);
    let entry_path = journal.path.clone();
    let entry_id = id(&journal);
    let good = fs::read(&record_path).unwrap();
    let too_large_drafts = serde_json::json!({"source": "x".repeat(MAX_DRAFT_BYTES + 1)});
    assert!(Snapshot::capture(&session, None, &too_large_drafts).is_err());
    let mut invalid = snapshot(&session);
    invalid.drafts =
        RawValue::from_string(serde_json::to_string(&"x".repeat(MAX_DRAFT_BYTES + 1)).unwrap())
            .unwrap();
    assert!(journal.checkpoint(invalid).is_err());
    assert_eq!(fs::read(&record_path).unwrap(), good);
    let mut external = good.clone();
    external.push(b'\n');
    fs::write(&record_path, &external).unwrap();
    assert!(journal
        .checkpoint(snapshot(&session))
        .err()
        .unwrap()
        .contains("outside"));
    assert_eq!(fs::read(&record_path).unwrap(), external);
    assert!(journal.retire().is_err());
    assert_eq!(fs::read(&record_path).unwrap(), external);
    assert!(
        store.scan().unwrap()[0].error.is_none(),
        "external whitespace is valid data, not save authority"
    );

    let mut bad_checksum: Value = serde_json::from_slice(&good).unwrap();
    bad_checksum["snapshot"]["document_sha256"] = "0".repeat(64).into();
    let mut duplicated = snapshot(&session);
    let version_field = format!("\"format_version\":{}", ruster_scene::FORMAT_VERSION);
    let duplicate_field = format!("{version_field},{version_field}");
    let duplicate_document =
        duplicated
            .document
            .get()
            .replacen(&version_field, &duplicate_field, 1);
    assert_ne!(
        duplicate_document,
        duplicated.document.get(),
        "fixture must insert a duplicate key"
    );
    duplicated.document = RawValue::from_string(duplicate_document).unwrap();
    duplicated.document_sha256 = digest(duplicated.document.get().as_bytes());
    let duplicated = serde_json::to_vec(&Record {
        format: "ruster-editor-recovery".into(),
        version: 1,
        sequence: 1,
        timestamp: 1,
        snapshot: duplicated,
    })
    .unwrap();
    for (label, bytes, expected_error) in [
        ("truncated", b"{".to_vec(), "invalid recovery"),
        (
            "oversized",
            vec![b'x'; MAX_RECORD_BYTES as usize + 1],
            "exceeds",
        ),
        (
            "checksum",
            serde_json::to_vec(&bad_checksum).unwrap(),
            "checksum",
        ),
        ("nested duplicate document", duplicated, "duplicate"),
    ] {
        fs::write(&record_path, &bytes).unwrap();
        let candidates = store.scan().unwrap();
        assert_eq!(candidates.len(), 1, "{label}");
        let error = candidates[0]
            .error
            .as_deref()
            .expect("corrupt record must be visible");
        assert!(
            error.to_lowercase().contains(expected_error),
            "{label}: {error}"
        );
        assert!(candidates[0].scene_id.is_none(), "{label}");
        let claim_error = store
            .claim(&entry_id)
            .err()
            .expect("corrupt record cannot be claimed");
        assert!(
            claim_error.to_lowercase().contains(expected_error),
            "{label}: {claim_error}"
        );
        assert_eq!(fs::read(&record_path).unwrap(), bytes, "{label}");
    }
    fs::write(&record_path, &good).unwrap();
    // Representative nonregular checkpoint and owner paths; neither may disappear
    // from scan as if it were an ordinary Busy lease, or be cleaned automatically.
    for path in [&record_path, &entry_path.join(".owner")] {
        fs::remove_file(path).unwrap();
        fs::create_dir(path).unwrap();
        let candidates = store.scan().unwrap();
        assert_eq!(candidates.len(), 1);
        assert!(candidates[0].error.is_some());
        assert!(store.claim(&entry_id).is_err());
        assert!(store.discard(&entry_id).is_err());
        assert!(path.is_dir());
        fs::remove_dir(path).unwrap();
        fs::write(
            path,
            if path == &record_path {
                good.as_slice()
            } else {
                &[]
            },
        )
        .unwrap();
    }
    #[cfg(unix)]
    {
        let outside = temp.path().join("outside");
        fs::write(&outside, b"creator-owned").unwrap();
        fs::remove_file(&record_path).unwrap();
        std::os::unix::fs::symlink(&outside, &record_path).unwrap();
        assert!(store.scan().unwrap()[0].error.is_some());
        assert!(store.claim(&entry_id).is_err());
        assert!(store.discard(&entry_id).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"creator-owned");
        fs::remove_file(&record_path).unwrap();
        fs::write(&record_path, &good).unwrap();
    }
    let unexpected = entry_path.join("creator-note.txt");
    fs::write(&unexpected, b"preserve").unwrap();
    assert!(store.discard(&entry_id).is_err());
    assert_eq!(fs::read(&record_path).unwrap(), good);
    assert_eq!(fs::read(&unexpected).unwrap(), b"preserve");
    fs::remove_file(unexpected).unwrap();
    store.discard(&entry_id).unwrap();
    assert!(!entry_path.exists());
}

// Unique risk: ordinary edits should debounce, continuous editing must still
// checkpoint within15s, and a completed/reset deadline must start a fresh window.
#[test]
fn deadlines_debounce_cap_continuous_edits_and_reset_without_sleeping() {
    let base = Instant::now();
    let mut deadline = Deadline::default();
    assert_eq!(deadline.remaining(base), None);
    assert!(!deadline.ready(base));
    deadline.changed(base);
    assert_eq!(deadline.remaining(base), Some(Duration::from_secs(2)));
    deadline.changed(base + Duration::from_secs(1));
    assert!(!deadline.ready(base + Duration::from_secs(2)));
    assert!(deadline.ready(base + Duration::from_secs(3)));
    for seconds in 2..=14 {
        deadline.changed(base + Duration::from_secs(seconds));
    }
    assert!(!deadline.ready(base + Duration::from_millis(14_999)));
    assert!(deadline.ready(base + Duration::from_secs(15)));
    assert_eq!(
        deadline.remaining(base + Duration::from_secs(20)),
        Some(Duration::ZERO)
    );
    deadline.clear();
    assert_eq!(deadline.remaining(base + Duration::from_secs(20)), None);
    deadline.changed(base + Duration::from_secs(20));
    assert!(!deadline.ready(base + Duration::from_secs(21)));
    assert!(deadline.ready(base + Duration::from_secs(22)));
}
