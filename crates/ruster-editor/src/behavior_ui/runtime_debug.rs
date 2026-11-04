//! Cached, owned debugger presentation and asynchronous operation receipts.
use super::*;
use ruster_behavior::debug::{Breakpoint, Resume, StopSnapshot, ValueNode};
use serde_json::{json, Value};
use std::collections::VecDeque;

#[derive(Default)]
pub(super) struct DebugUi {
    pub points: Vec<Breakpoint>,
    pub open: bool,
    source: String,
    line: String,
    next_operation: u64,
    active: Option<Receipt>,
    recent: VecDeque<Receipt>,
    last_stop: Option<ruster_behavior::debug::StopId>,
    hash_revision: Option<u64>,
    source_hashes: BTreeMap<String, String>,
}
struct Receipt {
    id: u64,
    key: RunKey,
    operation: Operation,
    state: &'static str,
}
fn run_id(key: RunKey) -> String {
    format!(
        "{}:{}:{}",
        key.generation, key.reset_epoch, key.authored_revision
    )
}
impl Receipt {
    fn value(&self) -> Value {
        json!({"operation_id":self.id.to_string(),"run_id":run_id(self.key),"operation":format!("{:?}",self.operation),"state":self.state})
    }
}
impl RuntimeUi {
    pub(super) fn clear_debug_document(&mut self) {
        self.debug = DebugUi {
            next_operation: self.debug.next_operation,
            ..Default::default()
        };
    }
    pub(super) fn fail_operation(&mut self) {
        self.awaiting = None;
        if let Some(mut receipt) = self.debug.active.take() {
            receipt.state = "failed";
            self.retain_receipt(receipt);
        }
    }
    pub(super) fn show_new_debug_stop(&mut self) {
        if let Some(stop) = self.debug_snapshot() {
            if self.debug.last_stop != Some(stop.stop) {
                self.debug.last_stop = Some(stop.stop);
                self.debug.open = true;
            }
        }
    }
    pub(crate) fn current_run(&self) -> Option<RunKey> {
        self.worker.as_ref().map(VmWorker::current_key)
    }
    pub(crate) fn debug_snapshot(&self) -> Option<Arc<StopSnapshot>> {
        let worker = self.worker.as_ref()?;
        worker.debug_stop(worker.current_key()).ok().flatten()
    }
    pub(super) fn supersede_operation(&mut self) {
        self.awaiting = None;
        if let Some(mut receipt) = self.debug.active.take() {
            receipt.state = "superseded";
            self.retain_receipt(receipt);
        }
    }
    fn retain_receipt(&mut self, receipt: Receipt) {
        if self.debug.recent.len() == 16 {
            self.debug.recent.pop_front();
        }
        self.debug.recent.push_back(receipt);
    }
    pub(super) fn accept_operation(&mut self, event: &VmEvent) {
        let operation = match event {
            VmEvent::Reset { .. } => Operation::Reset,
            VmEvent::Step { .. } => Operation::Step,
            VmEvent::State { state, .. } => match state {
                State::Playing => Operation::Play,
                State::Paused => Operation::Pause,
                State::Invalidated => Operation::Invalidate,
                State::Stopped => Operation::Stop,
            },
            VmEvent::Error { operation, .. } | VmEvent::Rejected { operation, .. } => *operation,
        };
        if self.awaiting == Some((event.key(), operation)) {
            self.awaiting = None;
        }
        if self
            .debug
            .active
            .as_ref()
            .is_some_and(|receipt| receipt.key == event.key() && receipt.operation == operation)
        {
            let mut receipt = self.debug.active.take().unwrap();
            receipt.state = if matches!(event, VmEvent::Error { .. } | VmEvent::Rejected { .. }) {
                "failed"
            } else {
                "completed"
            };
            self.retain_receipt(receipt);
        }
    }
    fn queue_control(&mut self, operation: Operation) -> Result<RunKey, String> {
        if self.shutdown
            || !self.ready
            || self.pending.is_some()
            || self.awaiting.is_some()
            || self.debug_snapshot().is_some()
        {
            return Err(
                "Runtime is busy or not ready; inspect it, or resume the current debugger stop."
                    .into(),
            );
        }
        let key = self
            .worker
            .as_ref()
            .ok_or("Reset runtime first.")?
            .current_key();
        self.pending = Some(match operation {
            Operation::Play => Request::Play(key),
            Operation::Pause => Request::Pause(key),
            Operation::Step if !self.playing => Request::Step(key),
            _ => return Err("Pause playback before stepping a whole tick.".into()),
        });
        self.flush();
        Ok(key)
    }
    pub(crate) fn breakpoint(
        &mut self,
        session: &Session,
        point: Breakpoint,
        enabled: bool,
    ) -> Result<(), String> {
        if enabled {
            let source = session
                .source(&point.source_id)
                .ok_or("Source is unavailable.")?;
            if ruster_behavior::source_hash(&source.text) != point.source_hash
                || point.line == 0
                || point.line > 16385
            {
                return Err("Breakpoint source hash or line is stale/invalid.".into());
            }
            if point.binding_id.as_ref().is_some_and(|id| {
                session
                    .binding(id)
                    .is_none_or(|binding| binding.source_id != point.source_id)
            }) {
                return Err("Breakpoint binding does not use this source.".into());
            }
        }
        let mut points = self.debug.points.clone();
        points.retain(|old| old != &point);
        if enabled {
            if points.len() == 256 {
                return Err("Maximum256 breakpoints reached.".into());
            }
            points.push(point);
        }
        if let Some(worker) = &self.worker {
            let state = worker
                .set_breakpoints(worker.current_key(), points.clone())
                .map_err(|e| e.to_string())?;
            self.status = format!("Breakpoint configuration: {state:?}");
        } else {
            self.status = "Breakpoint configured; Reset runtime to arm the debugger.".into();
        }
        self.debug.points = points;
        Ok(())
    }
    pub(crate) fn debug_window(
        &mut self,
        ctx: &egui::Context,
        session: &Session,
        blocked: bool,
    ) -> Option<ruster_behavior::Diagnostic> {
        if !self.debug.open {
            return None;
        }
        if self.debug.hash_revision != Some(session.scene().revision) {
            self.debug.source_hashes = session
                .authoring()
                .sources
                .iter()
                .map(|(id, source)| (id.clone(), ruster_behavior::source_hash(&source.text)))
                .collect();
            self.debug.hash_revision = Some(session.scene().revision);
        }
        let stop = self.debug_snapshot();
        let mut opened = true;
        let mut navigate = false;
        let mut point = None;
        let mut remove = None;
        let mut resume = None;
        egui::Window::new("Behavior debugger").open(&mut opened).default_width(600.0)
            .max_width((ctx.content_rect().width()-48.0).max(240.0)).show(ctx, |ui| {
            ui.small("Source lines are 1-based. Breakpoints use the exact saved source hash. First activation requires Reset.");
            ui.add_enabled_ui(!blocked, |ui| {
                egui::ComboBox::from_label("Breakpoint source").selected_text(if self.debug.source.is_empty() { "Choose source" } else { &self.debug.source }).show_ui(ui, |ui| {
                    for id in session.authoring().sources.keys() { ui.selectable_value(&mut self.debug.source, id.clone(), id); }
                });
                ui.horizontal(|ui| {
                    ui.label("Line");
                    let field=ui.add(egui::TextEdit::singleline(&mut self.debug.line).char_limit(5).desired_width(70.0));
                    let owns=field.has_focus()||field.lost_focus();
                    let enter=owns&&ui.input_mut(|input|input.consume_key(egui::Modifiers::NONE,egui::Key::Enter));
                    let escape=owns&&ui.input_mut(|input|input.consume_key(egui::Modifiers::NONE,egui::Key::Escape));
                    if enter { field.request_focus(); }
                    if escape { self.debug.open=false; field.surrender_focus(); }
                    if ui.button("Add breakpoint").clicked() || enter {
                        if let (Some(source), Ok(line)) = (session.source(&self.debug.source), self.debug.line.parse::<u32>()) {
                            point = Some(Breakpoint { source_id:self.debug.source.clone(), source_hash:ruster_behavior::source_hash(&source.text), line, binding_id:None });
                        } else { self.status = "Choose a source and a positive line number.".into(); }
                    }
                });
            });
            egui::ScrollArea::vertical().id_salt("breakpoint_list").max_height(110.0).show(ui, |ui| {
                for (index, point) in self.debug.points.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let stale = self.debug.source_hashes.get(&point.source_id) != Some(&point.source_hash);
                        ui.label(format!("{}:{} · {}{}", point.source_id, point.line, &point.source_hash[..12], if stale { " (stale)" } else { "" }));
                        if ui.add_enabled(!blocked, egui::Button::new("Remove")).clicked() { remove = Some(index); }
                    });
                }
            });
            if let Some(stop) = &stop {
                ui.separator();
                ui.label(format!("Stopped {}:{} · {:?} · stop {}", stop.source_id, stop.line.unwrap_or(0), stop.reason, stop.stop.get()));
                ui.label(format!("Committed tick {} · pending tick {} remains private", stop.committed_tick, stop.attempted_tick));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!blocked, egui::Button::new("Next (step over)")).clicked() { resume = Some(Resume::Next); }
                    if ui.add_enabled(!blocked, egui::Button::new("Continue behavior")).clicked() { resume = Some(Resume::Continue); }
                    if ui.button("Show stopped source").clicked() { navigate = true; }
                });
                egui::ScrollArea::vertical().id_salt("debug_locals").max_height(300.0).show(ui, |ui| {
                    for (index, frame) in stop.frames.iter().enumerate() {
                        ui.collapsing(format!("Frame {index}: {} · line {}", frame.name.as_deref().unwrap_or("<anonymous>"), frame.line.unwrap_or(0)), |ui| {
                            for local in &frame.locals {
                                ui.label(format!("{} #{} = {}", local.name, local.ordinal, node_summary(stop, local.node, 0)));
                            }
                        });
                    }
                    if stop.truncated { ui.small("Inspection bounds reached; some values are truncated."); }
                });
            } else { ui.label("No current source stop. Reset, then Step or Play to reach an armed breakpoint."); }
        });
        self.debug.open &= opened;
        if let Some(point) = point {
            if let Err(error) = self.breakpoint(session, point, true) {
                self.status = error;
            }
        }
        if let Some(index) = remove {
            let point = self.debug.points[index].clone();
            // Removing stale points is always allowed; it grants no VM/source authority.
            self.debug.points.remove(index);
            if let Some(worker) = &self.worker {
                if let Err(error) =
                    worker.set_breakpoints(worker.current_key(), self.debug.points.clone())
                {
                    self.debug.points.insert(index, point);
                    self.status = error.to_string();
                }
            }
        }
        if let (Some(stop), Some(action), Some(worker)) = (&stop, resume, &self.worker) {
            if let Err(error) = worker.debug_resume(worker.current_key(), stop.stop, action) {
                self.status = error.to_string();
            }
        }
        if navigate {
            let stop = stop?;
            return Some(ruster_behavior::Diagnostic {
                code: ruster_behavior::DiagnosticCode::Runtime,
                source_id: Some(stop.source_id.clone().into()),
                source_hash: Some(stop.source_hash.clone().into()),
                binding_id: Some(stop.binding_id.clone().into()),
                object_id: Some(stop.object_id.clone().into()),
                phase: stop.phase,
                message: "Debugger source stop".into(),
                line: stop.line,
            });
        }
        None
    }
    pub(crate) fn runtime_api(
        &mut self,
        op: &str,
        args: &Value,
        session: &Session,
        generation: u64,
    ) -> Result<Value, String> {
        let current = self.worker.as_ref().map(VmWorker::current_key);
        let offset = args
            .get("offset")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        if op == "behavior_runtime_inspect" {
            let requested = args
                .get("operation_id")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<u64>().ok());
            let receipt = requested
                .and_then(|id| {
                    self.debug
                        .active
                        .iter()
                        .chain(self.debug.recent.iter())
                        .find(|receipt| receipt.id == id)
                })
                .map(Receipt::value);
            let objects: Vec<_> = self
                .frame
                .iter()
                .flat_map(|frame| frame.scene.objects.iter())
                .skip(offset)
                .take(16)
                .map(|object| json!({"id":object.id,"transform":object.transform}))
                .collect();
            let total = self
                .frame
                .as_ref()
                .map_or(0, |frame| frame.scene.objects.len());
            return Ok(
                json!({"run_id":current.map(run_id),"frame_run_id":self.frame.as_ref().map(|frame|run_id(frame.key)),"stale":self.stale,"visible":self.visible,"ready":self.ready,"playing":self.playing,"busy":self.pending.is_some()||self.awaiting.is_some(),"debug_stop_id":self.debug_snapshot().map(|stop|stop.stop.get().to_string()),"tick":self.frame.as_ref().map(|frame|frame.tick.to_string()),"status":self.status,"diagnostic":self.diagnostic,"operation":receipt,"recent_operations":self.debug.recent.iter().map(Receipt::value).collect::<Vec<_>>(),"objects":objects,"object_count":total,"next_offset":(offset.saturating_add(16)<total).then(||offset.saturating_add(16).to_string())}),
            );
        }
        if op == "behavior_breakpoints" {
            let points:Vec<_> = self.debug.points.iter().skip(offset).take(16).map(|point|json!({"source_id":point.source_id,"source_hash":point.source_hash,"line":point.line.to_string(),"binding_id":point.binding_id})).collect();
            return Ok(
                json!({"breakpoints":points,"count":self.debug.points.len(),"next_offset":(offset.saturating_add(16)<self.debug.points.len()).then(||offset.saturating_add(16).to_string())}),
            );
        }
        if op == "behavior_breakpoint_set" {
            self.breakpoint(
                session,
                Breakpoint {
                    source_id: args["source_id"]
                        .as_str()
                        .ok_or("Missing source ID")?
                        .into(),
                    source_hash: args["source_hash"]
                        .as_str()
                        .ok_or("Missing source hash")?
                        .into(),
                    line: args["line"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .ok_or("Invalid line")?,
                    binding_id: args["binding_id"].as_str().map(str::to_owned),
                },
                args["enabled"].as_bool().ok_or("Invalid enabled flag")?,
            )?;
            return Ok(json!({"configured":true,"status":self.status}));
        }
        if op != "behavior_runtime_reset"
            && args["expected_run_id"].as_str() != current.map(run_id).as_deref()
        {
            return Err("stale_run".into());
        }
        if op.starts_with("behavior_debug_") {
            let stop = self.debug_snapshot().ok_or("stale_stop")?;
            if args["expected_stop_id"].as_str() != Some(&stop.stop.get().to_string()) {
                return Err("stale_stop".into());
            }
            if op == "behavior_debug_inspect" {
                let limit = args
                    .get("limit")
                    .and_then(Value::as_str)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(16)
                    .clamp(1, 16);
                let nodes: Vec<_> = stop
                    .nodes
                    .iter()
                    .enumerate()
                    .skip(offset)
                    .take(limit)
                    .map(|(index, node)| json!({"id":index.to_string(),"value":node_wire(node)}))
                    .collect();
                return Ok(
                    json!({"run_id":current.map(run_id),"stop_id":stop.stop.get().to_string(),"source_id":stop.source_id,"source_hash":stop.source_hash,"binding_id":stop.binding_id,"object_id":stop.object_id,"line":stop.line.map(|line|line.to_string()),"committed_tick":stop.committed_tick.to_string(),"attempted_tick":stop.attempted_tick.to_string(),"reason":stop.reason,"frames":stop.frames.iter().map(frame_wire).collect::<Vec<_>>(),"nodes":nodes,"node_count":stop.nodes.len(),"truncated":stop.truncated,"next_offset":(offset.saturating_add(limit)<stop.nodes.len()).then(||offset.saturating_add(limit).to_string())}),
                );
            }
            self.worker
                .as_ref()
                .unwrap()
                .debug_resume(
                    current.unwrap(),
                    stop.stop,
                    if op == "behavior_debug_next" {
                        Resume::Next
                    } else {
                        Resume::Continue
                    },
                )
                .map_err(|error| error.to_string())?;
            return Ok(json!({"accepted":true,"resumed_stop_id":stop.stop.get().to_string()}));
        }
        if self.debug.active.is_some()
            && !matches!(op, "behavior_runtime_stop" | "behavior_runtime_reset")
        {
            return Err("Runtime operation is pending; inspect its receipt first.".into());
        }
        let operation = match op {
            "behavior_runtime_reset" => Operation::Reset,
            "behavior_runtime_play" => Operation::Play,
            "behavior_runtime_pause" => Operation::Pause,
            "behavior_runtime_step" => Operation::Step,
            "behavior_runtime_stop" => Operation::Stop,
            _ => return Err("Unsupported runtime operation".into()),
        };
        let id = self
            .debug
            .next_operation
            .checked_add(1)
            .ok_or("Operation identity exhausted")?;
        let key = match operation {
            Operation::Reset => {
                let ctx = self
                    .context
                    .clone()
                    .ok_or("Native UI context unavailable")?;
                self.reset(&ctx, session, generation)?
            }
            Operation::Stop => {
                self.stop(generation, session.scene().revision);
                self.worker.as_ref().ok_or("No runtime")?.current_key()
            }
            _ => self.queue_control(operation)?,
        };
        if self.pending.is_none() && self.awaiting.is_none() {
            return Err(self.status.clone());
        }
        self.debug.next_operation = id;
        let receipt = Receipt {
            id,
            key,
            operation,
            state: "pending",
        };
        let value = receipt.value();
        self.debug.active = Some(receipt);
        Ok(value)
    }
}
fn node_wire(node: &ValueNode) -> Value {
    match node {
        ValueNode::Integer(value) => json!({"kind":"integer","value":value.to_string()}),
        _ => serde_json::to_value(node).expect("owned finite debugger values"),
    }
}
fn node_summary(stop: &StopSnapshot, index: u32, depth: usize) -> String {
    let Some(node) = stop.nodes.get(index as usize) else {
        return "<unavailable>".into();
    };
    match node {
        ValueNode::Nil => "nil".into(),
        ValueNode::Boolean(value) => value.to_string(),
        ValueNode::Integer(value) => value.to_string(),
        ValueNode::Number(value) => value.to_string(),
        ValueNode::Text(value) => format!("{value:?}"),
        ValueNode::Cycle => "<cycle>".into(),
        ValueNode::Opaque(value) => format!("<{value}>"),
        ValueNode::Truncated => "<truncated>".into(),
        ValueNode::Table(entries) if depth < 2 => format!(
            "{{{}}}",
            entries
                .iter()
                .take(4)
                .map(|entry| format!(
                    "{}: {}",
                    node_summary(stop, entry.key, depth + 1),
                    node_summary(stop, entry.value, depth + 1)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ValueNode::Table(_) => "<table>".into(),
    }
}

fn frame_wire(frame: &ruster_behavior::debug::Frame) -> Value {
    let clip = |text: &str| text.chars().take(48).collect::<String>();
    json!({"name":frame.name.as_deref().map(clip),"source":frame.source.as_deref().map(clip),"line":frame.line.map(|line|line.to_string()),"locals":frame.locals.iter().map(|local|json!({"name":clip(&local.name),"ordinal":local.ordinal.to_string(),"node":local.node.to_string()})).collect::<Vec<_>>()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn asynchronous_receipts_exact_debug_pages_and_stop_preemption() {
        use std::sync::mpsc;
        let mut session = Session::new(ruster_scene::Scene::demo()).unwrap();
        BehaviorUi::default()
            .execute(Action::Create("left".into()), &mut session)
            .unwrap();
        let id = session.authoring().sources.keys().next().unwrap().clone();
        let mut source = session.source(&id).unwrap().clone();
        source.text="return {update=function(o,p,dt)\n local amount=2\n o.translation[1]=o.translation[1]+amount*dt\n return o\nend}".into();
        session
            .set_source(session.scene().revision, &id, Some(source.clone()))
            .unwrap();
        let authored = session.snapshot_bytes().unwrap();
        let ctx = egui::Context::default();
        let (notify, pulses) = mpsc::sync_channel(1);
        let mut runtime = RuntimeUi {
            worker: Some(
                VmWorker::start(Some(Arc::new(move || {
                    let _ = notify.try_send(());
                })))
                .unwrap(),
            ),
            ..Default::default()
        };
        runtime.observe(1, session.scene().revision, None, 0);
        runtime.poll(&ctx);
        let drive = |runtime: &mut RuntimeUi, predicate: fn(&RuntimeUi) -> bool| {
            for _ in 0..40 {
                runtime.poll(&ctx);
                if predicate(runtime) {
                    return;
                }
                pulses
                    .recv_timeout(std::time::Duration::from_secs(3))
                    .unwrap_or_else(|error| panic!("UI operation deadline: {error:?}; status={}, ready={}, awaiting={:?}, active={:?}, tick={:?}, stop={:?}", format_args!("{} {:?}",runtime.status,runtime.diagnostic), runtime.ready, runtime.awaiting, runtime.debug.active.as_ref().map(|receipt|(receipt.id,receipt.operation,receipt.state)), runtime.frame.as_ref().map(|frame|frame.tick), runtime.debug_snapshot().map(|stop|stop.line)));
            }
            panic!("runtime predicate not reached");
        };
        runtime
            .breakpoint(
                &session,
                Breakpoint {
                    source_id: id,
                    source_hash: ruster_behavior::source_hash(&source.text),
                    line: 3,
                    binding_id: None,
                },
                true,
            )
            .unwrap();
        let reset = runtime
            .runtime_api("behavior_runtime_reset", &json!({}), &session, 1)
            .unwrap();
        drive(&mut runtime, |runtime| runtime.ready);
        let state = runtime
            .runtime_api(
                "behavior_runtime_inspect",
                &json!({"operation_id":reset["operation_id"]}),
                &session,
                1,
            )
            .unwrap();
        assert_eq!(state["operation"]["state"], "completed");
        let run = state["run_id"].clone();
        let request = json!({"expected_run_id":run});
        let step = runtime
            .runtime_api("behavior_runtime_step", &request, &session, 1)
            .unwrap();
        assert!(runtime
            .runtime_api("behavior_runtime_step", &request, &session, 1)
            .is_err());
        drive(&mut runtime, |runtime| runtime.debug_snapshot().is_some());
        assert_eq!(runtime.frame.as_ref().unwrap().tick, 0);
        let stop = runtime.debug_snapshot().unwrap();
        let exact = json!({"expected_run_id":run,"expected_stop_id":stop.stop.get().to_string()});
        let page = runtime
            .runtime_api("behavior_debug_inspect", &exact, &session, 1)
            .unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() < 60000);
        assert_eq!(page["committed_tick"], "0");
        runtime
            .runtime_api("behavior_debug_continue", &exact, &session, 1)
            .unwrap();
        assert!(runtime
            .runtime_api("behavior_debug_continue", &exact, &session, 1)
            .is_err());
        drive(&mut runtime, |runtime| {
            runtime.frame.as_ref().is_some_and(|frame| frame.tick == 1)
                && runtime.debug.active.is_none()
        });
        let state = runtime
            .runtime_api(
                "behavior_runtime_inspect",
                &json!({"operation_id":step["operation_id"]}),
                &session,
                1,
            )
            .unwrap();
        assert_eq!(state["operation"]["state"], "completed");
        runtime
            .runtime_api("behavior_runtime_step", &request, &session, 1)
            .unwrap();
        drive(&mut runtime, |runtime| runtime.debug_snapshot().is_some());
        let stopped = runtime
            .runtime_api("behavior_runtime_stop", &request, &session, 1)
            .unwrap();
        drive(&mut runtime, |runtime| runtime.debug.active.is_none());
        let state = runtime
            .runtime_api(
                "behavior_runtime_inspect",
                &json!({"operation_id":stopped["operation_id"]}),
                &session,
                1,
            )
            .unwrap();
        assert_eq!(state["operation"]["state"], "completed");
        assert!(!runtime.visible);
        assert_eq!(session.snapshot_bytes().unwrap(), authored);
        runtime.observe(2, session.scene().revision, None, 0);
        let fresh = runtime
            .runtime_api("behavior_runtime_reset", &json!({}), &session, 2)
            .unwrap();
        assert!(
            fresh["operation_id"]
                .as_str()
                .unwrap()
                .parse::<u64>()
                .unwrap()
                > stopped["operation_id"]
                    .as_str()
                    .unwrap()
                    .parse::<u64>()
                    .unwrap()
        );
        drive(&mut runtime, |runtime| runtime.ready);
        runtime.worker.as_mut().unwrap().request_shutdown().unwrap();
        let terminal = json!({"expected_run_id":run_id(runtime.current_run().unwrap())});
        assert!(runtime
            .runtime_api("behavior_runtime_step", &terminal, &session, 2)
            .is_err());
        assert!(
            runtime.debug.active.is_none(),
            "terminal enqueue failure must not create a pending receipt"
        );
        assert!(runtime.awaiting.is_none());
        runtime.shutdown();
    }
}
