use super::{OutputStatus, PlaybackGuard, MAX_ACTIVE_VOICES};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use kira::{
    backend::{Backend, Renderer},
    sound::{
        static_sound::{StaticSoundData, StaticSoundHandle},
        PlaybackState,
    },
    track::MainTrackBuilder,
    AudioManager, AudioManagerSettings, Decibels, Frame, Tween,
};
use ruster_assets::AudioClip;
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

struct Sink {
    device: cpal::Device,
    config: cpal::StreamConfig,
    failed: Arc<AtomicBool>,
    guard: Option<PlaybackGuard>,
    stream: Option<cpal::Stream>,
}

impl Backend for Sink {
    type Settings = (Option<String>, Option<PlaybackGuard>);
    type Error = String;

    fn setup((name, guard): Self::Settings, _: usize) -> Result<(Self, u32), String> {
        let host = match name.as_ref() {
            Some(_) => cpal::host_from_id(cpal::HostId::PulseAudio).map_err(|e| e.to_string())?,
            None => cpal::default_host(),
        };
        let device = match name {
            Some(name) => host
                .output_devices()
                .map_err(|e| e.to_string())?
                .find(|device| {
                    device
                        .description()
                        .is_ok_and(|description| description.name() == name)
                }),
            None => host.default_output_device(),
        }
        .ok_or("no requested audio output device")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let mut config = supported.config();
        let rate = config.sample_rate;
        if let cpal::SupportedBufferSize::Range { min, max } = *supported.buffer_size() {
            config.buffer_size = cpal::BufferSize::Fixed((rate / 100).clamp(min, max));
        }
        if !(1..=2).contains(&config.channels) || rate == 0 {
            return Err("audio output requires mono or stereo".into());
        }
        Ok((
            Self {
                device,
                config,
                failed: Arc::default(),
                guard,
                stream: None,
            },
            rate,
        ))
    }

    fn start(&mut self, mut renderer: Renderer) -> Result<(), String> {
        let channels = self.config.channels;
        let failed = self.failed.clone();
        let guard = self.guard.take();
        let stream = self
            .device
            .build_output_stream(
                self.config,
                move |samples: &mut [f32], _| {
                    if guard.as_ref().is_some_and(|guard| !guard()) {
                        samples.fill(0.0);
                    } else {
                        renderer.on_start_processing();
                        renderer.process(samples, channels);
                    }
                },
                move |_| failed.store(true, Ordering::Release),
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        Ok(())
    }
}

pub(super) struct Device {
    manager: AudioManager<Sink>,
    clips: BTreeMap<String, StaticSoundData>,
    active: Vec<StaticSoundHandle>,
}

impl Device {
    pub(super) fn open(guard: Option<PlaybackGuard>) -> Result<Self, String> {
        let device = std::env::var_os("RUSTER_AUDIO_DEVICE")
            .map(|name| name.into_string().map_err(|_| "invalid audio device name"))
            .transpose()?;
        let settings = AudioManagerSettings {
            backend_settings: (device, guard),
            main_track_builder: MainTrackBuilder::new().sound_capacity(MAX_ACTIVE_VOICES),
            ..AudioManagerSettings::default()
        };
        let manager = AudioManager::new(settings)?;
        Ok(Self {
            manager,
            clips: BTreeMap::new(),
            active: Vec::with_capacity(MAX_ACTIVE_VOICES),
        })
    }

    pub(super) fn failed(&mut self) -> bool {
        self.manager.backend_mut().failed.load(Ordering::Acquire)
    }

    pub(super) fn pause(&mut self, paused: bool) {
        for voice in &mut self.active {
            if paused {
                voice.pause(Tween::default());
            } else {
                voice.resume(Tween::default());
            }
        }
    }

    pub(super) fn play(&mut self, asset_id: &str, clip: &AudioClip, gain: f64) -> OutputStatus {
        self.active
            .retain(|voice| voice.state() != PlaybackState::Stopped);
        if self.active.len() == MAX_ACTIVE_VOICES {
            return OutputStatus::VoiceLimit;
        }
        let sound = self
            .clips
            .entry(asset_id.into())
            .or_insert_with(|| StaticSoundData {
                sample_rate: clip.sample_rate,
                frames: clip
                    .frames
                    .iter()
                    .map(|[left, right]| Frame::new(*left, *right))
                    .collect(),
                settings: Default::default(),
                slice: None,
            });
        let decibels = if gain == 0.0 {
            Decibels::SILENCE
        } else {
            Decibels((20.0 * gain.log10()) as f32)
        };
        match self.manager.play(sound.volume(decibels)) {
            Ok(voice) => {
                self.active.push(voice);
                OutputStatus::Ready
            }
            Err(_) => OutputStatus::VoiceLimit,
        }
    }
}
