//! Opt-in inspection-build capture spans; callback latency is an opaque backend interval.
//! Numeric-only fixed-capacity stderr records, at most 64 reservations per process.
#[derive(Clone, Default)]
pub(super) struct Probe {
    #[cfg(any(feature = "inspection", test))]
    inner: Option<std::sync::Arc<std::sync::Mutex<Run>>>,
}
#[cfg(any(feature = "inspection", test))]
struct Run {
    ids: [u64; 4],
    start: std::time::Instant,
    times: [u64; 7],
    next: usize,
    invalid: bool,
    finished: bool,
}
impl Probe {
    pub fn begin(ids: [u64; 4]) -> Self {
        #[cfg(feature = "inspection")]
        {
            use std::sync::{
                atomic::{AtomicUsize, Ordering},
                OnceLock,
            };
            static ENABLED: OnceLock<bool> = OnceLock::new();
            static COUNT: AtomicUsize = AtomicUsize::new(0);
            let enabled = *ENABLED
                .get_or_init(|| std::env::var_os("ROT_CAPTURE_TIMING").is_some_and(|v| v == "1"));
            let admitted = enabled
                && COUNT
                    .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                        (n < 64).then_some(n + 1)
                    })
                    .is_ok();
            Self::begin_with(admitted, ids, std::time::Instant::now)
        }
        #[cfg(not(feature = "inspection"))]
        {
            let _ = ids;
            Self::default()
        }
    }
    #[cfg(any(feature = "inspection", test))]
    pub(super) fn begin_with(
        enabled: bool,
        ids: [u64; 4],
        clock: impl FnOnce() -> std::time::Instant,
    ) -> Self {
        Self {
            inner: enabled.then(|| {
                std::sync::Arc::new(std::sync::Mutex::new(Run {
                    ids,
                    start: clock(),
                    times: [
                        0,
                        u64::MAX,
                        u64::MAX,
                        u64::MAX,
                        u64::MAX,
                        u64::MAX,
                        u64::MAX,
                    ],
                    next: 1,
                    invalid: false,
                    finished: false,
                }))
            }),
        }
    }
    pub fn mark(&self, phase: usize) {
        #[cfg(any(feature = "inspection", test))]
        if let Some(inner) = &self.inner {
            let mut run = inner.lock().unwrap_or_else(|e| e.into_inner());
            if run.finished {
                return;
            }
            if phase < run.next || phase >= 7 {
                run.invalid = true;
                return;
            }
            // Failed captures can skip encoder phases: retain later evidence,
            // but never certify such a record as complete.
            run.invalid |= phase != run.next;
            run.times[phase] = run.start.elapsed().as_nanos().min((u64::MAX - 1) as u128) as u64;
            run.next = phase + 1;
        }
        let _ = phase;
    }
    /// 1=ready reply construction, 2=failure, 3=cancellation/retirement.
    pub fn finish(&self, outcome: u8) {
        #[cfg(any(feature = "inspection", test))]
        if let Some(inner) = &self.inner {
            let mut run = inner.lock().unwrap_or_else(|e| e.into_inner());
            if run.finished {
                return;
            }
            run.finished = true;
            eprintln!("ROT_CAPTURE_TIMING {{\"ids\":{:?},\"ns\":{:?},\"outcome\":{},\"invalid\":{},\"complete\":{}}}", run.ids, run.times, outcome, u8::from(run.invalid), u8::from(run.next == 7 && !run.invalid && outcome == 1));
        }
        let _ = outcome;
    }
    #[cfg(test)]
    pub(super) fn snapshot(&self) -> Option<([u64; 7], bool)> {
        self.inner.as_ref().map(|inner| {
            let run = inner.lock().unwrap();
            (run.times, run.invalid)
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_never_reads_diagnostic_clock() {
        let probe = Probe::begin_with(false, [0; 4], || panic!("disabled clock"));
        probe.mark(1);
        probe.finish(3);
        assert!(probe.snapshot().is_none());
    }
    #[test]
    fn cancellation_and_failure_freeze_partial_evidence() {
        for outcome in [2, 3] {
            let probe = Probe::begin_with(true, [1, 2, 3, 4], std::time::Instant::now);
            probe.mark(1);
            let callback = probe.clone();
            probe.finish(outcome);
            let retained = probe.snapshot().unwrap();
            callback.mark(2);
            callback.finish(1);
            assert_eq!(probe.snapshot().unwrap(), retained);
            assert_eq!(retained.0[2], u64::MAX);
        }
        let reordered = Probe::begin_with(true, [1, 2, 3, 4], std::time::Instant::now);
        reordered.mark(2);
        reordered.mark(1);
        assert!(reordered.snapshot().unwrap().1);
        assert_eq!(reordered.snapshot().unwrap().0[1], u64::MAX);
    }
    #[test]
    fn finite_capture_milestones_are_complete_and_ordered() {
        let probe = Probe::begin_with(true, [1, 2, 3, 4], std::time::Instant::now);
        for phase in 1..7 {
            probe.mark(phase);
        }
        let (times, invalid) = probe
            .snapshot()
            .expect("enabled reservation must retain milestones");
        assert!(!invalid);
        assert_eq!(times[0], 0);
        assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
        probe.mark(7);
        let (retained, invalid) = probe.snapshot().unwrap();
        assert_eq!(retained, times, "overflow must never overwrite samples");
        assert!(invalid, "overflow is a diagnostic failure");
    }
}
