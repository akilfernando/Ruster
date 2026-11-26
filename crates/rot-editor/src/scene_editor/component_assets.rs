//! File-backed Assets panel for the component workspace.
use super::super::asset_inspection::{import_issue_text, ImportIssue};
use super::super::assets_browser::{asset_glyph, texture_thumbnail, TextureThumbnails};
use super::{ComponentWorkspace, Geometry, Transform};
use crate::ui_kit::tokens::{SPACE_L, SPACE_XS};
use crate::{editor_theme, strings::Text};
use eframe::egui;
use rot_assets::{
    AssetKind, AssetStateIdentity, AssetStatus, MaterialFile, MeshNormalPolicy, PrefabFile,
    ProjectFileIssue, TextureRole,
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(target_os = "linux")]
use super::super::project_assets::{
    material_filename, script_filename, split_asset_filename, FileAction, ScriptLocation,
};

#[cfg(target_os = "linux")]
enum ContextAction {
    File(FileAction),
    Reveal(PathBuf),
    CreateFolder(PathBuf),
    CreateScript(PathBuf),
    CreateMaterial(PathBuf),
    OpenScript { id: String, source: PathBuf },
    Rename(PathBuf),
    Move { id: String, source: PathBuf },
    Delete(PathBuf),
}

#[cfg(target_os = "linux")]
enum FileDialog {
    Script {
        parent: PathBuf,
        name: String,
    },
    Material {
        parent: PathBuf,
        name: String,
    },
    Folder {
        parent: PathBuf,
        name: String,
    },
    Rename {
        source: PathBuf,
        stem: String,
    },
    Move {
        id: String,
        source: PathBuf,
        folder: Option<PathBuf>,
    },
    Delete(PathBuf),
}

const SEARCH: &str = "v23_asset_search";
const ASSET_TYPES: [Text; 8] = [
    Text::Mesh,
    Text::AssetTypeTexture,
    Text::ComponentMaterial,
    Text::AssetTypeSound,
    Text::ComponentScript,
    Text::Model,
    Text::Prefab,
    Text::AssetImportSettings,
];

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, PartialEq, Eq)]
enum SourceRestore {
    Candidate,
    CacheUnavailable,
    SidecarInvalid,
}

#[cfg(target_os = "linux")]
impl SourceRestore {
    fn guidance(self) -> Text {
        match self {
            Self::Candidate => Text::AssetRestoreMissingGuide,
            Self::CacheUnavailable => Text::AssetRestoreCacheUnavailable,
            Self::SidecarInvalid => Text::AssetRestoreSidecarInvalid,
        }
    }
}

struct AssetRow {
    id: String,
    source: String,
    label: String,
    searchable: String,
    folder: PathBuf,
    kind: &'static str,
    settings: Option<&'static str>,
    can_import_roughness: bool,
    #[cfg(target_os = "linux")]
    failed_base_color: Option<String>,
    #[cfg(target_os = "linux")]
    source_restore: Option<SourceRestore>,
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
            can_import_roughness: false,
            #[cfg(target_os = "linux")]
            failed_base_color: None,
            #[cfg(target_os = "linux")]
            source_restore: None,
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
    material: bool,
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

    pub(in crate::scene_editor) fn material(
        &self,
        assets: Option<&rot_assets::AssetStore>,
        material_files: &BTreeMap<String, MaterialFile>,
    ) -> Option<String> {
        (self.material
            && assets.is_some_and(|store| store.matches_identity(&self.catalog))
            && material_files.contains_key(&self.id))
        .then(|| self.id.clone())
    }
}

pub(super) struct Browser {
    pub(super) open: bool,
    pub(super) focus_search: bool,
    pub(super) selected: Option<String>,
    reveal_focus: Option<String>,
    edit_material: Option<String>,
    pub(super) issues: BTreeMap<String, ImportIssue>,
    size: f32,
    folder: PathBuf,
    query: String,
    type_filter: Option<Text>,
    catalog: Option<AssetStateIdentity>,
    folders: Vec<PathBuf>,
    #[cfg(target_os = "linux")]
    move_folders: Vec<PathBuf>,
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
            reveal_focus: None,
            edit_material: None,
            issues: BTreeMap::new(),
            size: 64.0,
            folder: "Assets".into(),
            query: String::new(),
            type_filter: None,
            catalog: None,
            folders: vec![PathBuf::new(), "Assets".into()],
            #[cfg(target_os = "linux")]
            move_folders: Vec::new(),
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
    pub(super) fn open_new_script(&mut self, parent: PathBuf) {
        self.open = true;
        self.start_file_dialog(FileDialog::Script {
            parent,
            name: String::new(),
        });
    }

    #[cfg(target_os = "linux")]
    pub(super) fn open_new_material(&mut self, parent: PathBuf) {
        self.open = true;
        self.start_file_dialog(FileDialog::Material {
            parent,
            name: String::new(),
        });
    }

    pub(super) fn take_edit_material(&mut self) -> Option<String> {
        self.edit_material.take()
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
        let popup_owns_escape =
            matches!(&dialog, FileDialog::Move { .. }) && egui::Popup::is_any_open(ctx);
        let escaped = !popup_owns_escape
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let mut cancel = escaped;
        let mut submit = false;
        if let FileDialog::Move { id, source, folder } = &mut dialog {
            egui::Modal::new(egui::Id::unique("asset_move_folder"))
                .frame(editor_theme::dialog_frame(ctx))
                .show(ctx, |ui| {
                    ui.set_width((ctx.content_rect().width() - 48.0).clamp(220.0, 430.0));
                    ui.heading(Text::AssetMoveFolderTitle.plain());
                    let label = ui.label(Text::AssetMoveFolderLabel.plain());
                    let focus = super::super::combo_focus::ComboFocus::capture(
                        ui,
                        "asset_move_destination",
                    );
                    let response = egui::ComboBox::from_id_salt("asset_move_destination")
                        .selected_text(folder.as_ref().map_or_else(
                            || Text::AssetMoveChooseFolder.plain().to_owned(),
                            |folder| folder.display().to_string(),
                        ))
                        .show_ui(ui, |ui| {
                            let mut selected = false;
                            egui::ScrollArea::vertical()
                                .max_height(240.0)
                                .show(ui, |ui| {
                                    for candidate in &self.move_folders {
                                        if candidate.starts_with("Assets")
                                            && source.parent() != Some(candidate)
                                            && ui
                                                .selectable_label(
                                                    folder.as_ref() == Some(candidate),
                                                    candidate.display().to_string(),
                                                )
                                                .clicked()
                                        {
                                            *folder = Some(candidate.clone());
                                            selected = true;
                                            ui.close();
                                        }
                                    }
                                });
                            selected
                        });
                    focus.restore(ui, &response.response, response.inner.unwrap_or(false));
                    let response = response.response.labelled_by(label.id);
                    if focus_name {
                        response.request_focus();
                    }
                    if !self
                        .move_folders
                        .iter()
                        .any(|candidate| source.parent() != Some(candidate.as_path()))
                    {
                        ui.weak(Text::AssetMoveNoFolders.plain());
                    }
                    if let (Some(folder), Some(leaf)) = (folder.as_ref(), source.file_name()) {
                        ui.label(Text::AssetMoveDestination.plain());
                        ui.add(
                            egui::Label::new(folder.join(leaf).display().to_string())
                                .wrap()
                                .selectable(true),
                        );
                    }
                    ui.horizontal(|ui| {
                        submit = ui
                            .add_enabled_ui(folder.is_some(), |ui| {
                                editor_theme::primary_button(ui, Text::AssetMoveSubmit.plain())
                            })
                            .inner
                            .clicked();
                        cancel |= ui.button(Text::ComponentCancel.plain()).clicked();
                    });
                });
            if cancel {
                return None;
            }
            if submit {
                return Some((
                    catalog,
                    FileAction::Move {
                        id: id.clone(),
                        source: source.clone(),
                        folder: folder.take()?,
                    },
                ));
            }
            self.file_dialog = Some((catalog, dialog));
            return None;
        }
        egui::Modal::new(egui::Id::unique("asset_file_dialog"))
            .frame(editor_theme::dialog_frame(ctx))
            .show(ctx, |ui| {
                enum NameFormat {
                    Script(PathBuf),
                    Material(PathBuf),
                }
                let name_format = match &dialog {
                    FileDialog::Script { parent, .. } => Some(NameFormat::Script(parent.clone())),
                    FileDialog::Material { parent, .. } => {
                        Some(NameFormat::Material(parent.clone()))
                    }
                    _ => None,
                };
                let (heading, action, name) = match &mut dialog {
                    FileDialog::Script { name, .. } => (
                        Text::AssetCreateLuaScript.plain(),
                        Text::ComponentCreate.plain(),
                        Some(name),
                    ),
                    FileDialog::Material { name, .. } => (
                        Text::AssetCreateMaterial.plain(),
                        Text::ComponentCreate.plain(),
                        Some(name),
                    ),
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
                    FileDialog::Move { .. } => unreachable!("separate folder picker"),
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
                    let valid = match name_format {
                        Some(NameFormat::Script(parent)) => match script_filename(name) {
                            Ok(filename) => {
                                ui.label(parent.join(filename).display().to_string());
                                true
                            }
                            Err(error) => {
                                if !name.trim().is_empty() {
                                    ui.colored_label(ui.visuals().error_fg_color, error);
                                }
                                false
                            }
                        },
                        Some(NameFormat::Material(parent)) => match material_filename(name) {
                            Ok(filename) => {
                                ui.label(parent.join(filename).display().to_string());
                                true
                            }
                            Err(error) => {
                                if !name.trim().is_empty() {
                                    ui.colored_label(ui.visuals().error_fg_color, error);
                                }
                                false
                            }
                        },
                        None => !name.trim().is_empty(),
                    };
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
            FileDialog::Script { parent, name } => FileAction::CreateScript {
                parent,
                name: name.trim().to_owned(),
            },
            FileDialog::Material { parent, name } => FileAction::CreateMaterial {
                parent,
                name: name.trim().to_owned(),
            },
            FileDialog::Folder { parent, name } => FileAction::CreateFolder {
                parent,
                name: name.trim().to_owned(),
            },
            FileDialog::Rename { source, stem } => FileAction::Rename {
                source,
                stem: stem.trim().to_owned(),
            },
            FileDialog::Delete(source) => FileAction::Trash(source),
            FileDialog::Move { .. } => unreachable!("separate folder picker"),
        };
        Some((catalog, action))
    }

    #[cfg(target_os = "linux")]
    pub(super) fn source_restore_status(&self, id: &str) -> Option<(&'static str, Text)> {
        let row = &self.rows[*self.by_id.get(id)?];
        let status = row.source_restore?;
        Some((
            if status == SourceRestore::Candidate {
                "candidate"
            } else {
                "unavailable"
            },
            status.guidance(),
        ))
    }

    pub(super) fn refresh(
        &mut self,
        assets: &rot_assets::AssetStore,
        folders: Vec<PathBuf>,
        issues: &[ProjectFileIssue],
        prefabs: &[PrefabFile],
        materials: &[MaterialFile],
    ) {
        let selected_index = self
            .selected
            .as_ref()
            .and_then(|id| self.by_id.get(id))
            .copied();
        let preserve_selection =
            selected_index.is_some_and(|index| self.visible.binary_search(&index).is_err());
        let selected_source = selected_index
            .map(|index| &self.rows[index])
            .filter(|row| !row.registered)
            .map(|row| (row.source.clone(), row.kind));
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
                AssetKind::Script => crate::strings::Text::ComponentScript.plain(),
            };
            let mut row = AssetRow::new(&info.id, &info.source, kind, info.status.clone());
            row.diagnostic = info.diagnostic.as_deref().map(diagnostic);
            row.settings = match (info.mesh_normal_policy, info.texture_role) {
                (Some(MeshNormalPolicy::Flat), _) => Some("Normals: Flat"),
                (Some(MeshNormalPolicy::AuthoredWhenComplete), _) => Some("Normals: From the file"),
                (_, Some(TextureRole::BaseColor)) => Some(Text::BaseColorSrgb.plain()),
                (_, Some(TextureRole::Roughness)) => Some(Text::RoughnessMapLinear.plain()),
                (_, Some(TextureRole::Metallic)) => Some(Text::MetallicMapLinear.plain()),
                (_, Some(TextureRole::Normal)) => Some(Text::NormalMapLinear.plain()),
                (_, Some(TextureRole::Emissive)) => Some(Text::EmissiveMapSrgb.plain()),
                (_, Some(TextureRole::Occlusion)) => Some(Text::OcclusionMapLinear.plain()),
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
        self.rows.extend(materials.iter().map(|material| {
            AssetRow::new(
                &material.id,
                &material.source,
                "Material",
                AssetStatus::Ready,
            )
        }));
        self.issues.clear();
        for issue in issues {
            let entry = self
                .issues
                .entry(issue.path.clone())
                .or_insert(ImportIssue {
                    status: issue.status.clone(),
                    asset_id: issue.asset_id.clone(),
                    last_good_available: false,
                    diagnostic: None,
                });
            let detail = diagnostic(&issue.message);
            if entry
                .diagnostic
                .as_ref()
                .is_none_or(|previous| detail < *previous)
            {
                entry.asset_id.clone_from(&issue.asset_id);
                entry.status = issue.status.clone();
                entry.diagnostic = Some(detail);
            }
        }
        for row in &mut self.rows {
            if let Some(issue) = self.issues.get(&row.source).filter(|issue| {
                row.status == AssetStatus::Ready
                    && issue.asset_id.as_deref().is_none_or(|id| id == row.id)
            }) {
                row.status = issue.status.clone();
                row.diagnostic.clone_from(&issue.diagnostic);
            }
        }
        for row in &mut self.rows {
            #[cfg(target_os = "linux")]
            {
                row.source_restore =
                    assets
                        .missing_source_restore_available(&row.id)
                        .map(|cached| {
                            if !cached {
                                SourceRestore::CacheUnavailable
                            } else if self
                                .issues
                                .get(&format!("{}.rotmeta", row.source))
                                .is_some_and(|issue| issue.status == AssetStatus::Invalid)
                            {
                                SourceRestore::SidecarInvalid
                            } else {
                                SourceRestore::Candidate
                            }
                        });
            }
            if row.status != AssetStatus::Ready {
                // A model is registered in an open store only while its accepted
                // GLB payload is loaded. Other assets can have metadata alone.
                row.last_good_available = row.model
                    || assets.mesh(&row.id).is_some()
                    || assets.texture(&row.id).is_some()
                    || assets.sound(&row.id).is_some()
                    || assets.script(&row.id).is_some();
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
                if issue.asset_id.as_deref().is_some_and(|id| id == row.id) {
                    issue.last_good_available = row.last_good_available;
                } else if issue.asset_id.is_none()
                    || issue.last_good_available && !row.last_good_available
                {
                    issue.asset_id = Some(row.id.clone());
                    issue.status = row.status.clone();
                    issue.last_good_available = row.last_good_available;
                    issue.diagnostic.clone_from(&row.diagnostic);
                }
            }
        }
        let known_sources: BTreeSet<_> = self.rows.iter().map(|row| row.source.clone()).collect();
        let known_ids: BTreeSet<_> = self.rows.iter().map(|row| row.id.clone()).collect();
        let failed_texture_roles: BTreeMap<_, _> = issues
            .iter()
            .filter_map(|issue| issue.asset_id.as_deref().zip(issue.texture_role))
            .collect();
        #[cfg(target_os = "linux")]
        let ready_roughness: BTreeSet<_> = self
            .rows
            .iter()
            .filter(|row| {
                row.status == AssetStatus::Ready
                    && row.settings == Some(Text::RoughnessMapLinear.plain())
            })
            .map(|row| row.source.clone())
            .collect();
        self.rows
            .extend(self.issues.iter().filter_map(|(source, issue)| {
                let represented = issue.asset_id.as_deref().map_or_else(
                    || known_sources.contains(source),
                    |id| known_ids.contains(id),
                );
                if represented {
                    return None;
                }
                let kind = if source.ends_with(rot_assets::MATERIAL_EXTENSION) {
                    "Material"
                } else {
                    match Path::new(source).extension()?.to_str()? {
                        "obj" => "Mesh",
                        "png" => "Texture",
                        "wav" => "Sound",
                        "lua" => Text::ComponentScript.plain(),
                        "glb" => "Model",
                        "prefab" => "Prefab",
                        "rotmeta" => Text::AssetImportSettings.plain(),
                        _ => return None,
                    }
                };
                let mut row = AssetRow::new(source, source, kind, issue.status.clone());
                row.registered = false;
                row.settings = issue
                    .asset_id
                    .as_deref()
                    .and_then(|id| failed_texture_roles.get(id))
                    .map(|role| match role {
                        TextureRole::BaseColor => Text::BaseColorSrgb.plain(),
                        TextureRole::Roughness => Text::RoughnessMapLinear.plain(),
                        TextureRole::Metallic => Text::MetallicMapLinear.plain(),
                        TextureRole::Normal => Text::NormalMapLinear.plain(),
                        TextureRole::Emissive => Text::EmissiveMapSrgb.plain(),
                        TextureRole::Occlusion => Text::OcclusionMapLinear.plain(),
                    });
                row.diagnostic.clone_from(&issue.diagnostic);
                #[cfg(target_os = "linux")]
                if row.settings == Some(Text::BaseColorSrgb.plain())
                    && row.status == AssetStatus::Invalid
                    && ready_roughness.contains(source)
                {
                    row.failed_base_color.clone_from(&issue.asset_id);
                }
                Some(row)
            }));
        // A reserved sidecar ID can identify a native issue row, but MCP
        // reports an asset ID only after an interpretation is registered.
        for issue in self.issues.values_mut() {
            if issue
                .asset_id
                .as_deref()
                .is_some_and(|id| !known_ids.contains(id))
            {
                issue.asset_id = None;
            }
        }
        self.rows
            .sort_by(|left, right| left.searchable.cmp(&right.searchable));
        #[cfg(target_os = "linux")]
        {
            self.move_folders = folders
                .iter()
                .filter(|folder| folder.starts_with("Assets") && folder.to_str().is_some())
                .cloned()
                .collect();
        }
        let mut folders: BTreeSet<_> = folders.into_iter().collect();
        folders.extend([PathBuf::new(), "Assets".into()]);
        let mut names = HashMap::new();
        let roughness_sources: BTreeSet<_> = assets
            .infos()
            .filter(|info| info.texture_role == Some(TextureRole::Roughness))
            .map(|info| info.source.as_str())
            .collect();
        self.by_id.clear();
        for (index, row) in self.rows.iter().enumerate() {
            self.by_id.insert(row.id.clone(), index);
            folders.extend(row.folder.ancestors().map(Path::to_path_buf));
            *names.entry(row.source.clone()).or_insert(0) += 1;
        }
        for row in &mut self.rows {
            row.can_import_roughness = row.source.ends_with(".png")
                && Path::new(&row.source).starts_with("Assets")
                && row.status != AssetStatus::Missing
                && !roughness_sources.contains(row.source.as_str());
            if row.settings == Some(Text::RoughnessMapLinear.plain()) {
                row.label = format!("Roughness · {}", row.label);
            } else if names[&row.source] > 1 {
                if row.settings == Some(Text::BaseColorSrgb.plain()) {
                    row.label = format!("Base Color · {}", row.label);
                } else {
                    row.label = format!("{} · {}", row.label, row.settings.unwrap_or(&row.id));
                }
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
            self.selected = selected_source.and_then(|(source, kind)| {
                let mut replacements = self
                    .rows
                    .iter()
                    .filter(|row| row.source == source && row.kind == kind);
                let candidate = replacements.next()?;
                (replacements.next().is_none()).then(|| candidate.id.clone())
            });
        }
        self.filter_with_selection(preserve_selection);
    }

    fn filter(&mut self) {
        self.filter_with_selection(false);
    }

    fn filter_with_selection(&mut self, preserve_selection: bool) {
        let query = self.query.trim().to_lowercase();
        self.visible = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                self.type_filter.is_none_or(|kind| row.kind == kind.plain())
                    && if query.is_empty() {
                        row.folder == self.folder
                    } else {
                        row.searchable.contains(&query)
                    }
            })
            .map(|(index, _)| index)
            .collect();
        if self.selected.as_ref().is_some_and(|id| {
            self.by_id.get(id).is_none_or(|index| {
                !preserve_selection && self.visible.binary_search(index).is_err()
            })
        }) {
            self.selected = None;
        }
    }

    fn reveal(&mut self, id: &str) -> bool {
        let Some(row) = self
            .by_id
            .get(id)
            .map(|&index| &self.rows[index])
            .filter(|row| row.registered)
        else {
            return false;
        };
        self.folder.clone_from(&row.folder);
        self.query.clear();
        self.type_filter = None;
        self.focus_search = false;
        self.open = true;
        self.selected = Some(id.to_owned());
        self.reveal_focus = Some(id.to_owned());
        self.filter();
        true
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
        let mut filter_changed = false;
        let mut preserve_selection = false;
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
            ui.horizontal(|ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.query)
                        .id_salt(SEARCH)
                        .hint_text(Text::AssetSearch.plain())
                        .desired_width(150.0),
                );
                if std::mem::take(&mut self.focus_search) {
                    search.request_focus();
                }
                let escape = search.has_focus()
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    });
                let clear = !self.query.is_empty()
                    && crate::ui_kit::icon_button(ui, "×", Text::ClearSearch.plain(), None, false)
                        .clicked();
                if escape || clear {
                    self.query.clear();
                }
                if clear {
                    search.request_focus();
                    preserve_selection = true;
                }
                filter_changed = search.changed() || escape || clear;
            });
            ui.horizontal(|ui| {
                let label = ui.label(Text::AssetType.plain());
                let focus =
                    crate::scene_editor::combo_focus::ComboFocus::capture(ui, "asset_type_filter");
                let mut selected = false;
                let prior = self.type_filter;
                let current = prior.unwrap_or(Text::AssetAllTypes).plain();
                let combo = egui::ComboBox::from_id_salt("asset_type_filter")
                    .selected_text(current)
                    .width(90.0)
                    .truncate()
                    .show_ui(ui, |ui| {
                        for option in std::iter::once(None).chain(ASSET_TYPES.into_iter().map(Some))
                        {
                            if ui
                                .selectable_value(
                                    &mut self.type_filter,
                                    option,
                                    option.unwrap_or(Text::AssetAllTypes).plain(),
                                )
                                .clicked()
                            {
                                selected = true;
                                ui.close();
                            }
                        }
                    });
                focus.restore(ui, &combo.response, selected);
                combo.response.labelled_by(label.id).on_hover_text(current);
                if prior != self.type_filter {
                    filter_changed = true;
                    preserve_selection = false;
                }
            });
            ui.add(
                egui::Slider::new(&mut self.size, 40.0..=112.0)
                    .show_value(false)
                    .text(Text::ThumbnailSize.plain()),
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
                                    if ui
                                        .add_enabled(
                                            folder.starts_with("Assets"),
                                            egui::Button::new(Text::AssetNewLuaScript.plain()),
                                        )
                                        .clicked()
                                    {
                                        self.context_action =
                                            Some(ContextAction::CreateScript(folder.clone()));
                                        ui.close();
                                    }
                                    if ui
                                        .add_enabled(
                                            folder.starts_with("Assets"),
                                            egui::Button::new(Text::AssetNewMaterial.plain()),
                                        )
                                        .clicked()
                                    {
                                        self.context_action =
                                            Some(ContextAction::CreateMaterial(folder.clone()));
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
            filter_changed = true;
            preserve_selection = false;
        }
        if filter_changed {
            self.filter_with_selection(preserve_selection);
        }
        let reveal_index = self
            .reveal_focus
            .take()
            .and_then(|id| self.by_id.get(&id).copied());
        if self.visible.is_empty() {
            let status = if loading {
                Text::ImportingAssets
            } else if failed && self.catalog.is_none() {
                Text::AssetsUnavailable
            } else if !self.query.trim().is_empty() || self.type_filter.is_some() {
                Text::NoMatchingAssets
            } else if self.folder == Path::new("Assets") && self.rows.is_empty() {
                Text::EmptyAssets
            } else {
                Text::NoAssetsInThisFolder
            };
            ui.label(crate::strings::tr(ui.ctx(), status));
            return (None, None);
        }
        let tile_width = self.size.max(88.0);
        let tile_height = self.size + ui.text_style_height(&egui::TextStyle::Small) + 6.0;
        let gap = ui.spacing().item_spacing.x;
        let columns = ((ui.available_width() + gap) / (tile_width + gap))
            .floor()
            .max(1.0) as usize;
        let reveal_row = reveal_index
            .and_then(|index| self.visible.binary_search(&index).ok())
            .map(|index| index / columns);
        let mut scroll = egui::ScrollArea::vertical().id_salt("v23_asset_tiles");
        if let Some(line) = reveal_row {
            // Use the same stride as show_rows; the scroll area clamps the last row.
            scroll = scroll
                .vertical_scroll_offset(line as f32 * (tile_height + ui.spacing().item_spacing.y));
        } else if filter_changed {
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
                                if reveal_index == Some(index) {
                                    response.request_focus();
                                }
                                let placeable = can_place && row.can_place(meshes);
                                let editable_material =
                                    row.kind == "Material" && row.status == AssetStatus::Ready;
                                let draggable = placeable || editable_material;
                                let drag = || {
                                    self.catalog.clone().map(|catalog| AssetDrag {
                                        catalog,
                                        id: row.id.clone(),
                                        prefab: row.kind == "Prefab",
                                        material: row.kind == "Material",
                                    })
                                };
                                if response.drag_started() && draggable {
                                    if let Some(payload) = drag() {
                                        response.dnd_set_drag_payload(payload);
                                    }
                                }
                                if response.clicked() {
                                    selected = Some(row.id.clone());
                                }
                                if response.double_clicked() {
                                    if placeable {
                                        place = drag();
                                    } else if editable_material {
                                        self.edit_material = Some(row.id.clone());
                                    }
                                }
                                response.context_menu(|ui| {
                                    if placeable
                                        && ui.button(Text::AssetPlaceInScene.plain()).clicked()
                                    {
                                        place = drag();
                                        ui.close();
                                    }
                                    if editable_material
                                        && ui.button(Text::MaterialEditAsset.plain()).clicked()
                                    {
                                        self.edit_material = Some(row.id.clone());
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

    pub(super) fn selected_usage_asset(&self) -> Option<(&str, AssetKind)> {
        let row = self.rows.get(*self.by_id.get(self.selected.as_ref()?)?)?;
        if !row.registered {
            return None;
        }
        let kind = match row.kind {
            "Texture" => AssetKind::Texture,
            kind if kind == Text::ComponentScript.plain() => AssetKind::Script,
            _ => return None,
        };
        Some((&row.id, kind))
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
            #[cfg(target_os = "linux")]
            let (message, recovery) = if row.failed_base_color.is_some() {
                (
                    Text::AssetBaseColorImportFailed,
                    Text::AssetRoughnessStillReady,
                )
            } else {
                (message, recovery)
            };
            let message = if row.kind == "Prefab" && row.status == AssetStatus::Invalid {
                Text::PrefabInvalid
            } else if row.kind == "Material" && row.status == AssetStatus::Invalid {
                Text::MaterialInvalid
            } else {
                message
            };
            ui.colored_label(editor_theme::colors(ui).error, message.plain());
            #[cfg(target_os = "linux")]
            let recovery = row.source_restore.map_or(recovery, SourceRestore::guidance);
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
            #[cfg(target_os = "linux")]
            if let Some(id) = row
                .failed_base_color
                .as_ref()
                .filter(|_| ui.button(Text::AssetClearFailedBaseColor.plain()).clicked())
            {
                self.context_action = Some(ContextAction::File(FileAction::ClearFailedBaseColor {
                    source: PathBuf::from(&row.source),
                    id: id.clone(),
                }));
            }
        }
        #[cfg(target_os = "linux")]
        ui.menu_button(Text::AssetActions.plain(), |ui| {
            if let Some(action) = file_actions(ui, row) {
                self.context_action = Some(action);
            }
        });
        if row.kind == "Material"
            && row.status == AssetStatus::Ready
            && ui.button(Text::MaterialEditAsset.plain()).clicked()
        {
            self.edit_material = Some(row.id.clone());
        }
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
                material: false,
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
    let can_move_folder = can_move
        && row.registered
        && row.status == AssetStatus::Ready
        && source
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "obj" | "lua" | "wav"));
    let action = if row.source_restore == Some(SourceRestore::Candidate)
        && ui.button(Text::AssetRestoreMissingSource.plain()).clicked()
    {
        Some(ContextAction::File(FileAction::RestoreMissingSource {
            id: row.id.clone(),
        }))
    } else if row.registered
        && row.kind == Text::ComponentScript.plain()
        && ui
            .add_enabled(
                can_move,
                egui::Button::new(Text::AssetOpenCodeEditor.plain()),
            )
            .clicked()
    {
        Some(ContextAction::OpenScript {
            id: row.id.clone(),
            source: source.to_owned(),
        })
    } else if row.can_import_roughness && ui.button(Text::AssetImportAsRoughness.plain()).clicked()
    {
        Some(ContextAction::File(FileAction::InterpretRoughness(
            source.to_owned(),
        )))
    } else if ui
        .add_enabled(
            can_move,
            egui::Button::new(Text::AssetRenameCommand.plain()),
        )
        .clicked()
    {
        Some(ContextAction::Rename(source.to_owned()))
    } else if ui
        .add_enabled(
            can_move_folder,
            egui::Button::new(Text::AssetMoveFolderCommand.plain()),
        )
        .on_disabled_hover_text(Text::AssetMoveFolderUnsupported.plain())
        .clicked()
    {
        Some(ContextAction::Move {
            id: row.id.clone(),
            source: source.to_owned(),
        })
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
    pub(super) fn reveal_mesh_in_assets(&mut self, mesh_id: &str) {
        if self.authored_edit_pending() || self.ai_transition_pending() {
            self.error = Some(Text::AssetRevealMeshFinishEdit.plain().into());
            return;
        }
        let revealed = self
            .assets
            .as_ref()
            .filter(|assets| {
                self.asset_browser
                    .catalog
                    .as_ref()
                    .is_some_and(|catalog| assets.matches_identity(catalog))
            })
            .and_then(|assets| {
                if let Some(info) = assets
                    .asset_info(mesh_id)
                    .filter(|info| info.kind == AssetKind::Mesh)
                {
                    Some(info.id.as_str())
                } else {
                    assets
                        .model_mesh_owner(mesh_id)
                        .map(|model| model.id.as_str())
                }
            })
            .is_some_and(|id| self.asset_browser.reveal(id));
        self.error = (!revealed).then(|| Text::AssetRevealMeshUnavailable.plain().into());
    }

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
        if let Some(id) = self.asset_browser.take_edit_material() {
            self.begin_material_asset(&id);
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
            Some(ContextAction::CreateScript(parent)) => {
                self.asset_browser.open_new_script(parent);
                ctx.request_repaint();
            }
            Some(ContextAction::CreateMaterial(parent)) => {
                self.asset_browser.open_new_material(parent);
                ctx.request_repaint();
            }
            Some(ContextAction::OpenScript { id, source }) => {
                if let Some(catalog) = self.asset_browser.catalog.clone() {
                    self.submit_asset_action(
                        catalog,
                        FileAction::OpenScript {
                            id,
                            source,
                            launch: self.workspace.prefs.external_editor.clone(),
                            diagnostic: None,
                        },
                    );
                }
            }
            Some(ContextAction::Rename(source)) => {
                let stem = source
                    .file_name()
                    .and_then(|leaf| leaf.to_str())
                    .and_then(split_asset_filename)
                    .map(|(stem, _)| stem.to_owned())
                    .unwrap_or_else(|| {
                        source
                            .file_stem()
                            .map(|stem| stem.to_string_lossy().into_owned())
                            .unwrap_or_default()
                    });
                self.asset_browser
                    .start_file_dialog(FileDialog::Rename { source, stem });
                ctx.request_repaint();
            }
            Some(ContextAction::Move { id, source }) => {
                self.asset_browser.start_file_dialog(FileDialog::Move {
                    id,
                    source,
                    folder: None,
                });
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
                    crate::platform::shell::open_in_file_manager(&folder)
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
    pub(super) fn open_project_error_source(
        &mut self,
        id: String,
        hash: String,
        line: Option<u32>,
    ) {
        let (Some(catalog), Some(info)) = (
            self.asset_browser.catalog.clone(),
            self.assets.as_ref().and_then(|store| store.asset_info(&id)),
        ) else {
            self.asset_import_error = Some(Text::LuaErrorSourceUnavailable.plain().into());
            return;
        };
        self.submit_asset_action(
            catalog,
            FileAction::OpenScript {
                id,
                source: PathBuf::from(&info.source),
                launch: self.workspace.prefs.external_editor.clone(),
                diagnostic: Some(ScriptLocation {
                    expected_hash: hash,
                    line,
                }),
            },
        );
    }

    #[cfg(target_os = "linux")]
    fn submit_asset_action(&mut self, catalog: AssetStateIdentity, action: FileAction) {
        if self.asset_import.is_some() || self.ai_asset_write.is_some() {
            self.asset_import_error = Some(Text::AssetOperationBusy.plain().into());
            return;
        }
        let expected_move = match &action {
            FileAction::Move { id, source, folder } => source
                .file_name()
                .map(|leaf| (id.clone(), folder.join(leaf))),
            _ => None,
        };
        let result = self
            .asset_watcher
            .as_ref()
            .ok_or_else(|| Text::AssetProjectUnavailable.plain().to_owned())
            .and_then(|worker| worker.file_action(action, catalog));
        match result {
            Ok(reply) => {
                self.asset_import = Some(reply);
                self.asset_move = expected_move;
                self.asset_import_error = None;
            }
            Err(error) => self.asset_import_error = Some(error),
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn select_published_asset(&mut self, id: String, destination: &Path, created: bool) {
        if self
            .assets
            .as_ref()
            .and_then(|assets| assets.asset_info(&id))
            .is_some_and(|info| {
                Path::new(&info.source) == destination && info.status == AssetStatus::Ready
            })
        {
            if created {
                self.asset_browser.query.clear();
            }
            if let Some(parent) = destination.parent() {
                self.asset_browser.folder = parent.to_owned();
            }
            self.asset_browser.selected = Some(id);
            self.asset_browser.filter();
        } else {
            self.asset_import_error = Some(
                if created {
                    Text::AssetLuaNotReady
                } else {
                    Text::AssetMoveCatalogChanged
                }
                .plain()
                .into(),
            );
        }
    }

    #[cfg(target_os = "linux")]
    pub(super) fn select_created_material(&mut self, id: String, destination: &Path) {
        if self
            .material_files
            .get(&id)
            .is_some_and(|file| Path::new(&file.source) == destination)
        {
            self.asset_browser.query.clear();
            if let Some(parent) = destination.parent() {
                self.asset_browser.folder = parent.to_owned();
            }
            self.asset_browser.selected = Some(id);
            self.asset_browser.filter();
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

        browser.open_new_script("Assets/Props".into());
        frame(&mut browser, Vec::new());
        frame(
            &mut browser,
            vec![egui::Event::Text("Rail Signal.lua".into())],
        );
        let (_, action) = frame(&mut browser, vec![press(egui::Key::Enter)]).unwrap();
        assert!(matches!(action, FileAction::CreateScript { parent, name }
            if parent == Path::new("Assets/Props") && name == "Rail Signal.lua"));

        browser.open_new_script("Assets/Props".into());
        frame(&mut browser, Vec::new());
        assert!(frame(&mut browser, vec![press(egui::Key::Escape)]).is_none());
        assert!(!browser.file_dialog_open());

        browser.start_file_dialog(FileDialog::Delete("Assets/Prop.obj".into()));
        frame(&mut browser, Vec::new());
        assert!(frame(&mut browser, vec![press(egui::Key::Escape)]).is_none());
        assert!(!browser.file_dialog_open());
    }

    #[test]
    fn asset_type_combines_with_folder_search_and_catalog_refresh() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let nested = root.join("Assets/Depot");
        fs::create_dir_all(&nested).unwrap();
        let mesh = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";
        let mut store = rot_assets::AssetStore::open(root).unwrap();
        for (id, source, bytes) in [
            ("top-mesh", "Assets/Rail.obj", &mesh[..]),
            ("nested-mesh", "Assets/Depot/Rail.obj", &mesh[..]),
            ("nested-script", "Assets/Depot/Rail.lua", b"return nil"),
        ] {
            fs::write(root.join(source), bytes).unwrap();
            store.import(id, source).unwrap();
        }

        let mut browser = Browser::default();
        let visible = |browser: &Browser| {
            browser
                .visible
                .iter()
                .map(|&index| browser.rows[index].id.clone())
                .collect::<Vec<_>>()
        };
        browser.refresh(&store, Vec::new(), &[], &[], &[]);
        assert_eq!(visible(&browser), ["top-mesh"]);
        browser.selected = Some("top-mesh".into());

        browser.type_filter = Some(Text::ComponentScript);
        browser.filter();
        assert!(visible(&browser).is_empty());
        assert!(browser.selected.is_none());
        browser.query = "  rAiL  ".into();
        browser.filter();
        assert_eq!(visible(&browser), ["nested-script"]);
        browser.selected = Some("nested-script".into());

        let ctx = egui::Context::default();
        let frame = |browser: &mut Browser, events| {
            ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    browser.show(ui, &BTreeMap::new(), &BTreeMap::new(), false, false, false);
                },
            )
            .textures_delta
            .clear();
        };
        let key = |key| {
            [true, false]
                .into_iter()
                .map(|pressed| egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                })
                .collect()
        };
        browser.focus_search = true;
        frame(&mut browser, vec![]);
        let search_focus = ctx.memory(|memory| memory.focused()).unwrap();
        frame(&mut browser, key(egui::Key::Tab));
        assert_ne!(ctx.memory(|memory| memory.focused()), Some(search_focus));
        frame(&mut browser, key(egui::Key::Enter));
        assert!(browser.query.is_empty());
        assert_eq!(ctx.memory(|memory| memory.focused()), Some(search_focus));
        assert_eq!(browser.folder, Path::new("Assets"));
        assert_eq!(browser.type_filter, Some(Text::ComponentScript));
        assert!(visible(&browser).is_empty());
        assert_eq!(browser.selected.as_deref(), Some("nested-script"));
        browser.refresh(&store, Vec::new(), &[], &[], &[]);
        assert_eq!(browser.selected.as_deref(), Some("nested-script"));

        browser.folder = "Assets/Depot".into();
        browser.query.clear();
        browser.filter();
        assert_eq!(visible(&browser), ["nested-script"]);
        assert_eq!(browser.selected.as_deref(), Some("nested-script"));
        fs::write(nested.join("Support.lua"), "return nil").unwrap();
        store.import("support", "Assets/Depot/Support.lua").unwrap();
        browser.refresh(&store, Vec::new(), &[], &[], &[]);
        assert_eq!(browser.type_filter, Some(Text::ComponentScript));
        assert_eq!(visible(&browser), ["nested-script", "support"]);
        assert_eq!(browser.selected.as_deref(), Some("nested-script"));

        browser.type_filter = None;
        browser.filter();
        assert_eq!(visible(&browser).len(), 3);
        assert_eq!(browser.selected.as_deref(), Some("nested-script"));
    }

    #[test]
    fn reveal_mesh_uses_exact_asset_identity_and_focused_tile() {
        use rot_assets::fixtures::animated_glb;

        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        for side in ["A", "B"] {
            let folder = root.join("Assets").join(side);
            fs::create_dir_all(&folder).unwrap();
            fs::write(
                folder.join("Rail.obj"),
                "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
            )
            .unwrap();
        }
        let obj = "Assets/B/Rail.obj";
        let glb = "Assets/B/Rail.glb";
        fs::write(root.join(glb), animated_glb(Default::default())).unwrap();
        let mut store = rot_assets::AssetStore::open(root).unwrap();
        store.import("rail-a", "Assets/A/Rail.obj").unwrap();
        store.import("rail-b", obj).unwrap();
        let model = store
            .import_glb_at(store.revision(), "rail-model", glb)
            .unwrap();
        let mesh = model.nodes["Rig/Arm"].mesh_id.clone().unwrap();
        fs::write(root.join(glb), b"bad refresh").unwrap();
        assert!(store.refresh_glb_at(store.revision(), &model.id).is_err());

        let path = root.join("scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        editor.assets = Some(store);
        editor
            .asset_browser
            .refresh(editor.assets.as_ref().unwrap(), Vec::new(), &[], &[], &[]);
        let selection = editor.selection.clone();
        editor.asset_browser.selected = Some("rail-a".into());
        editor.asset_browser.query = "other".into();
        editor.asset_browser.type_filter = Some(Text::ComponentScript);
        editor.asset_browser.open = false;

        editor.reveal_mesh_in_assets("unknown");
        assert_eq!(editor.asset_browser.selected.as_deref(), Some("rail-a"));
        assert_eq!(editor.asset_browser.query, "other");
        assert!(!editor.asset_browser.open);

        editor.draft.as_mut().unwrap().name = "Pending Name".into();
        editor.reveal_mesh_in_assets("rail-b");
        assert_eq!(editor.draft.as_ref().unwrap().name, "Pending Name");
        assert_eq!(editor.session.revision(), 0);
        assert_eq!(editor.asset_browser.selected.as_deref(), Some("rail-a"));
        editor.rebuild_draft();

        editor.reveal_mesh_in_assets("rail-b");
        assert!(editor.asset_browser.open);
        assert_eq!(editor.asset_browser.folder, Path::new("Assets/B"));
        assert!(
            editor.asset_browser.query.is_empty() && editor.asset_browser.type_filter.is_none()
        );
        assert_eq!(editor.asset_browser.selected.as_deref(), Some("rail-b"));
        assert_eq!(editor.asset_browser.reveal_focus.as_deref(), Some("rail-b"));

        editor.reveal_mesh_in_assets(&mesh);
        assert_eq!(editor.asset_browser.selected.as_deref(), Some("rail-model"));
        assert_eq!(
            editor.asset_browser.reveal_focus.as_deref(),
            Some("rail-model")
        );
        assert_eq!(editor.session.scene(), &scene);
        assert_eq!(editor.selection, selection);
        assert_eq!(editor.session.revision(), 0);
        assert!(!editor.session.is_dirty());

        let ctx = egui::Context::default();
        let mut focused_selection = None;
        let mut draw = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events,
                    ..Default::default()
                },
                |ui| {
                    focused_selection = editor
                        .asset_browser
                        .show(ui, &editor.meshes, &editor.textures, false, false, false)
                        .0;
                },
            );
            output.textures_delta.clear();
        };
        draw(Vec::new());
        draw(vec![egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert_eq!(focused_selection.as_deref(), Some("rail-model"));
    }

    #[test]
    fn roughness_alias_keeps_its_ready_tile_and_scoped_issue() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        fs::create_dir(root.join("Assets")).unwrap();
        let source = "Assets/Gray.png";
        let mut png = Vec::new();
        let mut encoder = png::Encoder::new(&mut png, 1, 1);
        encoder.set_color(png::ColorType::Grayscale);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[128])
            .unwrap();
        fs::write(root.join(source), png).unwrap();
        let (mut store, diagnostics) = rot_assets::AssetStore::open_project(root).unwrap();
        assert_eq!(diagnostics.len(), 1);

        store
            .interpret_project_texture(source, TextureRole::Roughness, || true)
            .unwrap();
        let issues = store.reconcile_project_files(&BTreeSet::from([source.into()]), || true);
        let roughness = store
            .infos()
            .find(|info| info.texture_role == Some(TextureRole::Roughness))
            .unwrap()
            .id
            .clone();
        let mut browser = Browser::default();
        browser.refresh(&store, Vec::new(), &issues, &[], &[]);
        let ready = &browser.rows[browser.by_id[&roughness]];
        assert_eq!(ready.status, AssetStatus::Ready);
        assert_eq!(ready.settings, Some(Text::RoughnessMapLinear.plain()));
        assert_eq!(ready.label, "Roughness · Gray.png");
        let alias = &browser.rows[browser.by_id[source]];
        assert_eq!(alias.settings, Some(Text::BaseColorSrgb.plain()));
        assert_eq!(alias.status, AssetStatus::Invalid);
        assert_eq!(alias.label, "Base Color · Gray.png");
        #[cfg(target_os = "linux")]
        assert_eq!(alias.failed_base_color, issues[0].asset_id);
        assert_eq!(browser.issues[source].asset_id, None);
        assert!(!browser.issues[source].last_good_available);

        let same_id_failure = rot_assets::ProjectFileIssue {
            asset_id: Some(roughness.clone()),
            texture_role: Some(TextureRole::Roughness),
            path: source.into(),
            status: AssetStatus::Invalid,
            message: "Source refresh failed.".into(),
        };
        browser.refresh(&store, Vec::new(), &[same_id_failure], &[], &[]);
        assert!(browser.rows[browser.by_id[&roughness]].last_good_available);
        assert!(browser.issues[source].last_good_available);
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
        browser.refresh(&store, vec![PathBuf::from("Assets/Empty")], &[], &[], &[]);
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
            material: false,
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
            .refresh(&store, Vec::new(), &issues, &[], &[]);
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
