//! The Assets pane: a grid (or list) of thumbnails to drag from, with search,
//! a size slider and a context menu per asset. Files dropped from the file
//! manager, or picked with Import…, are copied into the project's `Assets`
//! folder and imported under an ID taken from the file name, with no form.
use super::*;
use std::collections::{BTreeSet, VecDeque};
use std::path::Path;
use std::sync::mpsc;

/// File types the importer reads.
const IMPORTABLE: [&str; 4] = ["glb", "obj", "png", "wav"];

/// A placed model dragged from Assets onto the Scene view.
#[derive(Clone)]
pub(super) struct ModelDrag {
    pub(super) asset_id: String,
    catalog_revision: u64,
    generation: u64,
}

pub(super) struct AssetsView {
    pub(super) list: bool,
    pub(super) size: f32,
    thumbnails: BTreeMap<String, (usize, egui::TextureHandle)>,
    /// Files waiting to be imported, one at a time.
    queue: VecDeque<PathBuf>,
    picking: Option<mpsc::Receiver<Vec<PathBuf>>>,
    /// Set when an asset was clicked: the Inspector shows it while the object
    /// selection stays what it was then.
    pub(super) inspect: Option<Option<String>>,
    /// Inspection builds: ROT_INSPECTION_DROP was delivered (scripted checks
    /// cannot send a file-manager drop through the inspection socket).
    #[cfg(feature = "inspection")]
    inspection_dropped: bool,
}
impl Default for AssetsView {
    fn default() -> Self {
        Self {
            list: false,
            size: 48.0,
            thumbnails: BTreeMap::new(),
            queue: VecDeque::new(),
            picking: None,
            inspect: None,
            #[cfg(feature = "inspection")]
            inspection_dropped: false,
        }
    }
}

#[derive(Clone)]
enum Item {
    Resource(usize),
    Model { id: String, clips: usize },
}

/// An asset ID from a file name: letters, digits, `_`, `-`, `.`; lower case.
fn id_from_name(stem: &str) -> String {
    let mut id: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "_-.".contains(c) {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    id.truncate(100);
    let id = id.trim_matches('_').to_string();
    if id.is_empty() {
        "asset".into()
    } else {
        id
    }
}

impl SceneEditor {
    fn asset_ids(&self) -> BTreeSet<String> {
        let Some(store) = &self.assets else {
            return BTreeSet::new();
        };
        store
            .inspect()
            .into_iter()
            .map(|info| info.id)
            .chain(store.models().map(|model| model.id.clone()))
            .collect()
    }

    /// Copy a file into `<project>/Assets` (unless it is already inside the
    /// project) and return its project-relative source.
    fn project_source(&self, path: &Path) -> Result<String, String> {
        let store = self
            .assets
            .as_ref()
            .ok_or("Save the scene first so it has a project folder.")?;
        let root = store.root();
        let file = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if let Ok(relative) = file.strip_prefix(root) {
            return Ok(relative.to_string_lossy().replace('\\', "/"));
        }
        let name = file
            .file_name()
            .ok_or("Not a file")?
            .to_string_lossy()
            .into_owned();
        let folder = root.join("Assets");
        std::fs::create_dir_all(&folder).map_err(|e| format!("{}: {e}", folder.display()))?;
        let (stem, extension) = name.rsplit_once('.').unwrap_or((&name, ""));
        let bytes = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        for n in 1.. {
            let leaf = if n == 1 {
                name.clone()
            } else {
                format!("{stem} {n}.{extension}")
            };
            let target = folder.join(&leaf);
            match std::fs::read(&target) {
                Ok(existing) if existing == bytes => return Ok(format!("Assets/{leaf}")),
                Ok(_) => continue,
                Err(_) => {
                    std::fs::write(&target, &bytes)
                        .map_err(|e| format!("{}: {e}", target.display()))?;
                    return Ok(format!("Assets/{leaf}"));
                }
            }
        }
        unreachable!()
    }

    /// Queue files for import; unsupported types are reported in the Console.
    pub(super) fn queue_imports(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            let supported = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| IMPORTABLE.contains(&e.to_ascii_lowercase().as_str()));
            if supported {
                self.assets_view.queue.push_back(path);
            } else {
                self.console.push(
                    crate::console::Level::Warning,
                    "Assets",
                    format!(
                        "Not imported: {} (Rot imports .glb, .obj, .png and .wav)",
                        path.display()
                    ),
                );
            }
        }
    }

    /// Files dropped on the window, Import… results, and the import queue.
    pub(super) fn assets_frame(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if !dropped.is_empty() {
            self.queue_imports(dropped);
        }
        #[cfg(feature = "inspection")]
        if !self.assets_view.inspection_dropped && self.assets.is_some() {
            if let Some(paths) = std::env::var_os("ROT_INSPECTION_DROP") {
                self.assets_view.inspection_dropped = true;
                self.queue_imports(std::env::split_paths(&paths).collect::<Vec<_>>());
            }
        }
        if let Some(picked) = self
            .assets_view
            .picking
            .as_ref()
            .and_then(|r| r.try_recv().ok())
        {
            self.assets_view.picking = None;
            self.queue_imports(picked);
        }
        if self.assets_view.queue.is_empty() || self.asset_ops.busy() || self.guard().is_err() {
            return;
        }
        let path = self.assets_view.queue.pop_front().unwrap();
        let source = match self.project_source(&path) {
            Ok(source) => source,
            Err(error) => {
                self.console.push(
                    crate::console::Level::Error,
                    "Assets",
                    format!("Not imported: {error}"),
                );
                return;
            }
        };
        let taken = self.asset_ids();
        let stem = path
            .file_stem()
            .map_or_else(|| "asset".into(), |s| s.to_string_lossy().into_owned());
        let base = id_from_name(&stem);
        let id = (1..)
            .map(|n| {
                if n == 1 {
                    base.clone()
                } else {
                    format!("{base}_{n}")
                }
            })
            .find(|id| !taken.contains(id))
            .unwrap();
        self.asset_normal_policy = rot_assets::MeshNormalPolicy::AuthoredWhenComplete;
        self.asset_texture_role = rot_assets::TextureRole::BaseColor;
        self.content_selected = Some(id.clone());
        self.start_human_asset(ctx, "asset_import", &id, &source, false);
        ctx.request_repaint();
    }

    /// Import… opens the system file dialog.
    pub(super) fn pick_imports(&mut self) {
        let (send, receive) = mpsc::sync_channel(1);
        let ctx = self.export_context.clone().unwrap_or_default();
        std::thread::spawn(move || {
            let picked = rfd::FileDialog::new()
                .set_title("Import Assets")
                .add_filter("Assets", &IMPORTABLE)
                .pick_files()
                .unwrap_or_default();
            let _ = send.send(picked);
            ctx.request_repaint();
        });
        self.assets_view.picking = Some(receive);
    }

    fn thumbnail(&mut self, ctx: &egui::Context, id: &str) -> Option<egui::TextureId> {
        let texture = self.resolved_textures.get(id)?.clone();
        let key = Arc::as_ptr(&texture) as usize;
        if let Some((seen, handle)) = self.assets_view.thumbnails.get(id) {
            if *seen == key {
                return Some(handle.id());
            }
        }
        // The smallest level at least 64 pixels wide keeps uploads small.
        let mip = texture
            .mips
            .iter()
            .rev()
            .find(|mip| mip.width >= 64)
            .unwrap_or(&texture.mips[0]);
        let size = [mip.width as usize, mip.height as usize];
        let image = match texture.role {
            rot_assets::TextureRole::BaseColor => {
                egui::ColorImage::from_rgba_unmultiplied(size, &mip.bytes)
            }
            rot_assets::TextureRole::Roughness => egui::ColorImage::from_gray(size, &mip.bytes),
        };
        let handle = ctx.load_texture(
            format!("asset-thumb-{id}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        let texture_id = handle.id();
        self.assets_view
            .thumbnails
            .insert(id.to_owned(), (key, handle));
        Some(texture_id)
    }

    fn items(&self) -> Vec<Item> {
        let query = self.content_query.to_lowercase();
        let models = self
            .assets
            .iter()
            .flat_map(|store| store.models())
            .filter(|m| {
                query.is_empty()
                    || m.id.to_lowercase().contains(&query)
                    || m.source.to_lowercase().contains(&query)
            })
            .map(|m| Item::Model {
                id: m.id.clone(),
                clips: m.clips.len(),
            });
        models
            .chain(
                self.content_search
                    .visible
                    .iter()
                    .map(|i| Item::Resource(*i)),
            )
            .collect()
    }

    fn item_id(&self, item: &Item) -> String {
        match item {
            Item::Resource(i) => self.content_search.rows[*i].info.id.clone(),
            Item::Model { id, .. } => id.clone(),
        }
    }

    fn item_kind(&self, item: &Item) -> &'static str {
        match item {
            Item::Model { clips: 0, .. } => "Model",
            Item::Model { .. } => "Animated model",
            Item::Resource(i) => match self.content_search.rows[*i].info.kind {
                rot_assets::AssetKind::Mesh => "Mesh",
                rot_assets::AssetKind::Sound => "Sound",
                rot_assets::AssetKind::Texture => "Texture",
            },
        }
    }

    fn item_glyph(kind: &str) -> &'static str {
        match kind {
            "Mesh" => "▲",
            "Sound" => "♪",
            "Texture" => "▦",
            _ => "◆",
        }
    }

    /// The Assets pane.
    pub(super) fn browser_contents(&mut self, ui: &mut egui::Ui) {
        let search = ui
            .horizontal(|ui| {
                if crate::editor_theme::primary_button(ui, "Import…")
                    .on_hover_text("Import .glb, .obj, .png or .wav files, or drop them here")
                    .clicked()
                {
                    self.pick_imports();
                }
                ui.menu_button("Tools", |ui| self.shader_tools_ui(ui))
                    .response
                    .on_hover_text("Shader and content tools");
                ui.selectable_value(&mut self.assets_view.list, false, "Grid");
                ui.selectable_value(&mut self.assets_view.list, true, "List");
                if !self.assets_view.list {
                    ui.add(
                        egui::Slider::new(&mut self.assets_view.size, 48.0..=160.0)
                            .show_value(false),
                    )
                    .on_hover_text("Thumbnail size");
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.content_query)
                        .hint_text("Search")
                        .char_limit(128)
                        .desired_width(ui.available_width().clamp(80.0, 240.0)),
                )
            })
            .inner;
        if self.content_focus {
            search.request_focus();
            self.content_focus = false;
        }
        if search.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.content_query.clear();
            search.surrender_focus();
        }
        self.content_search
            .update(self.assets.as_ref(), &self.content_query);
        self.content_diagnostics_ui(ui);
        if self.asset_loading() {
            ui.label("Loading project content…");
            return;
        }
        if self.assets.is_none() {
            if let Some(root) = self.asset_origin.clone() {
                ui.label(format!("Project content origin: {}", root.display()));
                if ui.button("Retry").clicked() {
                    if let Err(error) = self.retry_asset_open() {
                        self.error = error;
                    }
                }
            }
            crate::ui_kit::empty_state(
                ui,
                "Save the scene to import assets into its project.",
                None,
            );
            return;
        }
        let items = self.items();
        if items.is_empty() {
            let line = if self.content_query.is_empty() {
                "Drop .glb, .obj, .png or .wav files here to import them."
            } else {
                "No assets match this search."
            };
            crate::ui_kit::empty_state(ui, line, None);
            return;
        }
        let mut clicked = None;
        let mut action = None;
        // A pane too short for one row of tiles shows the list instead.
        let tile_height =
            self.assets_view.size + ui.text_style_height(&egui::TextStyle::Small) + 4.0;
        if self.assets_view.list || ui.available_height() < tile_height {
            for item in &items {
                let (response, act) = self.asset_row(ui, item);
                if response.clicked() {
                    clicked = Some(self.item_id(item));
                }
                action = action.or(act);
            }
        } else {
            let (tile_clicked, tile_action) = ui
                .horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    let mut clicked = None;
                    let mut action = None;
                    for item in &items {
                        let (response, act) = self.asset_tile(ui, item);
                        if response.clicked() {
                            clicked = Some(self.item_id(item));
                        }
                        action = action.or(act);
                    }
                    (clicked, action)
                })
                .inner;
            clicked = tile_clicked;
            action = tile_action;
        }
        if let Some(id) = clicked {
            self.content_selected = Some(id);
            self.assets_view.inspect = Some(self.selected.clone());
        }
        if let Some(action) = action {
            self.asset_action(action);
        }
    }

    fn drag_and_menu(&mut self, response: &egui::Response, item: &Item) -> Option<AssetAction> {
        let id = self.item_id(item);
        if response.drag_started() {
            match item {
                Item::Model { .. } => response.dnd_set_drag_payload(ModelDrag {
                    asset_id: id.clone(),
                    catalog_revision: self.assets.as_ref().map_or(0, AssetStore::revision),
                    generation: self.generation,
                }),
                Item::Resource(_) => match self.asset_payload(&id) {
                    Ok(payload) => response.dnd_set_drag_payload(payload),
                    Err(error) => self.error = error,
                },
            }
        }
        let mut action = None;
        let kind = self.item_kind(item);
        let has_target = self.selected.is_some();
        response.context_menu(|ui| {
            match kind {
                "Model" | "Animated model" => {
                    if ui.button("Place in Scene").clicked() {
                        action = Some(AssetAction::Place(id.clone()));
                    }
                }
                "Mesh" => {
                    if ui
                        .add_enabled(has_target, egui::Button::new("Assign to Selected Object"))
                        .clicked()
                    {
                        action = Some(AssetAction::Assign(id.clone()));
                    }
                }
                "Texture" => {
                    if ui
                        .add_enabled(
                            has_target,
                            egui::Button::new("Use on Selected Object's Material"),
                        )
                        .clicked()
                    {
                        action = Some(AssetAction::Texture(id.clone()));
                    }
                }
                _ => {}
            }
            if ui.button("Reimport").clicked() {
                action = Some(AssetAction::Refresh(id.clone()));
            }
            if matches!(item, Item::Resource(_)) {
                if ui.button("Rename…").clicked() {
                    action = Some(AssetAction::Rename(id.clone()));
                }
                if ui.button("Select Objects Using It").clicked() {
                    action = Some(AssetAction::References(id.clone()));
                }
            }
            if action.is_some() {
                ui.close();
            }
        });
        action
    }

    fn asset_tile(
        &mut self,
        ui: &mut egui::Ui,
        item: &Item,
    ) -> (egui::Response, Option<AssetAction>) {
        let id = self.item_id(item);
        let kind = self.item_kind(item);
        let size = self.assets_view.size;
        let label_height = ui.text_style_height(&egui::TextStyle::Small) + 4.0;
        // Names get at least 80 points even when thumbnails are small.
        let width = size.max(80.0);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(width, size + label_height),
            egui::Sense::click_and_drag(),
        );
        let hover = match item {
            Item::Resource(i) => self.content_search.rows[*i].label.clone(),
            Item::Model { .. } => format!("{id} · {kind}"),
        };
        let response = response.on_hover_text(hover);
        let selected = self.content_selected.as_deref() == Some(id.as_str());
        let palette = crate::editor_theme::colors(ui);
        let image_rect = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.min.y + size / 2.0),
            egui::vec2(size, size),
        );
        let fill = if selected {
            palette.accent.gamma_multiply(0.35)
        } else {
            palette.raised
        };
        ui.painter().rect_filled(image_rect, 4.0, fill);
        if response.hovered() || response.has_focus() {
            ui.painter().rect_stroke(
                image_rect,
                4.0,
                egui::Stroke::new(1.0, palette.accent),
                egui::StrokeKind::Inside,
            );
        }
        let thumbnail = (kind == "Texture")
            .then(|| self.thumbnail(ui.ctx(), &id))
            .flatten();
        if let Some(texture) = thumbnail {
            let inner = image_rect.shrink(4.0);
            ui.painter().image(
                texture,
                inner,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            ui.painter().text(
                image_rect.center(),
                egui::Align2::CENTER_CENTER,
                Self::item_glyph(kind),
                egui::FontId::proportional(size * 0.4),
                palette.secondary,
            );
        }
        // One line, cut with an ellipsis, like Unity's Project window.
        let mut job = egui::text::LayoutJob::simple_singleline(
            id.clone(),
            egui::FontId::proportional(ui.text_style_height(&egui::TextStyle::Small)),
            ui.visuals().text_color(),
        );
        job.wrap = egui::text::TextWrapping {
            max_width: width,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        };
        let galley = ui.painter().layout_job(job);
        let job_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, image_rect.max.y + 2.0),
            egui::vec2(width, label_height),
        );
        let text_left = job_rect.center().x - galley.size().x / 2.0;
        ui.painter().with_clip_rect(job_rect).galley(
            egui::pos2(text_left, job_rect.min.y),
            galley,
            ui.visuals().text_color(),
        );
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &id));
        let action = self.drag_and_menu(&response, item);
        (response, action)
    }

    fn asset_row(
        &mut self,
        ui: &mut egui::Ui,
        item: &Item,
    ) -> (egui::Response, Option<AssetAction>) {
        let id = self.item_id(item);
        let kind = self.item_kind(item);
        let selected = self.content_selected.as_deref() == Some(id.as_str());
        let text = match item {
            Item::Resource(i) => self.content_search.rows[*i].label.clone(),
            Item::Model { .. } => format!("{id} · {kind}"),
        };
        let response = ui.add(
            egui::Button::selectable(selected, text)
                .truncate()
                .sense(egui::Sense::click_and_drag()),
        );
        let action = self.drag_and_menu(&response, item);
        (response, action)
    }

    fn asset_action(&mut self, action: AssetAction) {
        match action {
            AssetAction::Place(id) => {
                if let Err(error) = self.place_model(&id, None) {
                    self.error = format!("Could not place {id}: {error}");
                }
            }
            AssetAction::Assign(id) => {
                if let Some(target) = self.selected.clone() {
                    match self.asset_payload(&id) {
                        Ok(payload) => self.queued_drop = Some((target, payload)),
                        Err(error) => self.error = error,
                    }
                }
            }
            AssetAction::Texture(id) => {
                let role = self.resolved_textures.get(&id).map(|t| t.role);
                if let Some(role) = role {
                    if let Err(error) = self.stage_material_texture(&id, role) {
                        self.error = error;
                    }
                }
            }
            AssetAction::Refresh(id) => self.queued_refresh = Some(id),
            AssetAction::Rename(id) => {
                let source = self
                    .content_search
                    .rows
                    .iter()
                    .find(|row| row.info.id == id)
                    .map(|row| row.info.source.clone())
                    .unwrap_or_default();
                self.asset_id = id;
                self.asset_source = source;
                self.asset_rename_intent = true;
                self.asset_source_focus = true;
                self.asset_focused = false;
                self.asset_dialog = true;
            }
            AssetAction::References(id) => self.queued_references = Some(id),
        }
    }

    /// Whether the Inspector shows the asset clicked last.
    pub(super) fn inspecting_asset(&self) -> bool {
        self.content_selected.is_some() && self.assets_view.inspect.as_ref() == Some(&self.selected)
    }

    /// The Inspector for an asset.
    pub(super) fn asset_inspector_ui(&mut self, ui: &mut egui::Ui) {
        let Some(id) = self.content_selected.clone() else {
            return;
        };
        let model = self.assets.as_ref().and_then(|s| s.model(&id)).map(|m| {
            (
                m.source.clone(),
                m.clips.keys().cloned().collect::<Vec<_>>(),
            )
        });
        ui.heading(&id);
        if let Some((source, clips)) = model {
            ui.label(if clips.is_empty() {
                "Model"
            } else {
                "Animated model"
            });
            ui.weak(source);
            if !clips.is_empty() {
                ui.label(format!("Clips: {}", clips.join(", ")));
            }
            ui.horizontal(|ui| {
                if crate::editor_theme::primary_button(ui, "Place in Scene").clicked() {
                    self.asset_action(AssetAction::Place(id.clone()));
                }
                if ui.button("Reimport").clicked() {
                    self.asset_action(AssetAction::Refresh(id.clone()));
                }
            });
            ui.weak("Or drag it from Assets into the Scene view.");
            return;
        }
        let Some(row) = self
            .content_search
            .rows
            .iter()
            .find(|row| row.info.id == id)
        else {
            ui.label("This asset is no longer in the project.");
            return;
        };
        let info = row.info.clone();
        let kind = match info.kind {
            rot_assets::AssetKind::Mesh => "Mesh",
            rot_assets::AssetKind::Sound => "Sound",
            rot_assets::AssetKind::Texture => "Texture",
        };
        ui.label(kind);
        ui.weak(&info.source);
        if info.kind == rot_assets::AssetKind::Texture {
            if let Some(texture) = self.thumbnail(ui.ctx(), &id) {
                let width = ui.available_width().min(256.0);
                ui.image((texture, egui::vec2(width, width)));
            }
        }
        // How a file is read is fixed per asset; another reading is a second
        // asset from the same file.
        let mut variant = None;
        if let Some(role) = info.texture_role {
            let (label, other, suffix, button) = match role {
                rot_assets::TextureRole::BaseColor => (
                    "Base color (sRGB)",
                    rot_assets::TextureRole::Roughness,
                    "roughness",
                    "Import as Roughness Map",
                ),
                rot_assets::TextureRole::Roughness => (
                    "Roughness map (linear)",
                    rot_assets::TextureRole::BaseColor,
                    "color",
                    "Import as Base Color",
                ),
            };
            ui.label(label);
            if ui
                .button(button)
                .on_hover_text("Adds a second asset that reads this file the other way")
                .clicked()
            {
                self.asset_texture_role = other;
                variant = Some(suffix);
            }
        }
        if let Some(policy) = info.mesh_normal_policy {
            let (label, other, suffix, button) = match policy {
                rot_assets::MeshNormalPolicy::Flat => (
                    "Normals: Flat",
                    rot_assets::MeshNormalPolicy::AuthoredWhenComplete,
                    "smooth",
                    "Import with the File's Normals",
                ),
                rot_assets::MeshNormalPolicy::AuthoredWhenComplete => (
                    "Normals: From the file",
                    rot_assets::MeshNormalPolicy::Flat,
                    "flat",
                    "Import with Flat Normals",
                ),
            };
            ui.label(label);
            if ui
                .button(button)
                .on_hover_text("Adds a second asset that reads this file the other way")
                .clicked()
            {
                self.asset_normal_policy = other;
                variant = Some(suffix);
            }
        }
        if let Some(suffix) = variant {
            let taken = self.asset_ids();
            let base = format!("{id}_{suffix}");
            let new_id = (1..)
                .map(|n| {
                    if n == 1 {
                        base.clone()
                    } else {
                        format!("{base}_{n}")
                    }
                })
                .find(|candidate| !taken.contains(candidate))
                .unwrap();
            self.content_selected = Some(new_id.clone());
            self.start_human_asset(ui.ctx(), "asset_import", &new_id, &info.source, false);
        }
        if let Some(error) = &info.diagnostic {
            ui.colored_label(crate::editor_theme::colors(ui).warning, error);
        }
        if ui.button("Reimport").clicked() {
            self.asset_action(AssetAction::Refresh(id.clone()));
        }
    }
}

enum AssetAction {
    Place(String),
    Assign(String),
    Texture(String),
    Refresh(String),
    Rename(String),
    References(String),
}

impl SceneEditor {
    /// A model dragged from Assets and released over the Scene view is
    /// placed in the scene.
    pub(crate) fn accept_model_drop(&mut self, ui: &egui::Ui, rect: egui::Rect) {
        let ctx = ui.ctx();
        if !egui::DragAndDrop::has_payload_of_type::<ModelDrag>(ctx) {
            return;
        }
        let over = ctx.pointer_hover_pos().is_some_and(|p| rect.contains(p));
        if !over {
            return;
        }
        let palette = crate::editor_theme::colors(ui);
        ui.painter().rect_stroke(
            rect.shrink(2.0),
            4.0,
            egui::Stroke::new(2.0, palette.accent),
            egui::StrokeKind::Inside,
        );
        if ctx.input(|i| i.pointer.any_released()) {
            if let Some(payload) = egui::DragAndDrop::take_payload::<ModelDrag>(ctx) {
                let current = self.assets.as_ref().map_or(0, AssetStore::revision);
                if payload.generation != self.generation || payload.catalog_revision != current {
                    self.error = "The project changed while dragging; drag the model again.".into();
                    return;
                }
                if let Err(error) = self.place_model(&payload.asset_id, None) {
                    self.error = format!("Could not place {}: {error}", payload.asset_id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_come_from_file_names() {
        assert_eq!(id_from_name("Crate Wood (2)"), "crate_wood__2");
        assert_eq!(id_from_name("Hero.v2"), "hero.v2");
        assert_eq!(id_from_name("木"), "asset");
    }

    // A file from outside the project is copied into Assets/ and imported
    // under an ID from its name; a second one with the same name gets a new
    // file and ID.
    #[test]
    fn dropped_files_are_copied_and_imported_without_a_form() {
        let project = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let scene = project.path().join("scene.json");
        let mut editor = SceneEditor::new(Session::new(Scene::authored_empty("drop")).unwrap());
        editor.session.save(scene).unwrap();
        editor.assets = Some(AssetStore::open(project.path()).unwrap());
        let obj = outside.path().join("Tri Angle.obj");
        std::fs::write(&obj, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        assert_eq!(editor.project_source(&obj).unwrap(), "Assets/Tri Angle.obj");
        assert!(project.path().join("Assets/Tri Angle.obj").is_file());
        assert_eq!(
            editor.project_source(&obj).unwrap(),
            "Assets/Tri Angle.obj",
            "same bytes reuse the copy"
        );
        std::fs::write(&obj, "v 0 0 0\nv 2 0 0\nv 0 2 0\nf 1 2 3\n").unwrap();
        assert_eq!(
            editor.project_source(&obj).unwrap(),
            "Assets/Tri Angle 2.obj"
        );
        let inside = project.path().join("Assets/Tri Angle.obj");
        assert_eq!(
            editor.project_source(&inside).unwrap(),
            "Assets/Tri Angle.obj"
        );

        editor.queue_imports([obj.clone(), outside.path().join("notes.txt")]);
        assert_eq!(editor.assets_view.queue.len(), 1);
        assert_eq!(editor.console.count(crate::console::Level::Warning), 1);
    }
}
