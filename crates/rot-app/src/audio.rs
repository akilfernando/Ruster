//! Device output for the live application. The simulation only supplies committed cues.
use rot_assets::AudioClip;
use std::{collections::BTreeMap, sync::Arc};

#[cfg(target_os = "linux")]
mod linux;

pub const MAX_ACTIVE_VOICES: usize = 16;
pub type Sounds = BTreeMap<String, Arc<AudioClip>>;
pub type PlaybackGuard = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputStatus {
    Idle,
    Ready,
    Paused,
    Unavailable,
    MissingAsset,
    VoiceLimit,
}

#[derive(Default)]
pub struct Output {
    sounds: Sounds,
    guard: Option<PlaybackGuard>,
    last_commit: Option<u64>,
    paused: bool,
    status: Option<OutputStatus>,
    unavailable: bool,
    #[cfg(target_os = "linux")]
    device: Option<linux::Device>,
}

impl Output {
    /// Own a fresh playback lifetime for a committed Reset and asset generation.
    pub fn reset(&mut self, sounds: Sounds) {
        self.reset_scoped(sounds, None);
    }

    /// The guard must be nonblocking and is checked for every device buffer.
    pub fn reset_scoped(&mut self, sounds: Sounds, guard: Option<PlaybackGuard>) {
        *self = Self {
            sounds,
            guard,
            ..Self::default()
        };
    }

    pub fn stop(&mut self) {
        *self = Self::default();
    }

    pub fn pause(&mut self, paused: bool) {
        if self.paused == paused {
            return;
        }
        self.paused = paused;
        #[cfg(target_os = "linux")]
        if let Some(device) = self.device.as_mut() {
            device.pause(paused);
            self.status = Some(if paused {
                OutputStatus::Paused
            } else {
                OutputStatus::Ready
            });
        }
    }

    /// Consume each successful receipt once in this runtime generation.
    /// Paused steps consume the receipt silently. Voices never carry over Reset.
    pub fn consume<'a>(
        &mut self,
        commit_revision: u64,
        cues: impl IntoIterator<Item = (&'a str, f64)>,
    ) {
        self.consume_spatial(
            commit_revision,
            cues.into_iter().map(|(id, gain)| (id, gain, None)),
        );
    }

    pub fn consume_spatial<'a>(
        &mut self,
        commit_revision: u64,
        cues: impl IntoIterator<Item = (&'a str, f64, Option<f64>)>,
    ) {
        if self.last_commit.is_some_and(|last| commit_revision <= last) {
            return;
        }
        self.last_commit = Some(commit_revision);
        if self.paused || self.guard.as_ref().is_some_and(|guard| !guard()) {
            return;
        }
        for (asset_id, gain, pan) in cues {
            if pan.is_some() && gain <= 0.0 {
                continue;
            }
            if !self.sounds.contains_key(asset_id) {
                self.status = Some(OutputStatus::MissingAsset);
                continue;
            }
            self.play(asset_id, gain, pan);
        }
    }

    pub fn status(&mut self) -> OutputStatus {
        #[cfg(target_os = "linux")]
        if self.device.as_mut().is_some_and(linux::Device::failed) {
            self.device = None;
            self.unavailable = true;
        }
        if self.unavailable {
            return OutputStatus::Unavailable;
        }
        self.status.unwrap_or(OutputStatus::Idle)
    }

    #[cfg(target_os = "linux")]
    fn play(&mut self, asset_id: &str, gain: f64, pan: Option<f64>) {
        if self.status() == OutputStatus::Unavailable {
            return;
        }
        if self.device.is_none() {
            let Ok(device) = linux::Device::open(self.guard.clone()) else {
                self.unavailable = true;
                return;
            };
            self.device = Some(device);
        }
        let device = self.device.as_mut().expect("opened output device");
        self.status = Some(device.play(asset_id, &self.sounds[asset_id], gain, pan));
    }

    #[cfg(not(target_os = "linux"))]
    fn play(&mut self, _asset_id: &str, _gain: f64, _pan: Option<f64>) {
        self.unavailable = true;
    }
}

/// One editor preview. It opens output only on explicit request and releases
/// the stream as soon as the voice ends or its owner cancels it.
#[derive(Default)]
pub struct Audition {
    #[cfg(target_os = "linux")]
    task: Option<audition::Task>,
}

impl Audition {
    pub fn active(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self.task.as_ref().is_some_and(|task| task.active());
        #[cfg(not(target_os = "linux"))]
        false
    }

    pub fn play(
        &mut self,
        clip: Arc<AudioClip>,
        wake: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        self.play_at_gain(clip, 1.0, wake)
    }

    pub fn play_at_gain(
        &mut self,
        clip: Arc<AudioClip>,
        gain: f64,
        wake: impl FnOnce() + Send + 'static,
    ) -> Result<(), String> {
        if !(0.0..=1.0).contains(&gain) {
            return Err("Preview gain must be a finite value from 0 to 1.".into());
        }
        #[cfg(target_os = "linux")]
        {
            if self.task.is_some() {
                return Err("The previous preview is still closing. Try again.".into());
            }
            self.task = Some(audition::Task::start(clip, gain, wake)?);
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (clip, wake);
            Err("WAV preview is available on Linux.".into())
        }
    }

    pub fn stop(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(task) = &self.task {
            task.stop();
        }
    }

    /// Poll only this preview's completion. No idle timer or device probes.
    pub fn poll(&mut self) -> Option<Result<(), String>> {
        #[cfg(target_os = "linux")]
        {
            let result = self.task.as_ref()?.completed()?;
            self.task = None;
            Some(result)
        }
        #[cfg(not(target_os = "linux"))]
        None
    }
}

#[cfg(target_os = "linux")]
mod audition {
    use super::{linux, AudioClip, OutputStatus};
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver, TryRecvError},
            Arc,
        },
        thread::{self, Thread},
        time::Duration,
    };

    pub(super) struct Task {
        live: Arc<AtomicBool>,
        worker: Thread,
        result: Receiver<Result<(), String>>,
    }

    impl Task {
        pub(super) fn start(
            clip: Arc<AudioClip>,
            gain: f64,
            wake: impl FnOnce() + Send + 'static,
        ) -> Result<Self, String> {
            let live = Arc::new(AtomicBool::new(true));
            let worker_live = live.clone();
            let (done, result) = mpsc::sync_channel(1);
            let worker = thread::Builder::new()
                .name("rot-wav-preview".into())
                .spawn(move || {
                    let mut result = audition(&worker_live, &clip, gain);
                    if !worker_live.load(Ordering::Acquire) {
                        result = Ok(());
                    }
                    let _ = done.send(result);
                    wake();
                })
                .map_err(|error| error.to_string())?;
            Ok(Self {
                live,
                worker: worker.thread().clone(),
                result,
            })
        }

        pub(super) fn active(&self) -> bool {
            self.live.load(Ordering::Acquire)
        }

        pub(super) fn stop(&self) {
            self.live.store(false, Ordering::Release);
            self.worker.unpark();
        }

        pub(super) fn completed(&self) -> Option<Result<(), String>> {
            match self.result.try_recv() {
                Ok(result) if self.active() => Some(result),
                Ok(_) => Some(Ok(())),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    Some(Err("The audio preview could not finish.".into()))
                }
            }
        }
    }

    impl Drop for Task {
        fn drop(&mut self) {
            self.stop();
        }
    }

    fn audition(live: &Arc<AtomicBool>, clip: &AudioClip, gain: f64) -> Result<(), String> {
        if !live.load(Ordering::Acquire) {
            return Ok(());
        }
        let guard = live.clone();
        let mut device =
            linux::Device::open(Some(Arc::new(move || guard.load(Ordering::Acquire))))?;
        if !live.load(Ordering::Acquire) {
            return Ok(());
        }
        if device.play("preview", clip, gain, None) != OutputStatus::Ready {
            return Err("The audio output could not start the preview.".into());
        }
        while live.load(Ordering::Acquire) {
            if device.failed() {
                return Err("The audio output disconnected during preview.".into());
            }
            if !device.has_active_voice() {
                return Ok(());
            }
            thread::park_timeout(Duration::from_millis(10));
        }
        Ok(())
    }
}
