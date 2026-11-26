//! Native and guarded MCP authoring of a scene-wide display profile.

use super::ComponentWorkspace;
use crate::{
    editor_theme,
    scene_editor::display_ui::{DisplayDraft, DisplayMode},
    strings::Text,
    ui_kit,
};
use eframe::egui;
use rot_scene::{
    AntiAliasing, BackgroundColor, BloomSettings, ColorGradingSettings, DisplayProfile, Fog,
    FogMode, PostProcess, SceneError, Sky, SsaoSettings, Tonemapper,
};

fn exposure_id() -> egui::Id {
    egui::Id::unique("v23_scene_display_exposure")
}

struct BackgroundDraft {
    solid: bool,
    color: BackgroundColor,
    hex: String,
    invalid_hex: bool,
}

impl BackgroundDraft {
    fn new(value: Option<BackgroundColor>) -> Self {
        let color = value.unwrap_or_else(|| BackgroundColor::new([0.025, 0.035, 0.055]).unwrap());
        Self {
            solid: value.is_some(),
            color,
            hex: srgb_hex(color),
            invalid_hex: false,
        }
    }

    fn value(&self) -> Result<Option<BackgroundColor>, Text> {
        if self.solid && self.invalid_hex {
            return Err(Text::SceneBackgroundHexInvalid);
        }
        Ok(self.solid.then_some(self.color))
    }
}

struct SkyDraft {
    enabled: bool,
    sky: Sky,
    intensity: String,
    invalid_intensity: bool,
}

impl SkyDraft {
    fn new(value: Option<Sky>) -> Self {
        let sky = value.unwrap_or_else(Sky::daylight);
        Self {
            enabled: value.is_some(),
            sky,
            intensity: format!("{}", sky.intensity),
            invalid_intensity: false,
        }
    }

    fn set_preset(&mut self, preset: Sky) {
        self.sky = Sky {
            show_backdrop: self.sky.show_backdrop,
            ..preset
        };
        self.intensity = format!("{}", self.sky.intensity);
        self.invalid_intensity = false;
    }

    fn value(&self) -> Result<Option<Sky>, Text> {
        if !self.enabled {
            return Ok(None);
        }
        if self.invalid_intensity {
            return Err(Text::SceneSkyIntensityInvalid);
        }
        let intensity = self
            .intensity
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite() && (0.0..=8.0).contains(v))
            .ok_or(Text::SceneSkyIntensityInvalid)?;
        Ok(Some(Sky {
            intensity,
            ..self.sky
        }))
    }
}

struct FogDraft {
    enabled: bool,
    fog: Fog,
    density: String,
    start: String,
    end: String,
    height: String,
    height_falloff: String,
    max_opacity: String,
    sky_affect: String,
}

impl FogDraft {
    fn new(value: Option<Fog>) -> Self {
        let fog = value.unwrap_or_else(Fog::mist);
        Self {
            enabled: value.is_some(),
            fog,
            density: format!("{}", fog.density),
            start: format!("{}", fog.start),
            end: format!("{}", fog.end),
            height: format!("{}", fog.height),
            height_falloff: format!("{}", fog.height_falloff),
            max_opacity: format!("{}", fog.max_opacity),
            sky_affect: format!("{}", fog.sky_affect),
        }
    }

    fn set_preset(&mut self, preset: Fog) {
        self.fog = preset;
        self.density = format!("{}", preset.density);
        self.start = format!("{}", preset.start);
        self.end = format!("{}", preset.end);
        self.height = format!("{}", preset.height);
        self.height_falloff = format!("{}", preset.height_falloff);
        self.max_opacity = format!("{}", preset.max_opacity);
        self.sky_affect = format!("{}", preset.sky_affect);
    }

    fn value(&self) -> Result<Option<Fog>, Text> {
        if !self.enabled {
            return Ok(None);
        }
        let parse = |raw: &str| raw.trim().parse::<f32>().map_err(|_| Text::SceneFogInvalid);
        let candidate = Fog {
            mode: self.fog.mode,
            color: self.fog.color,
            density: parse(&self.density)?,
            start: parse(&self.start)?,
            end: parse(&self.end)?,
            height: parse(&self.height)?,
            height_falloff: parse(&self.height_falloff)?,
            max_opacity: parse(&self.max_opacity)?,
            sky_affect: parse(&self.sky_affect)?,
        };
        candidate
            .validate()
            .map(|()| Some(candidate))
            .map_err(|_| Text::SceneFogInvalid)
    }
}

fn edit_fog_field(ui: &mut egui::Ui, label_text: &str, buf: &mut String) {
    ui_kit::property_row_with_label(ui, label_text, |ui, label| {
        ui.add(
            egui::TextEdit::singleline(buf)
                .char_limit(16)
                .desired_width(ui.available_width()),
        )
        .labelled_by(label);
    });
}

fn edit_fog(ui: &mut egui::Ui, draft: &mut FogDraft) {
    let selected = if !draft.enabled {
        Text::SceneFogNone
    } else {
        match draft.fog.mode {
            FogMode::Linear => Text::SceneFogLinear,
            FogMode::Exponential => Text::SceneFogExponential,
            FogMode::ExponentialSquared => Text::SceneFogExponentialSquared,
        }
    };
    ui_kit::property_row_with_label(ui, Text::SceneFog.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("fog_mode")
            .selected_text(selected.plain())
            .show_ui(ui, |ui| {
                for (choice, text) in [
                    (None, Text::SceneFogNone),
                    (Some(FogMode::Linear), Text::SceneFogLinear),
                    (Some(FogMode::Exponential), Text::SceneFogExponential),
                    (
                        Some(FogMode::ExponentialSquared),
                        Text::SceneFogExponentialSquared,
                    ),
                ] {
                    let is_current = match choice {
                        None => !draft.enabled,
                        Some(mode) => draft.enabled && draft.fog.mode == mode,
                    };
                    if ui.selectable_label(is_current, text.plain()).clicked() {
                        match choice {
                            None => draft.enabled = false,
                            Some(mode) => {
                                draft.enabled = true;
                                draft.fog.mode = mode;
                            }
                        }
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label);
    });
    if !draft.enabled {
        return;
    }
    ui_kit::property_row_with_label(ui, Text::SceneFogPreset.plain(), |ui, _| {
        ui.horizontal(|ui| {
            if ui.button(Text::SceneFogMist.plain()).clicked() {
                draft.set_preset(Fog::mist());
            }
            if ui.button(Text::SceneFogGroundHaze.plain()).clicked() {
                draft.set_preset(Fog::ground_haze());
            }
            if ui.button(Text::SceneFogLinear.plain()).clicked() {
                draft.set_preset(Fog::linear_distance());
            }
        });
    });
    ui_kit::property_row_with_label(ui, Text::SceneFogColor.plain(), |ui, label| {
        ui.color_edit_button_rgb(&mut draft.fog.color)
            .labelled_by(label);
    });
    if draft.fog.mode == FogMode::Linear {
        edit_fog_field(ui, Text::SceneFogStart.plain(), &mut draft.start);
        edit_fog_field(ui, Text::SceneFogEnd.plain(), &mut draft.end);
    } else {
        edit_fog_field(ui, Text::SceneFogDensity.plain(), &mut draft.density);
        edit_fog_field(ui, Text::SceneFogHeight.plain(), &mut draft.height);
        edit_fog_field(
            ui,
            Text::SceneFogHeightFalloff.plain(),
            &mut draft.height_falloff,
        );
    }
    edit_fog_field(ui, Text::SceneFogMaxOpacity.plain(), &mut draft.max_opacity);
    edit_fog_field(ui, Text::SceneFogSkyAffect.plain(), &mut draft.sky_affect);
}

struct PostProcessDraft {
    enabled: bool,
    anti_aliasing: AntiAliasing,
    tonemapper: Tonemapper,
    ssao_enabled: bool,
    ssao_radius: String,
    ssao_intensity: String,
    ssao_bias: String,
    bloom_enabled: bool,
    bloom_threshold: String,
    bloom_soft_knee: String,
    bloom_intensity: String,
    saturation: String,
    contrast: String,
    temperature: String,
    tint: String,
    vignette: String,
}

impl PostProcessDraft {
    fn new(value: Option<PostProcess>) -> Self {
        let p = value.unwrap_or_default();
        Self {
            enabled: value.is_some(),
            anti_aliasing: p.anti_aliasing,
            tonemapper: p.grading.tonemapper,
            ssao_enabled: p.ssao.enabled,
            ssao_radius: format!("{}", p.ssao.radius),
            ssao_intensity: format!("{}", p.ssao.intensity),
            ssao_bias: format!("{}", p.ssao.bias),
            bloom_enabled: p.bloom.enabled,
            bloom_threshold: format!("{}", p.bloom.threshold),
            bloom_soft_knee: format!("{}", p.bloom.soft_knee),
            bloom_intensity: format!("{}", p.bloom.intensity),
            saturation: format!("{}", p.grading.saturation),
            contrast: format!("{}", p.grading.contrast),
            temperature: format!("{}", p.grading.temperature),
            tint: format!("{}", p.grading.tint),
            vignette: format!("{}", p.grading.vignette),
        }
    }

    fn set_preset(&mut self, p: PostProcess) {
        self.enabled = true;
        self.anti_aliasing = p.anti_aliasing;
        self.tonemapper = p.grading.tonemapper;
        self.ssao_enabled = p.ssao.enabled;
        self.ssao_radius = format!("{}", p.ssao.radius);
        self.ssao_intensity = format!("{}", p.ssao.intensity);
        self.ssao_bias = format!("{}", p.ssao.bias);
        self.bloom_enabled = p.bloom.enabled;
        self.bloom_threshold = format!("{}", p.bloom.threshold);
        self.bloom_soft_knee = format!("{}", p.bloom.soft_knee);
        self.bloom_intensity = format!("{}", p.bloom.intensity);
        self.saturation = format!("{}", p.grading.saturation);
        self.contrast = format!("{}", p.grading.contrast);
        self.temperature = format!("{}", p.grading.temperature);
        self.tint = format!("{}", p.grading.tint);
        self.vignette = format!("{}", p.grading.vignette);
    }

    fn value(&self) -> Result<Option<PostProcess>, Text> {
        if !self.enabled {
            return Ok(None);
        }
        let parse = |raw: &str| {
            raw.trim()
                .parse::<f32>()
                .map_err(|_| Text::ScenePostInvalid)
        };
        let candidate = PostProcess {
            ssao: SsaoSettings {
                enabled: self.ssao_enabled,
                radius: parse(&self.ssao_radius)?,
                intensity: parse(&self.ssao_intensity)?,
                bias: parse(&self.ssao_bias)?,
            },
            bloom: BloomSettings {
                enabled: self.bloom_enabled,
                threshold: parse(&self.bloom_threshold)?,
                soft_knee: parse(&self.bloom_soft_knee)?,
                intensity: parse(&self.bloom_intensity)?,
            },
            grading: ColorGradingSettings {
                tonemapper: self.tonemapper,
                saturation: parse(&self.saturation)?,
                contrast: parse(&self.contrast)?,
                temperature: parse(&self.temperature)?,
                tint: parse(&self.tint)?,
                vignette: parse(&self.vignette)?,
            },
            anti_aliasing: self.anti_aliasing,
        };
        candidate
            .validate()
            .map(|()| Some(candidate))
            .map_err(|_| Text::ScenePostInvalid)
    }
}

fn edit_post_process(ui: &mut egui::Ui, draft: &mut PostProcessDraft) {
    ui_kit::property_row_with_label(ui, Text::ScenePost.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("post_mode")
            .selected_text(
                if draft.enabled {
                    Text::ScenePostCustom
                } else {
                    Text::ScenePostDefault
                }
                .plain(),
            )
            .show_ui(ui, |ui| {
                for (enabled, text) in [
                    (false, Text::ScenePostDefault),
                    (true, Text::ScenePostCustom),
                ] {
                    if ui
                        .selectable_value(&mut draft.enabled, enabled, text.plain())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label);
    });
    if !draft.enabled {
        return;
    }
    ui_kit::property_row_with_label(ui, Text::ScenePostPreset.plain(), |ui, _| {
        ui.horizontal(|ui| {
            if ui.button(Text::ScenePostCinematic.plain()).clicked() {
                draft.set_preset(PostProcess::cinematic());
            }
            if ui.button(Text::ScenePostPerformance.plain()).clicked() {
                draft.set_preset(PostProcess::performance());
            }
        });
    });
    let aa_label = |aa| match aa {
        AntiAliasing::Msaa4x => Text::ScenePostAaMsaa4x,
        AntiAliasing::Msaa2x => Text::ScenePostAaMsaa2x,
        AntiAliasing::Fxaa => Text::ScenePostAaFxaa,
        AntiAliasing::Off => Text::ScenePostAaOff,
    };
    ui_kit::property_row_with_label(ui, Text::ScenePostAntiAliasing.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("post_aa")
            .selected_text(aa_label(draft.anti_aliasing).plain())
            .show_ui(ui, |ui| {
                for aa in [
                    AntiAliasing::Msaa4x,
                    AntiAliasing::Msaa2x,
                    AntiAliasing::Fxaa,
                    AntiAliasing::Off,
                ] {
                    if ui
                        .selectable_value(&mut draft.anti_aliasing, aa, aa_label(aa).plain())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label);
    });
    let tm_label = |tm| match tm {
        Tonemapper::PbrNeutral => Text::ScenePostTmPbrNeutral,
        Tonemapper::AcesFitted => Text::ScenePostTmAces,
        Tonemapper::Reinhard => Text::ScenePostTmReinhard,
        Tonemapper::None => Text::ScenePostTmNone,
    };
    ui_kit::property_row_with_label(ui, Text::ScenePostTonemapper.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("post_tm")
            .selected_text(tm_label(draft.tonemapper).plain())
            .show_ui(ui, |ui| {
                for tm in [
                    Tonemapper::PbrNeutral,
                    Tonemapper::AcesFitted,
                    Tonemapper::Reinhard,
                    Tonemapper::None,
                ] {
                    if ui
                        .selectable_value(&mut draft.tonemapper, tm, tm_label(tm).plain())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label);
    });
    ui.checkbox(&mut draft.ssao_enabled, Text::ScenePostSsao.plain());
    if draft.ssao_enabled {
        edit_fog_field(ui, Text::ScenePostRadius.plain(), &mut draft.ssao_radius);
        edit_fog_field(
            ui,
            Text::ScenePostIntensity.plain(),
            &mut draft.ssao_intensity,
        );
        edit_fog_field(ui, Text::ScenePostBias.plain(), &mut draft.ssao_bias);
    }
    ui.checkbox(&mut draft.bloom_enabled, Text::ScenePostBloom.plain());
    if draft.bloom_enabled {
        edit_fog_field(
            ui,
            Text::ScenePostThreshold.plain(),
            &mut draft.bloom_threshold,
        );
        edit_fog_field(
            ui,
            Text::ScenePostSoftKnee.plain(),
            &mut draft.bloom_soft_knee,
        );
        edit_fog_field(
            ui,
            Text::ScenePostIntensity.plain(),
            &mut draft.bloom_intensity,
        );
    }
    edit_fog_field(ui, Text::ScenePostSaturation.plain(), &mut draft.saturation);
    edit_fog_field(ui, Text::ScenePostContrast.plain(), &mut draft.contrast);
    edit_fog_field(
        ui,
        Text::ScenePostTemperature.plain(),
        &mut draft.temperature,
    );
    edit_fog_field(ui, Text::ScenePostTint.plain(), &mut draft.tint);
    edit_fog_field(ui, Text::ScenePostVignette.plain(), &mut draft.vignette);
}

fn srgb_hex(color: BackgroundColor) -> String {
    let [r, g, b] = color.linear();
    let [r, g, b, _] = egui::Rgba::from_rgb(r, g, b).to_srgba_unmultiplied();
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn from_srgb_hex(value: &str) -> Option<BackgroundColor> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let rgb = u32::from_str_radix(hex, 16).ok()?;
    let [_, r, g, b] = rgb.to_be_bytes();
    let rgba = egui::Rgba::from(egui::Color32::from_rgb(r, g, b));
    BackgroundColor::new([rgba[0], rgba[1], rgba[2]].map(f64::from)).ok()
}

fn edit_background(ui: &mut egui::Ui, draft: &mut BackgroundDraft) {
    ui_kit::property_row_with_label(ui, Text::SceneBackground.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("background")
            .selected_text(
                if draft.solid {
                    Text::SceneBackgroundSolid
                } else {
                    Text::SceneBackgroundDefault
                }
                .plain(),
            )
            .show_ui(ui, |ui| {
                for (solid, text) in [
                    (false, Text::SceneBackgroundDefault),
                    (true, Text::SceneBackgroundSolid),
                ] {
                    if ui
                        .selectable_value(&mut draft.solid, solid, text.plain())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label);
    });
    if !draft.solid {
        return;
    }
    ui_kit::property_row_with_label(ui, Text::Color.plain(), |ui, label| {
        let mut rgb = draft.color.linear();
        if ui
            .color_edit_button_rgb(&mut rgb)
            .labelled_by(label)
            .changed()
        {
            if let Ok(color) = BackgroundColor::new(rgb.map(f64::from)) {
                draft.color = color;
                draft.hex = srgb_hex(color);
                draft.invalid_hex = false;
            }
        }
    });
    ui_kit::property_row_with_label(ui, Text::SceneBackgroundHex.plain(), |ui, label| {
        if ui
            .add(
                egui::TextEdit::singleline(&mut draft.hex)
                    .char_limit(16)
                    .desired_width(ui.available_width())
                    .hint_text("#RRGGBB"),
            )
            .labelled_by(label)
            .changed()
        {
            let parsed = from_srgb_hex(&draft.hex);
            draft.invalid_hex = parsed.is_none();
            if let Some(parsed) = parsed {
                draft.color = parsed;
            }
        }
    });
}

fn edit_sky(ui: &mut egui::Ui, draft: &mut SkyDraft) {
    ui_kit::property_row_with_label(ui, Text::SceneSky.plain(), |ui, label| {
        egui::ComboBox::from_id_salt("sky_mode")
            .selected_text(
                if draft.enabled {
                    Text::SceneSkyProcedural
                } else {
                    Text::SceneSkyNone
                }
                .plain(),
            )
            .show_ui(ui, |ui| {
                for (enabled, text) in [
                    (false, Text::SceneSkyNone),
                    (true, Text::SceneSkyProcedural),
                ] {
                    if ui
                        .selectable_value(&mut draft.enabled, enabled, text.plain())
                        .clicked()
                    {
                        ui.close();
                    }
                }
            })
            .response
            .labelled_by(label);
    });
    if !draft.enabled {
        return;
    }
    ui_kit::property_row_with_label(ui, Text::SceneSkyPreset.plain(), |ui, _| {
        ui.horizontal(|ui| {
            if ui.button(Text::SceneSkyDaylight.plain()).clicked() {
                draft.set_preset(Sky::daylight());
            }
            if ui.button(Text::SceneSkyStudio.plain()).clicked() {
                draft.set_preset(Sky::studio());
            }
            if ui.button(Text::SceneSkySunset.plain()).clicked() {
                draft.set_preset(Sky::sunset());
            }
        });
    });
    for (text, rgb) in [
        (Text::SceneSkyZenith, &mut draft.sky.zenith),
        (Text::SceneSkyHorizon, &mut draft.sky.horizon),
        (Text::SceneSkyGround, &mut draft.sky.ground),
    ] {
        ui_kit::property_row_with_label(ui, text.plain(), |ui, label| {
            ui.color_edit_button_rgb(rgb).labelled_by(label);
        });
    }
    ui_kit::property_row_with_label(ui, Text::SceneSkyIntensity.plain(), |ui, label| {
        if ui
            .add(
                egui::TextEdit::singleline(&mut draft.intensity)
                    .char_limit(16)
                    .desired_width(ui.available_width()),
            )
            .labelled_by(label)
            .changed()
        {
            draft.invalid_intensity = draft
                .intensity
                .trim()
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && (0.0..=8.0).contains(v))
                .is_none();
        }
    });
    ui.checkbox(&mut draft.sky.show_backdrop, Text::SceneSkyBackdrop.plain());
}

pub(super) struct Draft {
    value: DisplayDraft,
    background: BackgroundDraft,
    sky: SkyDraft,
    fog: FogDraft,
    post_process: PostProcessDraft,
    opened_frame: Option<u64>,
    error: Option<String>,
}

impl ComponentWorkspace {
    pub(super) fn begin_display(&mut self) {
        if self.authored_edit_pending() || self.runtime.in_play_mode() {
            return;
        }
        self.display_draft = Some(Draft {
            value: DisplayDraft::from_profile(
                self.session.revision(),
                self.session.scene().display,
            ),
            background: BackgroundDraft::new(self.session.scene().background),
            sky: SkyDraft::new(self.session.scene().sky),
            fog: FogDraft::new(self.session.scene().fog),
            post_process: PostProcessDraft::new(self.session.scene().post_process),
            opened_frame: None,
            error: None,
        });
        #[cfg(unix)]
        self.invalidate_display_capture();
    }

    fn commit_display(
        &mut self,
        revision: u64,
        display: DisplayProfile,
        background: Option<BackgroundColor>,
        sky: Option<Sky>,
        fog: Option<Fog>,
        post_process: Option<PostProcess>,
    ) -> Result<u64, SceneError> {
        let next =
            self.session
                .set_display(revision, display, background, sky, fog, post_process)?;
        if revision != next {
            if let Ok(preview) = &mut self.preview {
                preview.set_display(display, background, sky, fog, post_process);
            }
            #[cfg(unix)]
            self.invalidate_display_capture();
        }
        Ok(next)
    }

    pub(super) fn defer_display_close(&mut self, ctx: &egui::Context) -> bool {
        if !ctx.input(|input| input.viewport().close_requested()) {
            return false;
        }
        let Some(draft) = &mut self.display_draft else {
            return false;
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        draft.error = Some(Text::ComponentDisplayClosePending.plain().into());
        true
    }

    pub(super) fn show_display_draft(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.display_draft.take() else {
            return;
        };
        let popup_open = egui::Popup::is_any_open(ctx);
        let frame = ctx.cumulative_frame_nr();
        let opening = draft.opened_frame.is_none();
        let enter = draft.opened_frame.is_some_and(|opened| opened != frame)
            && ctx.input(|input| input.key_pressed(egui::Key::Enter));
        let exposure_focused = ctx.memory(|memory| memory.has_focus(exposure_id()));
        let profile_before = draft.value.profile;
        let response = egui::Modal::new(egui::Id::unique("v23_scene_display"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width(ui.available_width().min(360.));
                ui.heading(Text::SceneDisplay.plain());
                ui_kit::property_row_with_label(
                    ui,
                    Text::ComponentDisplayProfile.plain(),
                    |ui, label| {
                        let field = egui::ComboBox::from_id_salt("profile")
                            .selected_text(match draft.value.profile {
                                DisplayMode::Legacy => Text::LegacySdr.plain(),
                                DisplayMode::Surface => Text::SurfaceHdr.plain(),
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut draft.value.profile,
                                    DisplayMode::Legacy,
                                    Text::LegacySdr.plain(),
                                );
                                ui.selectable_value(
                                    &mut draft.value.profile,
                                    DisplayMode::Surface,
                                    Text::SurfaceHdr.plain(),
                                );
                            })
                            .response
                            .labelled_by(label);
                        if opening && draft.value.profile == DisplayMode::Legacy {
                            field.request_focus();
                        }
                    },
                );
                if draft.value.profile == DisplayMode::Surface {
                    ui_kit::property_row_with_label(ui, Text::ExposureEv.plain(), |ui, label| {
                        let exposure = ui
                            .add(
                                egui::TextEdit::singleline(&mut draft.value.exposure_stops)
                                    .id(exposure_id())
                                    .desired_width(ui.available_width()),
                            )
                            .labelled_by(label)
                            .on_hover_text(Text::DisplayUiAdjustBrightnessInExposure.plain());
                        if opening || draft.value.profile != profile_before {
                            exposure.request_focus();
                        }
                    });
                }
                egui::ScrollArea::vertical()
                    .max_height(520.0)
                    .show(ui, |ui| {
                        edit_background(ui, &mut draft.background);
                        edit_sky(ui, &mut draft.sky);
                        edit_fog(ui, &mut draft.fog);
                        edit_post_process(ui, &mut draft.post_process);
                    });
                if (draft.error.as_deref() == Some(Text::SceneBackgroundHexInvalid.plain())
                    && draft.background.value().is_ok())
                    || (draft.error.as_deref() == Some(Text::SceneSkyIntensityInvalid.plain())
                        && draft.sky.value().is_ok())
                    || (draft.error.as_deref() == Some(Text::SceneFogInvalid.plain())
                        && draft.fog.value().is_ok())
                    || (draft.error.as_deref() == Some(Text::ScenePostInvalid.plain())
                        && draft.post_process.value().is_ok())
                {
                    draft.error = None;
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                ui.horizontal(|ui| {
                    (
                        editor_theme::primary_button(ui, Text::ComponentApply.plain()).clicked(),
                        ui.button(Text::ComponentCancel.plain()).clicked(),
                    )
                })
                .inner
            });
        if opening {
            draft.opened_frame = Some(frame);
        }
        let (apply, cancel) = response.inner;
        let escape = response.is_top_modal
            && !response.any_popup_open
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        if cancel || escape {
            return;
        }
        if apply || enter && (exposure_focused || !popup_open && !egui::Popup::is_any_open(ctx)) {
            if self.session.revision() != draft.value.revision {
                draft.error = Some(Text::ComponentDisplayDraftChanged.plain().into());
            } else {
                let candidate = draft
                    .value
                    .value()
                    .map_err(|_| Text::ComponentDisplayExposureInvalid)
                    .and_then(|display| {
                        Ok((
                            display,
                            draft.background.value()?,
                            draft.sky.value()?,
                            draft.fog.value()?,
                            draft.post_process.value()?,
                        ))
                    });
                draft.error = Some(match candidate {
                    Ok((display, background, sky, fog, post_process)) => {
                        match self.commit_display(
                            draft.value.revision,
                            display,
                            background,
                            sky,
                            fog,
                            post_process,
                        ) {
                            Ok(_) => return,
                            Err(error) => error.to_string(),
                        }
                    }
                    Err(text) => text.plain().to_owned(),
                });
            }
        }
        self.display_draft = Some(draft);
    }

    #[cfg(unix)]
    pub(super) fn display_command(
        &mut self,
        token: &str,
        args: &serde_json::Value,
    ) -> Result<serde_json::Value, (String, String)> {
        use crate::scene_editor::material_commands::{exact_keys, number};
        use serde_json::json;

        let fail = |code: &str, message: &str| (code.to_owned(), message.to_owned());
        if !args.as_object().is_some_and(|map| {
            exact_keys(
                map,
                &["expected_session_token", "expected_revision"],
                &["display", "background", "sky", "fog", "post_process"],
            ) && (map.contains_key("display")
                || map.contains_key("background")
                || map.contains_key("sky")
                || map.contains_key("fog")
                || map.contains_key("post_process"))
        }) {
            return Err(fail("invalid_arguments", "Check the Scene Display fields."));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(fail("stale_session", "Inspect the open scene again."));
        }
        let revision = number(&args["expected_revision"])?;
        if self.session.revision() != revision {
            return Err(fail(
                "stale_revision",
                "Inspect the current scene revision again.",
            ));
        }
        if self.ai_transition_pending() || self.authored_edit_pending() {
            return Err(fail(
                "drafts_pending",
                "Finish the pending editor change first.",
            ));
        }
        if self.runtime.in_play_mode() {
            return Err(fail("play_active", "Stop Play to change Scene Display."));
        }
        let scene = self.session.scene();
        let candidate = args
            .get("display")
            .map(|value| serde_json::from_value(value.clone()))
            .transpose()
            .map_err(|error| ("invalid_arguments".into(), error.to_string()))?
            .unwrap_or(scene.display);
        let background = match args.get("background") {
            None => scene.background,
            Some(value) if value.is_null() => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|error| ("invalid_arguments".into(), error.to_string()))?,
            ),
        };
        let sky = match args.get("sky") {
            None => scene.sky,
            Some(value) if value.is_null() => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|error| ("invalid_arguments".into(), error.to_string()))?,
            ),
        };
        let fog = match args.get("fog") {
            None => scene.fog,
            Some(value) if value.is_null() => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|error| ("invalid_arguments".into(), error.to_string()))?,
            ),
        };
        let post_process = match args.get("post_process") {
            None => scene.post_process,
            Some(value) if value.is_null() => None,
            Some(value) => Some(
                serde_json::from_value(value.clone())
                    .map_err(|error| ("invalid_arguments".into(), error.to_string()))?,
            ),
        };
        let next = self
            .commit_display(revision, candidate, background, sky, fog, post_process)
            .map_err(|error| {
                let code = match error {
                    SceneError::RevisionOverflow => "revision_exhausted",
                    _ => "invalid_arguments",
                };
                (code.into(), error.to_string())
            })?;
        Ok(json!({
            "display":candidate, "background":background, "sky":sky, "fog":fog, "post_process":post_process, "changed":next != revision, "dirty":self.session.is_dirty(),
        }))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use rot_scene::component_scene::{ComponentScene, ComponentSession};
    use serde_json::json;

    #[test]
    fn background_edits_preserve_omitted_fields_and_reset_with_shared_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let scene_display = |editor: &mut ComponentWorkspace,
                             revision,
                             patch: serde_json::Value| {
            let mut args = json!({"expected_session_token":"scene", "expected_revision":revision});
            args.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            editor.ai_command("scene", "scene_display_set", &args)
        };

        let hex = from_srgb_hex("#3366CC").unwrap();
        assert_eq!(srgb_hex(hex), "#3366CC");
        assert!(from_srgb_hex("#1234567").is_none());
        assert!((hex.linear()[0] - 0.033_104_766).abs() < 1e-7);
        let invalid = scene_display(&mut editor, "0", json!({"background":[1.000000000001,0,0]}));
        assert_eq!(invalid["error"]["code"], "invalid_arguments");
        let changed = scene_display(&mut editor, "0", json!({"background":hex}));
        assert_eq!(changed["ok"], true, "{changed}");
        assert_eq!(changed["result"]["display"], json!({"type":"legacy_sdr"}));
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().background,
            Some(hex)
        );
        let mut engine = rot_behavior::Engine::new(Default::default()).unwrap();
        assert_eq!(
            engine
                .reset_component(editor.session.scene(), 0)
                .unwrap()
                .scene()
                .background,
            Some(hex)
        );

        let changed = scene_display(
            &mut editor,
            "1",
            json!({"display":{"type":"surface_hdr","exposure_stops":2}}),
        );
        assert_eq!(changed["result"]["background"], json!(hex));
        let reset = scene_display(&mut editor, "2", json!({"background":null}));
        assert_eq!(reset["result"]["background"], serde_json::Value::Null);
        assert_eq!(reset["result"]["display"]["exposure_stops"], 2.0);
        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({"expected_session_token":"scene","expected_revision":"3"}),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().background,
            Some(hex)
        );

        let persisted = directory.path().join("solid.scene.json");
        editor.session.scene().save_new(&persisted).unwrap();
        assert_eq!(
            ComponentSession::open(&persisted)
                .unwrap()
                .scene()
                .background,
            Some(hex)
        );
        let legacy_json = serde_json::to_value(legacy.scene()).unwrap();
        assert!(legacy_json.get("background").is_none());
    }

    #[test]
    fn scene_display_modal_guards_committed_state_and_uses_one_shared_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let authored = std::fs::read(&path).unwrap();
        assert!(
            serde_json::from_slice::<serde_json::Value>(&authored).unwrap()["scene"]
                .get("display")
                .is_none()
        );
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.select(None);
        let original_draw_rows = editor.preview.as_ref().unwrap().draw_matrices().as_ptr();
        editor.begin_display();
        assert_eq!(
            editor.display_draft.as_ref().unwrap().value.exposure_stops,
            "0"
        );
        let ctx = egui::Context::default();
        let frame = |editor: &mut ComponentWorkspace, events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(960., 680.),
                    )),
                    ..Default::default()
                },
                |ui| editor.show_display_draft(ui.ctx()),
            );
            output.textures_delta.clear();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        // The Enter which opened the modal may still be in the opening frame.
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert!(editor.display_draft.is_some());
        let mut close = egui::RawInput::default();
        close
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let mut output = ctx.run_ui(close, |ui| {
            assert!(editor.defer_display_close(ui.ctx()));
            editor.show_display_draft(ui.ctx());
        });
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, egui::ViewportCommand::CancelClose)));
        output.textures_delta.clear();
        let backdrop = egui::pos2(5.0, 5.0);
        for pressed in [true, false] {
            frame(
                &mut editor,
                vec![
                    egui::Event::PointerMoved(backdrop),
                    egui::Event::PointerButton {
                        pos: backdrop,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        assert!(editor.display_draft.is_some());

        let draft = &mut editor.display_draft.as_mut().unwrap().value;
        draft.profile = DisplayMode::Surface;
        draft.exposure_stops = "8.0000001".to_owned();
        frame(&mut editor, vec![]);
        ctx.memory_mut(|memory| memory.request_focus(exposure_id()));
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert_eq!(
            editor.display_draft.as_ref().unwrap().error.as_deref(),
            Some(Text::ComponentDisplayExposureInvalid.plain())
        );

        assert!(!editor.save());
        let mut command = json!({
            "expected_session_token":"scene","expected_revision":"0",
            "display":{"type":"surface_hdr","exposure_stops":1.0},
        });
        let refused = editor.ai_command("scene", "scene_display_set", &command);
        assert_eq!(refused["error"]["code"], "drafts_pending");
        let view = editor.ai_capture_command(
            &crate::bridge::Authority::test_approved(),
            "scene",
            "viewport_inspect",
            &json!({}),
        );
        assert_eq!(view["error"]["code"], "view_busy");
        let inspect = editor.ai_command("scene", "material_inspect", &json!({}));
        assert_eq!(inspect["result"]["pending_display"], true);
        assert_eq!(inspect["result"]["display"], json!({"type":"legacy_sdr"}));
        frame(&mut editor, vec![key(egui::Key::Escape)]);
        assert!(editor.display_draft.is_none());
        assert_eq!(editor.session.revision(), 0);
        command["display"]["exposure_stops"] = json!(8.0000001);
        assert_eq!(
            editor.ai_command("scene", "scene_display_set", &command)["error"]["code"],
            "invalid_arguments"
        );

        editor.begin_display();
        editor.display_draft.as_mut().unwrap().value.profile = DisplayMode::Surface;
        frame(&mut editor, vec![]);
        assert!(ctx.memory(|memory| memory.has_focus(exposure_id())));
        frame(&mut editor, vec![key(egui::Key::Enter)]);
        assert_eq!(editor.session.revision(), 1);
        assert_eq!(
            editor.session.scene().display,
            DisplayProfile::surface_hdr()
        );
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().display,
            DisplayProfile::surface_hdr()
        );
        assert_eq!(
            editor.preview.as_ref().unwrap().draw_matrices().as_ptr(),
            original_draw_rows
        );

        command["expected_revision"] = json!("1");
        command["display"]["exposure_stops"] = json!(1.5);
        let changed = editor.ai_command("scene", "scene_display_set", &command);
        assert_eq!(changed["result"]["display"]["exposure_stops"], 1.5);
        assert_eq!(editor.session.revision(), 2);
        assert_eq!(std::fs::read(&path).unwrap(), authored);
        for revision in ["2", "3"] {
            let undo = editor.ai_command(
                "scene",
                "scene_undo",
                &json!({
                    "expected_session_token":"scene","expected_revision":revision,
                }),
            );
            assert_eq!(undo["ok"], true, "{undo}");
        }
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().display,
            DisplayProfile::LegacySdr
        );
    }

    #[test]
    fn sky_edits_preserve_omitted_fields_and_reset_with_shared_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let scene_display = |editor: &mut ComponentWorkspace,
                             revision,
                             patch: serde_json::Value| {
            let mut args = json!({"expected_session_token":"scene", "expected_revision":revision});
            args.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            editor.ai_command("scene", "scene_display_set", &args)
        };
        let invalid = scene_display(
            &mut editor,
            "0",
            json!({"sky":{"zenith":[0.1,0.2,1.1],"horizon":[0.5,0.6,0.7],"ground":[0.1,0.1,0.1],"intensity":1.0}}),
        );
        assert_eq!(invalid["error"]["code"], "invalid_arguments");

        let sky = Sky {
            intensity: 2.0,
            show_backdrop: false,
            ..Sky::daylight()
        };
        let changed = scene_display(&mut editor, "0", json!({"sky": sky}));
        assert_eq!(changed["ok"], true, "{changed}");
        assert_eq!(editor.session.scene().sky, Some(sky));
        assert_eq!(editor.preview.as_ref().unwrap().scene().sky, Some(sky));

        let background = [0.2, 0.3, 0.4];
        let bg_only = scene_display(&mut editor, "1", json!({"background": background}));
        assert_eq!(bg_only["ok"], true, "{bg_only}");
        assert_eq!(editor.session.scene().sky, Some(sky));
        assert_eq!(bg_only["result"]["sky"], json!(sky));

        let reset = scene_display(&mut editor, "2", json!({"sky": null}));
        assert_eq!(reset["result"]["sky"], serde_json::Value::Null);
        assert_eq!(editor.session.scene().sky, None);
        assert_eq!(editor.preview.as_ref().unwrap().scene().sky, None);

        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({"expected_session_token":"scene", "expected_revision":"3"}),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(editor.session.scene().sky, Some(sky));
        assert_eq!(editor.preview.as_ref().unwrap().scene().sky, Some(sky));
    }

    #[test]
    fn fog_edits_preserve_omitted_fields_and_reset_with_shared_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let scene_display = |editor: &mut ComponentWorkspace,
                             revision,
                             patch: serde_json::Value| {
            let mut args = json!({"expected_session_token":"scene", "expected_revision":revision});
            args.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            editor.ai_command("scene", "scene_display_set", &args)
        };
        let invalid = scene_display(
            &mut editor,
            "0",
            json!({"fog":{"mode":"linear","color":[0.5,0.6,0.7],"start":50.0,"end":40.0}}),
        );
        assert_eq!(invalid["error"]["code"], "invalid_arguments");

        let fog = Fog::ground_haze();
        let changed = scene_display(&mut editor, "0", json!({"fog": fog}));
        assert_eq!(changed["ok"], true, "{changed}");
        assert_eq!(editor.session.scene().fog, Some(fog));
        assert_eq!(editor.preview.as_ref().unwrap().scene().fog, Some(fog));

        let background = [0.2, 0.3, 0.4];
        let bg_only = scene_display(&mut editor, "1", json!({"background": background}));
        assert_eq!(bg_only["ok"], true, "{bg_only}");
        assert_eq!(editor.session.scene().fog, Some(fog));
        assert_eq!(bg_only["result"]["fog"], json!(fog));

        let reset = scene_display(&mut editor, "2", json!({"fog": null}));
        assert_eq!(reset["result"]["fog"], serde_json::Value::Null);
        assert_eq!(editor.session.scene().fog, None);
        assert_eq!(editor.preview.as_ref().unwrap().scene().fog, None);

        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({"expected_session_token":"scene", "expected_revision":"3"}),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(editor.session.scene().fog, Some(fog));
        assert_eq!(editor.preview.as_ref().unwrap().scene().fog, Some(fog));
    }

    #[test]
    fn post_process_edits_preserve_omitted_fields_and_reset_with_shared_undo() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("scene.json");
        let legacy = rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let scene = ComponentScene::from_legacy(&legacy).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let scene_display = |editor: &mut ComponentWorkspace,
                             revision,
                             patch: serde_json::Value| {
            let mut args = json!({"expected_session_token":"scene", "expected_revision":revision});
            args.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            editor.ai_command("scene", "scene_display_set", &args)
        };
        let invalid = scene_display(
            &mut editor,
            "0",
            json!({"post_process":{"bloom":{"intensity":3.0}}}),
        );
        assert_eq!(invalid["error"]["code"], "invalid_arguments");

        let post = PostProcess::cinematic();
        let changed = scene_display(&mut editor, "0", json!({"post_process": post}));
        assert_eq!(changed["ok"], true, "{changed}");
        assert_eq!(editor.session.scene().post_process, Some(post));
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().post_process,
            Some(post)
        );

        let background = [0.2, 0.3, 0.4];
        let bg_only = scene_display(&mut editor, "1", json!({"background": background}));
        assert_eq!(bg_only["ok"], true, "{bg_only}");
        assert_eq!(editor.session.scene().post_process, Some(post));
        assert_eq!(bg_only["result"]["post_process"], json!(post));

        let reset = scene_display(&mut editor, "2", json!({"post_process": null}));
        assert_eq!(reset["result"]["post_process"], serde_json::Value::Null);
        assert_eq!(editor.session.scene().post_process, None);
        assert_eq!(editor.preview.as_ref().unwrap().scene().post_process, None);

        let undo = editor.ai_command(
            "scene",
            "scene_undo",
            &json!({"expected_session_token":"scene", "expected_revision":"3"}),
        );
        assert_eq!(undo["ok"], true, "{undo}");
        assert_eq!(editor.session.scene().post_process, Some(post));
        assert_eq!(
            editor.preview.as_ref().unwrap().scene().post_process,
            Some(post)
        );
    }
}
