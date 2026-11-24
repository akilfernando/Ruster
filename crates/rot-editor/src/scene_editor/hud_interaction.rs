//! One frozen HUD preparation, registered before viewport input and painted afterward.
use crate::behavior_ui::RuntimeUi;
use crate::behavior_worker::Frame;
use eframe::egui;
use rot_app::hud::{
    interact_hud_with_game_focus, prepare_hud, HudInteraction, HudInteractionState,
    HudPresentationMode, PreparedHud,
};
use rot_scene::hud::HudCache;
use rot_scene::Authoring;

#[derive(Default)]
pub(super) struct HudControls {
    pub(super) prepared: Option<PreparedHud>,
    interaction: HudInteractionState,
}

pub(super) struct HudViewport<'a> {
    pub rect: egui::Rect,
    pub frame: Option<&'a Frame>,
    pub authoring: &'a Authoring,
    pub scope: egui::Id,
    pub game_focus: Option<egui::Id>,
}

pub(super) fn interact(
    ui: &mut egui::Ui,
    view: HudViewport<'_>,
    cache: &mut HudCache,
    controls: &mut HudControls,
    runtime: &mut RuntimeUi,
    blocked: bool,
) -> (HudInteraction, Option<String>) {
    let hud = &view.authoring.hud;
    if (!hud.enabled
        || (hud.panels.len() <= 1 && !hud.elements().any(|row| row.source.is_button())))
        && !controls.interaction.has_input_lease()
    {
        controls.prepared = None;
        return (HudInteraction::default(), None);
    }
    let current = view
        .frame
        .is_some_and(|frame| !runtime.stale && runtime.current_run() == Some(frame.key));
    let resolved = cache.prepare(hud, view.authoring, |binding, field| {
        if let Some(frame) = view.frame {
            frame.state.binding(binding)?.get(field)
        } else {
            view.authoring
                .bindings
                .get(binding)?
                .state_defaults
                .get(field)
        }
    });
    let mode = if current {
        HudPresentationMode::Committed
    } else if view.frame.is_some() {
        HudPresentationMode::Stale
    } else {
        HudPresentationMode::AuthoredPreview
    };
    let mut prepared = prepare_hud(ui.painter(), view.rect, hud, resolved, mode);
    let mut input = interact_hud_with_game_focus(
        ui,
        &mut prepared,
        view.scope,
        current && !blocked && runtime.hud_controls_available(),
        runtime.hud_activation_busy(),
        &mut controls.interaction,
        view.game_focus,
    );
    controls.prepared = Some(prepared);
    let owned = input.pointer_owned || input.keyboard_owned;
    if owned && !input.keep_game_keyboard {
        runtime.release_input(ui.ctx());
    }
    if input.cancelled
        && (!current || blocked || !hud.elements().any(|row| row.source.action_id().is_some()))
    {
        runtime.cancel_native_hud_activation();
    }
    let error = if input.cancelled {
        None
    } else {
        input
            .activated
            .take()
            .and_then(|id| runtime.queue_hud_activation(&id, true).err())
    };
    (input, error)
}
