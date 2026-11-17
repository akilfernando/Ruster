//! Docked, tabbed panels (S-07a). Hierarchy, Scene, Game, Inspector, Assets
//! and Console are tabs in one `egui_tiles` tree. Declared from
//! `scene_editor.rs` so panes can reach the editor's private state.
use super::SceneEditor;
use eframe::egui;
use egui_tiles::{Container, Linear, LinearDir, SimplificationOptions, Tile, TileId, Tiles, Tree};
use std::path::{Path, PathBuf};

mod hierarchy;
mod inspector;
mod viewport;

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
    pub const ALL: [Self; 6] = [
        Self::Hierarchy,
        Self::Scene,
        Self::Game,
        Self::Inspector,
        Self::Assets,
        Self::Console,
    ];

    /// Scene and Game never close: they are the one render surface.
    pub fn closable(self) -> bool {
        !matches!(self, Self::Scene | Self::Game)
    }

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

/// The two layouts (S-07c). Each keeps its own arrangement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutKind {
    /// Unity's default: Hierarchy | Scene, Game | Inspector over Assets.
    Default,
    /// For a half-width window beside an AI app.
    SideBySide,
}

impl LayoutKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "Default",
            Self::SideBySide => "Side by Side",
        }
    }
    fn key(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::SideBySide => "side_by_side",
        }
    }
    fn tree(self) -> Tree<Pane> {
        match self {
            Self::Default => default_tree(),
            Self::SideBySide => side_by_side_tree(),
        }
    }
    fn other(self) -> Self {
        match self {
            Self::Default => Self::SideBySide,
            Self::SideBySide => Self::Default,
        }
    }
}

/// Below about 1100 px: the view over Assets on the left, Inspector and
/// Hierarchy sharing one dock on the right (UX-STANDARDS §3).
pub fn side_by_side_tree() -> Tree<Pane> {
    let mut tiles = Tiles::default();
    let scene = tiles.insert_pane(Pane::Scene);
    let game = tiles.insert_pane(Pane::Game);
    let assets = tiles.insert_pane(Pane::Assets);
    let console = tiles.insert_pane(Pane::Console);
    let inspector = tiles.insert_pane(Pane::Inspector);
    let hierarchy = tiles.insert_pane(Pane::Hierarchy);
    let views = tiles.insert_tab_tile(vec![scene, game]);
    let bottom = tiles.insert_tab_tile(vec![assets, console]);
    let right = tiles.insert_tab_tile(vec![inspector, hierarchy]);
    let mut left = Linear::new(LinearDir::Vertical, vec![views, bottom]);
    left.shares.set_share(views, 0.62);
    left.shares.set_share(bottom, 0.38);
    let left = tiles.insert_new(Tile::Container(Container::Linear(left)));
    let mut root = Linear::new(LinearDir::Horizontal, vec![left, right]);
    root.shares.set_share(left, 0.64);
    root.shares.set_share(right, 0.36);
    let root = tiles.insert_new(Tile::Container(Container::Linear(root)));
    Tree::new(egui::Id::unique("rot_dock_side_by_side"), root, tiles)
}

/// What layout.json holds.
pub struct SavedLayouts {
    pub active: LayoutKind,
    pub default: Tree<Pane>,
    pub side_by_side: Tree<Pane>,
    /// The file existed and was read.
    pub loaded: bool,
}

/// `layout.json` sits beside the preferences file (S-07b).
pub fn layout_path(state: Option<&Path>) -> Option<PathBuf> {
    Some(state?.parent()?.join("layout.json"))
}

/// Each pane exactly once, as panes.
fn complete(tree: &Tree<Pane>) -> bool {
    Pane::ALL.iter().all(|pane| {
        tree.tiles
            .iter()
            .filter(|(_, tile)| matches!(tile, Tile::Pane(found) if found == pane))
            .count()
            == 1
    })
}

/// The saved layouts, or the defaults with a warning when the file is
/// unreadable or damaged. A missing file is not a problem.
pub fn load_layouts(path: Option<&Path>) -> (SavedLayouts, Option<String>) {
    let fresh = |loaded| SavedLayouts {
        active: LayoutKind::Default,
        default: default_tree(),
        side_by_side: side_by_side_tree(),
        loaded,
    };
    let Some(path) = path else {
        return (fresh(false), None);
    };
    let bytes = match rot_assets::read_regular_file(path, 1024 * 1024) {
        Ok(bytes) => bytes,
        Err(_) if path.symlink_metadata().is_err() => return (fresh(false), None),
        Err(error) => {
            return (
                fresh(false),
                Some(format!(
                    "Couldn't read the panel layout; using the default. {error}"
                )),
            )
        }
    };
    let value = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .filter(|value| value["format"] == "rot-editor-layout" && value["version"] == 1);
    let tree = |kind: LayoutKind| {
        value.as_ref().and_then(|value| {
            serde_json::from_value::<Tree<Pane>>(value["layouts"][kind.key()].clone())
                .ok()
                .filter(complete)
        })
    };
    let Some(default) = tree(LayoutKind::Default) else {
        return (
            fresh(false),
            Some("The saved panel layout is damaged; using the default layout.".into()),
        );
    };
    let active = match value.as_ref().map(|value| value["active"].clone()) {
        Some(serde_json::Value::String(name)) if name == "side_by_side" => LayoutKind::SideBySide,
        _ => LayoutKind::Default,
    };
    (
        SavedLayouts {
            active,
            default,
            side_by_side: tree(LayoutKind::SideBySide).unwrap_or_else(side_by_side_tree),
            loaded: true,
        },
        None,
    )
}

/// Write both layouts by temporary file and rename.
pub fn save_layouts(
    path: &Path,
    active: LayoutKind,
    default: &Tree<Pane>,
    side_by_side: &Tree<Pane>,
) -> std::io::Result<()> {
    let value = serde_json::json!({
        "format": "rot-editor-layout",
        "version": 1,
        "active": active.key(),
        "layouts": { "default": default, "side_by_side": side_by_side },
    });
    let bytes = serde_json::to_vec_pretty(&value)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)
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
        // Window, Find and F6 focus a pane's first control: an anchor takes
        // focus for one frame, then Tab-order moves on to the next widget.
        if let Some((target, armed)) = editor.pane_focus {
            if target == *pane {
                // Stages follow frames, not passes: egui may lay a frame out twice.
                let frame = ui.ctx().cumulative_frame_nr();
                // An explicit id: allocating would shift the pane's auto ids
                // and the focused control would lose focus next frame.
                let anchor = ui.interact(
                    egui::Rect::from_min_size(ui.cursor().min, egui::Vec2::ZERO),
                    ui.scope_id().with("rot_pane_focus_anchor"),
                    egui::Sense::FOCUSABLE,
                );
                match armed {
                    None => {
                        anchor.request_focus();
                        editor.pane_focus = Some((target, Some(frame)));
                        crate::focus::move_on_next_frame(ui.ctx());
                    }
                    Some(armed_frame) if frame > armed_frame => editor.pane_focus = None,
                    Some(_) => {}
                }
            }
        }
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
                    let rect = ui.max_rect();
                    editor.viewport_pane_ui(ui, self.about_open);
                    editor.accept_model_drop(ui, rect);
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

    fn is_tab_closable(&self, tiles: &Tiles<Pane>, tile_id: TileId) -> bool {
        matches!(tiles.get(tile_id), Some(Tile::Pane(pane)) if pane.closable())
    }

    /// Closing hides the tab in place, so Window brings it back there.
    fn on_tab_close(&mut self, tiles: &mut Tiles<Pane>, tile_id: TileId) -> bool {
        tiles.set_visible(tile_id, false);
        self.editor.dock_dirty = true;
        false
    }

    fn on_edit(&mut self, _edit_action: egui_tiles::EditAction) {
        self.editor.dock_dirty = true;
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
        self.ensure_dock();
        self.follow_window_width(ui.ctx().content_rect().width());
        // The status bar's counts ask to show the Console.
        if std::mem::take(&mut self.console.open) {
            self.show_pane(Pane::Console, false);
        }
        let mut tree = self.dock.take().unwrap_or_else(default_tree);
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

    /// Load the saved layouts once.
    fn ensure_dock(&mut self) {
        if self.dock.is_some() {
            return;
        }
        let path = layout_path(self.workspace.state_path());
        let (saved, warning) = load_layouts(path.as_deref());
        if let Some(warning) = warning {
            self.console
                .push(crate::console::Level::Warning, "Layout", warning);
        }
        self.layout_loaded = saved.loaded;
        self.active_layout = saved.active;
        let (active, other) = match saved.active {
            LayoutKind::Default => (saved.default, saved.side_by_side),
            LayoutKind::SideBySide => (saved.side_by_side, saved.default),
        };
        self.dock = Some(active);
        self.other_dock = Some(other);
        if let Some(kind) = self.layout_override.take() {
            self.switch_layout(kind);
            self.layout_loaded = true;
        }
    }

    /// Show the other layout; each keeps its own arrangement.
    pub(super) fn switch_layout(&mut self, kind: LayoutKind) {
        self.ensure_dock();
        if kind == self.active_layout {
            return;
        }
        std::mem::swap(&mut self.dock, &mut self.other_dock);
        if self.dock.is_none() {
            self.dock = Some(kind.tree());
        }
        self.active_layout = kind;
        self.dock_dirty = true;
    }

    /// Crossing 1100 px of window width picks Side by Side below and
    /// Default above. A manual choice holds until the next crossing.
    fn follow_window_width(&mut self, width: f32) {
        let wide = width >= 1100.0;
        let first = self.layout_band.is_none();
        if self.layout_band != Some(wide) {
            self.layout_band = Some(wide);
            // At startup a saved or requested layout keeps its own choice.
            if !(first && self.layout_loaded) {
                self.switch_layout(if wide {
                    LayoutKind::Default
                } else {
                    LayoutKind::SideBySide
                });
            }
        }
    }

    pub(super) fn pane_visible(&self, pane: Pane) -> bool {
        self.dock
            .as_ref()
            .is_none_or(|tree| tile_of(tree, pane).is_some_and(|tile| tree.is_visible(tile)))
    }

    /// Show `pane`'s tab, bring it to the front, and optionally focus its
    /// first control.
    pub(super) fn show_pane(&mut self, pane: Pane, focus: bool) {
        self.ensure_dock();
        let tree = self.dock.as_mut().unwrap();
        if let Some(tile) = tile_of(tree, pane) {
            tree.set_visible(tile, true);
            tree.make_active(|id, _| id == tile);
            self.dock_dirty = true;
            if focus {
                self.pane_focus = Some((pane, None));
            }
        }
    }

    pub(super) fn hide_pane(&mut self, pane: Pane) {
        if !pane.closable() {
            return;
        }
        self.ensure_dock();
        let tree = self.dock.as_mut().unwrap();
        if let Some(tile) = tile_of(tree, pane) {
            tree.set_visible(tile, false);
            self.dock_dirty = true;
        }
    }

    pub(super) fn reset_layout(&mut self) {
        self.dock = Some(self.active_layout.tree());
        self.dock_dirty = true;
    }

    /// Write layout.json if the layout changed.
    pub(super) fn flush_layout(&mut self) {
        if !std::mem::take(&mut self.dock_dirty) {
            return;
        }
        let (Some(active), Some(path)) = (&self.dock, layout_path(self.workspace.state_path()))
        else {
            return;
        };
        let other = self
            .other_dock
            .clone()
            .unwrap_or_else(|| self.active_layout.other().tree());
        let (default, side_by_side) = match self.active_layout {
            LayoutKind::Default => (active, &other),
            LayoutKind::SideBySide => (&other, active),
        };
        if let Err(error) = save_layouts(&path, self.active_layout, default, side_by_side) {
            self.console.push(
                crate::console::Level::Warning,
                "Layout",
                format!("Couldn't save the panel layout: {error}"),
            );
        }
    }

    /// F6 / Shift+F6: the next visible dock in reading order, focusing the
    /// first control of its active tab.
    pub(super) fn cycle_panel(&mut self, forward: bool) {
        let Some(tree) = &self.dock else {
            return;
        };
        let mut docks: Vec<(egui::Rect, Pane)> = tree
            .tiles
            .iter()
            .filter(|(id, _)| tree.is_visible(**id))
            .filter_map(|(id, tile)| match tile {
                Tile::Container(Container::Tabs(tabs)) => {
                    let active = tabs.active.filter(|active| tree.is_visible(*active))?;
                    match tree.tiles.get(active) {
                        Some(Tile::Pane(pane)) => Some((tree.tiles.rect(*id)?, *pane)),
                        _ => None,
                    }
                }
                _ => None,
            })
            .collect();
        if docks.is_empty() {
            return;
        }
        docks.sort_by(|a, b| {
            (a.0.top().round(), a.0.left().round())
                .partial_cmp(&(b.0.top().round(), b.0.left().round()))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let focused = self
            .export_context
            .as_ref()
            .and_then(|ctx| {
                ctx.memory(|m| m.focused())
                    .and_then(|id| ctx.read_response(id))
            })
            .map(|response| response.rect.center());
        let current =
            focused.and_then(|point| docks.iter().position(|(rect, _)| rect.contains(point)));
        let count = docks.len();
        let next = match (current, forward) {
            (Some(index), true) => (index + 1) % count,
            (Some(index), false) => (index + count - 1) % count,
            (None, true) => 0,
            (None, false) => count - 1,
        };
        self.pane_focus = Some((docks[next].1, None));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(editor: &mut SceneEditor, ctx: &egui::Context, events: Vec<egui::Event>) {
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
    }

    fn run_at(editor: &mut SceneEditor, ctx: &egui::Context, size: egui::Vec2) {
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            },
            |ui| editor.ui(ui),
        );
        output.textures_delta.clear();
    }

    /// S-07c: crossing 1100 px switches layouts, each keeps its own
    /// arrangement, both survive a restart, and the narrow view is roomy.
    #[test]
    fn layouts_follow_window_width_and_keep_their_arrangement() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("editor.json");
        let session = || rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let wide = egui::vec2(1280.0, 800.0);
        let narrow = egui::vec2(960.0, 1080.0);
        let mut editor = SceneEditor::with_workspace(
            session(),
            crate::workspace::Workspace::at_for_test(state.clone()),
        );
        run_at(&mut editor, &ctx, wide);
        assert_eq!(editor.active_layout, LayoutKind::Default);
        editor.hide_pane(Pane::Hierarchy);
        run_at(&mut editor, &ctx, narrow);
        run_at(&mut editor, &ctx, narrow);
        assert_eq!(editor.active_layout, LayoutKind::SideBySide);
        assert!(editor.pane_visible(Pane::Hierarchy), "its own arrangement");
        let view = editor.captures.rect.unwrap();
        assert!(
            view.width() >= 560.0 && view.height() >= 560.0,
            "view {view:?}"
        );
        editor.hide_pane(Pane::Console);
        run_at(&mut editor, &ctx, wide);
        assert_eq!(editor.active_layout, LayoutKind::Default);
        assert!(
            !editor.pane_visible(Pane::Hierarchy),
            "Default kept its change"
        );
        // A manual choice holds until the next crossing.
        editor.switch_layout(LayoutKind::SideBySide);
        run_at(&mut editor, &ctx, wide);
        assert_eq!(editor.active_layout, LayoutKind::SideBySide);
        assert!(editor.pane_visible(Pane::Hierarchy));
        editor.flush_layout();

        let mut reopened =
            SceneEditor::with_workspace(session(), crate::workspace::Workspace::at_for_test(state));
        run_at(&mut reopened, &ctx, wide);
        assert_eq!(
            reopened.active_layout,
            LayoutKind::SideBySide,
            "saved choice"
        );
        reopened.switch_layout(LayoutKind::Default);
        run_at(&mut reopened, &ctx, wide);
        assert!(!reopened.pane_visible(Pane::Hierarchy), "Default restored");
        reopened.switch_layout(LayoutKind::SideBySide);
        run_at(&mut reopened, &ctx, wide);
        assert!(
            !reopened.pane_visible(Pane::Console),
            "Side by Side restored"
        );
    }

    /// S-07b: a closed panel reopens in the same dock, the layout survives a
    /// restart, and a damaged file falls back to the default with a warning.
    #[test]
    fn layout_saves_reopens_and_survives_damage() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("editor.json");
        let session = || rot_scene::Session::new(rot_scene::Scene::demo()).unwrap();
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        let mut editor = SceneEditor::with_workspace(
            session(),
            crate::workspace::Workspace::at_for_test(state.clone()),
        );
        run(&mut editor, &ctx, vec![]);
        let tree = editor.dock.as_ref().unwrap();
        let parent = tree
            .tiles
            .parent_of(tile_of(tree, Pane::Hierarchy).unwrap());
        editor.hide_pane(Pane::Hierarchy);
        run(&mut editor, &ctx, vec![]);
        assert!(
            !editor.pane_visible(Pane::Hierarchy),
            "Window shows it unchecked"
        );
        editor.flush_layout();
        let layout = layout_path(Some(&state)).unwrap();
        assert!(layout.is_file());

        let mut reopened = SceneEditor::with_workspace(
            session(),
            crate::workspace::Workspace::at_for_test(state.clone()),
        );
        run(&mut reopened, &ctx, vec![]);
        assert!(
            !reopened.pane_visible(Pane::Hierarchy),
            "closed after restart"
        );
        reopened.show_pane(Pane::Hierarchy, true);
        run(&mut reopened, &ctx, vec![]);
        let tree = reopened.dock.as_ref().unwrap();
        assert!(reopened.pane_visible(Pane::Hierarchy));
        assert_eq!(
            tree.tiles
                .parent_of(tile_of(tree, Pane::Hierarchy).unwrap()),
            parent,
            "reopens in the same dock"
        );

        std::fs::write(&layout, b"{not json").unwrap();
        let mut damaged =
            SceneEditor::with_workspace(session(), crate::workspace::Workspace::at_for_test(state));
        run(&mut damaged, &ctx, vec![]);
        assert!(damaged.pane_visible(Pane::Hierarchy), "default layout");
        assert_eq!(damaged.console.count(crate::console::Level::Warning), 1);
    }

    /// S-07b: F6 visits each dock once in reading order and wraps.
    #[test]
    fn f6_visits_each_dock_once_and_wraps() {
        let mut editor =
            SceneEditor::new(rot_scene::Session::new(rot_scene::Scene::demo()).unwrap());
        let ctx = egui::Context::default();
        crate::editor_theme::configure(&ctx);
        run(&mut editor, &ctx, vec![]);
        run(&mut editor, &ctx, vec![]);
        let f6 = egui::Event::Key {
            key: egui::Key::F6,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut visited = Vec::new();
        for _ in 0..5 {
            run(&mut editor, &ctx, vec![f6.clone()]);
            for _ in 0..3 {
                run(&mut editor, &ctx, vec![]);
            }
            let center = ctx
                .memory(|m| m.focused())
                .and_then(|id| ctx.read_response(id))
                .expect("F6 focuses a control")
                .rect
                .center();
            let tree = editor.dock.as_ref().unwrap();
            let pane = Pane::ALL
                .into_iter()
                .find(|pane| {
                    tile_of(tree, *pane)
                        .and_then(|tile| tree.tiles.rect(tile))
                        .is_some_and(|rect| {
                            tree.is_visible(tile_of(tree, *pane).unwrap()) && rect.contains(center)
                        })
                })
                .expect("focus inside a pane");
            visited.push(pane);
        }
        assert_eq!(
            visited,
            vec![
                Pane::Hierarchy,
                Pane::Scene,
                Pane::Inspector,
                Pane::Assets,
                Pane::Hierarchy
            ]
        );
    }

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
