//! Audition owns device playback, never a scene or a Play receipt.
use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_assets::{AssetStore, AudioClip};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Preview {
    Asset,
    Cue(f64),
}

struct Target {
    preview: Preview,
    id: String,
    clip: Arc<AudioClip>,
    cue_source: Option<String>,
}

#[derive(Default)]
pub(super) struct SoundAudition {
    output: rot_app::audio::Audition,
    target: Option<Target>,
    error: Option<String>,
}

impl SoundAudition {
    pub(super) fn stop(&mut self) {
        self.output.stop();
        self.target = None;
        self.error = None;
    }

    fn poll(&mut self, preview: Preview, selected: Option<&str>, store: Option<&AssetStore>) {
        if self.target.as_ref().is_some_and(|target| {
            target.preview != preview
                || selected != Some(target.id.as_str())
                || store.is_none_or(|store| {
                    super::component_sound::ready_sound(store, &target.id).is_none_or(|info| {
                        target
                            .cue_source
                            .as_deref()
                            .is_some_and(|source| source != info.source)
                            || store
                                .sound(&target.id)
                                .is_none_or(|current| !std::ptr::eq(current, target.clip.as_ref()))
                    })
                })
        }) {
            self.stop();
        }
        if let Some(result) = self.output.poll() {
            if self.target.is_some() {
                self.error = result.err();
            }
        }
    }

    pub(super) fn controls(
        &mut self,
        ui: &mut egui::Ui,
        preview: Preview,
        selected: Option<&str>,
        store: Option<&AssetStore>,
        blocked: bool,
    ) {
        let (label, gain) = match preview {
            Preview::Asset => (Text::SoundPreview, 1.0),
            Preview::Cue(gain) => (Text::SoundPreviewAtGain, gain),
        };
        let ready = store.zip(selected).and_then(|(store, id)| {
            let info = super::component_sound::ready_sound(store, id)?;
            Some((store, id, info))
        });
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.preview == preview)
            && self.output.active()
        {
            if ui.button(Text::SoundStopPreview.plain()).clicked() {
                self.stop();
            }
        } else {
            let platform = cfg!(target_os = "linux");
            let button = ui.add_enabled(
                platform && !blocked && ready.is_some() && (0.0..=1.0).contains(&gain),
                egui::Button::new(label.plain()),
            );
            let button = if !platform {
                button.on_disabled_hover_text(Text::SoundPreviewLinux.plain())
            } else if preview == Preview::Asset {
                button.on_disabled_hover_text(Text::SoundPreviewStopPlay.plain())
            } else {
                button
            };
            if let Some((store, id, info)) = ready.filter(|_| button.clicked()) {
                let clip = store.sound_snapshot(id).expect("ready sound");
                let context = ui.ctx().clone();
                let wake = move || context.request_repaint();
                let result = match preview {
                    Preview::Asset => self.output.play(clip.clone(), wake),
                    Preview::Cue(gain) => self.output.play_at_gain(clip.clone(), gain, wake),
                };
                self.target = Some(Target {
                    preview,
                    id: id.into(),
                    clip,
                    cue_source: matches!(preview, Preview::Cue(_)).then(|| info.source.clone()),
                });
                self.error = result.err();
            }
        }
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.preview == preview)
        {
            if let Some(error) = &self.error {
                ui.colored_label(
                    editor_theme::colors(ui).error,
                    Text::SoundPreviewUnavailable.plain(),
                );
                ui.add(egui::Label::new(error).wrap());
            }
        }
    }

    pub(super) fn poll_cue(
        &mut self,
        gain: f64,
        selected: Option<&str>,
        store: Option<&AssetStore>,
    ) {
        self.poll(Preview::Cue(gain), selected, store);
    }
}

impl ComponentWorkspace {
    pub(super) fn update_sound_audition(&mut self) {
        if self.runtime.in_play_mode() || self.close_pending || self.close_approved {
            self.sound_audition.stop();
        }
        let (preview, selected) = self.sound_cue.as_ref().map_or(
            (Preview::Asset, self.asset_browser.selected.as_deref()),
            |cue| {
                let (gain, asset) = cue.audition_selection();
                (Preview::Cue(gain), asset)
            },
        );
        self.sound_audition
            .poll(preview, selected, self.assets.as_ref());
    }

    pub(super) fn sound_audition_controls(&mut self, ui: &mut egui::Ui, blocked: bool) {
        let Some(id) = self.asset_browser.selected.as_deref() else {
            return;
        };
        let Some(clip) = self.assets.as_ref().and_then(|store| {
            super::component_sound::ready_sound(store, id)?;
            store.sound(id)
        }) else {
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
        self.sound_audition.controls(
            ui,
            Preview::Asset,
            Some(id),
            self.assets.as_ref(),
            blocked || self.runtime.in_play_mode(),
        );
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
        let target = |clip, preview, cue_source| {
            Some(Target {
                id: "cue".into(),
                clip,
                preview,
                cue_source,
            })
        };
        audition.target = target(accepted.clone(), Preview::Asset, None);
        let snapshot = store.project_snapshot();
        audition.poll(Preview::Asset, Some("cue"), Some(&snapshot));
        assert!(Arc::ptr_eq(
            &audition.target.as_ref().unwrap().clip,
            &accepted
        ));
        audition.poll(Preview::Asset, None, Some(&snapshot));
        assert!(audition.target.is_none());

        let mut replacement = *wav;
        *replacement.last_mut().unwrap() = 1;
        std::fs::write(&path, replacement).unwrap();
        store.refresh("cue").unwrap();
        let cue = Preview::Cue(0.25);
        audition.target = target(accepted.clone(), cue, Some("cue.wav".into()));
        audition.poll(cue, Some("cue"), Some(&snapshot));
        assert!(audition.target.is_some());
        audition.poll(Preview::Cue(0.), Some("cue"), Some(&snapshot));
        assert!(audition.target.is_none());
        audition.target = target(accepted.clone(), cue, Some("different.wav".into()));
        audition.poll(cue, Some("cue"), Some(&snapshot));
        assert!(audition.target.is_none());
        audition.target = target(accepted, cue, Some("cue.wav".into()));
        audition.poll(cue, Some("cue"), Some(&store));
        assert!(audition.target.is_none());

        let refreshed = store.sound_snapshot("cue").unwrap();
        std::fs::remove_file(&path).unwrap();
        let missing = AssetStore::open(project.path()).unwrap();
        assert!(missing.sound("cue").is_some(), "retained accepted PCM");
        audition.target = target(refreshed, cue, Some("cue.wav".into()));
        audition.poll(cue, Some("cue"), Some(&missing));
        assert!(audition.target.is_none());
    }
}
