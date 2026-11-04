//! Integration coverage for the bundled Lua VM's hook ordering and pause ownership.
use ruster_behavior::{
    debug::{
        Breakpoint, DebugError, DebugHandle, DebugRunId, DebugSession, Resume, StopSnapshot,
        ValueNode,
    },
    source_hash, Diagnostic, DiagnosticCode, Engine, Limits,
};
use ruster_scene::{Authoring, BehaviorBinding, BehaviorSource, Object, PropertyValue, Scene};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc,
    },
    thread,
    time::Duration,
};

fn fixture(text: &str) -> (Scene, Authoring) {
    let mut scene = Scene::empty("debug");
    scene.objects = ["a", "b"].map(|id| Object::cube(id, None)).into();
    let mut authoring = Authoring::default();
    authoring.sources.insert(
        "source".into(),
        BehaviorSource {
            label: None,
            language_id: "lua54".into(),
            host_api_version: 1,
            text: text.into(),
        },
    );
    authoring.bindings.insert(
        "binding-a".into(),
        BehaviorBinding {
            object_id: "a".into(),
            source_id: "source".into(),
            enabled: true,
            properties: BTreeMap::new(),
        },
    );
    (scene, authoring)
}
fn point(text: &str, line: u32) -> Breakpoint {
    Breakpoint {
        source_id: "source".into(),
        source_hash: source_hash(text),
        line,
        binding_id: None,
    }
}
struct Report {
    result: Result<(), Diagnostic>,
    scene: Scene,
    tick: u64,
    pause: bool,
}
struct Job {
    handle: Option<DebugHandle>,
    wakes: Receiver<()>,
    start: SyncSender<()>,
    done: Receiver<Report>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Job {
    fn spawn(scene: Scene, authoring: Authoring, points: Vec<Breakpoint>, limits: Limits) -> Self {
        let (created_tx, created) = mpsc::sync_channel(1);
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let (wake, wakes) = mpsc::sync_channel(1);
        let (start, start_rx) = mpsc::sync_channel(1);
        let (done_tx, done) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || {
            let cancel = Arc::new(AtomicBool::new(false));
            let mut engine = Engine::with_cancellation(limits, cancel.clone()).unwrap();
            let (session, handle) = DebugSession::new(
                DebugRunId::new(17),
                cancel,
                points,
                Some(Arc::new(move || {
                    let _ = wake.try_send(());
                })),
            )
            .unwrap();
            engine.set_debugger(Some(session)).unwrap();
            created_tx.send(handle).unwrap();
            let mut runtime = engine.reset(&scene, &authoring).unwrap();
            ready_tx.send(()).unwrap();
            if start_rx.recv().is_err() {
                return;
            }
            let result = engine.step(&mut runtime);
            let _ = done_tx.send(Report {
                result,
                scene: runtime.scene().clone(),
                tick: runtime.tick(),
                pause: engine.take_debug_pause_requested(),
            });
        });
        let handle = created.recv_timeout(Duration::from_secs(3)).unwrap();
        // Reset runs module+update at dt0; armed breakpoints must never pause it.
        ready.recv_timeout(Duration::from_secs(3)).unwrap();
        Self {
            handle: Some(handle),
            wakes,
            start,
            done,
            thread: Some(thread),
        }
    }
    fn handle(&self) -> &DebugHandle {
        self.handle.as_ref().unwrap()
    }
    fn start(&self) {
        self.start.send(()).unwrap();
    }
    fn stop(&self) -> Arc<StopSnapshot> {
        for _ in 0..3 {
            if let Some(stop) = self.handle().stop() {
                return stop;
            }
            self.wakes.recv_timeout(Duration::from_secs(3)).unwrap();
        }
        panic!("notified without a stop")
    }
    fn finish(mut self) -> Report {
        let report = self.done.recv_timeout(Duration::from_secs(3)).unwrap();
        self.thread.take().unwrap().join().unwrap();
        report
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        drop(self.handle.take());
        if self.thread.as_ref().is_some_and(|t| t.is_finished()) {
            let _ = self.thread.take().unwrap().join();
        }
    }
}

// Unique risk: step-over must follow activations, including tail replacement,
// rather than treating the next line number or an arbitrary callee as Next.
#[test]
fn next_nested_tail_same_line_and_root_return() {
    let nested = "local function inner(o)\n o.translation[1]=o.translation[1]+1\n return o\nend\nreturn {update=function(o,p,dt,host)\n local result=inner(o)\n result.translation[1]=result.translation[1]+2\n return result\nend}";
    for callee_break in [false, true] {
        let (scene, authoring) = fixture(nested);
        let mut points = vec![point(nested, 6)];
        if callee_break {
            points.push(point(nested, 2));
        }
        let job = Job::spawn(scene, authoring, points, Limits::default());
        job.start();
        let first = job.stop();
        assert_eq!(first.line, Some(6));
        assert_eq!(first.committed_tick, 0);
        assert_eq!(first.attempted_tick, 1);
        job.handle().resume(first.stop, Resume::Next).unwrap();
        assert_eq!(
            job.handle().resume(first.stop, Resume::Continue),
            Err(DebugError::StaleStop)
        );
        let second = job.stop();
        assert_eq!(second.line, Some(if callee_break { 2 } else { 7 }));
        job.handle().set_breakpoints(vec![]).unwrap();
        job.handle().resume(second.stop, Resume::Continue).unwrap();
        let report = job.finish();
        report.result.unwrap();
        assert_eq!(report.tick, 1);
        assert_eq!(report.scene.objects[0].transform.translation[0], 3.0);
    }
    let tail = "local function replacement(o)\n o.translation[1]=4\n return o\nend\nreturn {update=function(o)\n return replacement(o)\nend}";
    let (scene, authoring) = fixture(tail);
    let job = Job::spawn(scene, authoring, vec![point(tail, 6)], Limits::default());
    job.start();
    let stop = job.stop();
    job.handle().resume(stop.stop, Resume::Next).unwrap();
    let report = job.finish();
    report.result.unwrap();
    assert!(report.pause);
    assert_eq!(report.tick, 1);
    assert_eq!(report.scene.objects[0].transform.translation[0], 4.0);
    let looped = "return {update=function(o)\n local i=0\n while i<3 do i=i+1 end\n return o\nend}";
    let (scene, authoring) = fixture(looped);
    let job = Job::spawn(scene, authoring, vec![point(looped, 3)], Limits::default());
    job.start();
    let first = job.stop();
    job.handle().set_breakpoints(vec![]).unwrap();
    job.handle().resume(first.stop, Resume::Next).unwrap();
    let second = job.stop();
    assert_eq!(second.line, Some(3));
    job.handle().resume(second.stop, Resume::Continue).unwrap();
    job.finish().result.unwrap();
}

// Unique risk: locals contain cycles, shadowed names, nonfinite numbers and
// huge strings; inspection must remain raw, bounded and owned after resume.
#[test]
fn owned_snapshot_bounds_and_exact_stops() {
    let text = format!(
        "return {{update=function(o)\n local shadow=1; local nan=0/0; local infinity=1/0; local hidden=function() end\n do local shadow=2; local cycle={{}}; cycle.self=cycle\n local big=\"{}\"\n local huge={{}}; for i=1,80 do huge[i]=big end\n o.translation[1]=shadow\n end\n return o\nend}}",
        "界".repeat(4_000)
    );
    let (scene, authoring) = fixture(&text);
    let job = Job::spawn(scene, authoring, vec![point(&text, 6)], Limits::default());
    job.start();
    let stop = job.stop();
    assert_eq!(stop.run, DebugRunId::new(17));
    assert!(stop.frames.len() <= 8);
    assert!(stop.nodes.len() <= 2048);
    assert!(stop.frames.iter().map(|f| f.locals.len()).sum::<usize>() <= 64);
    assert!(stop.truncated);
    assert!(stop.nodes.iter().any(|n| matches!(n, ValueNode::Cycle)));
    assert!(stop
        .nodes
        .iter()
        .any(|n| matches!(n, ValueNode::Opaque("nonfinite"))));
    assert!(stop
        .nodes
        .iter()
        .all(|n| !matches!(n,ValueNode::Number(v) if !v.is_finite())));
    assert!(
        stop.nodes
            .iter()
            .filter_map(|n| if let ValueNode::Text(s) = n {
                Some(s.len())
            } else {
                None
            })
            .sum::<usize>()
            <= 64 * 1024
    );
    assert!(stop
        .nodes
        .iter()
        .all(|node| !matches!(node, ValueNode::Text(text) if text.chars().count() > 256)));
    assert!(stop
        .nodes
        .iter()
        .all(|node| !matches!(node, ValueNode::Text(text) if text.contains('\u{fffd}'))));
    let shadowed: Vec<_> = stop.frames[0]
        .locals
        .iter()
        .filter(|local| local.name == "shadow")
        .collect();
    assert_eq!(shadowed.len(), 2);
    assert_ne!(shadowed[0].ordinal, shadowed[1].ordinal);
    serde_json::to_vec(&*stop).unwrap();
    job.handle().resume(stop.stop, Resume::Continue).unwrap();
    let report = job.finish();
    report.result.unwrap();
    assert!(
        !stop.frames.is_empty(),
        "owned old snapshot remains readable, never current"
    );
}

// Unique risk: a paused VM is not servicing its actor queue, and cancellation or
// disconnect must unwind it and discard all staged transforms from earlier bindings.
#[test]
fn cancel_disconnect_and_later_binding_failure_are_atomic() {
    let text = "return {update=function(o,p,dt)\n o.translation[1]=7\n if p.fail and dt>0 then local bad=nil+1 end\n return o\nend}";
    for mode in 0..3 {
        let (scene, mut authoring) = fixture(text);
        let mut second = authoring.bindings["binding-a"].clone();
        second.object_id = "b".into();
        second
            .properties
            .insert("fail".into(), PropertyValue::Bool(true));
        authoring.bindings.insert("binding-b".into(), second);
        let mut bp = point(text, 3);
        bp.binding_id = Some("binding-b".into());
        let mut job = Job::spawn(scene.clone(), authoring, vec![bp], Limits::default());
        job.start();
        let stop = job.stop();
        match mode {
            0 => job.handle().cancel(),
            1 => drop(job.handle.take()),
            _ => job.handle().resume(stop.stop, Resume::Continue).unwrap(),
        }
        let report = job.finish();
        let error = report.result.unwrap_err();
        assert_eq!(
            error.code,
            if mode < 2 {
                DiagnosticCode::Cancelled
            } else {
                DiagnosticCode::Runtime
            }
        );
        assert_eq!(report.tick, 0);
        assert_eq!(report.scene, scene);
    }
    // Reject a token mismatch before it can strand a stopped hook on a different token.
    let mut engine = Engine::new(Limits::default()).unwrap();
    let (foreign, _handle) = DebugSession::new(
        DebugRunId::new(9),
        Arc::new(AtomicBool::new(false)),
        vec![],
        None,
    )
    .unwrap();
    assert_eq!(
        engine.set_debugger(Some(foreign)).unwrap_err().code,
        DiagnosticCode::InvalidLimits
    );
    let (session, detached) =
        DebugSession::new(DebugRunId::new(10), engine.cancellation(), vec![], None).unwrap();
    engine.set_debugger(Some(session)).unwrap();
    engine.set_debugger(None).unwrap();
    assert_eq!(
        detached.set_breakpoints(vec![]),
        Err(DebugError::Disconnected)
    );
    // A stale handle must not poison an Engine token subsequently reused by a new run.
    let cancel = Arc::new(AtomicBool::new(false));
    let (old, handle) =
        DebugSession::new(DebugRunId::new(1), cancel.clone(), vec![], None).unwrap();
    drop(old);
    drop(handle);
    assert!(!cancel.load(Ordering::SeqCst));
}

// Unique risk: adding line/call hooks must not charge their events as instructions.
// Compare the actual acceptance boundary with count-only execution, not wall time.
#[test]
fn armed_budget_parity_and_hash_bound_breakpoints() {
    let text = "return {update=function(o) local s=0; for i=1,160 do s=s+i end; o.translation[1]=s; return o end}";
    let (scene, authoring) = fixture(text);
    for budget in [100, 200, 400, 800, 1600] {
        let limits = Limits {
            binding_instructions: budget,
            hook_interval: 10,
            ..Limits::default()
        };
        let mut plain = Engine::new(limits.clone()).unwrap();
        let plain = plain
            .reset(&scene, &authoring)
            .map(|_| ())
            .map_err(|e| e.code);
        let mut armed = Engine::new(limits).unwrap();
        let mut wrong = point(text, 1);
        wrong.source_hash = "0".repeat(64);
        let (session, _handle) =
            DebugSession::new(DebugRunId::new(2), armed.cancellation(), vec![wrong], None).unwrap();
        armed.set_debugger(Some(session)).unwrap();
        let debug = armed
            .reset(&scene, &authoring)
            .map(|_| ())
            .map_err(|e| e.code);
        assert_eq!(plain, debug, "budget {budget}");
        if debug.is_ok() {
            let mut runtime = armed.reset(&scene, &authoring).unwrap();
            armed.step(&mut runtime).unwrap();
            assert_eq!(runtime.tick(), 1);
        }
    }
}
