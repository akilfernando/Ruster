use super::ComponentWorkspace;
use crate::editor_theme;
use eframe::egui;
use rot_assets::{AssetInfo, AssetKind, AssetStatus, AssetStore};
use rot_scene::component_scene::{Component, ComponentInstance, ObjectNode, ScriptSourceKind};
use rot_scene::{PropertyValue, SoundPlayback, SoundRef};
use std::collections::BTreeMap;

pub(super) fn sound_script(
    component: &ComponentInstance,
) -> Option<&BTreeMap<String, PropertyValue>> {
    match &component.kind {
        Component::Script {
            binding,
            source_kind: Some(ScriptSourceKind::Asset),
            ..
        } => Some(&binding.properties),
        _ => None,
    }
}

pub(super) fn ready_sound<'a>(store: &'a AssetStore, id: &str) -> Option<&'a AssetInfo> {
    store.asset_info(id).filter(|info| {
        info.kind == AssetKind::Sound
            && info.status == AssetStatus::Ready
            && store.sound(id).is_some()
    })
}

pub(super) fn changed_cues<'a>(
    before: &'a ObjectNode,
    after: &'a ObjectNode,
) -> impl Iterator<Item = (&'a String, &'a PropertyValue)> {
    after
        .components
        .iter()
        .filter_map(|component| {
            let cues = sound_script(component)?;
            let old = before
                .components
                .iter()
                .find(|old| old.id == component.id)
                .and_then(sound_script);
            Some(cues.iter().filter(move |(name, value)| {
                matches!(value, PropertyValue::SoundRef(_))
                    && old.and_then(|old| old.get(*name)) != Some(*value)
            }))
        })
        .flatten()
}

pub(super) fn validate_cue(
    name: &str,
    value: &PropertyValue,
    assets: Option<&AssetStore>,
) -> Result<(), String> {
    rot_scene::validate_id(name).map_err(|error| error.to_string())?;
    value.validate().map_err(|error| error.to_string())?;
    let PropertyValue::SoundRef(cue) = value else {
        return Err("File-backed Object Lua accepts Sound Cues only.".into());
    };
    let id = cue
        .asset_id
        .as_deref()
        .ok_or("Choose an imported WAV for the sound cue.")?;
    assets
        .and_then(|assets| ready_sound(assets, id))
        .ok_or_else(|| format!("WAV {id:?} is unavailable. Choose a ready Sound in Assets."))?;
    Ok(())
}

pub(in crate::scene_editor) struct Request {
    pub component: String,
    pub name: Option<String>,
}

pub(super) struct Draft {
    object: String,
    component: String,
    original: Option<String>,
    name: String,
    asset: Option<String>,
    gain: f64,
    positional: bool,
    range: f64,
    revision: u64,
    asset_revision: u64,
    choices: Vec<(String, String)>,
    focus: bool,
    error: Option<String>,
}

impl ComponentWorkspace {
    pub(super) fn begin_sound_cue(&mut self, request: Request) {
        if self.authored_edit_pending()
            || self.runtime.in_play_mode()
            || self.ai_transition_pending()
        {
            return;
        }
        let Some(index) = self.selected_index else {
            return;
        };
        let object = &self.session.scene().objects[index];
        let Some(cues) = object
            .components
            .iter()
            .find(|component| component.id == request.component)
            .and_then(sound_script)
        else {
            return;
        };
        let mut name = request.name.clone().unwrap_or_else(|| "cue".into());
        if request.name.is_none() {
            for index in 2..=65 {
                if !cues.contains_key(&name) {
                    break;
                }
                name = format!("cue-{index}");
            }
        }
        let (asset, gain, range) = match cues.get(&name) {
            Some(PropertyValue::SoundRef(value)) => {
                (value.asset_id.clone(), value.gain, value.playback.range())
            }
            _ => (None, 1.0, None),
        };
        let choices = self.assets.as_ref().map_or_else(Vec::new, |store| {
            store
                .infos()
                .filter_map(|info| {
                    ready_sound(store, &info.id).map(|info| (info.id.clone(), info.source.clone()))
                })
                .collect()
        });
        self.sound_cue = Some(Draft {
            object: object.id.clone(),
            component: request.component,
            original: request.name,
            name,
            asset,
            gain,
            positional: range.is_some(),
            range: range.unwrap_or(10.0),
            revision: self.session.revision(),
            asset_revision: self.material_asset_revision(),
            choices,
            focus: true,
            error: None,
        });
    }

    pub(super) fn show_sound_cue(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.sound_cue.take() else {
            return;
        };
        let popup_before = egui::Popup::is_any_open(ctx);
        let mut cancel = false;
        let mut apply = false;
        let mut remove = false;
        egui::Modal::new(egui::Id::unique("sound-cue"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 410.0));
                ui.heading(if draft.original.is_some() {
                    "Edit Sound Cue"
                } else {
                    "Add Sound Cue"
                });
                let name_label = ui.label("Name");
                let name = ui
                    .add(egui::TextEdit::singleline(&mut draft.name).char_limit(128))
                    .labelled_by(name_label.id);
                if std::mem::take(&mut draft.focus) {
                    name.request_focus();
                }
                if (name.has_focus() || name.lost_focus())
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    })
                {
                    apply = true;
                }
                let wav = draft
                    .asset
                    .as_ref()
                    .map_or("Choose a WAV", |id| {
                        draft
                            .choices
                            .iter()
                            .find(|(choice, _)| choice == id)
                            .map_or(id.as_str(), |(_, path)| path.as_str())
                    })
                    .to_owned();
                let label = ui.label("WAV");
                let focus = super::super::combo_focus::ComboFocus::capture(ui, "cue-wav");
                let mut selected = false;
                let response = egui::ComboBox::from_id_salt("cue-wav")
                    .selected_text(wav)
                    .show_ui(ui, |ui| {
                        for (id, path) in &draft.choices {
                            if ui
                                .selectable_label(draft.asset.as_deref() == Some(id), path)
                                .clicked()
                            {
                                draft.asset = Some(id.clone());
                                selected = true;
                                ui.close();
                            }
                        }
                    });
                focus.restore(ui, &response.response, selected);
                response.response.labelled_by(label.id);
                let label = ui.label("Gain");
                let precise = ui.memory(|memory| memory.has_focus(ui.next_auto_id()));
                let gain = ui
                    .add(
                        egui::DragValue::new(&mut draft.gain)
                            .range(0.0..=1.0)
                            .speed(0.01)
                            .custom_formatter(move |value, decimals| {
                                if precise {
                                    value.to_string()
                                } else {
                                    egui::emath::format_with_decimals_in_range(value, decimals)
                                }
                            }),
                    )
                    .labelled_by(label.id);
                apply |= (gain.has_focus() || gain.lost_focus())
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                    });
                let label = ui.label("Playback");
                let focus = super::super::combo_focus::ComboFocus::capture(ui, "cue-playback");
                let mut selected = false;
                let response = egui::ComboBox::from_id_salt("cue-playback")
                    .selected_text(if draft.positional { "Positional" } else { "2D" })
                    .show_ui(ui, |ui| {
                        for (positional, text) in [(false, "2D"), (true, "Positional")] {
                            if ui
                                .selectable_label(draft.positional == positional, text)
                                .clicked()
                            {
                                draft.positional = positional;
                                selected = true;
                                ui.close();
                            }
                        }
                    });
                focus.restore(ui, &response.response, selected);
                response.response.labelled_by(label.id);
                if draft.positional {
                    let label = ui.label("Range");
                    let precise = ui.memory(|memory| memory.has_focus(ui.next_auto_id()));
                    let range = ui
                        .add(
                            egui::DragValue::new(&mut draft.range)
                                .speed(0.1)
                                .custom_formatter(move |value, decimals| {
                                    if precise {
                                        value.to_string()
                                    } else {
                                        egui::emath::format_with_decimals_in_range(value, decimals)
                                    }
                                }),
                        )
                        .labelled_by(label.id)
                        .on_hover_text("Scene units. Requires an active Transform Game Camera.");
                    apply |= (range.has_focus() || range.lost_focus())
                        && ui.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                        });
                }
                if rot_scene::validate_id(draft.name.trim()).is_ok() {
                    ui.monospace(format!("host.play_sound({:?})", draft.name.trim()));
                }
                if draft.choices.is_empty() {
                    ui.label("Import a WAV into Assets first.");
                }
                if let Some(error) = &draft.error {
                    ui.colored_label(editor_theme::colors(ui).error, error);
                }
                ui.horizontal_wrapped(|ui| {
                    apply |= editor_theme::primary_button(ui, "Apply").clicked();
                    if draft.original.is_some() {
                        remove = ui.button("Remove Cue").clicked();
                    }
                    cancel = ui.button("Cancel").clicked();
                });
                cancel |= !popup_before
                    && !egui::Popup::is_any_open(ctx)
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
            });
        if cancel {
            return;
        }
        if apply || remove {
            match self.commit_sound_cue(&draft, remove) {
                Ok(()) => return,
                Err(error) => draft.error = Some(error),
            }
        }
        self.sound_cue = Some(draft);
    }

    fn commit_sound_cue(&mut self, draft: &Draft, remove: bool) -> Result<(), String> {
        if draft.revision != self.session.revision() {
            return Err("The scene changed. Close and reopen this cue.".into());
        }
        let mut object = self
            .session
            .scene()
            .objects
            .iter()
            .find(|object| object.id == draft.object)
            .ok_or("The script owner is unavailable.")?
            .clone();
        let component = object
            .components
            .iter_mut()
            .find(|component| component.id == draft.component)
            .ok_or("The script component is unavailable.")?;
        let Component::Script {
            binding,
            source_kind: Some(ScriptSourceKind::Asset),
            ..
        } = &mut component.kind
        else {
            return Err("The file-backed script is unavailable.".into());
        };
        if !remove {
            if self.assets_loading() || draft.asset_revision != self.material_asset_revision() {
                return Err("Assets changed. Close and reopen this cue.".into());
            }
            let name = draft.name.trim();
            if binding.properties.contains_key(name) && draft.original.as_deref() != Some(name) {
                return Err(format!(
                    "A cue named {name:?} already exists on this script."
                ));
            }
            let value = PropertyValue::SoundRef(SoundRef {
                asset_id: draft.asset.clone(),
                gain: draft.gain,
                playback: if draft.positional {
                    SoundPlayback::Positional { range: draft.range }
                } else {
                    SoundPlayback::TwoD
                },
            });
            validate_cue(name, &value, self.assets.as_ref())?;
            if let Some(old) = &draft.original {
                binding.properties.remove(old);
            }
            binding.properties.insert(name.into(), value);
        } else if let Some(old) = &draft.original {
            binding.properties.remove(old);
        }
        self.session
            .put_object(draft.revision, object)
            .map_err(|error| error.to_string())?;
        self.error = None;
        self.refresh();
        Ok(())
    }
}
