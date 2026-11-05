//! Opt-in inspection-build observations, never input-to-photon measurements.
//! Hook Host entry before dispatch; finish only after reply and retirement.
use eframe::egui;
use serde_json::{json, Value};
use std::sync::OnceLock;
use std::time::Instant;

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

struct Run {
    operation: String,
    start: Option<Point>,
    terminal: Option<(u64, &'static str)>,
    spans: Vec<Value>,
    gaps: Vec<Value>,
    gap_count: u64,
    maximum_gap_ms: f64,
    focus_lost: bool,
    overflow: bool,
    overlap: bool,
}

#[derive(Default)]
pub(super) struct Probe {
    latest: Option<Point>,
    run: Option<Run>,
    #[cfg(test)]
    last_summary: Option<Value>,
}

impl Probe {
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
            start: self.latest,
            terminal: None,
            spans: Vec::new(),
            gaps: Vec::new(),
            gap_count: 0,
            maximum_gap_ms: 0.,
            focus_lost: self.latest.is_some_and(|point| !point.focused),
            overflow: false,
            overlap: false,
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
            run.terminal = Some((ctx.cumulative_frame_nr_for(egui::ViewportId::ROOT), outcome));
            ctx.request_repaint();
        }
    }

    fn summary(&mut self, trailing: bool, reason: &str) -> Value {
        let run = self
            .run
            .take()
            .expect("summary requires an active observation");
        json!({"operation":run.operation,"outcome":run.terminal.map(|(_,outcome)|outcome),
            "operation_filter":operation_filter(),
            "reason":reason,"trailing_frame":trailing,"overflow":run.overflow,"overlap":run.overlap,
            "complete":trailing && run.start.is_some() && run.gap_count>0 && !run.overflow && !run.overlap,
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
        assert!(probe.frame_record(point(5, 5093., false)).is_none());
        let result = probe.summary(false, "test_shutdown");
        assert_eq!(result["complete"], false);
        assert_eq!(result["focus_lost"], true);
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
    }
}
