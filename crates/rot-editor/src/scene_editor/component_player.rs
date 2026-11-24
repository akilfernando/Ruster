//! Native project Player settings. The observation is held until Apply or Cancel.
use super::super::project_startup::{Observation, ProjectDirectory};
use super::ComponentWorkspace;
use crate::{editor_theme, strings::Text, ui_kit};
use eframe::egui;
use rot_app::creation::{PlayerWindow, PlayerWindowMode, PlayerWindowSize, DEFAULT_WINDOW_SIZE};

#[derive(Clone, Default, PartialEq, Eq)]
struct Fields {
    title: String,
    width: String,
    height: String,
    mode: Option<PlayerWindowMode>,
}

impl From<&PlayerWindow> for Fields {
    fn from(player: &PlayerWindow) -> Self {
        Self {
            title: player.title.clone().unwrap_or_default(),
            mode: player.mode,
            width: player
                .initial_size
                .map(|size| size.width.to_string())
                .unwrap_or_default(),
            height: player
                .initial_size
                .map(|size| size.height.to_string())
                .unwrap_or_default(),
        }
    }
}

impl Fields {
    fn value(&self) -> Result<PlayerWindow, String> {
        let initial_size = if self.width.is_empty() && self.height.is_empty() {
            None
        } else {
            let parse = |value: &str| {
                value
                    .parse::<u32>()
                    .map_err(|_| "Enter both dimensions as whole numbers.".to_owned())
            };
            Some(PlayerWindowSize {
                width: parse(&self.width)?,
                height: parse(&self.height)?,
            })
        };
        let window = PlayerWindow {
            title: (!self.title.is_empty()).then(|| self.title.clone()),
            initial_size,
            mode: self.mode,
        };
        window.validate()?;
        Ok(window)
    }
}

pub(super) struct PlayerDraft {
    observed: Observation,
    original: Fields,
    fields: Fields,
    error: Option<String>,
}

impl PlayerDraft {
    pub(super) fn pending(&self) -> bool {
        self.fields != self.original
    }
}

impl ComponentWorkspace {
    pub(super) fn player_pending(&self) -> bool {
        self.player_settings
            .as_ref()
            .is_some_and(|settings| settings.as_ref().is_ok_and(PlayerDraft::pending))
    }

    pub(super) fn show_player_settings(&mut self, ui: &mut egui::Ui) {
        if self.player_settings.is_none() {
            self.player_settings = Some(
                self.startup_project
                    .as_ref()
                    .map_err(Clone::clone)
                    .and_then(ProjectDirectory::observe)
                    .map(|observed| {
                        let player = observed.player_window();
                        let mut error = player.as_ref().err().cloned();
                        if error.is_some() && !observed.player_settings_editable() {
                            error = Some(
                                "Repair the project settings file before changing the Player."
                                    .into(),
                            );
                        }
                        let fields = player.map_or_else(|_| Fields::default(), Fields::from);
                        PlayerDraft {
                            observed,
                            original: fields.clone(),
                            fields,
                            error,
                        }
                    }),
            );
        }
        let Some(Ok(draft)) = &mut self.player_settings else {
            if let Some(Err(error)) = &self.player_settings {
                ui.colored_label(editor_theme::colors(ui).error, error);
            }
            return;
        };
        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
        let mut focused = false;
        let mut field = |ui: &mut egui::Ui, label: Text, value: &mut String, hint: &str, limit| {
            let response = ui_kit::property_row_with_label(ui, label.plain(), |ui, label| {
                ui.add(
                    egui::TextEdit::singleline(value)
                        .hint_text(hint)
                        .char_limit(limit)
                        .desired_width(ui.available_width()),
                )
                .labelled_by(label)
            });
            focused |= response.has_focus() || response.lost_focus();
        };
        field(
            ui,
            Text::PlayerWindowTitle,
            &mut draft.fields.title,
            Text::PlayerDefault.plain(),
            129,
        );
        field(
            ui,
            Text::PlayerInitialWidth,
            &mut draft.fields.width,
            &DEFAULT_WINDOW_SIZE.width.to_string(),
            32,
        );
        field(
            ui,
            Text::PlayerInitialHeight,
            &mut draft.fields.height,
            &DEFAULT_WINDOW_SIZE.height.to_string(),
            32,
        );
        ui_kit::property_row_with_label(ui, Text::PlayerWindowMode.plain(), |ui, label| {
            let response = egui::ComboBox::from_id_salt("player-window-mode")
                .selected_text(draft.fields.mode.unwrap_or_default().label())
                .show_ui(ui, |ui| {
                    for mode in [
                        PlayerWindowMode::Windowed,
                        PlayerWindowMode::BorderlessFullscreen,
                    ] {
                        if ui
                            .selectable_label(
                                draft.fields.mode.unwrap_or_default() == mode,
                                mode.label(),
                            )
                            .clicked()
                        {
                            if draft.fields.mode.unwrap_or_default() != mode {
                                draft.fields.mode = Some(mode);
                            }
                            ui.close();
                        }
                    }
                })
                .response;
            response.labelled_by(label);
        });
        ui.weak(Text::PlayerWindowHelp.plain());
        if let Some(error) = self.error.as_ref().or(draft.error.as_ref()) {
            ui.colored_label(editor_theme::colors(ui).error, error);
        }
        let available = draft.observed.player_settings_editable();
        let pending = draft.pending();
        let (apply, cancel) = ui
            .horizontal(|ui| {
                let apply = ui
                    .add_enabled_ui(available && (pending || draft.error.is_some()), |ui| {
                        editor_theme::primary_button(ui, Text::ComponentApply.plain())
                    })
                    .inner
                    .clicked();
                let cancel = ui_kit::button(ui, Text::ComponentCancel.plain()).clicked();
                (apply, cancel)
            })
            .inner;
        if cancel {
            self.player_settings = None;
            self.error = None;
        } else if available && (apply || (focused && enter && (pending || draft.error.is_some()))) {
            match draft
                .fields
                .value()
                .and_then(|player| draft.observed.set_player_window(player))
            {
                Ok(()) => {
                    self.player_settings = None;
                    self.startup_settings = None;
                    self.error = None;
                }
                Err(error) => {
                    self.error = None;
                    draft.error = Some(if error == super::super::project_startup::CHANGED {
                        Text::PlayerReopen.plain().into()
                    } else {
                        error
                    });
                }
            }
        }
    }
}
