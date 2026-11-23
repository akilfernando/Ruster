//! Window preferences are independent from scene data and runtime state.
use crate::strings::Text;
use eframe::egui;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet},
    io::Write,
    path::{Path, PathBuf},
};

const MAX_SETTINGS_BYTES: usize = 64 * 1024;
const MAX_WORKSPACE_BYTES: usize = 1024 * 1024;

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(super) struct SceneViewState {
    pub hidden: BTreeSet<String>,
    pub locked: BTreeSet<String>,
}

fn read_scene_view(path: &Path) -> Result<SceneViewState, String> {
    match path.symlink_metadata() {
        Ok(metadata) if !metadata.is_file() => {
            return Err("Scene view state must be a regular file.".into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(SceneViewState::default());
        }
        Err(error) => return Err(error.to_string()),
        Ok(_) => {}
    }
    let input = std::fs::File::open(path).map_err(|error| error.to_string())?;
    serde_json::from_reader(std::io::BufReader::new(input)).map_err(|error| error.to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    New,
    Open,
    Save,
    SaveAs,
    Export,
    Undo,
    Redo,
    Copy,
    Paste,
    Duplicate,
    Delete,
    Move,
    Rotate,
    Scale,
    Frame,
    Projection,
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
    Isometric,
    FreeView,
    Quit,
    FindObjects,
    FindContent,
    Rename,
    SelectAll,
    FocusRuntime,
    CreateCube,
    CreateSphere,
    CreateCapsule,
    CreateTriangularPrism,
    CreateLight,
    Play,
    Pause,
    Step,
    NextPanel,
    PreviousPanel,
    NewProject,
    OpenProject,
    InputActions,
    ProjectSettings,
    Preferences,
    ImportAsset,
    CreateShaderSource,
    RefreshShader,
    CreateEmpty,
    GroupSelection,
    CreateDirectionalLight,
    GameCamera,
    SetGameCameraFromView,
    GameUi,
    AssignCube,
    ToggleHierarchy,
    ToggleInspector,
    ToggleAssets,
    ToggleConsole,
    AiConnection,
    PlayChecks,
    ScriptDebugger,
    RecoveryCheckpoints,
    LayoutDefault,
    LayoutSideBySide,
    ResetLayout,
    Documentation,
    KeyboardShortcuts,
    GameControls,
    About,
    CommandPalette,
}
impl Action {
    pub const ALL: [Self; 71] = [
        Self::New,
        Self::Open,
        Self::Save,
        Self::SaveAs,
        Self::Export,
        Self::Undo,
        Self::Redo,
        Self::Copy,
        Self::Paste,
        Self::Duplicate,
        Self::Delete,
        Self::Move,
        Self::Rotate,
        Self::Scale,
        Self::Frame,
        Self::Projection,
        Self::Front,
        Self::Back,
        Self::Left,
        Self::Right,
        Self::Top,
        Self::Bottom,
        Self::Isometric,
        Self::FreeView,
        Self::Quit,
        Self::FindObjects,
        Self::FindContent,
        Self::Rename,
        Self::SelectAll,
        Self::FocusRuntime,
        Self::CreateCube,
        Self::CreateSphere,
        Self::CreateCapsule,
        Self::CreateTriangularPrism,
        Self::CreateLight,
        Self::Play,
        Self::Pause,
        Self::Step,
        Self::NextPanel,
        Self::PreviousPanel,
        Self::NewProject,
        Self::OpenProject,
        Self::InputActions,
        Self::ProjectSettings,
        Self::Preferences,
        Self::ImportAsset,
        Self::CreateShaderSource,
        Self::RefreshShader,
        Self::CreateEmpty,
        Self::GroupSelection,
        Self::CreateDirectionalLight,
        Self::GameCamera,
        Self::SetGameCameraFromView,
        Self::GameUi,
        Self::AssignCube,
        Self::ToggleHierarchy,
        Self::ToggleInspector,
        Self::ToggleAssets,
        Self::ToggleConsole,
        Self::AiConnection,
        Self::PlayChecks,
        Self::ScriptDebugger,
        Self::RecoveryCheckpoints,
        Self::LayoutDefault,
        Self::LayoutSideBySide,
        Self::ResetLayout,
        Self::Documentation,
        Self::KeyboardShortcuts,
        Self::GameControls,
        Self::About,
        Self::CommandPalette,
    ];
    /// Title Case menu label (UX-STANDARDS §8). `name` stays the stored id.
    pub fn label(self) -> &'static str {
        match self {
            Self::New => Text::LabelNew.plain(),
            Self::Open => Text::LabelOpen.plain(),
            Self::Save => Text::LabelSave.plain(),
            Self::SaveAs => Text::LabelSaveAs.plain(),
            Self::Export => Text::LabelExport.plain(),
            Self::Undo => Text::LabelUndo.plain(),
            Self::Redo => Text::LabelRedo.plain(),
            Self::Copy => Text::LabelCopy.plain(),
            Self::Paste => Text::LabelPaste.plain(),
            Self::Duplicate => Text::LabelDuplicate.plain(),
            Self::Delete => Text::MaterialDelete.plain(),
            Self::Move => Text::LabelMove.plain(),
            Self::Rotate => Text::LabelRotate.plain(),
            Self::Scale => Text::LabelScale.plain(),
            Self::Frame => Text::LabelFrame.plain(),
            Self::Projection => Text::LabelProjection.plain(),
            Self::Front => Text::LabelFront.plain(),
            Self::Back => Text::LabelBack.plain(),
            Self::Left => Text::LabelLeft.plain(),
            Self::Right => Text::LabelRight.plain(),
            Self::Top => Text::LabelTop.plain(),
            Self::Bottom => Text::LabelBottom.plain(),
            Self::Isometric => Text::LabelIsometric.plain(),
            Self::FreeView => Text::LabelFreeView.plain(),
            Self::Quit => Text::LabelQuit.plain(),
            Self::FindObjects => Text::LabelFindObjects.plain(),
            Self::FindContent => Text::AssetSearch.plain(),
            Self::Rename => Text::AssetRename.plain(),
            Self::SelectAll => Text::LabelSelectAll.plain(),
            Self::FocusRuntime => Text::LabelFocusRuntime.plain(),
            Self::CreateCube => Text::LabelCreateCube.plain(),
            Self::CreateSphere => Text::ComponentSphere.plain(),
            Self::CreateCapsule => Text::LabelCreateCapsule.plain(),
            Self::CreateTriangularPrism => Text::LabelCreateTriangularPrism.plain(),
            Self::CreateLight => Text::LabelCreateLight.plain(),
            Self::Play => Text::LabelPlay.plain(),
            Self::Pause => Text::LabelPause.plain(),
            Self::Step => Text::LabelStep.plain(),
            Self::NextPanel => Text::LabelNextPanel.plain(),
            Self::PreviousPanel => Text::LabelPreviousPanel.plain(),
            Self::NewProject => Text::LabelNewProject.plain(),
            Self::OpenProject => Text::LabelOpenProject.plain(),
            Self::InputActions => Text::LabelInputActions.plain(),
            Self::ProjectSettings => Text::LabelProjectSettings.plain(),
            Self::Preferences => Text::LabelPreferences.plain(),
            Self::ImportAsset => Text::LabelImportAsset.plain(),
            Self::CreateShaderSource => Text::LabelCreateShaderSource.plain(),
            Self::RefreshShader => Text::LabelRefreshShader.plain(),
            Self::CreateEmpty => Text::LabelCreateEmpty.plain(),
            Self::GroupSelection => Text::LabelGroupSelection.plain(),
            Self::CreateDirectionalLight => Text::LabelCreateDirectionalLight.plain(),
            Self::GameCamera => Text::LabelGameCamera.plain(),
            Self::SetGameCameraFromView => Text::SetGameCameraFromView.plain(),
            Self::GameUi => Text::LabelGameUi.plain(),
            Self::AssignCube => Text::LabelAssignCube.plain(),
            Self::ToggleHierarchy => Text::ComponentHierarchy.plain(),
            Self::ToggleInspector => Text::ComponentInspector.plain(),
            Self::ToggleAssets => Text::ComponentAssets.plain(),
            Self::ToggleConsole => Text::LabelToggleConsole.plain(),
            Self::AiConnection => Text::LabelAiConnection.plain(),
            Self::PlayChecks => Text::LabelPlayChecks.plain(),
            Self::ScriptDebugger => Text::LabelScriptDebugger.plain(),
            Self::RecoveryCheckpoints => Text::LabelRecoveryCheckpoints.plain(),
            Self::LayoutDefault => Text::LabelLayoutDefault.plain(),
            Self::LayoutSideBySide => Text::LabelLayoutSideBySide.plain(),
            Self::ResetLayout => Text::LabelResetLayout.plain(),
            Self::Documentation => Text::LabelDocumentation.plain(),
            Self::KeyboardShortcuts => Text::LabelKeyboardShortcuts.plain(),
            Self::GameControls => Text::LabelGameControls.plain(),
            Self::About => Text::LabelAbout.plain(),
            Self::CommandPalette => Text::LabelCommandPalette.plain(),
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::New => "New scene",
            Self::Open => "Open scene",
            Self::Save => "Save scene",
            Self::SaveAs => "Save scene as",
            Self::Export => "Export application",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::Duplicate => "Duplicate",
            Self::Delete => "Delete",
            Self::Move => "Move tool",
            Self::Rotate => "Rotate tool",
            Self::Scale => "Scale tool",
            Self::Frame => "Frame selected",
            Self::Projection => "Toggle projection",
            Self::Front => "Front view",
            Self::Back => "Back view",
            Self::Left => "Left view",
            Self::Right => "Right view",
            Self::Top => "Top view",
            Self::Bottom => "Bottom view",
            Self::Isometric => "Isometric view",
            Self::FreeView => "Free view",
            Self::Quit => "Close window",
            Self::FindObjects => "Search objects",
            Self::FindContent => "Search Content",
            Self::Rename => "Rename active object",
            Self::SelectAll => "Select all scene objects",
            Self::FocusRuntime => "Focus runtime controls",
            Self::CreateCube => "Create cube",
            Self::CreateSphere => "Create sphere",
            Self::CreateCapsule => "Create capsule",
            Self::CreateTriangularPrism => "Create triangular prism",
            Self::CreateLight => "Create light",
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::Step => "Step",
            Self::NextPanel => "Next panel",
            Self::PreviousPanel => "Previous panel",
            Self::NewProject => "New project",
            Self::OpenProject => "Open project",
            Self::InputActions => "Input actions",
            Self::ProjectSettings => "Project settings",
            Self::Preferences => "Preferences",
            Self::ImportAsset => "Import asset",
            Self::CreateShaderSource => "Create shader source",
            Self::RefreshShader => "Refresh scene shader",
            Self::CreateEmpty => "Create empty",
            Self::GroupSelection => "Group selection",
            Self::CreateDirectionalLight => "Create directional light",
            Self::GameCamera => "Game camera",
            Self::SetGameCameraFromView => "Set Game Camera from View",
            Self::GameUi => "Game UI",
            Self::AssignCube => "Assign procedural cube",
            Self::ToggleHierarchy => "Toggle Hierarchy",
            Self::ToggleInspector => "Toggle Inspector",
            Self::ToggleAssets => "Toggle Assets",
            Self::ToggleConsole => "Toggle Console",
            Self::AiConnection => "AI connection",
            Self::PlayChecks => "Play checks",
            Self::ScriptDebugger => "Script debugger",
            Self::RecoveryCheckpoints => "Recovery checkpoints",
            Self::LayoutDefault => "Default layout",
            Self::LayoutSideBySide => "Side by side layout",
            Self::ResetLayout => "Reset layout",
            Self::Documentation => "Documentation",
            Self::KeyboardShortcuts => "Keyboard shortcuts",
            Self::GameControls => "Game controls",
            Self::About => "About",
            Self::CommandPalette => "Command palette",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Binding {
    pub action: Action,
    pub key: Option<egui::Key>,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}
impl Binding {
    pub fn text(self) -> String {
        let Some(key) = self.key else {
            return "Unbound".into();
        };
        format!(
            "{}{}{}{}",
            if self.ctrl { "Ctrl+" } else { "" },
            if self.alt { "Alt+" } else { "" },
            if self.shift { "Shift+" } else { "" },
            key.symbol_or_name()
        )
    }
    fn modifiers(self) -> egui::Modifiers {
        egui::Modifiers {
            ctrl: self.ctrl,
            shift: self.shift,
            alt: self.alt,
            ..Default::default()
        }
    }
    fn same_keys(self, other: Self) -> bool {
        self.key.is_some()
            && self.key == other.key
            && self.ctrl == other.ctrl
            && self.shift == other.shift
            && self.alt == other.alt
    }
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Preferences {
    pub hierarchy_width: f32,
    pub inspector_width: f32,
    pub hierarchy_side: Side,
    pub inspector_side: Side,
    pub hierarchy_visible: bool,
    pub inspector_visible: bool,
    pub content_visible: bool,
    pub scale: f32,
    pub movement_speed: f32,
    pub recent: Vec<PathBuf>,
    /// Most recently opened scene per project directory, newest first.
    pub projects: Vec<PathBuf>,
    pub cameras: Vec<(PathBuf, rot_render::CameraState)>,
    pub bindings: Vec<Binding>,
    pub external_editor: crate::behavior_files::LaunchSpec,
    pub theme: crate::editor_theme::ThemeChoice,
    /// Rot or Plain. `None` until the creator picks one at first launch.
    pub style: Option<crate::editor_theme::Style>,
    /// Playful Rot lines where the wording table has them (UX-STANDARDS §9).
    pub themed_wording: bool,
    /// Let AI apps find this editor and ask to connect. Approval still applies.
    pub allow_ai_connections: bool,
}
impl Default for Preferences {
    fn default() -> Self {
        use egui::Key;
        let defaults = [
            (Action::New, Key::N, true, false),
            (Action::Open, Key::O, true, false),
            (Action::Save, Key::S, true, false),
            (Action::SaveAs, Key::S, true, true),
            (Action::Export, Key::E, true, true),
            (Action::Undo, Key::Z, true, false),
            (Action::Redo, Key::Z, true, true),
            (Action::Copy, Key::C, true, false),
            (Action::Paste, Key::V, true, false),
            (Action::Duplicate, Key::D, true, false),
            (Action::Delete, Key::Delete, false, false),
            (Action::Move, Key::W, false, false),
            (Action::Rotate, Key::E, false, false),
            (Action::Scale, Key::R, false, false),
            (Action::Frame, Key::F, false, false),
            (Action::Projection, Key::Num5, false, false),
            (Action::Front, Key::Num1, false, false),
            (Action::Back, Key::Num1, true, false),
            (Action::Left, Key::Num3, true, false),
            (Action::Right, Key::Num3, false, false),
            (Action::Top, Key::Num7, false, false),
            (Action::Bottom, Key::Num7, true, false),
            (Action::Isometric, Key::Num0, false, false),
            (Action::FreeView, Key::Home, false, false),
            (Action::Quit, Key::Q, true, false),
            (Action::FindObjects, Key::F, true, false),
            (Action::FindContent, Key::F, true, true),
            (Action::Rename, Key::F2, false, false),
            (Action::SelectAll, Key::A, true, false),
            (Action::FocusRuntime, Key::G, true, true),
            (Action::GroupSelection, Key::G, true, false),
            (Action::Play, Key::P, true, false),
            (Action::Pause, Key::P, true, true),
            (Action::NextPanel, Key::F6, false, false),
            (Action::CommandPalette, Key::K, true, false),
            (Action::Preferences, Key::Comma, true, false),
            (Action::PreviousPanel, Key::F6, false, true),
        ];
        Self {
            hierarchy_width: 180.0,
            inspector_width: 340.0,
            hierarchy_side: Side::Left,
            inspector_side: Side::Right,
            hierarchy_visible: true,
            inspector_visible: true,
            content_visible: true,
            theme: Default::default(),
            style: None,
            themed_wording: true,
            allow_ai_connections: true,
            scale: 1.0,
            movement_speed: 4.0,
            recent: Vec::new(),
            projects: Vec::new(),
            cameras: Vec::new(),
            external_editor: crate::behavior_files::LaunchSpec {
                executable: PathBuf::from("zeditor"),
                args: vec!["--new".into(), "{workspace}".into(), "{file}:{line}".into()],
            },
            bindings: defaults
                .into_iter()
                .map(|(action, key, ctrl, shift)| Binding {
                    action,
                    key: Some(key),
                    ctrl,
                    shift,
                    alt: false,
                })
                .chain(
                    [
                        Action::CreateCube,
                        Action::CreateSphere,
                        Action::CreateCapsule,
                        Action::CreateTriangularPrism,
                        Action::CreateLight,
                        Action::NewProject,
                        Action::OpenProject,
                        Action::InputActions,
                        Action::ProjectSettings,
                        Action::ImportAsset,
                        Action::CreateShaderSource,
                        Action::RefreshShader,
                        Action::CreateEmpty,
                        Action::CreateDirectionalLight,
                        Action::GameCamera,
                        Action::SetGameCameraFromView,
                        Action::GameUi,
                        Action::AssignCube,
                        Action::ToggleHierarchy,
                        Action::ToggleInspector,
                        Action::ToggleAssets,
                        Action::ToggleConsole,
                        Action::AiConnection,
                        Action::PlayChecks,
                        Action::ScriptDebugger,
                        Action::RecoveryCheckpoints,
                        Action::LayoutDefault,
                        Action::LayoutSideBySide,
                        Action::ResetLayout,
                        Action::Documentation,
                        Action::KeyboardShortcuts,
                        Action::GameControls,
                        Action::About,
                    ]
                    .into_iter()
                    .map(|action| Binding {
                        action,
                        key: None,
                        ctrl: false,
                        shift: false,
                        alt: false,
                    }),
                )
                // Step is Ctrl+Alt+P, as in Unity.
                .chain([Binding {
                    action: Action::Step,
                    key: Some(Key::P),
                    ctrl: true,
                    shift: false,
                    alt: true,
                }])
                .collect(),
        }
    }
}
impl Preferences {
    #[cfg(test)]
    pub fn json_for_test(&self) -> Value {
        self.json()
    }
    fn json(&self) -> Value {
        json!({"format":"rot-editor-preferences","version":1,"hierarchy_width":self.hierarchy_width,"inspector_width":self.inspector_width,"hierarchy_side":if self.hierarchy_side==Side::Left {"left"} else {"right"},"inspector_side":if self.inspector_side==Side::Left {"left"} else {"right"},"hierarchy_visible":self.hierarchy_visible,"inspector_visible":self.inspector_visible,"content_visible":self.content_visible,"theme":self.theme.name(),"style":self.style.map(|style|style.name()),"themed_wording":self.themed_wording,"allow_ai_connections":self.allow_ai_connections,"external_editor":self.external_editor,"ui_scale":self.scale,"movement_speed":self.movement_speed,"recent":self.recent,"projects":self.projects,"cameras":self.cameras.iter().map(|(path,camera)|json!({"path":path,"target":camera.target,"yaw":camera.yaw,"pitch":camera.pitch,"distance":camera.distance,"orthographic":camera.orthographic})).collect::<Vec<_>>(),"bindings":self.bindings.iter().map(|b|json!({"action":b.action.name(),"key":b.key.map(|key|key.name()),"ctrl":b.ctrl,"shift":b.shift,"alt":b.alt})).collect::<Vec<_>>()})
    }
    pub(super) fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_WORKSPACE_BYTES {
            return Err("Editor preferences and project history exceed 1 MiB.".into());
        }
        let mut v: Value =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid preferences: {e}"))?;
        let raw_projects = v
            .as_object_mut()
            .ok_or("Preferences must be an object.")?
            .remove("projects");
        if bytes.len() > MAX_SETTINGS_BYTES
            && (raw_projects.is_none()
                || serde_json::to_vec_pretty(&v)
                    .map_err(|e| e.to_string())?
                    .len()
                    > MAX_SETTINGS_BYTES)
        {
            return Err(
                "Editor settings exceed 64 KiB; reduce recent context before saving.".into(),
            );
        }
        let keys = [
            "format",
            "version",
            "hierarchy_width",
            "inspector_width",
            "hierarchy_side",
            "inspector_side",
            "hierarchy_visible",
            "inspector_visible",
            "content_visible",
            "ui_scale",
            "movement_speed",
            "recent",
            "cameras",
            "bindings",
            "external_editor",
            "theme",
            "allow_ai_connections",
            "style",
            "themed_wording",
        ];
        let map = v.as_object().ok_or("Preferences must be an object.")?;
        // content_visible, external_editor, theme, allow_ai_connections,
        // style and themed_wording are optional later fields.
        if !(keys.len() - 6..=keys.len()).contains(&map.len())
            || map.keys().any(|k| !keys.contains(&k.as_str()))
            || v["format"] != "rot-editor-preferences"
            || v["version"] != 1
        {
            return Err("Couldn't read your preferences. Reset them to continue.".into());
        }
        let number = |key: &str, low: f64, high: f64| {
            v[key]
                .as_f64()
                .filter(|n| n.is_finite() && *n >= low && *n <= high)
                .map(|v| v as f32)
                .ok_or_else(|| format!("Invalid {key} preference."))
        };
        let side = |key: &str| match v[key].as_str() {
            Some("left") => Ok(Side::Left),
            Some("right") => Ok(Side::Right),
            _ => Err(format!("Invalid {key} preference.")),
        };
        let boolean = |key: &str| {
            v[key]
                .as_bool()
                .ok_or_else(|| format!("Invalid {key} preference."))
        };
        let paths = v["recent"]
            .as_array()
            .filter(|v| v.len() <= 10)
            .ok_or("Recent projects exceeds ten entries.")?;
        let recent = paths
            .iter()
            .map(|v| {
                v.as_str()
                    .filter(|s| s.len() <= 4096)
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .ok_or_else(|| {
                        "Recent project paths must be bounded absolute paths.".to_string()
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let projects = match raw_projects {
            None => {
                // Legacy files retained only ten recent scene paths.
                let mut roots = HashSet::new();
                recent
                    .iter()
                    .filter(|path| path.parent().is_some_and(|root| roots.insert(root)))
                    .cloned()
                    .collect()
            }
            Some(value) => {
                let mut roots = HashSet::new();
                value
                    .as_array()
                    .ok_or("Project history must be an array.")?
                    .iter()
                    .map(|entry| {
                        let scene = entry
                            .as_str()
                            .filter(|s| s.len() <= 4096)
                            .map(Path::new)
                            .filter(|path| path.is_absolute())
                            .ok_or("Invalid project history scene path.")?;
                        let root = scene
                            .parent()
                            .ok_or("Project history requires a scene file.")?;
                        if !roots.insert(root) {
                            return Err("Duplicate project directory in project history.");
                        }
                        Ok(scene.to_owned())
                    })
                    .collect::<Result<Vec<_>, &str>>()?
            }
        };
        let raw_cameras = v["cameras"]
            .as_array()
            .filter(|v| v.len() <= 10)
            .ok_or("Camera context exceeds ten scenes.")?;
        let mut cameras = Vec::with_capacity(raw_cameras.len());
        for entry in raw_cameras {
            let map = entry.as_object().ok_or("Invalid camera context.")?;
            if map.len() != 6
                || map.keys().any(|k| {
                    !["path", "target", "yaw", "pitch", "distance", "orthographic"]
                        .contains(&k.as_str())
                })
            {
                return Err("Invalid camera context fields.".into());
            }
            let path = entry["path"]
                .as_str()
                .filter(|s| s.len() <= 4096)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .ok_or("Invalid camera scene path.")?;
            let target = entry["target"]
                .as_array()
                .filter(|v| v.len() == 3)
                .ok_or("Invalid camera target.")?;
            let mut xyz = [0.0; 3];
            for (i, value) in target.iter().enumerate() {
                xyz[i] = value.as_f64().ok_or("Invalid camera target.")? as f32;
            }
            let camera = rot_render::CameraState {
                target: xyz,
                yaw: entry["yaw"].as_f64().ok_or("Invalid camera yaw.")? as f32,
                pitch: entry["pitch"].as_f64().ok_or("Invalid camera pitch.")? as f32,
                distance: entry["distance"]
                    .as_f64()
                    .ok_or("Invalid camera distance.")? as f32,
                orthographic: entry["orthographic"]
                    .as_bool()
                    .ok_or("Invalid camera projection.")?,
            };
            if !camera.is_valid()
                || !recent.contains(&path)
                || cameras.iter().any(|(old, _)| old == &path)
            {
                return Err("Invalid, duplicate or unrelated camera context.".into());
            }
            cameras.push((path, camera));
        }
        let raw = v["bindings"]
            .as_array()
            .filter(|v| v.len() <= Action::ALL.len())
            .ok_or("Shortcut table exceeds the supported action count.")?;
        let mut bindings = Vec::with_capacity(raw.len());
        for b in raw {
            let map = b.as_object().ok_or("Invalid shortcut.")?;
            if map.len() != 5
                || map
                    .keys()
                    .any(|k| !["action", "key", "ctrl", "shift", "alt"].contains(&k.as_str()))
            {
                return Err("Invalid shortcut fields.".into());
            }
            let action = Action::ALL
                .into_iter()
                .find(|a| Some(a.name()) == b["action"].as_str())
                .ok_or("Unknown shortcut action.")?;
            let key = if b["key"].is_null() {
                None
            } else {
                Some(
                    b["key"]
                        .as_str()
                        .and_then(egui::Key::from_name)
                        .ok_or("Unknown shortcut key.")?,
                )
            };
            if key == Some(egui::Key::Escape) {
                return Err("Escape is reserved for cancelling edits.".into());
            }
            let binding = Binding {
                action,
                key,
                ctrl: b["ctrl"].as_bool().ok_or("Invalid shortcut modifier.")?,
                shift: b["shift"].as_bool().ok_or("Invalid shortcut modifier.")?,
                alt: b["alt"].as_bool().ok_or("Invalid shortcut modifier.")?,
            };
            if bindings
                .iter()
                .any(|old: &Binding| old.action == action || old.same_keys(binding))
            {
                return Err("Duplicate shortcut action or conflicting shortcut keys.".into());
            }
            bindings.push(binding);
        }
        // Missing known actions receive their default binding without rewriting
        // existing choices. A newly conflicting default remains visibly Unbound.
        for default in Self::default().bindings {
            if !bindings
                .iter()
                .any(|binding| binding.action == default.action)
            {
                let mut binding = default;
                if bindings.iter().any(|old| old.same_keys(default)) {
                    binding.key = None;
                }
                bindings.push(binding);
            }
        }
        let external_editor = match v.get("external_editor") {
            Some(value) => {
                serde_json::from_value::<crate::behavior_files::LaunchSpec>(value.clone())
                    .map_err(|error| format!("Invalid external editor settings: {error}"))?
            }
            None => Self::default().external_editor,
        };
        external_editor
            .validate()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            hierarchy_width: number("hierarchy_width", 120.0, 800.0)?,
            inspector_width: number("inspector_width", 240.0, 1000.0)?,
            hierarchy_side: side("hierarchy_side")?,
            inspector_side: side("inspector_side")?,
            hierarchy_visible: boolean("hierarchy_visible")?,
            inspector_visible: boolean("inspector_visible")?,
            content_visible: match v.get("content_visible") {
                None => true,
                Some(value) => value
                    .as_bool()
                    .ok_or("Invalid content visibility preference.")?,
            },
            // Files from before theme selection keep their dark editor.
            theme: match v.get("theme") {
                None => Default::default(),
                Some(value) => value
                    .as_str()
                    .and_then(crate::editor_theme::ThemeChoice::from_name)
                    .ok_or("Invalid theme preference.")?,
            },
            style: match v.get("style") {
                None | Some(Value::Null) => None,
                Some(value) => Some(
                    value
                        .as_str()
                        .and_then(crate::editor_theme::Style::from_name)
                        .ok_or("Invalid style preference.")?,
                ),
            },
            themed_wording: match v.get("themed_wording") {
                None => true,
                Some(value) => value
                    .as_bool()
                    .ok_or("Invalid themed wording preference.")?,
            },
            allow_ai_connections: match v.get("allow_ai_connections") {
                None => true,
                Some(value) => value.as_bool().ok_or("Invalid AI connection preference.")?,
            },
            scale: number("ui_scale", 0.75, 2.0)?,
            movement_speed: number("movement_speed", 0.05, 1000.0)?,
            recent,
            projects,
            cameras,
            bindings,
            external_editor,
        })
    }
}
struct PreferenceLock {
    path: PathBuf,
    file: std::fs::File,
}
impl PreferenceLock {
    fn acquire(path: &Path) -> Result<Self, String> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .map_err(|e| e.to_string())?;
        let name = path.file_name().ok_or("Preference file name required.")?;
        let path = parent.join(name);
        let mut lock_name = std::ffi::OsString::from(".");
        lock_name.push(name);
        lock_name.push(".rot-lock");
        let lock_path = parent.join(lock_name);
        if lock_path.symlink_metadata().is_ok_and(|m| !m.is_file()) {
            return Err("Preference lock must be a regular file.".into());
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)
            .map_err(|e| e.to_string())?;
        file.try_lock()
            .map_err(|e| format!("Editor preferences are busy or cannot be locked: {e}"))?;
        Ok(Self { path, file })
    }
}
impl Drop for PreferenceLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
struct Storage {
    path: Option<PathBuf>,
    baseline: Option<Vec<u8>>,
}
pub(super) struct Workspace {
    pub prefs: Preferences,
    pub error: String,
    pub settings_open: bool,
    pub welcome: bool,
    /// No preferences file existed: offer the style chooser once.
    pub first_launch: bool,
    pub layout_generation: u64,
    capture: Option<Action>,
    /// The style the first-launch chooser is previewing.
    style_preview: Option<crate::editor_theme::Style>,
    storage: Storage,
    dirty: bool,
    recovery_required: bool,
    editor_executable: String,
    editor_args: String,
    editor_dirty: bool,
    /// The Preferences page on show.
    pub settings_page: SettingsPage,
    /// Keyboard focus was placed in the Preferences window.
    settings_focused: bool,
    shortcut_query: String,
}

/// Preferences pages, in sidebar order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum SettingsPage {
    #[default]
    General,
    Appearance,
    Viewport,
    Shortcuts,
    ExternalTools,
    Ai,
}
impl SettingsPage {
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Appearance,
        Self::Viewport,
        Self::Shortcuts,
        Self::ExternalTools,
        Self::Ai,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::General => Text::LabelGeneral.plain(),
            Self::Appearance => Text::LabelAppearance.plain(),
            Self::Viewport => Text::LabelViewport.plain(),
            Self::Shortcuts => Text::LabelShortcuts.plain(),
            Self::ExternalTools => Text::LabelExternalTools.plain(),
            Self::Ai => Text::LabelAi.plain(),
        }
    }
}

impl Workspace {
    pub fn detached() -> Self {
        Self::at(None)
    }
    #[cfg(test)]
    pub fn at_for_test(path: PathBuf) -> Self {
        Self::at(Some(path))
    }
    pub fn load() -> Self {
        let path = std::env::var_os("ROT_EDITOR_STATE")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
                    .map(|p| p.join("rot/editor.json"))
            });
        Self::at(path)
    }
    fn at(path: Option<PathBuf>) -> Self {
        let mut workspace = Self {
            prefs: Preferences::default(),
            error: String::new(),
            settings_open: false,
            welcome: true,
            first_launch: false,
            layout_generation: 0,
            capture: None,
            settings_page: SettingsPage::General,
            settings_focused: false,
            shortcut_query: String::new(),
            style_preview: None,
            storage: Storage {
                path,
                baseline: None,
            },
            dirty: false,
            recovery_required: false,
            editor_executable: String::new(),
            editor_args: String::new(),
            editor_dirty: false,
        };
        if let Some(path) = &workspace.storage.path {
            workspace.first_launch = path.symlink_metadata().is_err();
            if path.exists() || path.symlink_metadata().is_ok() {
                match rot_assets::read_regular_file(path, MAX_WORKSPACE_BYTES) {
                    Ok(bytes) => {
                        match Preferences::parse(&bytes) {
                            Ok(prefs) => workspace.prefs = prefs,
                            Err(e) => {
                                workspace.error = e;
                                workspace.recovery_required = true;
                            }
                        }
                        workspace.storage.baseline = Some(bytes);
                    }
                    Err(e) => {
                        workspace.error =
                            format!("Couldn't read preferences at {}: {e}.", path.display())
                    }
                }
            }
        } else {
            workspace.error = Text::WorkspaceCouldntFindAPlace.plain().into();
        }
        workspace.reset_editor_draft();
        workspace
    }
    /// First launch: choose Rot or Plain, and a mode, before anything else.
    /// Choices preview live; Continue (or Enter) keeps them.
    pub fn style_chooser_ui(&mut self, ctx: &egui::Context) {
        use crate::editor_theme::{Style, ThemeChoice};
        use crate::ui_kit::tokens::*;
        if !self.first_launch || self.prefs.style.is_some() {
            return;
        }
        let chosen = self.style_preview.unwrap_or_default();
        let mut keep = false;
        egui::Modal::new(egui::Id::unique("rot_style_chooser")).show(ctx, |ui| {
            ui.set_width(520.0f32.min(ctx.content_rect().width() - 48.0));
            ui.heading(Text::ChooseYourStyle.plain());
            ui.add_space(SPACE_S);
            ui.columns(2, |columns| {
                for (column, style) in columns.iter_mut().zip(Style::ALL) {
                    let palette =
                        style.palette(ctx.theme(), self.prefs.theme == ThemeChoice::OledBlack);
                    let (rect, response) = column.allocate_exact_size(
                        egui::vec2(column.available_width(), 120.0),
                        egui::Sense::click(),
                    );
                    let painter = column.painter();
                    let selected = chosen == style;
                    painter.rect_filled(rect, RADIUS_MENU, palette.background);
                    let panel = egui::Rect::from_min_size(
                        rect.min + egui::vec2(SPACE_S, SPACE_S),
                        egui::vec2(rect.width() * 0.35, rect.height() - 2.0 * SPACE_S),
                    );
                    painter.rect_filled(panel, RADIUS_CONTROL, palette.surface);
                    for row in 0..3 {
                        let y = panel.top() + SPACE_S + row as f32 * 14.0;
                        painter.rect_filled(
                            egui::Rect::from_min_size(
                                egui::pos2(panel.left() + SPACE_S, y),
                                egui::vec2(panel.width() * (0.7 - row as f32 * 0.15), 6.0),
                            ),
                            2,
                            if row == 0 {
                                palette.accent
                            } else {
                                palette.secondary
                            },
                        );
                    }
                    let button = egui::Rect::from_min_size(
                        egui::pos2(panel.right() + SPACE_M, rect.bottom() - 36.0),
                        egui::vec2(72.0, 22.0),
                    );
                    painter.rect_filled(button, RADIUS_CONTROL, palette.accent);
                    painter.text(
                        egui::pos2(panel.right() + SPACE_M, rect.top() + SPACE_M),
                        egui::Align2::LEFT_TOP,
                        style.label(),
                        crate::ui_kit::semibold(TEXT_DIALOG_TITLE),
                        palette.text,
                    );
                    painter.rect_stroke(
                        rect,
                        RADIUS_MENU,
                        egui::Stroke::new(
                            if selected { FOCUS_RING } else { 1.0 },
                            if selected {
                                palette.accent
                            } else {
                                palette.divider
                            },
                        ),
                        egui::StrokeKind::Inside,
                    );
                    let response = response.on_hover_text(match style {
                        Style::Rot => Text::TheRotEngineLook.plain(),
                        Style::Plain => Text::WorkspaceANeutralLookLike.plain(),
                    });
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(
                            egui::Role::RadioButton,
                            true,
                            selected,
                            style.label(),
                        )
                    });
                    if response.clicked() {
                        self.style_preview = Some(style);
                    }
                }
            });
            ui.add_space(SPACE_S);
            ui.horizontal(|ui| {
                ui.label(Text::Mode.plain());
                for choice in [ThemeChoice::Dark, ThemeChoice::Light, ThemeChoice::System] {
                    if ui
                        .selectable_value(&mut self.prefs.theme, choice, choice.label())
                        .changed()
                    {
                        self.dirty = true;
                    }
                }
            });
            if chosen == Style::Rot {
                ui.checkbox(&mut self.prefs.themed_wording, Text::ThemedWording.plain());
            }
            ui.add_space(SPACE_S);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    keep |= crate::ui_kit::primary_button(ui, Text::Continue.plain()).clicked();
                });
            });
            keep |=
                ui.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape));
        });
        if keep {
            self.prefs.style = Some(chosen);
            self.style_preview = None;
            self.changed();
        }
    }
    /// The style the first-launch chooser is previewing, if any.
    pub fn previewed_style(&self) -> Option<crate::editor_theme::Style> {
        self.style_preview.filter(|_| self.prefs.style.is_none())
    }
    pub fn state_path(&self) -> Option<&Path> {
        self.storage.path.as_deref()
    }
    fn reset_editor_draft(&mut self) {
        self.editor_executable = self
            .prefs
            .external_editor
            .executable
            .to_string_lossy()
            .into_owned();
        self.editor_args = serde_json::to_string_pretty(&self.prefs.external_editor.args)
            .expect("string vector is serializable");
        self.editor_dirty = false;
    }
    fn apply_editor_draft(&mut self) -> Result<(), String> {
        if !self.editor_dirty {
            return Ok(());
        }
        let args: Vec<String> = serde_json::from_str(&self.editor_args).map_err(|error| {
            format!("Editor arguments must be a JSON array of strings; draft preserved: {error}")
        })?;
        let config = crate::behavior_files::LaunchSpec {
            executable: PathBuf::from(&self.editor_executable),
            args,
        };
        config.validate().map_err(|error| error.to_string())?;
        self.prefs.external_editor = config;
        self.editor_dirty = false;
        self.dirty = true;
        Ok(())
    }
    fn editor_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.strong(Text::ScriptEditor.plain());
        ui.weak(Text::WorkspaceRunsWithoutAShell.plain());
        let ids = [
            egui::Id::unique("external-editor-executable"),
            egui::Id::unique("external-editor-arguments"),
        ];
        let owns = ui.memory(|memory| ids.iter().any(|id| memory.focused() == Some(*id)));
        let ctrl_enter = owns
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::Enter));
        let save =
            owns && ui.input_mut(|input| input.consume_key(egui::Modifiers::CTRL, egui::Key::S));
        let enter =
            ui.input(|input| input.modifiers.is_none() && input.key_pressed(egui::Key::Enter));
        ui.label(Text::EditorExecutable.plain());
        let executable = ui.add(
            egui::TextEdit::singleline(&mut self.editor_executable)
                .id(ids[0])
                .char_limit(4096),
        );
        self.editor_dirty |= executable.changed();
        ui.label(Text::ArgumentsJsonStringArray.plain());
        self.editor_dirty |= ui
            .add(
                egui::TextEdit::multiline(&mut self.editor_args)
                    .id(ids[1])
                    .char_limit(16 * 1024)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            )
            .changed();
        // Changes apply when you leave a field or press Enter, like any other
        // field; an invalid entry stays as typed with its error shown.
        let left = executable.lost_focus()
            || ui.memory(|memory| {
                memory
                    .focused()
                    .is_none_or(|focused| !ids.contains(&focused))
            }) && self.editor_dirty
                && ui
                    .ctx()
                    .memory(|m| m.data.get_temp::<bool>(ids[1].with("had_focus")))
                    .unwrap_or(false);
        ui.ctx()
            .memory_mut(|m| m.data.insert_temp(ids[1].with("had_focus"), owns));
        let apply =
            ctrl_enter || left || (enter && (executable.has_focus() || executable.lost_focus()));
        if self.editor_dirty && ui.button(Text::Revert.plain()).clicked() {
            self.reset_editor_draft();
        }
        if apply || save {
            match self
                .apply_editor_draft()
                .and_then(|()| if save { self.persist() } else { Ok(()) })
            {
                Ok(()) => self.error.clear(),
                Err(error) => self.error = error,
            }
        }
    }
    pub fn persist(&mut self) -> Result<(), String> {
        if self.recovery_required {
            return Err("Preferences are invalid. Reset them before saving.".into());
        }
        let path = self
            .storage
            .path
            .as_ref()
            .ok_or("No editor preference path is configured.")?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Cannot create preference directory: {e}"))?;
        let lock = PreferenceLock::acquire(path)?;
        let path = &lock.path;
        if path.symlink_metadata().is_ok_and(|m| !m.is_file()) {
            return Err(format!(
                "Couldn't use {} for preferences. Move it aside, then reopen.",
                path.display()
            ));
        }
        let current = match rot_assets::read_regular_file(path, MAX_WORKSPACE_BYTES) {
            Ok(bytes) => Some(bytes),
            Err(_) if !path.exists() => None,
            Err(e) => {
                return Err(format!(
                    "Couldn't read preferences at {}: {e}.",
                    path.display()
                ));
            }
        };
        if current != self.storage.baseline {
            return Err("Preferences changed outside the editor. Reopen it to save.".into());
        }
        let bytes = serde_json::to_vec_pretty(&self.prefs.json()).map_err(|e| e.to_string())?;
        Preferences::parse(&bytes)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        temp.write_all(&bytes)
            .and_then(|()| temp.flush())
            .and_then(|()| temp.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        if current.is_some() {
            temp.persist(path).map_err(|e| e.to_string())?;
        } else {
            temp.persist_noclobber(path).map_err(|e| e.to_string())?;
        }
        self.storage.baseline = Some(bytes);
        self.dirty = false;
        Ok(())
    }
    fn reset_preferences(&mut self) {
        self.prefs = Preferences::default();
        self.reset_editor_draft();
        self.layout_generation = self.layout_generation.wrapping_add(1);
        self.capture = None;
        self.dirty = true;
        self.recovery_required = false;
        match self.persist() {
            Ok(()) => self.error.clear(),
            Err(e) => self.error = e,
        }
    }
    pub fn flush(&mut self) {
        if self.dirty {
            if let Err(error) = self.persist() {
                self.error = error;
            }
        }
    }

    fn scene_view_path(&self, scene: &Path) -> Result<Option<PathBuf>, String> {
        let Some(state) = &self.storage.path else {
            return Ok(None);
        };
        let scene = scene.canonicalize().map_err(|error| error.to_string())?;
        let id = Sha256::digest(scene.as_os_str().as_encoded_bytes());
        Ok(Some(
            state
                .with_file_name("scene-views")
                .join(format!("{id:x}.json")),
        ))
    }

    pub fn scene_view(&self, scene: &Path) -> Result<SceneViewState, String> {
        self.scene_view_path(scene)?.map_or_else(
            || Ok(SceneViewState::default()),
            |path| read_scene_view(&path),
        )
    }

    pub fn change_scene_view(
        &self,
        scene: &Path,
        state: &mut SceneViewState,
        change: impl FnOnce(&mut SceneViewState),
    ) -> Result<(), String> {
        let Some(path) = self.scene_view_path(scene)? else {
            change(state);
            return Ok(());
        };
        let parent = path.parent().expect("scene view directory");
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let lock = PreferenceLock::acquire(&path)?;
        let mut updated = read_scene_view(&lock.path)?;
        change(&mut updated);
        let mut output =
            tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut output, &updated).map_err(|error| error.to_string())?;
        output
            .as_file()
            .sync_all()
            .map_err(|error| error.to_string())?;
        output
            .persist(&lock.path)
            .map_err(|error| error.to_string())?;
        *state = updated;
        Ok(())
    }
    /// Fly speed changed from the viewport (wheel while flying); saved on the
    /// next flush.
    pub fn set_movement_speed(&mut self, speed: f32) {
        if speed.is_finite() && speed != self.prefs.movement_speed {
            self.prefs.movement_speed = speed.clamp(0.05, 1000.0);
            self.dirty = true;
        }
    }
    pub fn camera_for(&self, path: &Path) -> Option<rot_render::CameraState> {
        self.prefs
            .cameras
            .iter()
            .find(|(old, _)| old == path)
            .map(|(_, camera)| *camera)
    }
    pub fn remember_camera(&mut self, path: &Path, camera: rot_render::CameraState) {
        if !camera.is_valid()
            || !self.prefs.recent.iter().any(|p| p == path)
            || self.camera_for(path) == Some(camera)
        {
            return;
        }
        self.prefs.cameras.retain(|(old, _)| old != path);
        self.prefs.cameras.insert(0, (path.to_path_buf(), camera));
        self.prefs.cameras.truncate(10);
        self.dirty = true;
    }
    pub fn remember(&mut self, path: &Path) {
        if self.stage_recent(path) {
            self.flush();
        }
    }
    /// Save adoption publishes recent-file and camera preferences together.
    pub fn remember_saved(&mut self, path: &Path, camera: Option<rot_render::CameraState>) {
        self.stage_recent(path);
        if let Some(camera) = camera {
            self.remember_camera(path, camera);
        }
        self.flush();
    }
    fn stage_recent(&mut self, path: &Path) -> bool {
        if let Ok(path) = path.canonicalize() {
            if !path.to_str().is_some_and(|name| name.len() <= 4096) {
                return false;
            }
            if let Some(root) = path.parent() {
                self.prefs
                    .projects
                    .retain(|scene| scene.parent() != Some(root));
                self.prefs.projects.insert(0, path.clone());
            }
            self.prefs.recent.retain(|p| p != &path);
            self.prefs.recent.insert(0, path);
            self.prefs.recent.truncate(10);
            self.prefs
                .cameras
                .retain(|(old, _)| self.prefs.recent.contains(old));
            self.dirty = true;
            true
        } else {
            false
        }
    }
    pub fn changed(&mut self) {
        self.dirty = true;
    }
    /// Shortcut text for menus, tooltips and the palette: empty when unbound.
    pub fn shortcut_hint(&self, action: Action) -> String {
        self.prefs
            .bindings
            .iter()
            .find(|b| b.action == action && b.key.is_some())
            .map(|b| b.text())
            .unwrap_or_default()
    }
    pub fn shortcut_text(&self, action: Action) -> String {
        self.prefs
            .bindings
            .iter()
            .find(|b| b.action == action)
            .map(|b| b.text())
            .unwrap_or_default()
    }
    pub fn shortcuts(&mut self, ctx: &egui::Context, blocked: bool) -> Option<Action> {
        if let Some(action) = self.capture {
            if !ctx.input(|i| i.focused) {
                return None;
            }
            let next = ctx.input(|i| {
                i.events.iter().find_map(|event| {
                    if let egui::Event::Key {
                        key,
                        pressed: true,
                        repeat: false,
                        modifiers,
                        ..
                    } = event
                    {
                        Some((*key, *modifiers))
                    } else {
                        None
                    }
                })
            });
            if let Some((key, modifiers)) = next {
                if key == egui::Key::Escape {
                    ctx.input_mut(|i| i.consume_key(modifiers, egui::Key::Escape));
                    self.capture = None;
                    return None;
                }
                // A captured key belongs to capture, including rejected attempts.
                // Otherwise Enter/Space can activate the still-focused capture button.
                ctx.input_mut(|i| i.consume_key(modifiers, key));
                let candidate = Binding {
                    action,
                    key: Some(key),
                    ctrl: modifiers.ctrl || modifiers.command,
                    alt: modifiers.alt,
                    shift: modifiers.shift,
                };
                if self
                    .prefs
                    .bindings
                    .iter()
                    .any(|b| b.action != action && b.same_keys(candidate))
                {
                    self.error = Text::WorkspaceThatShortcutIsTaken.plain().into();
                } else if let Some(binding) =
                    self.prefs.bindings.iter_mut().find(|b| b.action == action)
                {
                    *binding = candidate;
                    self.capture = None;
                    self.dirty = true;
                    self.error.clear();
                }
            }
            return None;
        }
        let widget_focused = ctx.egui_wants_keyboard_input();
        if blocked || self.settings_open || !ctx.input(|i| i.focused) {
            return None;
        }
        // Text fields own typing; only panel switching (F6) passes through.
        let text_focused = widget_focused && ctx.text_edit_focused();
        // Match each press exactly, even if its modifiers were released later
        // in this frame. Preserve Ctrl+Shift+S independently from Ctrl+S.
        self.prefs.bindings.iter().find_map(|binding| {
            // Focus navigation remains reachable from ordinary focused controls.
            if text_focused && !matches!(binding.action, Action::NextPanel | Action::PreviousPanel)
            {
                return None;
            }
            if widget_focused
                && !matches!(
                    binding.action,
                    Action::FocusRuntime
                        | Action::GroupSelection
                        | Action::Play
                        | Action::Pause
                        | Action::Step
                        | Action::NextPanel
                        | Action::PreviousPanel
                )
            {
                return None;
            }
            let key = binding.key?;
            let action = ctx.input_mut(|i| {
                let mut consumed = false;
                i.events.retain(|event| {
                    let matches = matches!(
                        event,
                        egui::Event::Key {
                            key: pressed_key,
                            pressed: true,
                            modifiers,
                            ..
                        } if *pressed_key == key
                            && modifiers.ctrl == binding.ctrl
                            && modifiers.shift == binding.shift
                            && modifiers.alt == binding.alt
                            && modifiers.matches_logically(binding.modifiers())
                    );
                    consumed |= matches;
                    !matches
                });
                consumed.then_some(binding.action)
            });
            if action == Some(Action::FocusRuntime) {
                // A separate navigation event wins over a focus jump in the
                // same batch, including when it follows the default shortcut.
                let direction = ctx.input(|input| {
                    input.events.iter().rev().find_map(|event| {
                        let egui::Event::Key {
                            key,
                            pressed: true,
                            modifiers,
                            ..
                        } = event
                        else {
                            return None;
                        };
                        match key {
                            egui::Key::ArrowUp if modifiers.is_none() => {
                                Some(egui::FocusDirection::Up)
                            }
                            egui::Key::ArrowDown if modifiers.is_none() => {
                                Some(egui::FocusDirection::Down)
                            }
                            egui::Key::ArrowLeft if modifiers.is_none() => {
                                Some(egui::FocusDirection::Left)
                            }
                            egui::Key::ArrowRight if modifiers.is_none() => {
                                Some(egui::FocusDirection::Right)
                            }
                            egui::Key::Tab if modifiers.is_none() => {
                                Some(egui::FocusDirection::Next)
                            }
                            egui::Key::Tab if modifiers.shift_only() => {
                                Some(egui::FocusDirection::Previous)
                            }
                            egui::Key::Escape if modifiers.is_none() => {
                                Some(egui::FocusDirection::None)
                            }
                            _ => None,
                        }
                    })
                });
                if !binding.ctrl
                    && !binding.alt
                    && matches!(
                        key,
                        egui::Key::Tab
                            | egui::Key::ArrowUp
                            | egui::Key::ArrowDown
                            | egui::Key::ArrowLeft
                            | egui::Key::ArrowRight
                    )
                    && (!binding.shift || key == egui::Key::Tab)
                {
                    // begin_pass queued navigation before the shortcut consumed
                    // its event. Retain only the remaining event's direction.
                    ctx.memory_mut(|memory| {
                        memory.move_focus(direction.unwrap_or(egui::FocusDirection::None))
                    });
                }
                if direction.is_some() {
                    return None;
                }
            }
            action
        })
    }
    /// Open Preferences on a page (Ctrl+, opens General; Keyboard
    /// Shortcuts… opens Shortcuts).
    pub fn open_settings(&mut self, page: SettingsPage) {
        self.settings_open = true;
        self.settings_page = page;
    }
    fn close_settings(&mut self) {
        self.capture = None;
        self.settings_focused = false;
        self.settings_open = false;
        self.flush();
    }
    /// Reset the settings on one page to their defaults.
    fn reset_page(&mut self, page: SettingsPage) {
        let defaults = Preferences::default();
        match page {
            SettingsPage::General => self.prefs.scale = defaults.scale,
            SettingsPage::Appearance => {
                self.prefs.style = defaults.style;
                self.prefs.theme = defaults.theme;
                self.prefs.themed_wording = defaults.themed_wording;
            }
            SettingsPage::Viewport => self.prefs.movement_speed = defaults.movement_speed,
            SettingsPage::Shortcuts => {
                self.prefs.bindings = defaults.bindings;
                self.capture = None;
            }
            SettingsPage::ExternalTools => {
                self.prefs.external_editor = defaults.external_editor;
                self.reset_editor_draft();
            }
            SettingsPage::Ai => self.prefs.allow_ai_connections = defaults.allow_ai_connections,
        }
    }
    fn general_page(&mut self, ui: &mut egui::Ui) {
        crate::ui_kit::property_row(ui, Text::UiScale.plain(), |ui| {
            ui.add(egui::Slider::new(&mut self.prefs.scale, 0.75..=2.0).fixed_decimals(2));
        });
    }
    fn appearance_page(&mut self, ui: &mut egui::Ui) {
        crate::ui_kit::property_row(ui, Text::Style.plain(), |ui| {
            let mut style = self.prefs.style.unwrap_or_default();
            for choice in crate::editor_theme::Style::ALL {
                ui.selectable_value(&mut style, choice, choice.label());
            }
            if Some(style) != self.prefs.style
                && (self.prefs.style.is_some() || style != crate::editor_theme::Style::default())
            {
                self.prefs.style = Some(style);
            }
        });
        crate::ui_kit::property_row(ui, Text::Mode.plain(), |ui| {
            ui.horizontal_wrapped(|ui| {
                for choice in crate::editor_theme::ThemeChoice::ALL {
                    ui.selectable_value(&mut self.prefs.theme, choice, choice.label());
                }
            });
        });
        ui.checkbox(&mut self.prefs.themed_wording, Text::ThemedWording.plain())
            .on_hover_text(Text::WorkspaceOccasionalRotFlavoredLines.plain());
    }
    fn viewport_page(&mut self, ui: &mut egui::Ui) {
        crate::ui_kit::property_row(ui, Text::FlySpeed.plain(), |ui| {
            ui.add(
                egui::DragValue::new(&mut self.prefs.movement_speed)
                    .range(0.05..=1000.0)
                    .speed(0.1)
                    .suffix(Text::UnitsS.plain()),
            )
            .on_hover_text(Text::WorkspaceScrollWhileFlyingWith.plain());
        });
    }
    fn shortcuts_page(&mut self, ui: &mut egui::Ui) {
        let search =
            crate::ui_kit::search_field(ui, &mut self.shortcut_query, Text::Search.plain());
        if !self.settings_focused {
            search.request_focus();
            self.settings_focused = true;
        }
        ui.add_space(crate::ui_kit::tokens::SPACE_XS);
        let query = self.shortcut_query.to_lowercase();
        for action in Action::ALL {
            if !query.is_empty() && !action.label().to_lowercase().contains(&query) {
                continue;
            }
            let binding = self
                .prefs
                .bindings
                .iter()
                .find(|b| b.action == action)
                .copied()
                .unwrap_or(Binding {
                    action,
                    key: None,
                    ctrl: false,
                    shift: false,
                    alt: false,
                });
            let conflict = self
                .prefs
                .bindings
                .iter()
                .find(|other| other.action != action && other.same_keys(binding))
                .map(|other| other.action.label());
            crate::ui_kit::property_row(ui, action.label(), |ui| {
                let text = if self.capture == Some(action) {
                    "Press keys…".into()
                } else {
                    binding.text()
                };
                if ui
                    .add(egui::Button::new(text).min_size(egui::vec2(140.0, 0.0)))
                    .on_hover_text(Text::WorkspaceClickThenPressThe.plain())
                    .clicked()
                {
                    self.capture = Some(action);
                }
                if binding.key.is_some()
                    && ui
                        .small_button("×")
                        .on_hover_text(Text::RemoveThisShortcut.plain())
                        .clicked()
                {
                    if let Some(b) = self.prefs.bindings.iter_mut().find(|b| b.action == action) {
                        b.key = None;
                    }
                }
                if let Some(other) = conflict {
                    ui.colored_label(ui.visuals().warn_fg_color, format!("Also {other}"));
                }
            });
        }
    }
    fn ai_page(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(
            &mut self.prefs.allow_ai_connections,
            Text::AllowAiAppsToConnect.plain(),
        )
        .on_hover_text(Text::NativeWhenOffThisEditor.plain());
        ui.weak(Text::WorkspaceTrustedAppsAndDiagnostics.plain());
    }
    pub fn settings_ui(&mut self, ctx: &egui::Context) {
        let _ = self.settings_ui_with_footer(ctx, false);
    }
    pub fn shortcuts_ui(&mut self, ctx: &egui::Context) {
        let _ = self.settings_ui_with_footer(ctx, true);
    }
    /// The Preferences window: a page list on the left, the page on the
    /// right. Changes apply at once and are saved when it closes.
    fn settings_ui_with_footer(
        &mut self,
        ctx: &egui::Context,
        shortcuts_only: bool,
    ) -> Option<[egui::Rect; 3]> {
        if !self.settings_open {
            return None;
        }
        if self.capture.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.close_settings();
            return None;
        }
        let screen = ctx.content_rect();
        let mut open = true;
        let mut footer = [egui::Rect::NOTHING; 3];
        let title = if shortcuts_only {
            Action::KeyboardShortcuts.label().trim_end_matches('…')
        } else {
            "Preferences"
        };
        egui::Window::new(title)
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .fixed_size(egui::vec2(
                (screen.width() - 64.0).clamp(220.0, 640.0),
                (screen.height() - 120.0).clamp(160.0, 520.0),
            ))
            .show(ctx, |ui| {
                let before = self.prefs.clone();
                let footer_height = ui.spacing().interact_size.y * 2.0 + 8.0;
                let body_height = (ui.available_height() - footer_height).max(48.0);
                ui.horizontal_top(|ui| {
                    if !shortcuts_only {
                        ui.vertical(|ui| {
                            ui.set_width(120.0);
                            for page in SettingsPage::ALL {
                                let tab = ui.selectable_value(
                                    &mut self.settings_page,
                                    page,
                                    page.label(),
                                );
                                // Focus starts in the window, on the open page.
                                if !self.settings_focused && page == self.settings_page {
                                    tab.request_focus();
                                    self.settings_focused = true;
                                }
                            }
                        });
                        ui.separator();
                    }
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical()
                            .id_salt("preferences-page")
                            .max_height(body_height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| match self.settings_page {
                                SettingsPage::General => self.general_page(ui),
                                SettingsPage::Appearance => self.appearance_page(ui),
                                SettingsPage::Viewport => self.viewport_page(ui),
                                SettingsPage::Shortcuts => self.shortcuts_page(ui),
                                SettingsPage::ExternalTools => self.editor_settings_ui(ui),
                                SettingsPage::Ai => self.ai_page(ui),
                            });
                    });
                });
                if before != self.prefs {
                    self.dirty = true;
                    self.layout_generation = self.layout_generation.wrapping_add(1);
                }
                if !self.error.is_empty() {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(&self.error).color(ui.visuals().error_fg_color),
                        )
                        .truncate(),
                    )
                    .on_hover_text(&self.error);
                }
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let reset = ui
                        .button(Text::ResetToDefaults.plain())
                        .on_hover_text(format!("Reset the {} page", self.settings_page.label()));
                    footer[0] = reset.rect;
                    if reset.clicked() {
                        self.reset_page(self.settings_page);
                        self.dirty = true;
                        self.layout_generation = self.layout_generation.wrapping_add(1);
                    }
                    if !shortcuts_only {
                        let all = ui
                            .button(Text::ResetAllPreferences.plain())
                            .on_hover_text(Text::WorkspaceAlsoRepairsAnUnreadable.plain());
                        footer[1] = all.rect;
                        if all.clicked() {
                            self.reset_preferences();
                        }
                    }
                    let close = ui.button(Text::CloseErrorDetails.plain());
                    footer[2] = close.rect;
                    if close.clicked() {
                        self.close_settings();
                    }
                });
            });
        if !open {
            self.close_settings();
        }
        Some(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn editor_configuration_keyboard_applies_valid_argv_and_preserves_invalid_drafts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut workspace = Workspace::at(Some(path.clone()));
        let ctx = egui::Context::default();
        let frame =
            |workspace: &mut Workspace, events: Vec<egui::Event>, focus: Option<egui::Id>| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        if let Some(focus) = focus {
                            ui.memory_mut(|memory| memory.request_focus(focus));
                        }
                        workspace.editor_settings_ui(ui);
                    },
                );
                output.textures_delta.clear();
            };
        let key = |key, modifiers| {
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
            ]
        };
        workspace.editor_executable = "/tmp/editor with spaces".into();
        workspace.editor_args = "[not valid JSON".into();
        workspace.editor_dirty = true;
        let original = workspace.prefs.external_editor.clone();
        frame(
            &mut workspace,
            key(egui::Key::Enter, egui::Modifiers::CTRL),
            Some(egui::Id::unique("external-editor-arguments")),
        );
        assert_eq!(workspace.prefs.external_editor, original);
        assert_eq!(workspace.editor_args, "[not valid JSON");
        assert!(workspace.editor_dirty);
        assert!(!path.exists());
        workspace.editor_args = "[\"--new\", \"{workspace}\", \"{file}:{line}\"]".into();
        frame(
            &mut workspace,
            key(egui::Key::S, egui::Modifiers::CTRL),
            Some(egui::Id::unique("external-editor-arguments")),
        );
        assert!(!workspace.editor_dirty, "{}", workspace.error);
        assert_eq!(
            workspace.prefs.external_editor.executable,
            PathBuf::from("/tmp/editor with spaces")
        );
        assert_eq!(
            Workspace::at(Some(path)).prefs.external_editor,
            workspace.prefs.external_editor
        );
        workspace.editor_args = "[broken again".into();
        workspace.editor_dirty = true;
        let before = workspace.prefs.external_editor.clone();
        frame(
            &mut workspace,
            key(egui::Key::Enter, egui::Modifiers::CTRL),
            Some(egui::Id::unique("another-widget")),
        );
        assert_eq!(workspace.prefs.external_editor, before);
        assert!(
            workspace.editor_dirty,
            "unrelated widget must retain CtrlEnter ownership"
        );
    }
    #[test]
    fn project_history_migrates_recent_and_preserves_more_than_ten_roots() {
        let home = tempfile::tempdir().unwrap();
        let state = home.path().join("editor.json");
        let old_root = home.path().join("existing-project");
        std::fs::create_dir(&old_root).unwrap();
        let first = old_root.join("latest.json");
        let older = old_root.join("older.json");
        std::fs::write(&first, "scene").unwrap();
        let mut legacy = Preferences::default().json();
        legacy.as_object_mut().unwrap().remove("projects");
        legacy["recent"] = json!([first, older]);
        std::fs::write(&state, serde_json::to_vec_pretty(&legacy).unwrap()).unwrap();
        let mut workspace = Workspace::at(Some(state.clone()));
        assert_eq!(
            workspace.prefs.projects.as_slice(),
            std::slice::from_ref(&first)
        );
        assert_eq!(workspace.prefs.recent, [first.clone(), older.clone()]);

        for index in 0..12 {
            let root = home.path().join(format!("project-{index}"));
            std::fs::create_dir(&root).unwrap();
            let scene = root.join("scene.json");
            std::fs::write(&scene, "scene").unwrap();
            workspace.remember(&scene);
        }
        let history = Workspace::at(Some(state.clone()));
        assert!(history.error.is_empty());
        assert_eq!(history.prefs.projects.len(), 13);
        assert_eq!(history.prefs.projects.last(), Some(&first));
        assert_eq!(history.prefs.recent.len(), 10);
        assert!(!history.prefs.recent.contains(&first));

        std::fs::write(&older, "scene").unwrap();
        workspace.remember_saved(&older, None);
        let history = Workspace::at(Some(state));
        assert!(history.error.is_empty());
        assert_eq!(history.prefs.projects.len(), 13);
        assert_eq!(history.prefs.projects.first(), Some(&older));
        assert!(!history.prefs.projects.contains(&first));
    }

    #[test]
    fn preferences_roundtrip_recovery_conflict_and_recent_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/editor.json");
        let mut w = Workspace::at(Some(path.clone()));
        w.prefs.scale = 1.25;
        w.prefs.inspector_side = Side::Left;
        w.prefs.hierarchy_visible = false;
        w.prefs.theme = crate::editor_theme::ThemeChoice::OledBlack;
        w.persist().unwrap();
        let before_lock = std::fs::read(&path).unwrap();
        let lock = PreferenceLock::acquire(&path).unwrap();
        let inherited = lock.file.try_clone().unwrap();
        assert!(w.persist().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before_lock);
        drop(lock);
        w.persist().unwrap();
        drop(inherited);
        let reopened = Workspace::at(Some(path.clone()));
        assert_eq!(reopened.prefs, w.prefs);
        assert!(reopened.error.is_empty());
        for n in 0..12 {
            let project = dir.path().join(format!("scene-{n}.json"));
            std::fs::write(&project, "example").unwrap();
            w.remember(&project);
        }
        assert_eq!(w.prefs.recent.len(), 10);
        assert!(w.prefs.recent[0].ends_with("scene-11.json"));
        let recent = w.prefs.recent[0].clone();
        w.remember(&recent);
        assert_eq!(w.prefs.recent.len(), 10);
        let camera = rot_render::CameraState {
            target: [1.0, 2.0, 3.0],
            yaw: 0.6,
            pitch: 0.4,
            distance: 8.0,
            orthographic: true,
        };
        w.remember_camera(&recent, camera);
        w.persist().unwrap();
        assert_eq!(
            Workspace::at(Some(path.clone())).camera_for(&recent),
            Some(camera)
        );
        let batched = dir.path().join("scene-batched.json");
        std::fs::write(&batched, "example").unwrap();
        w.remember_saved(&batched, Some(camera));
        let reopened = Workspace::at(Some(path.clone()));
        assert_eq!(reopened.prefs.recent[0], batched);
        assert_eq!(reopened.camera_for(&batched), Some(camera));
        assert_eq!(reopened.camera_for(&recent), Some(camera));
        let mut invalid_theme = w.prefs.json();
        invalid_theme["theme"] = "sepia".into();
        assert!(Preferences::parse(&serde_json::to_vec(&invalid_theme).unwrap()).is_err());
        let mut invalid_camera = w.prefs.json();
        invalid_camera["cameras"][0]["distance"] = (-1).into();
        assert!(Preferences::parse(&serde_json::to_vec(&invalid_camera).unwrap()).is_err());
        let mut oversized = w.prefs.json();
        let paths: Vec<_> = (0..10)
            .map(|n| format!("/{}-{n}", "a".repeat(4000)))
            .collect();
        oversized["recent"] = json!(paths);
        oversized["cameras"]=json!(paths.iter().map(|path|json!({"path":path,"target":[0,0,0],"yaw":0,"pitch":0,"distance":8,"orthographic":false})).collect::<Vec<_>>());
        assert!(Preferences::parse(&serde_json::to_vec(&oversized).unwrap()).is_err());
        let original = std::fs::read(&path).unwrap();
        std::fs::write(&path, b"{broken").unwrap();
        let pending_camera = rot_render::CameraState {
            distance: 9.0,
            ..camera
        };
        w.remember_saved(&batched, Some(pending_camera));
        assert!(!w.error.is_empty());
        assert!(w.dirty);
        assert_eq!(w.camera_for(&batched), Some(pending_camera));
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        w.prefs.scale = 1.5;
        assert!(w.persist().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        let mut broken = Workspace::at(Some(path.clone()));
        assert!(!broken.error.is_empty());
        broken.prefs.scale = 1.5;
        assert!(broken.persist().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"{broken");
        broken.reset_preferences();
        assert!(broken.error.is_empty());
        assert_eq!(
            Workspace::at(Some(path.clone())).prefs,
            Preferences::default()
        );
        let occupied = dir.path().join("directory");
        std::fs::create_dir(&occupied).unwrap();
        let mut invalid = Workspace::at(Some(occupied));
        assert!(invalid.persist().is_err());
        #[cfg(unix)]
        {
            let link = dir.path().join("state-link");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            let mut symlink = Workspace::at(Some(link));
            assert!(symlink.persist().is_err());
            assert_eq!(
                std::fs::read(&path).unwrap(),
                serde_json::to_vec_pretty(&Preferences::default().json()).unwrap()
            );
        }
        assert!(!original.is_empty());
    }
    #[test]
    fn shortcut_capture_consumes_enter_space_and_conflicts_before_buttons() {
        let ctx = egui::Context::default();
        let mut w = Workspace::detached();
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                ..Default::default()
            },
            |ui| {
                ui.button("Capture shortcut").request_focus();
            },
        );
        output.textures_delta.clear();
        for (key, modifiers, conflict) in [
            (egui::Key::Enter, egui::Modifiers::NONE, false),
            (egui::Key::Space, egui::Modifiers::NONE, false),
            (egui::Key::S, egui::Modifiers::CTRL, true),
        ] {
            w.capture = Some(Action::Frame);
            let mut clicked = false;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: vec![
                        egui::Event::ModifiersChanged(modifiers),
                        egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers,
                        },
                    ],
                    ..Default::default()
                },
                |ui| {
                    assert_eq!(w.shortcuts(&ctx, false), None);
                    assert!(
                        !ui.input(|i| i.key_pressed(key)),
                        "accepted and rejected capture attempts must consume the key"
                    );
                    let response = ui.button("Capture shortcut");
                    clicked = response.clicked();
                    response.request_focus();
                },
            );
            output.textures_delta.clear();
            assert!(
                !clicked,
                "captured Enter/Space must not reactivate the focused capture control"
            );
            assert_eq!(w.capture, if conflict { Some(Action::Frame) } else { None });
        }
    }

    #[test]
    fn scaled_settings_footer_stays_inside_small_supported_windows() {
        for physical in [egui::vec2(960.0, 640.0), egui::vec2(1400.0, 1000.0)] {
            let ctx = egui::Context::default();
            crate::ui::configure(&ctx);
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, physical)),
                    ..Default::default()
                },
                |_| {},
            );
            output.textures_delta.clear();
            ctx.set_zoom_factor(2.0);
            let mut w = Workspace::at(Some(PathBuf::from(format!(
                "/tmp/{}.json",
                "long-state-path".repeat(40)
            ))));
            w.settings_open = true;
            w.prefs.scale = 2.0;
            w.error = format!(
                "Cannot read preferences {}. Move the displayed state path aside and reopen.",
                "long error details ".repeat(30)
            );
            let logical = egui::Rect::from_min_size(egui::Pos2::ZERO, physical / 2.0);
            let mut footer = None;
            for _ in 0..4 {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        focused: true,
                        screen_rect: Some(logical),
                        ..Default::default()
                    },
                    |_| footer = w.settings_ui_with_footer(&ctx, false),
                );
                output.textures_delta.clear();
            }
            assert_eq!(ctx.zoom_factor(), 2.0);
            let screen = ctx.content_rect();
            assert!((screen.size() - logical.size()).length() <= 0.5);
            for rect in footer.unwrap() {
                assert!(rect.is_positive());
                assert!(
                    screen.expand(0.5).contains_rect(rect),
                    "footer control {rect:?} outside supported2x viewport {screen:?}"
                );
            }
        }
    }

    #[test]
    fn shortcut_capture_conflicts_reset_and_focus_ownership() {
        let ctx = egui::Context::default();
        let mut w = Workspace::detached();
        let runtime_modifiers = egui::Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        let frame = |w: &mut Workspace, focused, events: Vec<egui::Event>, blocked| {
            let mut action = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused,
                    events,
                    ..Default::default()
                },
                |_| action = w.shortcuts(&ctx, blocked),
            );
            output.textures_delta.clear();
            action
        };
        let key = |key, modifiers| {
            vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                },
                egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers,
                },
            ]
        };
        assert_eq!(w.shortcut_text(Action::FocusRuntime), "Ctrl+Shift+G");
        // S-05: Unity's Play-mode shortcuts.
        assert_eq!(
            frame(
                &mut w,
                true,
                key(egui::Key::P, egui::Modifiers::CTRL),
                false
            ),
            Some(Action::Play)
        );
        assert_eq!(
            frame(&mut w, true, key(egui::Key::P, runtime_modifiers), false),
            Some(Action::Pause)
        );
        assert_eq!(
            frame(
                &mut w,
                true,
                key(egui::Key::P, egui::Modifiers::CTRL | egui::Modifiers::ALT),
                false
            ),
            Some(Action::Step)
        );
        assert_eq!(
            frame(&mut w, true, key(egui::Key::G, runtime_modifiers), false),
            Some(Action::FocusRuntime)
        );
        let mut released_shortcut = key(egui::Key::G, runtime_modifiers);
        released_shortcut.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        assert_eq!(
            frame(&mut w, true, released_shortcut, false),
            Some(Action::FocusRuntime),
            "A modifier release in the same frame preserves the shortcut press"
        );
        let mut shifted_save = key(egui::Key::S, runtime_modifiers);
        shifted_save.push(egui::Event::ModifiersChanged(egui::Modifiers::CTRL));
        assert_eq!(
            frame(&mut w, true, shifted_save, false),
            Some(Action::SaveAs),
            "Releasing Shift cannot reinterpret Save As as Save"
        );
        let mut mixed_saves = key(egui::Key::S, runtime_modifiers);
        mixed_saves.extend(key(egui::Key::S, egui::Modifiers::CTRL));
        mixed_saves.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                events: mixed_saves,
                ..Default::default()
            },
            |_| {
                assert_eq!(w.shortcuts(&ctx, false), Some(Action::Save));
                assert_eq!(w.shortcuts(&ctx, false), Some(Action::SaveAs));
                assert_eq!(w.shortcuts(&ctx, false), None);
            },
        );
        output.textures_delta.clear();
        for (focused, blocked) in [(false, false), (true, true)] {
            assert_eq!(
                frame(
                    &mut w,
                    focused,
                    key(egui::Key::P, runtime_modifiers),
                    blocked
                ),
                None
            );
        }
        w.settings_open = true;
        assert_eq!(
            frame(&mut w, true, key(egui::Key::G, runtime_modifiers), false),
            None
        );
        w.settings_open = false;
        w.capture = Some(Action::FocusRuntime);
        assert_eq!(
            frame(&mut w, true, key(egui::Key::G, runtime_modifiers), false),
            None,
            "Shortcut capture owns the key before runtime focus dispatch"
        );
        assert!(w.capture.is_none());
        assert_eq!(
            frame(
                &mut w,
                true,
                key(egui::Key::S, egui::Modifiers::CTRL),
                false
            ),
            Some(Action::Save)
        );
        assert_eq!(
            frame(
                &mut w,
                false,
                key(egui::Key::S, egui::Modifiers::CTRL),
                false
            ),
            None
        );
        assert_eq!(
            frame(&mut w, true, key(egui::Key::S, egui::Modifiers::CTRL), true),
            None
        );
        w.capture = Some(Action::Frame);
        frame(
            &mut w,
            true,
            key(egui::Key::S, egui::Modifiers::CTRL),
            false,
        );
        assert!(!w.error.is_empty());
        assert_eq!(w.capture, Some(Action::Frame));
        frame(
            &mut w,
            true,
            key(egui::Key::G, egui::Modifiers::NONE),
            false,
        );
        assert_eq!(w.capture, None);
        assert!(w.error.is_empty());
        assert_eq!(
            frame(
                &mut w,
                true,
                key(egui::Key::G, egui::Modifiers::NONE),
                false
            ),
            Some(Action::Frame)
        );
        assert_eq!(
            frame(
                &mut w,
                true,
                key(egui::Key::F, egui::Modifiers::NONE),
                false
            ),
            None
        );
        w.capture = Some(Action::Frame);
        frame(
            &mut w,
            true,
            key(egui::Key::Escape, egui::Modifiers::NONE),
            false,
        );
        assert!(w.capture.is_none());
        let mut previous = w.prefs.json();
        previous.as_object_mut().unwrap().remove("content_visible");
        previous.as_object_mut().unwrap().remove("external_editor");
        previous.as_object_mut().unwrap().remove("theme");
        previous["bindings"]
            .as_array_mut()
            .unwrap()
            .retain(|binding| {
                ![
                    Action::Quit,
                    Action::FindObjects,
                    Action::FindContent,
                    Action::Rename,
                    Action::SelectAll,
                    Action::FocusRuntime,
                    Action::Play,
                    Action::Pause,
                    Action::Step,
                    Action::GroupSelection,
                ]
                .iter()
                .any(|action| binding["action"] == action.name())
            });
        let upgraded = Preferences::parse(&serde_json::to_vec(&previous).unwrap()).unwrap();
        assert!(upgraded.content_visible);
        assert_eq!(upgraded.theme, crate::editor_theme::ThemeChoice::Dark);
        assert_eq!(
            upgraded.external_editor,
            Preferences::default().external_editor
        );
        assert_eq!(upgraded.bindings.len(), Action::ALL.len());
        assert!(upgraded.bindings.iter().any(|binding| {
            binding.action == Action::GroupSelection && binding.text() == "Ctrl+G"
        }));
        assert_eq!(
            upgraded
                .bindings
                .iter()
                .find(|binding| binding.action == Action::FocusRuntime),
            Preferences::default()
                .bindings
                .iter()
                .find(|binding| binding.action == Action::FocusRuntime)
        );
        assert_eq!(
            upgraded
                .bindings
                .iter()
                .find(|binding| binding.action == Action::Frame)
                .unwrap()
                .key,
            Some(egui::Key::G)
        );
        assert_eq!(
            upgraded
                .bindings
                .iter()
                .find(|binding| binding.action == Action::Quit)
                .unwrap()
                .key,
            Some(egui::Key::Q)
        );
        // A previously valid remap owns its keys when a new action is added.
        let old_frame = previous["bindings"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|binding| binding["action"] == Action::Frame.name())
            .unwrap();
        old_frame["key"] = "F".into();
        old_frame["ctrl"] = true.into();
        let migrated = Preferences::parse(&serde_json::to_vec(&previous).unwrap()).unwrap();
        assert_eq!(
            migrated
                .bindings
                .iter()
                .find(|b| b.action == Action::FindObjects)
                .unwrap()
                .key,
            None
        );
        assert_eq!(
            Preferences::parse(&serde_json::to_vec(&migrated.json()).unwrap()).unwrap(),
            migrated
        );
        let mut old_workspace = Workspace::detached();
        old_workspace.prefs = migrated;
        assert_eq!(old_workspace.shortcut_text(Action::FindObjects), "Unbound");
        assert_eq!(
            frame(
                &mut old_workspace,
                true,
                key(egui::Key::F, egui::Modifiers::CTRL),
                false
            ),
            Some(Action::Frame)
        );
        old_workspace.capture = Some(Action::FindObjects);
        frame(
            &mut old_workspace,
            true,
            key(egui::Key::H, egui::Modifiers::CTRL),
            false,
        );
        assert_eq!(old_workspace.capture, None);
        assert_eq!(
            frame(
                &mut old_workspace,
                true,
                key(egui::Key::H, egui::Modifiers::CTRL),
                false
            ),
            Some(Action::FindObjects)
        );
        // An older custom shortcut keeps CtrlShiftP. Only the newly added
        // Pause default becomes Unbound, then supports normal remapping.
        let mut runtime_conflict = previous.clone();
        let custom = runtime_conflict["bindings"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|binding| binding["action"] == Action::Frame.name())
            .unwrap();
        custom["key"] = "P".into();
        custom["ctrl"] = true.into();
        custom["shift"] = true.into();
        let migrated_runtime =
            Preferences::parse(&serde_json::to_vec(&runtime_conflict).unwrap()).unwrap();
        assert_eq!(
            migrated_runtime
                .bindings
                .iter()
                .find(|binding| binding.action == Action::Pause)
                .unwrap()
                .key,
            None
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime-shortcut.json");
        let mut remapped = Workspace::at(Some(path.clone()));
        remapped.prefs = migrated_runtime;
        assert_eq!(remapped.shortcut_text(Action::Pause), "Unbound");
        assert_eq!(
            frame(
                &mut remapped,
                true,
                key(egui::Key::P, runtime_modifiers),
                false
            ),
            Some(Action::Frame)
        );
        remapped.capture = Some(Action::Pause);
        assert_eq!(
            frame(
                &mut remapped,
                true,
                key(egui::Key::R, runtime_modifiers),
                false
            ),
            None
        );
        assert!(remapped.capture.is_none());
        assert_eq!(
            frame(
                &mut remapped,
                true,
                key(egui::Key::R, runtime_modifiers),
                false
            ),
            Some(Action::Pause)
        );
        remapped.flush();
        assert!(remapped.error.is_empty());
        assert_eq!(Workspace::at(Some(path)).prefs, remapped.prefs);
        let mut duplicate = w.prefs.json();
        duplicate["bindings"][13]["key"] = "S".into();
        duplicate["bindings"][13]["ctrl"] = true.into();
        assert!(Preferences::parse(&serde_json::to_vec(&duplicate).unwrap()).is_err());
        for (pressed, modifiers, expected) in [
            (egui::Key::G, runtime_modifiers, Some(Action::FocusRuntime)),
            (egui::Key::G, egui::Modifiers::NONE, None),
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: key(pressed, modifiers),
                    ..Default::default()
                },
                |ui| {
                    let button = ui.button("Unrelated focused control");
                    button.request_focus();
                    assert_eq!(w.shortcuts(&ctx, false), expected);
                },
            );
            output.textures_delta.clear();
        }
        let mut text = String::new();
        for (pressed, modifiers) in [
            (egui::Key::G, egui::Modifiers::NONE),
            (egui::Key::P, runtime_modifiers),
        ] {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    focused: true,
                    events: key(pressed, modifiers),
                    ..Default::default()
                },
                |ui| {
                    let edit = ui.text_edit_singleline(&mut text);
                    edit.request_focus();
                    assert_eq!(w.shortcuts(&ctx, false), None);
                },
            );
            output.textures_delta.clear();
        }
    }

    // Every command is listed by its menu name, a shared shortcut is flagged,
    // and Reset to Defaults on the page restores the default bindings.
    #[test]
    fn shortcuts_page_lists_every_command_and_flags_conflicts() {
        let ctx = egui::Context::default();
        let mut w = Workspace::detached();
        w.open_settings(SettingsPage::Shortcuts);
        let frame = |w: &mut Workspace| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 20000.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::Frame::new().show(ui, |ui| w.shortcuts_page(ui));
                },
            );
            output.textures_delta.clear();
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => Some(text.galley.job.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let texts = frame(&mut w);
        for action in Action::ALL {
            assert!(
                texts.iter().any(|t| t == action.label()),
                "missing {}",
                action.label()
            );
        }
        assert!(!texts.iter().any(|t| t.starts_with("Also ")));
        let undo = w
            .prefs
            .bindings
            .iter()
            .find(|b| b.action == Action::Undo)
            .copied()
            .unwrap();
        let redo = w
            .prefs
            .bindings
            .iter_mut()
            .find(|b| b.action == Action::Redo)
            .unwrap();
        redo.key = undo.key;
        redo.ctrl = undo.ctrl;
        redo.shift = undo.shift;
        assert!(frame(&mut w)
            .iter()
            .any(|t| t == &format!("Also {}", Action::Undo.label())));
        w.reset_page(SettingsPage::Shortcuts);
        assert_eq!(w.prefs.bindings, Preferences::default().bindings);
        w.shortcut_query = "redo".into();
        let filtered = frame(&mut w);
        assert!(filtered.iter().any(|t| t == Action::Redo.label()));
        assert!(!filtered.iter().any(|t| t == Action::Undo.label()));
    }

    // Changes apply at once; closing saves them; nothing needs a Save button.
    #[test]
    fn preferences_apply_at_once_and_save_on_close() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("editor.json");
        let mut w = Workspace::at(Some(path.clone()));
        w.open_settings(SettingsPage::Ai);
        w.prefs.allow_ai_connections = false;
        w.dirty = true;
        w.close_settings();
        assert!(!w.settings_open);
        assert!(!Workspace::at(Some(path.clone())).prefs.allow_ai_connections);

        let ctx = egui::Context::default();
        w.open_settings(SettingsPage::Shortcuts);
        w.capture = Some(Action::GroupSelection);
        let mut output = ctx.run_ui(
            egui::RawInput {
                focused: true,
                events: vec![egui::Event::Key {
                    key: egui::Key::G,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::ALT,
                }],
                ..Default::default()
            },
            |_| assert!(w.shortcuts(&ctx, false).is_none()),
        );
        output.textures_delta.clear();
        assert!(w.capture.is_none());
        assert_eq!(w.shortcut_hint(Action::GroupSelection), "Alt+G");
        w.close_settings();
        assert_eq!(
            Workspace::at(Some(path)).shortcut_hint(Action::GroupSelection),
            "Alt+G"
        );
        w.reset_page(SettingsPage::Ai);
        assert!(w.prefs.allow_ai_connections);
    }
}
