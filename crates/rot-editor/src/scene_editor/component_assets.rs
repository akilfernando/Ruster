//! File-backed Assets panel for the component workspace.
use super::super::asset_inspection::{import_issue_text, ImportIssue};
use super::super::assets_browser::{asset_glyph, texture_thumbnail, TextureThumbnails};
use super::{ComponentWorkspace, Geometry, Transform};
use crate::ui_kit::tokens::{SPACE_L, SPACE_XS};
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_assets::{
    AssetKind, AssetStateIdentity, AssetStatus, MeshNormalPolicy, PrefabFile, ProjectFileIssue,
    TextureRole,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(target_os = "linux")]
use super::super::project_assets::FileAction;

#[cfg(target_os = "linux")]
enum ContextAction {
    File(FileAction),
    Reveal(PathBuf),
    CreateFolder(PathBuf),
    Rename(PathBuf),
    Delete(PathBuf),
}

#[cfg(target_os = "linux")]
enum FileDialog {
    Folder { parent: PathBuf, name: String },
    Rename { source: PathBuf, stem: String },
    Delete(PathBuf),
}

const SEARCH: &str = "v23_asset_search";

struct AssetRow {
    id: String,
    source: String,
    label: String,
    searchable: String,
    folder: PathBuf,
    kind: &'static str,
    settings: Option<&'static str>,
    model: bool,
    registered: bool,
    status: AssetStatus,
    diagnostic: Option<Arc<str>>,
    last_good_available: bool,
    warnings: Vec<String>,
}

impl AssetRow {
    fn new(id: &str, source: &str, kind: &'static str, status: AssetStatus) -> Self {
        let path = Path::new(source);
        Self {
            id: id.to_owned(),
            source: source.to_owned(),
            label: path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(id)
                .into(),
            searchable: format!("{source} {id} {kind}").to_lowercase(),
            folder: path.parent().unwrap_or(Path::new("")).to_owned(),
            kind,
            settings: None,
            model: false,
            registered: true,
            status,
            diagnostic: None,
            last_good_available: false,
            warnings: Vec::new(),
        }
    }

    fn can_place(&self, meshes: &rot_app::MeshMap) -> bool {
        self.model
            || self.kind == "Prefab" && self.status == AssetStatus::Ready
            || self.kind == "Mesh" && meshes.contains_key(&self.id)
    }

    fn status_label(&self) -> &'static str {
        match self.status {
            AssetStatus::Ready => Text::AssetReady.plain(),
            AssetStatus::Invalid => Text::AssetInvalid.plain(),
            AssetStatus::Missing => Text::AssetMissing.plain(),
        }
    }
}

pub(in crate::scene_editor) struct AssetDrag {
    id: String,
    catalog: AssetStateIdentity,
    prefab: bool,
}

impl AssetDrag {
    pub(in crate::scene_editor) fn released(
        ui: &egui::Ui,
        target: &egui::Response,
    ) -> Option<std::sync::Arc<Self>> {
        // egui consumes any payload on release, including the wrong type.
        target.dnd_hover_payload::<Self>()?;
        ui.painter().rect_stroke(
            target.rect,
            3.0,
            egui::Stroke::new(1.0, editor_theme::colors(ui).accent),
            egui::StrokeKind::Inside,
        );
        target.dnd_release_payload()
    }

    pub(in crate::scene_editor) fn geometry(
        &self,
        assets: Option<&rot_assets::AssetStore>,
        meshes: &rot_app::MeshMap,
    ) -> Option<Geometry> {
        (assets.is_some_and(|store| store.matches_identity(&self.catalog))
            && meshes.contains_key(&self.id))
        .then(|| Geometry::Imported {
            asset: self.id.clone(),
        })
    }
}

pub(super) struct Browser {
    pub(super) open: bool,
    pub(super) focus_search: bool,
    pub(super) selected: Option<String>,
    pub(super) issues: BTreeMap<String, ImportIssue>,
    size: f32,
    folder: PathBuf,
    query: String,
    catalog: Option<AssetStateIdentity>,
    folders: Vec<PathBuf>,
    rows: Vec<AssetRow>,
    by_id: HashMap<String, usize>,
    visible: Vec<usize>,
    thumbnails: TextureThumbnails,
    #[cfg(target_os = "linux")]
    context_action: Option<ContextAction>,
    #[cfg(target_os = "linux")]
    file_dialog: Option<(AssetStateIdentity, FileDialog)>,
    #[cfg(target_os = "linux")]
    focus_file_name: bool,
}

impl Default for Browser {
    fn default() -> Self {
        Self {
            open: true,
            focus_search: false,
            selected: None,
            issues: BTreeMap::new(),
            size: 64.0,
            folder: "Assets".into(),
            query: String::new(),
            catalog: None,
            folders: vec![PathBuf::new(), "Assets".into()],
            rows: Vec::new(),
            by_id: HashMap::new(),
            visible: Vec::new(),
            thumbnails: TextureThumbnails::new(),
            #[cfg(target_os = "linux")]
            context_action: None,
            #[cfg(target_os = "linux")]
            file_dialog: None,
            #[cfg(target_os = "linux")]
            focus_file_name: false,
        }
    }
}

impl Browser {
    #[cfg(target_os = "linux")]
    pub(super) fn import_folder(&self) -> PathBuf {
        if self.folder.starts_with("Assets") {
            self.folder.clone()
        } else {
            "Assets".into()
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn open_new_folder(&mut self) {
        self.open = true;
        self.start_file_dialog(FileDialog::Folder {
            parent: self.import_folder(),
            name: String::new(),
        });
    }

    #[cfg(target_os = "linux")]
    fn start_file_dialog(&mut self, dialog: FileDialog) {
        self.file_dialog = self.catalog.clone().map(|catalog| (catalog, dialog));
        self.focus_file_name = true;
    }

    pub(super) fn file_dialog_open(&self) -> bool {
        #[cfg(target_os = "linux")]
        return self.file_dialog.is_some();
        #[cfg(not(target_os = "linux"))]
        false
    }

    #[cfg(target_os = "linux")]
    fn show_file_dialog(
        &mut self,
        ctx: &egui::Context,
    ) -> Option<(AssetStateIdentity, FileAction)> {
        let (catalog, mut dialog) = self.file_dialog.take()?;
        let focus_name = std::mem::take(&mut self.focus_file_name);
        let escaped =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let mut cancel = escaped;
        let mut submit = false;
        egui::Modal::new(egui::Id::unique("asset_file_dialog"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                let (heading, action, name) = match &mut dialog {
                    FileDialog::Folder { name, .. } => (
                        Text::AssetNewFolder.plain(),
                        Text::AssetCreateFolder.plain(),
                        Some(name),
                    ),
                    FileDialog::Rename { stem, .. } => (
                        Text::AssetRenameHeading.plain(),
                        Text::AssetRename.plain(),
                        Some(stem),
                    ),
                    FileDialog::Delete(_) => (
                        Text::AssetDeleteHeading.plain(),
                        Text::AssetMoveToTrash.plain(),
                        None,
                    ),
                };
                ui.heading(heading);
                let valid = if let Some(name) = name {
                    let label = ui.label(Text::ComponentName.plain());
                    let text = ui
                        .add(egui::TextEdit::singleline(name).id_salt("asset_file_name"))
                        .labelled_by(label.id);
                    if focus_name {
                        text.request_focus();
                        if let Some(mut state) = egui::TextEdit::load_state(ctx, text.id) {
                            use egui::text::{CCursor, CCursorRange};
                            state.cursor.set_char_range(Some(CCursorRange::two(
                                CCursor::new(0),
                                CCursor::new(name.chars().count()),
                            )));
                            state.store(ctx, text.id);
                        }
                    }
                    let valid = !name.trim().is_empty();
                    submit = valid
                        && text.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter));
                    valid
                } else {
                    if let FileDialog::Delete(source) = &dialog {
                        ui.label(source.display().to_string());
                    }
                    ui.label(Text::AssetTrashQuestion.plain());
                    true
                };
                ui.horizontal(|ui| {
                    submit |= ui.add_enabled(valid, egui::Button::new(action)).clicked();
                    cancel |= ui.button(Text::ComponentCancel.plain()).clicked();
                });
            });
        if cancel {
            return None;
        }
        if !submit {
            self.file_dialog = Some((catalog, dialog));
            return None;
        }
        let action = match dialog {
            FileDialog::Folder { parent, name } => FileAction::CreateFolder {
                parent,
                name: name.trim().to_owned(),
            },
            FileDialog::Rename { source, stem } => FileAction::Rename {
                source,
                stem: stem.trim().to_owned(),
            },
            FileDialog::Delete(source) => FileAction::Trash(source),
        };
        Some((catalog, action))
    }

    pub(super) fn refresh(
        &mut self,
        assets: &rot_assets::AssetStore,
        folders: Vec<PathBuf>,
        issues: &[ProjectFileIssue],
        prefabs: &[PrefabFile],
    ) {
        let selected_source = self
            .selected
            .as_ref()
            .and_then(|id| self.by_id.get(id))
            .map(|&index| self.rows[index].source.clone());
        let project_prefix = format!("{}{}", assets.root().display(), std::path::MAIN_SEPARATOR);
        let diagnostic = |text: &str| -> Arc<str> {
            if text.contains(&project_prefix) {
                text.replace(&project_prefix, "").into()
            } else {
                text.into()
            }
        };
        self.catalog = Some(assets.state_identity());
        self.rows.clear();
        for info in assets.infos() {
            let kind = match info.kind {
                AssetKind::Mesh => "Mesh",
                AssetKind::Texture => "Texture",
                AssetKind::Sound => "Sound",
            };
            let mut row = AssetRow::new(&info.id, &info.source, kind, info.status.clone());
            row.diagnostic = info.diagnostic.as_deref().map(diagnostic);
            row.settings = match (info.mesh_normal_policy, info.texture_role) {
                (Some(MeshNormalPolicy::Flat), _) => Some("Normals: Flat"),
                (Some(MeshNormalPolicy::AuthoredWhenComplete), _) => Some("Normals: From the file"),
                (_, Some(TextureRole::BaseColor)) => Some("Base color (sRGB)"),
                (_, Some(TextureRole::Roughness)) => Some("Roughness map (linear)"),
                _ => None,
            };
            self.rows.push(row);
        }
        self.rows.extend(assets.models().map(|model| {
            let mut row = AssetRow::new(&model.id, &model.source, "Model", AssetStatus::Ready);
            row.model = true;
            row.warnings.clone_from(&model.warnings);
            row
        }));
        self.rows.extend(
            prefabs.iter().map(|prefab| {
                AssetRow::new(&prefab.id, &prefab.source, "Prefab", AssetStatus::Ready)
            }),
        );
        self.issues.clear();
        for issue in issues {
            let entry = self
                .issues
                .entry(issue.path.clone())
                .or_insert(ImportIssue {
                    status: issue.status.clone(),
                    asset_id: None,
                    last_good_available: false,
                    diagnostic: None,
                });
            let detail = diagnostic(&issue.message);
            if entry
                .diagnostic
                .as_ref()
                .is_none_or(|previous| detail < *previous)
            {
                entry.status = issue.status.clone();
                entry.diagnostic = Some(detail);
            }
        }
        for row in &mut self.rows {
            if let Some(issue) = self
                .issues
                .get(&row.source)
                .filter(|_| row.status == AssetStatus::Ready)
            {
                row.status = issue.status.clone();
                row.diagnostic.clone_from(&issue.diagnostic);
            }
        }
        for row in &mut self.rows {
            if row.status != AssetStatus::Ready {
                // A model is registered in an open store only while its accepted
                // GLB payload is loaded. Other assets can have metadata alone.
                row.last_good_available = row.model
                    || assets.mesh(&row.id).is_some()
                    || assets.texture(&row.id).is_some()
                    || assets.sound(&row.id).is_some();
                let issue = self
                    .issues
                    .entry(row.source.clone())
                    .or_insert(ImportIssue {
                        status: row.status.clone(),
                        asset_id: None,
                        last_good_available: false,
                        diagnostic: row.diagnostic.clone(),
                    });
                // Separate interpretations share a source. If one is
                // unavailable, report that representation before the others.
                if issue.asset_id.is_none() || issue.last_good_available && !row.last_good_available
                {
                    issue.asset_id = Some(row.id.clone());
                    issue.status = row.status.clone();
                    issue.last_good_available = row.last_good_available;
                    issue.diagnostic.clone_from(&row.diagnostic);
                }
            }
        }
        self.rows
            .extend(self.issues.iter().filter_map(|(source, issue)| {
                if issue.asset_id.is_some() {
                    return None;
                }
                let kind = match Path::new(source).extension()?.to_str()? {
                    "obj" => "Mesh",
                    "png" => "Texture",
                    "wav" => "Sound",
                    "glb" => "Model",
                    "prefab" => "Prefab",
                    "rotmeta" => Text::AssetImportSettings.plain(),
                    _ => return None,
                };
                let mut row = AssetRow::new(source, source, kind, issue.status.clone());
                row.registered = false;
                row.diagnostic.clone_from(&issue.diagnostic);
                Some(row)
            }));
        self.rows
            .sort_by(|left, right| left.searchable.cmp(&right.searchable));
        let mut folders: BTreeSet<_> = folders.into_iter().collect();
        folders.extend([PathBuf::new(), "Assets".into()]);
        let mut names = HashMap::new();
        self.by_id.clear();
        for (index, row) in self.rows.iter().enumerate() {
            self.by_id.insert(row.id.clone(), index);
            folders.extend(row.folder.ancestors().map(Path::to_path_buf));
            *names.entry(row.source.clone()).or_insert(0) += 1;
        }
        for row in &mut self.rows {
            if names[&row.source] > 1 {
                row.label = format!("{} · {}", row.label, row.id);
            }
        }
        self.folders = folders.into_iter().collect();
        while !self.folders.contains(&self.folder) {
            self.folder.pop();
        }
        self.thumbnails.retain(|id, _| self.by_id.contains_key(id));
        if self
            .selected
            .as_ref()
            .is_some_and(|id| !self.by_id.contains_key(id))
        {
            self.selected = selected_source.and_then(|source| {
                self.rows
                    .iter()
                    .find(|row| row.source == source)
                    .map(|row| row.id.clone())
            });
        }
        self.filter();
    }

    fn filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        self.visible = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                if query.is_empty() {
                    row.folder == self.folder
                } else {
                    row.searchable.contains(&query)
                }
            })
            .map(|(index, _)| index)
            .collect();
    }

    fn show(
        &mut self,
        ui: &mut egui::Ui,
        meshes: &rot_app::MeshMap,
        textures: &rot_app::TextureMap,
        loading: bool,
        failed: bool,
        can_place: bool,
    ) -> (Option<String>, Option<AssetDrag>) {
        let mut opened_folder = None;
        let mut query_changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.strong(Text::ComponentAssets.plain());
            ui.separator();
            for folder in self
                .folder
                .ancestors()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                if folder != Path::new("") {
                    ui.label("/");
                }
                if ui.button(folder_name(folder)).clicked() {
                    opened_folder = Some(folder.to_owned());
                }
            }
            ui.separator();
            let search = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .id_salt(SEARCH)
                    .hint_text(Text::AssetSearch.plain())
                    .desired_width(150.0),
            );
            if std::mem::take(&mut self.focus_search) {
                search.request_focus();
            }
            let clear = search.has_focus()
                && ui
                    .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            if clear {
                self.query.clear();
            }
            query_changed = search.changed() || clear;
            ui.add(
                egui::Slider::new(&mut self.size, 40.0..=112.0)
                    .show_value(false)
                    .text("Thumbnail size"),
            );
        });
        ui.separator();
        let mut selected = None;
        let mut place = None;
        egui::Panel::left("v23_asset_folders")
            .default_size(150.0)
            .size_range(85.0..=240.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    20.0,
                    self.folders.len(),
                    |ui, range| {
                        for folder in &self.folders[range] {
                            ui.horizontal(|ui| {
                                ui.add_space(8.0 * folder.components().count().min(8) as f32);
                                let row = ui
                                    .add(
                                        egui::Button::selectable(
                                            self.query.is_empty() && *folder == self.folder,
                                            folder_name(folder),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(folder.display().to_string());
                                if row.clicked() {
                                    opened_folder = Some(folder.clone());
                                }
                                #[cfg(target_os = "linux")]
                                row.context_menu(|ui| {
                                    if ui
                                        .add_enabled(
                                            folder.starts_with("Assets"),
                                            egui::Button::new(Text::AssetNewFolderCommand.plain()),
                                        )
                                        .clicked()
                                    {
                                        self.context_action =
                                            Some(ContextAction::CreateFolder(folder.clone()));
                                        ui.close();
                                    }
                                    if ui.button(Text::AssetShowInFileManager.plain()).clicked() {
                                        self.context_action =
                                            Some(ContextAction::Reveal(folder.clone()));
                                        ui.close();
                                    }
                                });
                            });
                        }
                    },
                );
            });
        if let Some(folder) = opened_folder {
            self.folder = folder;
            self.query.clear();
            query_changed = true;
        }
        if query_changed {
            self.filter();
        }
        if self.visible.is_empty() {
            ui.label(if loading {
                Text::ImportingAssets.plain()
            } else if failed && self.catalog.is_none() {
                Text::AssetsUnavailable.plain()
            } else if self.query.trim().is_empty() {
                Text::NoAssetsInThisFolder.plain()
            } else {
                Text::NoMatchingAssets.plain()
            });
            return (None, None);
        }
        let tile_width = self.size.max(88.0);
        let tile_height = self.size + ui.text_style_height(&egui::TextStyle::Small) + 6.0;
        let gap = ui.spacing().item_spacing.x;
        let columns = ((ui.available_width() + gap) / (tile_width + gap))
            .floor()
            .max(1.0) as usize;
        let mut scroll = egui::ScrollArea::vertical().id_salt("v23_asset_tiles");
        if query_changed {
            scroll = scroll.vertical_scroll_offset(0.0);
        }
        scroll.show_rows(
            ui,
            tile_height,
            self.visible.len().div_ceil(columns),
            |ui, range| {
                for line in range {
                    let start = line * columns;
                    let end = (start + columns).min(self.visible.len());
                    ui.horizontal(|ui| {
                        for &index in &self.visible[start..end] {
                            let row = &self.rows[index];
                            ui.push_id(&row.id, |ui| {
                                let texture = textures.get(&row.id).map(|texture| {
                                    let dimensions =
                                        egui::vec2(texture.width as f32, texture.height as f32);
                                    let thumbnail = texture_thumbnail(
                                        &mut self.thumbnails,
                                        ui.ctx(),
                                        &row.id,
                                        texture,
                                    );
                                    (thumbnail, dimensions / dimensions.max_elem())
                                });
                                let response = asset_tile(
                                    ui,
                                    row,
                                    egui::vec2(tile_width, tile_height),
                                    self.size,
                                    texture,
                                    self.selected.as_ref() == Some(&row.id),
                                );
                                let placeable = can_place && row.can_place(meshes);
                                let drag = || {
                                    self.catalog.clone().map(|catalog| AssetDrag {
                                        catalog,
                                        id: row.id.clone(),
                                        prefab: row.kind == "Prefab",
                                    })
                                };
                                if response.drag_started() && placeable {
                                    if let Some(payload) = drag() {
                                        response.dnd_set_drag_payload(payload);
                                    }
                                }
                                if response.clicked() {
                                    selected = Some(row.id.clone());
                                }
                                if response.double_clicked() && placeable {
                                    place = drag();
                                }
                                response.context_menu(|ui| {
                                    if placeable
                                        && ui.button(Text::AssetPlaceInScene.plain()).clicked()
                                    {
                                        place = drag();
                                        ui.close();
                                    }
                                    #[cfg(target_os = "linux")]
                                    if let Some(action) = file_actions(ui, row) {
                                        self.context_action = Some(action);
                                    }
                                });
                            });
                        }
                    });
                }
            },
        );
        (selected, place)
    }

    pub(super) fn inspect(
        &mut self,
        ui: &mut egui::Ui,
        meshes: &rot_app::MeshMap,
        can_place: bool,
    ) -> Option<AssetDrag> {
        let row = &self.rows[*self.by_id.get(self.selected.as_ref()?)?];
        ui.strong(&row.label);
        ui.label(&row.source);
        ui.label(format!("{} · {}", row.kind, row.status_label()));
        if let Some(settings) = row.settings {
            ui.separator();
            ui.strong(Text::AssetImportSettings.plain());
            ui.label(settings);
        }
        if row.status != AssetStatus::Ready {
            let (message, recovery) = import_issue_text(&row.status);
            let message = if row.kind == "Prefab" && row.status == AssetStatus::Invalid {
                Text::PrefabInvalid
            } else {
                message
            };
            ui.colored_label(editor_theme::colors(ui).error, message.plain());
            ui.label(recovery.plain());
            if row.registered {
                let fallback = if row.last_good_available {
                    Text::AssetLastGoodAvailable
                } else {
                    Text::AssetNoLastGood
                };
                ui.label(fallback.plain());
            }
            if let Some(diagnostic) = &row.diagnostic {
                egui::CollapsingHeader::new(Text::ErrorDetails.plain())
                    .id_salt(&row.id)
                    .show(ui, |ui| {
                        egui::ScrollArea::vertical()
                            .max_height(ui.available_height() / 2.0)
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(diagnostic.as_ref())
                                        .wrap()
                                        .selectable(true),
                                );
                            });
                    });
            }
        }
        #[cfg(target_os = "linux")]
        ui.menu_button(Text::AssetActions.plain(), |ui| {
            if let Some(action) = file_actions(ui, row) {
                self.context_action = Some(action);
            }
        });
        if (row.kind == "Mesh" || row.model || row.kind == "Prefab")
            && ui
                .add_enabled(
                    can_place && row.can_place(meshes),
                    egui::Button::new(Text::AssetPlaceInScene.plain()),
                )
                .clicked()
        {
            return Some(AssetDrag {
                id: row.id.clone(),
                catalog: self.catalog.clone()?,
                prefab: row.kind == "Prefab",
            });
        }
        let ready_detail = row
            .diagnostic
            .as_deref()
            .filter(|_| row.status == AssetStatus::Ready);
        if ready_detail.is_some() || !row.warnings.is_empty() {
            ui.separator();
            ui.heading(Text::AssetImportWarnings.plain());
            egui::ScrollArea::vertical()
                .id_salt("model_import_warnings")
                .show(ui, |ui| {
                    for warning in ready_detail
                        .into_iter()
                        .chain(row.warnings.iter().map(String::as_str))
                    {
                        ui.colored_label(ui.visuals().warn_fg_color, warning);
                    }
                });
        }
        None
    }
}

#[cfg(target_os = "linux")]
fn file_actions(ui: &mut egui::Ui, row: &AssetRow) -> Option<ContextAction> {
    let source = Path::new(&row.source);
    let supported = rot_assets::AssetStore::project_source_limit(&row.source).is_ok();
    let duplicate = supported && row.status != AssetStatus::Missing;
    let can_move = duplicate && source.starts_with("Assets");
    let action = if ui
        .add_enabled(
            can_move,
            egui::Button::new(Text::AssetRenameCommand.plain()),
        )
        .clicked()
    {
        Some(ContextAction::Rename(source.to_owned()))
    } else if ui
        .add_enabled(
            duplicate,
            egui::Button::new(crate::workspace::Action::Duplicate.label()),
        )
        .clicked()
    {
        Some(ContextAction::File(FileAction::Duplicate(
            source.to_owned(),
        )))
    } else if ui
        .add_enabled(supported, egui::Button::new(Text::AssetReimport.plain()))
        .clicked()
    {
        Some(ContextAction::File(FileAction::Reimport(source.to_owned())))
    } else if ui
        .add_enabled(
            can_move,
            egui::Button::new(Text::AssetDeleteCommand.plain()),
        )
        .clicked()
    {
        Some(ContextAction::Delete(source.to_owned()))
    } else if ui.button(Text::AssetShowInFileManager.plain()).clicked() {
        Some(ContextAction::Reveal(row.folder.clone()))
    } else {
        None
    };
    if action.is_some() {
        ui.close();
    }
    action
}

fn folder_name(folder: &Path) -> &str {
    folder
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Project")
}

fn asset_tile(
    ui: &mut egui::Ui,
    row: &AssetRow,
    size: egui::Vec2,
    thumbnail_size: f32,
    thumbnail: Option<(egui::TextureId, egui::Vec2)>,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let image = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.min.y + thumbnail_size / 2.0),
        egui::Vec2::splat(thumbnail_size),
    );
    let colors = editor_theme::colors(ui);
    let fill = if selected {
        colors.accent.gamma_multiply(0.35)
    } else {
        colors.raised
    };
    ui.painter().rect_filled(image, 4.0, fill);
    if response.hovered() || response.has_focus() {
        ui.painter().rect_stroke(
            image,
            4.0,
            egui::Stroke::new(1.0, colors.accent),
            egui::StrokeKind::Inside,
        );
    }
    if let Some((thumbnail, aspect)) = thumbnail {
        let fit = egui::Rect::from_center_size(image.center(), aspect * image.shrink(4.0).width());
        ui.painter().image(
            thumbnail,
            fit,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    } else {
        ui.painter().text(
            image.center(),
            egui::Align2::CENTER_CENTER,
            asset_glyph(row.kind),
            egui::FontId::proportional(thumbnail_size * 0.4),
            colors.secondary,
        );
    }
    if row.status != AssetStatus::Ready {
        let center = image.right_top() + egui::vec2(-SPACE_L, SPACE_L);
        ui.painter()
            .circle_filled(center, SPACE_L / 2.0, colors.error);
        ui.painter().text(
            center,
            egui::Align2::CENTER_CENTER,
            "!",
            egui::FontId::proportional(SPACE_L - SPACE_XS),
            colors.surface,
        );
    }
    let color = ui.visuals().text_color();
    let label = ui.text_style_height(&egui::TextStyle::Small);
    let galley = ui.painter().layout_job(egui::text::LayoutJob {
        wrap: egui::text::TextWrapping {
            max_width: size.x,
            max_rows: 1,
            break_anywhere: true,
            overflow_character: Some('…'),
        },
        ..egui::text::LayoutJob::simple_singleline(
            row.label.clone(),
            egui::FontId::proportional(label),
            color,
        )
    });
    ui.painter().galley(
        egui::pos2(rect.center().x - galley.size().x / 2.0, image.max.y + 2.0),
        galley,
        color,
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &row.label));
    response.on_hover_text(format!(
        "{} · {}\n{}",
        row.kind,
        row.status_label(),
        row.source
    ))
}

impl ComponentWorkspace {
    pub(super) fn show_assets(&mut self, ui: &mut egui::Ui, blocked: bool) {
        if !self.asset_browser.open {
            return;
        }
        let loading = self.assets_loading();
        let can_place = !blocked && !self.runtime.in_play_mode();
        let (selected, place) = egui::Panel::bottom("component_assets")
            .default_size(200.0)
            .size_range(138.0..=360.0)
            .show(ui, |ui| {
                self.asset_browser.show(
                    ui,
                    &self.meshes,
                    &self.textures,
                    loading,
                    self.asset_error.is_some(),
                    can_place,
                )
            })
            .inner;
        if let Some(id) = selected.filter(|_| !blocked && self.apply()) {
            self.asset_browser.selected = Some(id);
        }
        if let Some(place) = place {
            self.place_asset(&place, None);
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn asset_context_frame(&mut self, ctx: &egui::Context, blocked: bool) {
        if blocked {
            self.asset_browser.context_action = None;
            self.asset_browser.file_dialog = None;
            return;
        }
        if let Some((catalog, action)) = self.asset_browser.show_file_dialog(ctx) {
            self.submit_asset_action(catalog, action);
        }
        match self.asset_browser.context_action.take() {
            Some(ContextAction::CreateFolder(parent)) => {
                self.asset_browser.start_file_dialog(FileDialog::Folder {
                    parent,
                    name: String::new(),
                });
                ctx.request_repaint();
            }
            Some(ContextAction::Rename(source)) => {
                let stem = source
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.asset_browser
                    .start_file_dialog(FileDialog::Rename { source, stem });
                ctx.request_repaint();
            }
            Some(ContextAction::Delete(source)) => {
                self.asset_browser
                    .start_file_dialog(FileDialog::Delete(source));
                ctx.request_repaint();
            }
            Some(ContextAction::Reveal(folder)) => {
                let result = (|| {
                    let root = self
                        .assets
                        .as_ref()
                        .ok_or(Text::AssetProjectUnavailable.plain())?
                        .root();
                    let folder = root
                        .join(folder)
                        .canonicalize()
                        .map_err(|_| "The folder is unavailable.")?;
                    if !folder.starts_with(root) || !folder.is_dir() {
                        return Err("The folder is outside the open project.");
                    }
                    std::process::Command::new("xdg-open")
                        .arg(folder)
                        .spawn()
                        .map_err(|_| Text::AssetOpenFileManagerError.plain())?;
                    Ok(())
                })();
                if let Err(error) = result {
                    self.asset_import_error = Some(error.into());
                }
            }
            Some(ContextAction::File(action)) => {
                if let Some(catalog) = self.asset_browser.catalog.clone() {
                    self.submit_asset_action(catalog, action);
                }
            }
            None => {}
        }
    }

    #[cfg(target_os = "linux")]
    fn submit_asset_action(&mut self, catalog: AssetStateIdentity, action: FileAction) {
        if self.asset_import.is_some() || self.ai_asset_write.is_some() {
            self.asset_import_error = Some(Text::AssetOperationBusy.plain().into());
            return;
        }
        let result = self
            .asset_watcher
            .as_ref()
            .ok_or_else(|| Text::AssetProjectUnavailable.plain().to_owned())
            .and_then(|worker| worker.file_action(action, catalog));
        match result {
            Ok(reply) => {
                self.asset_import = Some(reply);
                self.asset_import_error = None;
            }
            Err(error) => self.asset_import_error = Some(error),
        }
    }

    pub(super) fn place_asset(&mut self, drag: &AssetDrag, parent: Option<String>) {
        if self.runtime.in_play_mode() || !self.apply() {
            return;
        }
        let Some(store) = self
            .assets
            .as_ref()
            .filter(|store| store.matches_identity(&drag.catalog))
        else {
            let message = Text::AssetChanged.plain();
            if drag.prefab {
                self.report_prefab_result(Text::PrefabPlace, Err::<(), _>(message));
            } else {
                self.error = Some(message.into());
            }
            return;
        };
        if drag.prefab {
            let result = self.place_prefab(&drag.id, None, Transform::default(), parent);
            self.report_prefab_result(Text::PrefabPlace, result);
            return;
        }
        let result = if store.model(&drag.id).is_some() {
            self.place_model(&drag.id, None, parent)
        } else if let Some(geometry) = drag.geometry(Some(store), &self.meshes) {
            let row = &self.asset_browser.rows[self.asset_browser.by_id[&drag.id]];
            let name = Path::new(&row.source)
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or(&row.id)
                .to_owned();
            self.create_object(None, &name, Some(geometry), parent, Transform::default())
        } else {
            self.error = Some(Text::AssetMeshUnavailable.plain().into());
            return;
        };
        self.error = result.err().map(|error| error.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rot_scene::{
        component_scene::{Component, ComponentScene, ComponentSession},
        Scene, Session,
    };
    use std::fs;

    #[cfg(target_os = "linux")]
    #[test]
    fn asset_file_dialog_uses_enter_and_escape() {
        let project = tempfile::tempdir().unwrap();
        let store = rot_assets::AssetStore::open(project.path()).unwrap();
        let mut browser = Browser {
            catalog: Some(store.state_identity()),
            ..Browser::default()
        };
        let ctx = egui::Context::default();
        let frame = |browser: &mut Browser, events| {
            let mut action = None;
            ctx.run_ui(
                egui::RawInput {
                    events,
                    focused: true,
                    ..Default::default()
                },
                |ui| action = browser.show_file_dialog(ui.ctx()),
            )
            .textures_delta
            .clear();
            action
        };
        let press = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };

        browser.open_new_folder();
        frame(&mut browser, Vec::new());
        frame(&mut browser, vec![egui::Event::Text("Props".into())]);
        let (_, action) = frame(&mut browser, vec![press(egui::Key::Enter)]).unwrap();
        assert!(matches!(
            action, FileAction::CreateFolder { parent, name } if parent == Path::new("Assets") && name == "Props"
        ));

        browser.start_file_dialog(FileDialog::Delete("Assets/Prop.obj".into()));
        frame(&mut browser, Vec::new());
        assert!(frame(&mut browser, vec![press(egui::Key::Escape)]).is_none());
        assert!(!browser.file_dialog_open());
    }

    #[test]
    fn catalog_navigation_and_mesh_placement_follow_asset_changes() {
        let project = tempfile::tempdir().unwrap();
        let path = project.path().join("scene.json");
        ComponentScene::from_legacy(&Session::new(Scene::empty("assets")).unwrap())
            .unwrap()
            .save_new(&path)
            .unwrap();
        let folder = PathBuf::from("Assets/Props");
        fs::create_dir_all(project.path().join(&folder)).unwrap();
        let source = folder.join("Arrow.obj");
        fs::write(
            project.path().join(&source),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let (mut store, diagnostics) =
            rot_assets::AssetStore::open_project(project.path()).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let id = store.infos().next().unwrap().id.clone();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let browser = &mut editor.asset_browser;
        browser.refresh(&store, vec![PathBuf::from("Assets/Empty")], &[], &[]);
        assert!(browser.folders.contains(&PathBuf::from("Assets/Empty")));
        assert!(browser.visible.is_empty());
        browser.folder = folder;
        browser.filter();
        assert_eq!(browser.rows[browser.visible[0]].id, id);
        browser.folder = PathBuf::new();
        browser.query = "ARROW.obj".into();
        browser.filter();
        assert_eq!(browser.rows[browser.visible[0]].id, id);

        let asset = AssetDrag {
            id,
            catalog: store.state_identity(),
            prefab: false,
        };
        editor.meshes = store.mesh_snapshots();
        editor.assets = Some(store.project_snapshot());
        let parent = editor
            .create_object(Some("parent"), "Parent", None, None, Transform::default())
            .unwrap();
        editor.place_asset(&asset, Some(parent.clone()));
        let scene = editor.session.scene().clone();
        let placed = &scene.objects[1];
        assert_eq!(placed.parent, Some(parent));
        assert_eq!(placed.name, "Arrow");
        assert!(matches!(
            &placed.components[0].kind,
            Component::MeshRenderer { geometry: Geometry::Imported { asset: id }, .. } if id == &asset.id
        ));
        editor.session.undo(editor.session.revision()).unwrap();
        assert_eq!(editor.session.scene().objects.len(), 1);
        editor.session.redo(editor.session.revision()).unwrap();
        assert!(editor.save());
        assert_eq!(ComponentSession::open(&path).unwrap().scene(), &scene);

        fs::write(project.path().join(&source), "bad obj").unwrap();
        let never_imported = PathBuf::from("Assets/New.obj");
        fs::write(project.path().join(&never_imported), "also bad").unwrap();
        let issues =
            store.reconcile_project_files(&BTreeSet::from([source, never_imported]), || true);
        editor
            .asset_browser
            .refresh(&store, Vec::new(), &issues, &[]);
        let new = &editor.asset_browser.rows[editor.asset_browser.by_id["Assets/New.obj"]];
        assert_eq!(new.status, AssetStatus::Invalid);
        assert!(new.diagnostic.is_some());
        editor.assets = Some(store);
        let row = &editor.asset_browser.rows[editor.asset_browser.by_id[&asset.id]];
        assert_eq!(row.status, AssetStatus::Invalid);
        assert!(row.diagnostic.is_some());
        editor.place_asset(&asset, None);
        assert_eq!(editor.session.scene(), &scene);
    }
}
