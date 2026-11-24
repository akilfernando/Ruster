//! Audition owns device playback, never a scene or a Play receipt.
use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_assets::{AssetKind, AssetStatus, AssetStore, AudioClip};
use std::sync::Arc;

#[derive(Default)]
pub(super) struct SoundAudition {
    output: rot_app::audio::Audition,
    target: Option<(String, Arc<AudioClip>)>,
    error: Option<String>,
}

impl SoundAudition {
    pub(super) fn stop(&mut self) {
        self.output.stop();
        self.target = None;
        self.error = None;
    }

    fn poll(&mut self, selected: Option<&str>, store: Option<&AssetStore>) {
        if self.target.as_ref().is_some_and(|(id, clip)| {
            selected != Some(id)
                || ready_sound(store, id)
                    .is_none_or(|current| !std::ptr::eq(current, clip.as_ref()))
        }) {
            self.stop();
        }
        if let Some(result) = self.output.poll() {
            if self.target.is_some() {
                self.error = result.err();
            }
        }
    }
}

fn ready_sound<'a>(store: Option<&'a AssetStore>, id: &str) -> Option<&'a AudioClip> {
    let store = store?;
    let info = store.asset_info(id)?;
    if info.kind != AssetKind::Sound || info.status != AssetStatus::Ready {
        return None;
    }
    store.sound(id)
}

impl ComponentWorkspace {
    pub(super) fn update_sound_audition(&mut self) {
        if self.runtime.in_play_mode() || self.close_pending || self.close_approved {
            self.sound_audition.stop();
        }
        self.sound_audition
            .poll(self.asset_browser.selected.as_deref(), self.assets.as_ref());
    }

    pub(super) fn sound_audition_controls(&mut self, ui: &mut egui::Ui, blocked: bool) {
        let Some(id) = self.asset_browser.selected.as_deref() else {
            return;
        };
        let Some(clip) = ready_sound(self.assets.as_ref(), id) else {
            return;
        };
        ui.separator();
        ui.label(format!(
            "{} · {:.3} s",
            Text::SoundDuration.plain(),
            clip.duration_seconds()
        ));
        ui.label(format!(
            "{} · {} Hz",
            Text::SoundSampleRate.plain(),
            clip.sample_rate
        ));
        if self.sound_audition.output.active() {
            if ui.button(Text::SoundStopPreview.plain()).clicked() {
                self.sound_audition.stop();
            }
        } else {
            let platform = cfg!(target_os = "linux");
            let unavailable = if platform {
                Text::SoundPreviewStopPlay
            } else {
                Text::SoundPreviewLinux
            };
            if ui
                .add_enabled(
                    platform && !blocked && !self.runtime.in_play_mode(),
                    egui::Button::new(Text::SoundPreview.plain()),
                )
                .on_disabled_hover_text(unavailable.plain())
                .clicked()
            {
                let id = id.to_owned();
                let clip = self
                    .assets
                    .as_ref()
                    .and_then(|store| store.sound_snapshot(&id))
                    .expect("ready sound");
                let context = ui.ctx().clone();
                let result = self
                    .sound_audition
                    .output
                    .play(clip.clone(), move || context.request_repaint());
                self.sound_audition.target = Some((id, clip));
                self.sound_audition.error = result.err();
            }
        }
        if let Some(error) = &self.sound_audition.error {
            ui.colored_label(
                editor_theme::colors(ui).error,
                Text::SoundPreviewUnavailable.plain(),
            );
            ui.add(egui::Label::new(error).wrap());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_metadata_and_preview_identity_follow_the_accepted_catalog() {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("cue.wav");
        let wav = b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x02\0\0\0\0\0";
        std::fs::write(&path, wav).unwrap();
        let mut store = AssetStore::open(project.path()).unwrap();
        store.import("cue", "cue.wav").unwrap();
        let profile = crate::scene_editor::asset_inspection::page(&store, 0).unwrap();
        let row = &profile["assets"][0];
        assert_eq!(row["status"], "ready");
        assert_eq!(row["duration_seconds"], 1.0 / 8_000.0);
        assert_eq!(row["sample_rate_hz"], 8_000);

        // Seed the selected identity without asking for an output device.
        let mut audition = SoundAudition::default();
        let accepted = store.sound_snapshot("cue").unwrap();
        audition.target = Some(("cue".into(), accepted.clone()));
        let snapshot = store.project_snapshot();
        audition.poll(Some("cue"), Some(&snapshot));
        assert!(Arc::ptr_eq(&audition.target.as_ref().unwrap().1, &accepted));
        audition.poll(None, Some(&snapshot));
        assert!(audition.target.is_none());

        let mut replacement = *wav;
        *replacement.last_mut().unwrap() = 1;
        std::fs::write(&path, replacement).unwrap();
        store.refresh("cue").unwrap();
        audition.target = Some(("cue".into(), accepted));
        audition.poll(Some("cue"), Some(&store));
        assert!(audition.target.is_none());

        let refreshed = store.sound_snapshot("cue").unwrap();
        std::fs::remove_file(&path).unwrap();
        let missing = AssetStore::open(project.path()).unwrap();
        assert!(missing.sound("cue").is_some(), "retained accepted PCM");
        audition.target = Some(("cue".into(), refreshed));
        audition.poll(Some("cue"), Some(&missing));
        assert!(audition.target.is_none());
    }
}
