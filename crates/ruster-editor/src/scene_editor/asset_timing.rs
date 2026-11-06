//! Opt-in inspection-build observations, never input-to-photon measurements.
//! Hook Host entry before dispatch; finish only after reply and retirement.
use eframe::egui;
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Instant;

// eframe calls App::logic inside run_ui and inside run_logic. Only real
// egui pass hooks can distinguish them; cumulative frame/input state is stale
// during hidden logic. This marker is inspection-only and owns no resources.
pub(super) fn install_pass_marker(ctx: &egui::Context) {
    let installed = egui::Id::unique("ruster_asset_timing_hooks");
    if ctx.data_mut(|data| data.get_temp::<bool>(installed).unwrap_or(false)) {
        return;
    }
    ctx.data_mut(|data| data.insert_temp(installed, true));
    ctx.on_begin_pass(
        "ruster asset timing pass",
        std::sync::Arc::new(|ui| {
            if ui.ctx().viewport_id() == egui::ViewportId::ROOT {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique("ruster_asset_timing_pass"), true)
                });
            }
        }),
    );
    ctx.on_end_pass(
        "ruster asset timing pass",
        std::sync::Arc::new(|ui| {
            if ui.ctx().viewport_id() == egui::ViewportId::ROOT {
                ui.ctx().data_mut(|data| {
                    data.insert_temp(egui::Id::unique("ruster_asset_timing_pass"), false)
                });
            }
        }),
    );
}
pub(super) fn in_real_pass(ctx: &egui::Context) -> bool {
    ctx.viewport_id() == egui::ViewportId::ROOT
        && ctx.data_mut(|data| {
            data.get_temp::<bool>(egui::Id::unique("ruster_asset_timing_pass"))
                .unwrap_or(false)
        })
}

const LIMIT: usize = 512;
fn epoch() -> &'static Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now)
}

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("RUSTER_ASSET_TIMING").is_some_and(|v| v == "1"))
}

// Select a cohort before launch, never by dropping inconvenient observations.
// The native import/Refresh workload excludes automatic startup opens this way.
fn operation_filter() -> &'static Option<Vec<String>> {
    static FILTER: OnceLock<Option<Vec<String>>> = OnceLock::new();
    FILTER.get_or_init(|| {
        std::env::var("RUSTER_ASSET_TIMING_OPS").ok().map(|text| {
            if text.len() > 256 {
                return Vec::new(); // Invalid configuration produces no qualified samples.
            }
            text.split(',').map(|name| name.trim().to_owned()).collect()
        })
    })
}

#[derive(Clone, Copy)]
struct Point {
    frame: u64,
    ms: f64,
    focused: bool,
}

// Copy counters only: never clone resource payloads or the bounded timing
// vectors, and release the metrics mutex before formatting observation output.
#[derive(Clone, serde::Serialize)]
struct Resources {
    frames: u64,
    lighting_bytes: u64,
    lighting_uploads: u64,
    lighting_upload_bytes: u64,
    mesh_bytes: u64,
    mesh_uploads: u64,
    mesh_upload_bytes: u64,
    texture_bytes: u64,
    texture_uploads: u64,
    texture_upload_bytes: u64,
    instance_capacity_bytes: u64,
    target_bytes: u64,
    target_retiring_bytes: u64,
    target_allocations: u64,
    display_requested_hdr: bool,
    display_applied_hdr: bool,
    display_requested_exposure_stops: f32,
    display_applied_exposure_stops: f32,
    target_size: [u32; 2],
    sample_count: u32,
    objects: usize,
    draw_calls: usize,
}
impl Resources {
    fn read(handle: &ruster_render::MetricsHandle) -> Self {
        let m = handle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        Self {
            frames: m.frames,
            lighting_bytes: m.lighting_bytes,
            lighting_uploads: m.lighting_uploads,
            lighting_upload_bytes: m.lighting_upload_bytes,
            mesh_bytes: m.mesh_bytes,
            mesh_uploads: m.mesh_uploads,
            mesh_upload_bytes: m.mesh_upload_bytes,
            texture_bytes: m.texture_bytes,
            texture_uploads: m.texture_uploads,
            texture_upload_bytes: m.texture_upload_bytes,
            instance_capacity_bytes: m.instance_capacity_bytes,
            target_bytes: m.target_bytes,
            target_retiring_bytes: m.target_retiring_bytes,
            target_allocations: m.target_allocations,
            display_requested_hdr: m.display_requested_hdr,
            display_applied_hdr: m.display_applied_hdr,
            display_requested_exposure_stops: m.display_requested_exposure_stops,
            display_applied_exposure_stops: m.display_applied_exposure_stops,
            target_size: m.target_size,
            sample_count: m.sample_count,
            objects: m.objects,
            draw_calls: m.draw_calls,
        }
    }
}

struct Run {
    operation: String,
    origin: Option<&'static str>,
    immediate: bool,
    committed_frame: Option<u64>,
    resources_before: Option<Resources>,
    render_frames_at_finish: Option<u64>,
    start: Option<Point>,
    terminal: Option<(u64, &'static str)>,
    spans: Vec<Value>,
    gaps: Vec<Value>,
    gap_count: u64,
    maximum_gap_ms: f64,
    focus_lost: bool,
    overflow: bool,
    overlap: bool,
    nonvisual: bool,
}

#[derive(Default)]
pub(super) struct Probe {
    metrics: Option<ruster_render::MetricsHandle>,
    latest: Option<Point>,
    run: Option<Run>,
    #[cfg(test)]
    last_summary: Option<Value>,
}

impl Probe {
    pub fn attach_metrics(&mut self, metrics: Option<ruster_render::MetricsHandle>) {
        self.metrics = metrics;
    }
    pub fn begin_immediate(&mut self, operation: &str, origin: &'static str) -> Option<Instant> {
        let started = self.begin(operation)?;
        let run = self.run.as_mut().unwrap();
        run.origin = Some(origin);
        run.immediate = true;
        Some(started)
    }
    pub fn immediate_record(&mut self, started: Option<Instant>) {
        if started.is_none() {
            return;
        }
        self.record("foreground_commit_elapsed", started);
        self.run.as_mut().unwrap().committed_frame = self.latest.map(|point| point.frame);
    }
    pub fn hidden_logic(&mut self) {
        // No stale idle-frame anchor and no invented trailing frame. Raw
        // operation spans remain useful, but this cannot qualify a paint cohort.
        self.latest = None;
        if let Some(run) = &mut self.run {
            run.nonvisual = true;
        }
    }
    /// The caller must invoke this at Host entry, not after editor work.
    pub fn frame(&mut self, ctx: &egui::Context) {
        if !enabled()
            || ctx.viewport_id() != egui::ViewportId::ROOT
            || ctx.current_pass_index() != 0
        {
            return;
        }
        let point = Point {
            frame: ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT),
            ms: epoch().elapsed().as_secs_f64() * 1000.,
            focused: ctx.input(|input| input.focused),
        };
        if let Some(summary) = self.frame_record(point) {
            #[cfg(test)]
            {
                self.last_summary = Some(summary.clone());
            }
            emit(summary);
        }
    }

    fn frame_record(&mut self, point: Point) -> Option<Value> {
        if self.latest.is_some_and(|last| last.frame == point.frame) {
            return None;
        }
        let previous = self.latest.replace(point);
        let run = self.run.as_mut()?;
        run.focus_lost |= !point.focused;
        if let Some(previous) =
            previous.filter(|last| run.start.is_some_and(|start| last.frame >= start.frame))
        {
            let gap = point.ms - previous.ms;
            run.gap_count += 1;
            run.maximum_gap_ms = run.maximum_gap_ms.max(gap);
            run.focus_lost |= !previous.focused || !point.focused;
            if run.gaps.len() < LIMIT {
                run.gaps
                    .push(json!({"from_frame":previous.frame,"to_frame":point.frame,
                    "start_ms":previous.ms,"end_ms":point.ms,"elapsed_ms":gap,
                    "focused_from":previous.focused,"focused_to":point.focused}));
            } else {
                run.overflow = true;
            }
        }
        if run.terminal.is_some_and(|(frame, _)| point.frame > frame) {
            Some(self.summary(true, "trailing_frame"))
        } else {
            None
        }
    }

    /// Call around the outer admission handler, before validation or cloning.
    pub fn begin(&mut self, operation: &str) -> Option<Instant> {
        if !enabled()
            || operation_filter()
                .as_ref()
                .is_some_and(|names| !names.iter().any(|name| name == operation))
        {
            return None;
        }
        if !self.begin_record(operation) {
            return None;
        }
        let _ = epoch();
        Some(Instant::now())
    }
    pub fn active(&self) -> bool {
        self.run.is_some()
    }
    #[cfg(test)]
    pub fn last_summary(&self) -> Option<&Value> {
        self.last_summary.as_ref()
    }

    fn begin_record(&mut self, operation: &str) -> bool {
        if let Some(run) = &mut self.run {
            // Keep the original operation's attribution. A rejected overlapping
            // admission must not become the owner of its eventual completion.
            run.overlap = true;
            return false;
        }
        self.run = Some(Run {
            operation: operation.chars().take(64).collect(),
            origin: None,
            immediate: false,
            committed_frame: None,
            resources_before: self.metrics.as_ref().map(Resources::read),
            render_frames_at_finish: None,
            start: self.latest,
            terminal: None,
            spans: Vec::new(),
            gaps: Vec::new(),
            gap_count: 0,
            maximum_gap_ms: 0.,
            focus_lost: self.latest.is_some_and(|point| !point.focused),
            overflow: false,
            overlap: false,
            nonvisual: self.latest.is_none(),
        });
        true
    }

    pub fn span_start(&self) -> Option<Instant> {
        if enabled() && self.run.is_some() {
            let _ = epoch();
            Some(Instant::now())
        } else {
            None
        }
    }

    /// Preserve raw elapsed spans; aggregation/percentile policy belongs to the
    /// named workload client, separating success, cancellation and failure.
    pub fn record(&mut self, name: &'static str, started: Option<Instant>) {
        let Some((run, started)) = self.run.as_mut().zip(started) else {
            return;
        };
        let ended = Instant::now();
        let start_ms = started.saturating_duration_since(*epoch()).as_secs_f64() * 1000.;
        let end_ms = ended.saturating_duration_since(*epoch()).as_secs_f64() * 1000.;
        let ms = ended.duration_since(started).as_secs_f64() * 1000.;
        if run.spans.len() < LIMIT {
            run.spans
                .push(json!({"name":name,"start_ms":start_ms,"end_ms":end_ms,"elapsed_ms":ms}));
        } else {
            run.overflow = true;
        }
    }

    /// The job, terminal response and retirement acknowledgement must all be
    /// complete. Request one trailing frame to include final paint/upload cost.
    pub fn finish(&mut self, ctx: &egui::Context, outcome: &'static str) {
        if let Some(run) = self.run.as_mut().filter(|run| run.terminal.is_none()) {
            let frame = ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT);
            // MCP can commit after this frame's viewport callback was built.
            // First let a later genuine UI frame build the changed scene, then
            // observe after its paint at the following real Host entry.
            if (run.immediate || self.metrics.is_some())
                && frame <= *run.committed_frame.get_or_insert(frame)
            {
                ctx.request_repaint();
                return;
            }
            run.render_frames_at_finish = self
                .metrics
                .as_ref()
                .map(|metrics| Resources::read(metrics).frames);
            run.terminal = Some((frame, outcome));
            ctx.request_repaint();
        }
    }

    fn summary(&mut self, trailing: bool, reason: &str) -> Value {
        let run = self
            .run
            .take()
            .expect("summary requires an active observation");
        let after = self.metrics.as_ref().map(Resources::read);
        let resources = match (&run.resources_before, &after) {
            (Some(before), Some(after)) => {
                let rendered = trailing
                    && run
                        .render_frames_at_finish
                        .is_some_and(|frames| after.frames > frames);
                let monotonic = after.frames >= before.frames
                    && after.lighting_uploads >= before.lighting_uploads
                    && after.lighting_upload_bytes >= before.lighting_upload_bytes
                    && after.mesh_uploads >= before.mesh_uploads
                    && after.mesh_upload_bytes >= before.mesh_upload_bytes
                    && after.texture_uploads >= before.texture_uploads
                    && after.texture_upload_bytes >= before.texture_upload_bytes
                    && after.target_allocations >= before.target_allocations;
                json!({"before":before,"after":after,"rendered_after_finish":rendered,
                    "complete":rendered && monotonic && !run.nonvisual && !run.overlap && !run.overflow,
                    "counter_monotonic":monotonic,
                    "delta":{"frames":after.frames.checked_sub(before.frames),
                        "target_allocations":after.target_allocations.checked_sub(before.target_allocations),
                        "lighting_uploads":after.lighting_uploads.checked_sub(before.lighting_uploads),
                        "lighting_upload_bytes":after.lighting_upload_bytes.checked_sub(before.lighting_upload_bytes),
                        "mesh_uploads":after.mesh_uploads.checked_sub(before.mesh_uploads),
                        "mesh_upload_bytes":after.mesh_upload_bytes.checked_sub(before.mesh_upload_bytes),
                        "texture_uploads":after.texture_uploads.checked_sub(before.texture_uploads),
                        "texture_upload_bytes":after.texture_upload_bytes.checked_sub(before.texture_upload_bytes)},
                    "scope":"logical renderer payload/cumulative uploads; not CPU ownership, driver allocation or peak memory"})
            }
            _ => json!({"complete":false,"reason":"renderer_metrics_unavailable"}),
        };
        json!({"operation":run.operation,"origin":run.origin,"outcome":run.terminal.map(|(_,outcome)|outcome),
            "resources":resources,
            "operation_filter":operation_filter(),
            "reason":reason,"trailing_frame":trailing,"overflow":run.overflow,"overlap":run.overlap,
            "complete":trailing && run.start.is_some() && run.gap_count>0 && !run.overflow && !run.overlap && !run.nonvisual,
            "nonvisual":run.nonvisual,
            "focus_lost":run.focus_lost,"gap_count":run.gap_count,
            "maximum_gap_ms":run.maximum_gap_ms,"gaps":run.gaps,"spans":run.spans,
            "scope":"inspection feature; foreground elapsed and Host frame-start gaps; not presentation latency"})
    }
}

fn emit(summary: Value) {
    eprintln!("RUSTER_ASSET_TIMING {summary}");
}

impl Drop for Probe {
    fn drop(&mut self) {
        if self.run.is_some() {
            emit(self.summary(false, "probe_dropped_before_trailing_frame"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribution_excludes_idle_but_keeps_trailing_gap_and_marks_incomplete_windows() {
        let mut probe = Probe::default();
        let point = |frame, ms, focused| Point { frame, ms, focused };
        assert!(probe.frame_record(point(1, 0., true)).is_none());
        assert!(probe.frame_record(point(2, 5000., true)).is_none());
        probe.begin_record("asset_import");
        assert!(probe.frame_record(point(3, 5016., true)).is_none());
        probe.run.as_mut().unwrap().terminal = Some((3, "success"));
        let result = probe.frame_record(point(4, 5077., true)).unwrap();
        assert_eq!(result["maximum_gap_ms"], 61.);
        assert_eq!(result["gap_count"], 2);
        assert_eq!(result["complete"], true);
        assert_eq!(result["focus_lost"], false);
        probe.begin_record("asset_refresh");
        probe.hidden_logic();
        assert!(probe.frame_record(point(5, 5093., false)).is_none());
        let result = probe.summary(false, "test_shutdown");
        assert_eq!(result["complete"], false);
        assert_eq!(result["focus_lost"], true);
        assert_eq!(result["nonvisual"], true);
        probe.hidden_logic();
        probe.begin_record("asset_refresh");
        assert!(probe.run.as_ref().unwrap().start.is_none());
        let hidden = probe.summary(false, "hidden_start");
        assert_eq!(hidden["nonvisual"], true);
        assert_eq!(hidden["complete"], false);
        probe.frame_record(point(5, 5093., true));
        probe.begin_record("asset_refresh");
        for frame in 6..=(LIMIT as u64 + 6) {
            probe.frame_record(point(frame, 5100. + frame as f64, true));
        }
        probe.run.as_mut().unwrap().terminal = Some((LIMIT as u64 + 6, "success"));
        let result = probe
            .frame_record(point(LIMIT as u64 + 7, 5800., true))
            .unwrap();
        assert_eq!(result["overflow"], true);
        assert_eq!(result["complete"], false);
        assert_eq!(result["gaps"].as_array().unwrap().len(), LIMIT);
        assert!(probe.begin_record("asset_import"));
        assert!(!probe.begin_record("asset_refresh"));
        let result = probe.summary(false, "test_overlap");
        assert_eq!(result["operation"], "asset_import");
        assert_eq!(result["overlap"], true);
        assert_eq!(result["complete"], false);

        // Scalar fixture verifies sampling order/qualification, not real GPU
        // execution. The native gate owns actual upload and resource evidence.
        let metrics = std::sync::Arc::new(std::sync::Mutex::new(ruster_render::Metrics {
            frames: 10,
            lighting_bytes: 192,
            lighting_uploads: 1,
            lighting_upload_bytes: 192,
            ..Default::default()
        }));
        let mut observed = Probe::default();
        observed.attach_metrics(Some(metrics.clone()));
        let ctx = egui::Context::default();
        ctx.run_ui(Default::default(), |_| {
            observed.frame_record(point(0, 0., true));
            observed.begin_record("scene_ambient_set");
            let run = observed.run.as_mut().unwrap();
            run.origin = Some("mcp");
            run.immediate = true;
            run.committed_frame = Some(0);
            observed.finish(&ctx, "changed");
            assert!(observed.run.as_ref().unwrap().terminal.is_none());
        })
        .drop_without_applying_deltas();
        // This may be the OLD callback, built before a UI-end command.
        metrics.lock().unwrap().frames = 11;
        ctx.run_ui(Default::default(), |_| {
            assert!(observed.frame_record(point(1, 16., true)).is_none());
            observed.finish(&ctx, "changed");
            assert_eq!(
                observed.run.as_ref().unwrap().terminal,
                Some((1, "changed"))
            );
        })
        .drop_without_applying_deltas();
        {
            let mut m = metrics.lock().unwrap();
            m.frames = 12;
            m.lighting_uploads = 2;
            m.lighting_upload_bytes = 384;
        }
        let result = observed.frame_record(point(2, 32., true)).unwrap();
        assert_eq!(result["origin"], "mcp");
        assert_eq!(result["resources"]["complete"], true);
        assert_eq!(result["resources"]["before"]["lighting_upload_bytes"], 192);
        assert_eq!(result["resources"]["delta"]["lighting_upload_bytes"], 192);
        assert_eq!(result["resources"]["delta"]["mesh_uploads"], 0);
        assert_eq!(result["resources"]["delta"]["texture_uploads"], 0);
        observed.begin_record("scene_ambient_set");
        let run = observed.run.as_mut().unwrap();
        run.terminal = Some((2, "no_op"));
        run.render_frames_at_finish = Some(12);
        let missing_render = observed.frame_record(point(3, 48., true)).unwrap();
        assert_eq!(missing_render["resources"]["complete"], false);
        assert_eq!(missing_render["resources"]["rendered_after_finish"], false);
        observed.begin_record("scene_ambient_set");
        observed.hidden_logic();
        let run = observed.run.as_mut().unwrap();
        run.terminal = Some((3, "changed"));
        run.render_frames_at_finish = Some(12);
        metrics.lock().unwrap().frames = 13;
        let hidden = observed.frame_record(point(4, 64., true)).unwrap();
        assert_eq!(hidden["complete"], false);
        assert_eq!(hidden["resources"]["complete"], false);
    }
}
