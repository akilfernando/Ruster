use ruster_scene::{Object, Scene, Session, Transform, MAX_FILE_BYTES};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const SAMPLES: usize = 25;
const WARMUP: usize = 5;

fn scene_with(count: usize) -> Scene {
    let mut scene = Scene::empty("authoring-cost");
    scene.objects = (0..count)
        .map(|n| Object::cube(format!("object-{n:03}"), None))
        .collect();
    scene
}

fn edits(session: &Session, count: usize, _iteration: usize) -> Vec<(String, Transform)> {
    session.scene().objects.iter().take(count).map(|object| {
        let mut transform = object.transform.clone();
        transform.translation[0] = if transform.translation[0] == 0.0 { 0.001 } else { 0.0 };
        (object.id.clone(), transform)
    }).collect()
}

fn millis(d: Duration) -> f64 { d.as_secs_f64() * 1_000.0 }

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let index = (((sorted.len() - 1) as f64) * p).ceil() as usize;
    sorted[index]
}

fn report(label: &str, values: &[Duration], failures: usize) {
    let mut ms: Vec<_> = values.iter().copied().map(millis).collect();
    ms.sort_by(f64::total_cmp);
    if ms.is_empty() {
        println!("{label}: successes=0 failures={failures}");
        return;
    }
    println!("{label}: successes={} failures={} p50_ms={:.3} p95_ms={:.3} max_ms={:.3}",
        ms.len(), failures, percentile(&ms, 0.50), percentile(&ms, 0.95), ms[ms.len()-1]);
}

fn measured_edits(session: &mut Session, count: usize, label: &str, start_i: usize) {
    let mut warmup_failures = 0;
    for i in 0..WARMUP {
        let e = edits(session, count, start_i + i);
        let rev = session.scene().revision;
        if session.edit_transforms(rev, &e).is_err() { warmup_failures += 1; }
        let e2 = edits(session, count, start_i + i + 1);
        let rev = session.scene().revision;
        if session.edit_transforms(rev, &e2).is_err() { warmup_failures += 1; }
    }
    let mut times = Vec::new();
    let mut rejected_times = Vec::new();
    let mut noop_times = Vec::new();
    for i in 0..SAMPLES {
        let e = edits(session, count, start_i + WARMUP + i);
        let rev = session.scene().revision;
        let begin = Instant::now();
        let result = session.edit_transforms(rev, &e);
        let elapsed = begin.elapsed();
        match result {
            Ok(_) if session.scene().revision != rev => times.push(elapsed),
            Ok(_) => noop_times.push(elapsed),
            Err(err) => { rejected_times.push(elapsed); eprintln!("{label} edit rejected: {err}"); }
        }
    }
    report(label, &times, rejected_times.len() + noop_times.len());
    report(&format!("{label} rejected attempts"), &rejected_times, 0);
    report(&format!("{label} no-op attempts"), &noop_times, 0);
    println!("{label} warmup_capacity_rejections={warmup_failures}");
}

fn measured_history(session: &mut Session, label: &str) {
    let mut undo_times = Vec::new();
    let mut redo_times = Vec::new();
    let mut failures = 0;
    for i in 0..(WARMUP + SAMPLES) {
        let rev = session.scene().revision;
        let begin = Instant::now();
        let undo = session.undo(rev);
        let undo_elapsed = begin.elapsed();
        if let Err(err) = undo { failures += 1; eprintln!("{label} undo rejected: {err}"); break; }
        let rev = session.scene().revision;
        let begin = Instant::now();
        let redo = session.redo(rev);
        let redo_elapsed = begin.elapsed();
        if let Err(err) = redo { failures += 1; eprintln!("{label} redo rejected: {err}"); break; }
        if i >= WARMUP { undo_times.push(undo_elapsed); redo_times.push(redo_elapsed); }
    }
    report(&format!("{label} undo"), &undo_times, failures);
    report(&format!("{label} redo"), &redo_times, failures);
}

fn seeded_session(count: usize, history: usize, group: usize) -> Session {
    let mut session = Session::new(scene_with(count)).unwrap();
    for i in 0..history {
        let e = edits(&session, group, i);
        let rev = session.scene().revision;
        session.edit_transforms(rev, &e).expect("history seed should be accepted");
    }
    session
}

fn ordinary(count: usize, history: usize) {
    let group = if count == 256 && history > 0 { 3 } else { count };
    let mut session = seeded_session(count, history, group);
    let label = format!("objects={count} history={history} transform(group={group})");
    measured_edits(&mut session, group, &label, history * 2);
    let retained = session.undo_len();
    measured_history(&mut session, &format!("objects={count} history~{retained}"));
}

fn near_limit() {
    let mut session = Session::new(scene_with(256)).unwrap();
    let path = PathBuf::from("/tmp/ruster-authoring-bench/near-bound.scene.json");
    let mut accepted_seeds = 0usize;
    let mut last_bytes = 0u64;
    for i in 0..32 {
        let e = edits(&session, 256, i);
        let rev = session.scene().revision;
        match session.edit_transforms(rev, &e) {
            Ok(_) => accepted_seeds += 1,
            Err(err) => { eprintln!("near-bound seed stopped at history={accepted_seeds}: {err}"); break; }
        }
        session.save(&path).expect("seed save should succeed");
        last_bytes = fs::metadata(&path).unwrap().len();
        if last_bytes >= (MAX_FILE_BYTES as f64 * 0.82) as u64 { break; }
    }
    let ratio = last_bytes as f64 / MAX_FILE_BYTES as f64;
    println!("near-bound setup: objects=256 large_group_history={accepted_seeds} serialized_bytes={last_bytes} limit_bytes={MAX_FILE_BYTES} occupancy={:.1}%", ratio * 100.0);
    if last_bytes == 0 { eprintln!("near-bound setup did not produce a saved size"); return; }
    measured_edits(&mut session, 1, "objects=256 near-byte-bound transform(group=1)", accepted_seeds * 2);
    let retained = session.undo_len();
    measured_history(&mut session, &format!("objects=256 near-byte-bound history={retained}"));
    let _ = fs::remove_file(path);
}


fn grouped_delete_case() {
    let mut times_delete = Vec::new();
    let mut times_undo = Vec::new();
    let mut times_redo = Vec::new();
    let mut failures = 0;
    for sample in 0..SAMPLES {
        let mut session = Session::new(scene_with(256)).unwrap();
        // Seed substantial retained history so delete validation also walks large deltas.
        for i in 0..8 {
            let e = edits(&session, 3, i);
            let rev = session.scene().revision;
            if session.edit_transforms(rev, &e).is_err() { failures += 1; break; }
        }
        let ids: Vec<String> = session.scene().objects.iter().map(|o| o.id.clone()).collect();
        let rev = session.scene().revision;
        let begin = Instant::now();
        let deleted = session.delete_many(rev, &ids);
        let elapsed = begin.elapsed();
        if let Err(err) = deleted { failures += 1; eprintln!("grouped delete sample rejected: {err}"); continue; }
        times_delete.push(elapsed);
        let rev = session.scene().revision;
        let begin = Instant::now();
        let undone = session.undo(rev);
        let elapsed = begin.elapsed();
        if let Err(err) = undone { failures += 1; eprintln!("grouped delete undo rejected: {err}"); continue; }
        times_undo.push(elapsed);
        let rev = session.scene().revision;
        let begin = Instant::now();
        let redone = session.redo(rev);
        let elapsed = begin.elapsed();
        if let Err(err) = redone { failures += 1; eprintln!("grouped delete redo rejected: {err}"); continue; }
        times_redo.push(elapsed);
        if sample == SAMPLES - 1 {
            println!("grouped delete setup: objects=256 retained_transform_history={} result_objects={} (undo restores all 256)", session.undo_len(), session.scene().objects.len());
        }
    }
    report("objects=256 history=8 delete(group=256)", &times_delete, failures);
    report("objects=256 history=9 undo(group delete restores 256)", &times_undo, failures);
    report("objects=256 history=8 redo(group delete)", &times_redo, failures);
}

fn main() {
    println!("release benchmark samples={SAMPLES} warmup={WARMUP}; CPU-only Session API calls; no GPU/UI");
    for count in [3, 64, 256] {
        ordinary(count, 0);
        ordinary(count, 8);
    }
    near_limit();
    grouped_delete_case();
}
