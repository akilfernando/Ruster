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
