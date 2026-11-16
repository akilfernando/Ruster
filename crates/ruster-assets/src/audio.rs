//! Bounded PCM WAV decoding shared by the catalog and packaged runtime.
use super::{error, AssetError};
use std::{io::Cursor, sync::Arc};

pub const MAX_SOUND_SOURCE_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_SOUND_RESOLVED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_SOUND_SECONDS: u32 = 10;
pub(super) const SOUND_CACHE_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq)]
pub struct AudioClip {
    pub sample_rate: u32,
    /// Interleaved stereo frames, with mono duplicated into both channels.
    pub frames: Arc<[[f32; 2]]>,
}

impl AudioClip {
    pub fn payload_bytes(&self) -> usize {
        std::mem::size_of_val(self.frames.as_ref())
    }
}

/// Decode a short uncompressed, signed 16-bit mono or stereo PCM WAV.
pub fn decode_wav(bytes: &[u8]) -> Result<AudioClip, AssetError> {
    if bytes.len() > MAX_SOUND_SOURCE_BYTES {
        return Err(error("WAV source exceeds 2 MiB"));
    }
    let declared_size = bytes
        .get(4..8)
        .map(|size| u32::from_le_bytes(size.try_into().expect("4-byte RIFF size")));
    if declared_size.map(u64::from).map(|size| size + 8) != Some(bytes.len() as u64) {
        return Err(error("WAV RIFF length does not match the file"));
    }
    let mut reader = Cursor::new(bytes);
    let wav = hound::WavReader::new(&mut reader).map_err(wav_error)?;
    let spec = wav.spec();
    if !matches!(spec.channels, 1 | 2)
        || !(8_000..=48_000).contains(&spec.sample_rate)
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err(error("WAV requires mono or stereo PCM16 at 8000–48000 Hz"));
    }
    let duration = wav.duration();
    if duration == 0 || duration > spec.sample_rate * MAX_SOUND_SECONDS {
        return Err(error(
            "WAV requires at least one frame and a duration of at most 10 seconds",
        ));
    }
    let data_start = wav.into_inner().position();
    let expected_bytes = u64::from(duration) * u64::from(spec.channels) * 2;
    if data_start + expected_bytes > bytes.len() as u64 {
        return Err(error("WAV PCM data is truncated"));
    }
    reader.set_position(0);
    let mut samples = hound::WavReader::new(reader)
        .map_err(wav_error)?
        .into_samples::<i16>();
    let mut next = || {
        samples
            .next()
            .ok_or_else(|| error("WAV PCM data is truncated"))?
            .map(|sample| f32::from(sample) / 32768.)
            .map_err(wav_error)
    };
    let mut frames = Vec::with_capacity(duration as usize);
    for _ in 0..duration {
        let left = next()?;
        let right = if spec.channels == 2 { next()? } else { left };
        frames.push([left, right]);
    }
    Ok(AudioClip {
        sample_rate: spec.sample_rate,
        frames: frames.into(),
    })
}

fn wav_error(source: hound::Error) -> AssetError {
    error(format!("invalid WAV: {source}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AssetKind, AssetStatus, AssetStore, ResourcePin};
    use std::{collections::BTreeMap, fs};

    fn wav(channels: u16, sample_rate: u32, samples: impl IntoIterator<Item = i16>) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut writer = hound::WavWriter::new(
            Cursor::new(&mut bytes),
            hound::WavSpec {
                channels,
                sample_rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for sample in samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
        bytes
    }

    #[test]
    fn bounded_pcm16_wav_decodes_shared_frames_and_refuses_malformed_input() {
        let mono = wav(1, 48_000, [0, i16::MIN]);
        assert_eq!(
            decode_wav(&mono).unwrap().frames.as_ref(),
            [[0.; 2], [-1.; 2]]
        );
        assert_eq!(
            decode_wav(&wav(2, 8_000, [16_384, -16_384]))
                .unwrap()
                .frames
                .as_ref(),
            [[0.5, -0.5]]
        );
        assert!(decode_wav(&wav(1, 8_000, [])).is_err());
        assert!(decode_wav(&wav(1, 96_000, [0])).is_err());
        assert!(decode_wav(&wav(3, 16_000, [0; 3])).is_err());
        assert!(decode_wav(&wav(1, 8_000, std::iter::repeat_n(0, 80_001))).is_err());
        let mut truncated = mono.clone();
        truncated.truncate(truncated.len() - 2);
        let length = (truncated.len() as u32 - 8).to_le_bytes();
        truncated[4..8].copy_from_slice(&length);
        assert!(decode_wav(&truncated)
            .unwrap_err()
            .to_string()
            .contains("truncated"));
        assert!(decode_wav(&vec![0; MAX_SOUND_SOURCE_BYTES + 1]).is_err());
    }

    #[test]
    fn wav_catalog_refresh_preserves_last_good_and_pins_match_exact_source() {
        let project = tempfile::tempdir().unwrap();
        let source = project.path().join("cue.wav");
        let first = wav(1, 16_000, [16_384]);
        fs::write(&source, &first).unwrap();
        let mut store = AssetStore::open(project.path()).unwrap();
        let info = store.import("cue", "cue.wav").unwrap();
        assert_eq!(info.kind, AssetKind::Sound);
        assert_eq!(store.revision(), 1);
        let registry_path = project.path().join(".ruster-assets.json");
        let registry = fs::read(&registry_path).unwrap();
        let mut fields: serde_json::Value = serde_json::from_slice(&registry).unwrap();
        assert_eq!(fields["version"], 6);
        let accepted = store.sound_snapshots()["cue"].clone();
        let ids = ["cue".into()].into();
        let snapshot = store.export_snapshot(&ids).unwrap();
        let pin = crate::resource_identity(&BTreeMap::from([(
            "cue".into(),
            ResourcePin::sound(&first),
        )]));
        assert_eq!(snapshot.resource_identity().unwrap(), pin);
        fs::write(&source, b"not a wave").unwrap();
        assert!(store.refresh("cue").is_err());
        assert_eq!(store.revision(), 1);
        assert!(Arc::ptr_eq(&accepted, &store.sound_snapshots()["cue"]));
        assert!(snapshot.read_sources().is_err());
        drop(store);

        let mut reopened = AssetStore::open(project.path()).unwrap();
        assert_eq!(reopened.inspect()[0].status, AssetStatus::Invalid);
        assert_eq!(reopened.sound("cue").unwrap(), accepted.as_ref());
        assert_eq!(fs::read(&registry_path).unwrap(), registry);
        let replacement = wav(2, 8_000, [8192, -8192]);
        fs::write(&source, &replacement).unwrap();
        assert!(reopened.refresh_at(0, "cue").is_err());
        reopened.refresh("cue").unwrap();
        assert_eq!(reopened.revision(), 2);
        assert_eq!(
            reopened.sound("cue").unwrap().frames.as_ref(),
            [[0.25, -0.25]]
        );
        assert_ne!(
            reopened
                .export_snapshot(&ids)
                .unwrap()
                .resource_identity()
                .unwrap(),
            pin
        );

        fields["version"] = 5.into();
        fs::write(&registry_path, serde_json::to_vec(&fields).unwrap()).unwrap();
        assert!(AssetStore::open(project.path())
            .unwrap_err()
            .to_string()
            .contains("sound assets require asset catalog version 6"));
    }
}
