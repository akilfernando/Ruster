//! Device output for the live application. The simulation only supplies committed cues.
use ruster_assets::AudioClip;
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
        if self.last_commit.is_some_and(|last| commit_revision <= last) {
            return;
        }
        self.last_commit = Some(commit_revision);
        if self.paused || self.guard.as_ref().is_some_and(|guard| !guard()) {
            return;
        }
        for (asset_id, gain) in cues {
            if !self.sounds.contains_key(asset_id) {
                self.status = Some(OutputStatus::MissingAsset);
                continue;
            }
            self.play(asset_id, gain);
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
    fn play(&mut self, asset_id: &str, gain: f64) {
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
        self.status = Some(device.play(asset_id, &self.sounds[asset_id], gain));
    }

    #[cfg(not(target_os = "linux"))]
    fn play(&mut self, _asset_id: &str, _gain: f64) {
        self.unavailable = true;
    }
}
