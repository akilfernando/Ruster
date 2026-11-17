//! Docked, tabbed panels (S-07a). Hierarchy, Scene, Game, Inspector, Assets
//! and Console are tabs in one `egui_tiles` tree. Declared from
//! `scene_editor.rs` so panes can reach the editor's private state.
use super::SceneEditor;
use eframe::egui;
use egui_tiles::{Container, Linear, LinearDir, SimplificationOptions, Tile, TileId, Tiles, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Pane {
    Hierarchy,
    Scene,
    Game,
    Inspector,
    Assets,
    Console,
}

impl Pane {
    pub fn title(self) -> &'static str {
        match self {
            Self::Hierarchy => "Hierarchy",
            Self::Scene => "Scene",
            Self::Game => "Game",
            Self::Inspector => "Inspector",
            Self::Assets => "Assets",
            Self::Console => "Console",
        }
    }
}

/// Unity's default arrangement: Hierarchy | Scene, Game | Inspector over
/// Assets, Console (UX-STANDARDS §3).
pub fn default_tree() -> Tree<Pane> {
    let mut tiles = Tiles::default();
    let hierarchy = tiles.insert_pane(Pane::Hierarchy);
    let scene = tiles.insert_pane(Pane::Scene);
    let game = tiles.insert_pane(Pane::Game);
    let inspector = tiles.insert_pane(Pane::Inspector);
    let assets = tiles.insert_pane(Pane::Assets);
    let console = tiles.insert_pane(Pane::Console);
    let left = tiles.insert_tab_tile(vec![hierarchy]);
    let views = tiles.insert_tab_tile(vec![scene, game]);
    let right = tiles.insert_tab_tile(vec![inspector]);
    let bottom = tiles.insert_tab_tile(vec![assets, console]);
    let mut top = Linear::new(LinearDir::Horizontal, vec![left, views, right]);
    top.shares.set_share(left, 0.14);
    top.shares.set_share(views, 0.59);
    top.shares.set_share(right, 0.27);
    let top = tiles.insert_new(Tile::Container(Container::Linear(top)));
    let mut root = Linear::new(LinearDir::Vertical, vec![top, bottom]);
    root.shares.set_share(top, 0.78);
    root.shares.set_share(bottom, 0.22);
    let root = tiles.insert_new(Tile::Container(Container::Linear(root)));
    Tree::new(egui::Id::unique("rot_dock"), root, tiles)
}

/// The tile showing `pane`, if the tree has one.
pub fn tile_of(tree: &Tree<Pane>, pane: Pane) -> Option<TileId> {
    tree.tiles.find_pane(&pane)
}

/// Where `pane` was drawn last frame.
#[cfg(test)]
pub fn pane_rect(tree: &Tree<Pane>, pane: Pane) -> Option<egui::Rect> {
    tile_of(tree, pane).and_then(|tile| tree.tiles.rect(tile))
}

pub struct Dock<'a> {
    pub editor: &'a mut SceneEditor,
    pub about_open: bool,
}

impl egui_tiles::Behavior<Pane> for Dock<'_> {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: TileId,
        pane: &mut Pane,
    ) -> egui_tiles::UiResponse {
        let editor = &mut *self.editor;
        let margin = crate::ui_kit::tokens::SPACE_S as i8;
        match pane {
            Pane::Scene | Pane::Game => {
                // One render surface: the active tab picks what it shows.
                let game = *pane == Pane::Game;
                if game && !editor.player_preview_available() {
                    egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                        let set_up = crate::ui_kit::empty_state(
                            ui,
                            "Set up the game camera to see what the player sees.",
                            Some("Game Camera…"),
                        );
                        if set_up.is_some_and(|response| response.clicked()) {
                            editor.open_player_view_editor(ui.ctx());
                        }
                    });
                } else {
                    if editor.navigation.player_view != game {
                        editor.set_player_preview(game);
                    }
                    editor.viewport_pane_ui(ui, self.about_open);
                }
            }
            Pane::Hierarchy => {
                egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("hierarchy_panel_scroll")
                        .show(ui, |ui| editor.hierarchy_contents(ui));
                });
            }
            Pane::Inspector => {
                egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                    editor.inspector_scroll_ui(ui);
                });
            }
            Pane::Assets => {
                egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("content_panel_scroll")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::focus::scroll_focus_into_view(ui);
                            editor.browser_contents(ui)
                        });
                });
            }
            Pane::Console => {
                egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                    if let Some(crate::console::Request::OpenSource(source, line)) =
                        editor.console.ui(ui)
                    {
                        let ctx = ui.ctx().clone();
                        editor.external_action(
                            &ctx,
                            &source,
                            crate::behavior_external::Intent::Open { line },
                        );
                    }
                });
            }
        }
        egui_tiles::UiResponse::None
    }

    fn tab_title_for_pane(&mut self, pane: &Pane) -> egui::WidgetText {
        let title = pane.title();
        let errors = self.editor.console.count(crate::console::Level::Error);
        if *pane == Pane::Console && errors > 0 {
            format!("{title} ({errors})").into()
        } else {
            title.into()
        }
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        crate::ui_kit::tokens::PANEL_TAB
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        2.0
    }

    fn min_size(&self) -> f32 {
        96.0
    }

    fn simplification_options(&self) -> SimplificationOptions {
        SimplificationOptions {
            all_panes_must_have_tabs: true,
            ..Default::default()
        }
    }

    /// Scene and Game share the one render surface, so they stay together.
    fn is_tile_draggable(&self, tiles: &Tiles<Pane>, tile_id: TileId) -> bool {
        !matches!(
            tiles.get(tile_id),
            Some(Tile::Pane(Pane::Scene | Pane::Game))
        )
    }

    fn tab_outline_stroke(
        &self,
        visuals: &egui::Visuals,
        _tiles: &Tiles<Pane>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Stroke {
        if state.active {
            egui::Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color)
        } else {
            egui::Stroke::NONE
        }
    }

    fn tab_bar_color(&self, visuals: &egui::Visuals) -> egui::Color32 {
        visuals.faint_bg_color
    }

    fn tab_bg_color(
        &self,
        visuals: &egui::Visuals,
        _tiles: &Tiles<Pane>,
        _tile_id: TileId,
        state: &egui_tiles::TabState,
    ) -> egui::Color32 {
        if state.active {
            visuals.panel_fill
        } else {
            visuals.faint_bg_color
        }
    }
}

impl SceneEditor {
    /// The dock below the toolbar: every panel in a tab (S-07a).
    pub(super) fn dock_ui(&mut self, ui: &mut egui::Ui, about_open: bool) {
        let mut tree = self.dock.take().unwrap_or_else(default_tree);
        // Window menu checkboxes show and hide panels.
        let prefs = &self.workspace.prefs;
        for (pane, visible) in [
            (Pane::Hierarchy, prefs.hierarchy_visible),
            (Pane::Inspector, prefs.inspector_visible),
            (Pane::Assets, prefs.content_visible),
            (Pane::Console, prefs.content_visible),
        ] {
            if let Some(tile) = tile_of(&tree, pane) {
                if tree.is_visible(tile) != visible {
                    tree.set_visible(tile, visible);
                }
            }
        }
        // The status bar's counts ask to show the Console.
        if std::mem::take(&mut self.console.open) {
            if let Some(tile) = tile_of(&tree, Pane::Console) {
                tree.make_active(|id, _| id == tile);
            }
        }
        self.dock_dragging = tree.dragged_id(ui.ctx()).is_some();
        tree.ui(
            &mut Dock {
                editor: self,
                about_open,
            },
            ui,
        );
        self.dock = Some(tree);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// S-07a: Tab skips the dock's divider strips and tab-bar backgrounds.
    #[test]
    fn tab_skips_dock_surfaces() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let frame = |editor: &mut SceneEditor, events: Vec<egui::Event>| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| editor.ui(ui),
            );
            output.textures_delta.clear();
        };
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![]);
        let tab = egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        for _ in 0..70 {
            frame(&mut editor, vec![tab.clone()]);
            frame(&mut editor, vec![]);
            let Some(response) = ctx
                .memory(|m| m.focused())
                .and_then(|id| ctx.read_response(id))
            else {
                continue;
            };
            let rect = response.rect;
            assert!(
                rect.width().min(rect.height()) > 6.0,
                "divider focused at {rect:?}"
            );
            assert!(
                !(rect.width() > 150.0
                    && (rect.height() - 28.0).abs() < 1.0
                    && rect.top() > 70.0
                    && rect.top() < 110.0),
                "tab bar focused at {rect:?}"
            );
        }
    }

    /// S-07a: dragging the Console tab onto the Inspector's tab bar moves it
    /// into that dock (ui_drive cannot drag, so this runs in process).
    #[test]
    fn dragging_console_into_the_inspector_dock() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let mut time = 0.0;
        let mut frame = |editor: &mut SceneEditor, events: Vec<egui::Event>| {
            time += 1.0 / 60.0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 800.0),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| editor.ui(ui),
            );
            output.textures_delta.clear();
        };
        frame(&mut editor, vec![]);
        frame(&mut editor, vec![]);
        let camera = editor.navigation.camera;
        let tree = editor.dock.as_ref().unwrap();
        let console = tile_of(tree, Pane::Console).unwrap();
        let inspector = tile_of(tree, Pane::Inspector).unwrap();
        let parent = |tree: &Tree<Pane>, tile| tree.tiles.parent_of(tile);
        assert_ne!(parent(tree, console), parent(tree, inspector));
        let tab = ctx
            .read_response(console.egui_id(tree.id()))
            .unwrap()
            .rect
            .center();
        let target = ctx
            .read_response(inspector.egui_id(tree.id()))
            .unwrap()
            .rect
            .right_center()
            + egui::vec2(40.0, 0.0);
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut editor,
            vec![egui::Event::PointerMoved(tab), button(tab, true)],
        );
        for step in 1..=20 {
            let pos = tab + (target - tab) * (step as f32 / 20.0);
            frame(&mut editor, vec![egui::Event::PointerMoved(pos)]);
        }
        frame(&mut editor, vec![button(target, false)]);
        frame(&mut editor, vec![]);
        let tree = editor.dock.as_ref().unwrap();
        let console = tile_of(tree, Pane::Console).unwrap();
        let inspector = tile_of(tree, Pane::Inspector).unwrap();
        assert_eq!(
            parent(tree, console),
            parent(tree, inspector),
            "Console now shares the Inspector's dock"
        );
        assert!(camera.is_some());
        assert_eq!(
            editor.navigation.camera, camera,
            "the camera survives a layout change"
        );
    }
}
