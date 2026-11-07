//! Window preferences are independent from scene data and runtime state.
use eframe::egui;
use serde_json::{json, Value};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

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
}
impl Action {
    pub const ALL: [Self; 30] = [
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
    ];
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
            key.name()
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
    pub cameras: Vec<(PathBuf, ruster_render::CameraState)>,
    pub bindings: Vec<Binding>,
    pub external_editor: crate::behavior_files::LaunchSpec,
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
            (Action::FocusRuntime, Key::P, true, true),
        ];
        Self {
            hierarchy_width: 180.0,
            inspector_width: 340.0,
            hierarchy_side: Side::Left,
            inspector_side: Side::Right,
            hierarchy_visible: true,
            inspector_visible: true,
            content_visible: true,
            scale: 1.0,
            movement_speed: 4.0,
            recent: Vec::new(),
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
                .collect(),
        }
    }
}
impl Preferences {
    fn json(&self) -> Value {
        json!({"format":"ruster-editor-preferences","version":1,"hierarchy_width":self.hierarchy_width,"inspector_width":self.inspector_width,"hierarchy_side":if self.hierarchy_side==Side::Left {"left"} else {"right"},"inspector_side":if self.inspector_side==Side::Left {"left"} else {"right"},"hierarchy_visible":self.hierarchy_visible,"inspector_visible":self.inspector_visible,"content_visible":self.content_visible,"external_editor":self.external_editor,"ui_scale":self.scale,"movement_speed":self.movement_speed,"recent":self.recent,"cameras":self.cameras.iter().map(|(path,camera)|json!({"path":path,"target":camera.target,"yaw":camera.yaw,"pitch":camera.pitch,"distance":camera.distance,"orthographic":camera.orthographic})).collect::<Vec<_>>(),"bindings":self.bindings.iter().map(|b|json!({"action":b.action.name(),"key":b.key.map(|key|key.name()),"ctrl":b.ctrl,"shift":b.shift,"alt":b.alt})).collect::<Vec<_>>()})
    }
    fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > 64 * 1024 {
            return Err(
                "Editor preferences exceed64KiB; reset or reduce recent context before saving."
                    .into(),
            );
        }
        let v: Value =
            serde_json::from_slice(bytes).map_err(|e| format!("Invalid preferences: {e}"))?;
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
        ];
        let map = v.as_object().ok_or("Preferences must be an object.")?;
        if !(keys.len() - 2..=keys.len()).contains(&map.len())
            || map.keys().any(|k| !keys.contains(&k.as_str()))
            || v["format"] != "ruster-editor-preferences"
            || v["version"] != 1
        {
            return Err("Unsupported editor preference format/version or fields; Reset preferences to recover.".into());
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
            let camera = ruster_render::CameraState {
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
            scale: number("ui_scale", 0.75, 2.0)?,
            movement_speed: number("movement_speed", 0.05, 1000.0)?,
            recent,
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
        lock_name.push(".ruster-lock");
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
    pub layout_generation: u64,
    capture: Option<Action>,
    storage: Storage,
    dirty: bool,
    recovery_required: bool,
    editor_executable: String,
    editor_args: String,
    editor_dirty: bool,
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
        let path = std::env::var_os("RUSTER_EDITOR_STATE")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .filter(|p| p.is_absolute())
                    .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
                    .map(|p| p.join("ruster/editor.json"))
            });
        Self::at(path)
    }
    fn at(path: Option<PathBuf>) -> Self {
        let mut workspace = Self {
            prefs: Preferences::default(),
            error: String::new(),
            settings_open: false,
            welcome: true,
            layout_generation: 0,
            capture: None,
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
            if path.exists() || path.symlink_metadata().is_ok() {
                match ruster_assets::read_regular_file(path,64*1024) {
                    Ok(bytes)=> {match Preferences::parse(&bytes) {Ok(prefs)=>workspace.prefs=prefs,Err(e)=>{workspace.error=e;workspace.recovery_required=true;}}workspace.storage.baseline=Some(bytes);},
                    Err(e)=>workspace.error=format!("Cannot read editor preferences {}: {e}. Move this file aside or repair access, then reopen. Reset cannot replace an unreadable, oversized or nonregular baseline.",path.display()),
                }
            }
        } else {
            workspace.error="Editor preferences path unavailable; this window still works without saving preferences.".into();
        }
        workspace.reset_editor_draft();
        workspace
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
        ui.heading("External code editor · Zed");
        ui.label("Executable and argument vector; no shell is used. Placeholders: {workspace}, {file}, {line}.");
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
        ui.label("Editor executable");
        let executable = ui.add(
            egui::TextEdit::singleline(&mut self.editor_executable)
                .id(ids[0])
                .char_limit(4096),
        );
        self.editor_dirty |= executable.changed();
        ui.label("Arguments (JSON string array)");
        self.editor_dirty |= ui
            .add(
                egui::TextEdit::multiline(&mut self.editor_args)
                    .id(ids[1])
                    .char_limit(16 * 1024)
                    .desired_rows(4)
                    .desired_width(f32::INFINITY),
            )
            .changed();
        let mut apply =
            ctrl_enter || (enter && (executable.has_focus() || executable.lost_focus()));
        ui.horizontal_wrapped(|ui| {
            apply |= ui.button("Apply editor configuration").clicked();
            if ui.button("Revert editor configuration draft").clicked() {
                self.reset_editor_draft();
            }
        });
        if self.editor_dirty {
            ui.small("Unapplied editor configuration · Enter in executable / Ctrl+Enter applies · Ctrl+S applies and saves preferences.");
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
            return Err("Preferences are invalid; use Reset preferences explicitly before saving replacements.".into());
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
            return Err(format!("Preference destination {} must be a regular file. Move it aside or repair access, then reopen.",path.display()));
        }
        let current = match ruster_assets::read_regular_file(path, 64 * 1024) {
            Ok(bytes) => Some(bytes),
            Err(_) if !path.exists() => None,
            Err(e) => return Err(format!("Cannot read preference baseline {}: {e}. Move this file aside or repair access, then reopen; Reset cannot replace an unreadable or oversized baseline.",path.display())),
        };
        if current != self.storage.baseline {
            return Err("Editor preferences changed outside this window; reopen the editor or choose another state path before saving.".into());
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
    pub fn camera_for(&self, path: &Path) -> Option<ruster_render::CameraState> {
        self.prefs
            .cameras
            .iter()
            .find(|(old, _)| old == path)
            .map(|(_, camera)| *camera)
    }
    pub fn remember_camera(&mut self, path: &Path, camera: ruster_render::CameraState) {
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
    pub fn remember_saved(&mut self, path: &Path, camera: Option<ruster_render::CameraState>) {
        self.stage_recent(path);
        if let Some(camera) = camera {
            self.remember_camera(path, camera);
        }
        self.flush();
    }
    fn stage_recent(&mut self, path: &Path) -> bool {
        if let Ok(path) = path.canonicalize() {
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
                    self.error="Shortcut already belongs to another action; choose another combination or Escape.".into();
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
        if blocked
            || self.settings_open
            || (widget_focused && ctx.text_edit_focused())
            || !ctx.input(|i| i.focused)
        {
            return None;
        }
        // Exact modifiers prevent Ctrl+Shift+S being consumed as Ctrl+S.
        self.prefs.bindings.iter().find_map(|binding| {
            // Focus navigation remains reachable from ordinary focused controls.
            if widget_focused && binding.action != Action::FocusRuntime {
                return None;
            }
            let key = binding.key?;
            let action = ctx.input_mut(|i| {
                let m = i.modifiers;
                (m.ctrl == binding.ctrl
                    && m.shift == binding.shift
                    && m.alt == binding.alt
                    && i.consume_key(binding.modifiers(), key))
                .then_some(binding.action)
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
    pub fn settings_ui(&mut self, ctx: &egui::Context) {
        let _ = self.settings_ui_with_footer(ctx);
    }
    fn settings_ui_with_footer(&mut self, ctx: &egui::Context) -> Option<[egui::Rect; 3]> {
        if !self.settings_open {
            return None;
        }
        if self.capture.is_none()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.settings_open = false;
            self.flush();
            return None;
        }
        let response=egui::Modal::new(egui::Id::unique("workspace-settings")).show(ctx, |ui| {
            // Work in current logical points: at2x a960×640 window is480×320.
            let screen=ctx.content_rect();
            ui.set_width((screen.width()-64.0).clamp(220.0,640.0));
            let start_y=ui.cursor().min.y;
            ui.heading("Workspace and shortcuts");
            if let Some(path) = self.state_path() {
                let path=path.display().to_string();
                ui.add(egui::Label::new(egui::RichText::new(&path).small()).truncate()).on_hover_text(path);
            }
            let spacing=ui.spacing().item_spacing.y;
            let button_height=ui.spacing().interact_size.y.max(ui.text_style_height(&egui::TextStyle::Button)+2.0*ui.spacing().button_padding.y);
            // Reserve the worstcase three wrapped footer rows plus one error line.
            // The body alone scrolls, leaving recovery/close controls reachable.
            let header_height=ui.cursor().min.y-start_y;
            let footer_budget=3.0*button_height+3.0*spacing;
            let error_budget=ui.text_style_height(&egui::TextStyle::Body)+spacing;
            let body_height=(screen.height()-64.0-header_height-footer_budget-error_budget).max(24.0);
            egui::ScrollArea::vertical()
                .max_height(body_height)
                .show(ui, |ui| {
                    let before = self.prefs.clone();
                    ui.horizontal(|ui| {
                        ui.label("UI scale");
                        ui.add(egui::Slider::new(&mut self.prefs.scale, 0.75..=2.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Navigation speed (units/second)");
                        ui.add(
                            egui::DragValue::new(&mut self.prefs.movement_speed)
                                .range(0.05..=1000.0)
                                .speed(0.1),
                        );
                    });
                    ui.checkbox(&mut self.prefs.hierarchy_visible, "Show hierarchy");
                    ui.checkbox(&mut self.prefs.inspector_visible, "Show inspector");
                    ui.checkbox(&mut self.prefs.content_visible,"Show Content browser");
                    for (label, side) in [
                        ("Hierarchy side", &mut self.prefs.hierarchy_side),
                        ("Inspector side", &mut self.prefs.inspector_side),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(label);
                            ui.selectable_value(side, Side::Left, "Left");
                            ui.selectable_value(side, Side::Right, "Right");
                        });
                    }
                    if ui.button("Reset layout").clicked() {
                        let defaults = Preferences::default();
                        self.prefs.hierarchy_width = defaults.hierarchy_width;
                        self.prefs.inspector_width = defaults.inspector_width;
                        self.prefs.hierarchy_side = defaults.hierarchy_side;
                        self.prefs.inspector_side = defaults.inspector_side;
                        self.prefs.hierarchy_visible = true;
                        self.prefs.inspector_visible = true;
                        self.prefs.content_visible = true;
                    }
                    ui.separator();
                    self.editor_settings_ui(ui);
                    ui.separator();
                    ui.label(
                        "Select a binding, then press the new combination. Escape cancels capture.",
                    );
                    for binding in self.prefs.bindings.clone() {
                        ui.horizontal(|ui| {
                            ui.label(binding.action.name());
                            if ui
                                .button(if self.capture == Some(binding.action) {
                                    "Press keys…".into()
                                } else {
                                    binding.text()
                                })
                                .clicked()
                            {
                                self.capture = Some(binding.action);
                            }
                        });
                    }
                    if ui.button("Reset shortcuts").clicked() {
                        self.prefs.bindings = Preferences::default().bindings;
                        self.capture = None;
                    }
                    if before != self.prefs {
                        self.dirty = true;
                        self.layout_generation = self.layout_generation.wrapping_add(1);
                    }
                });
            if !self.error.is_empty() {
                ui.add(egui::Label::new(egui::RichText::new(&self.error).color(egui::Color32::LIGHT_RED)).truncate()).on_hover_text(&self.error);
            }
            let mut footer=[egui::Rect::NOTHING;3];
            ui.horizontal_wrapped(|ui| {
                let save=ui.button("Save workspace preferences");footer[0]=save.rect;
                if save.clicked() {
                    match self.apply_editor_draft().and_then(|()|self.persist()) {
                        Ok(()) => self.error.clear(),
                        Err(e) => self.error = e,
                    }
                }
                let reset=ui.button("Reset preferences").on_hover_text("Resets readable malformed settings. For unreadable, oversized or nonregular paths, move the displayed state path aside or repair access, then reopen.");footer[1]=reset.rect;
                if reset.clicked() {
                    self.reset_preferences();
                }
                let close=ui.button("Close workspace settings");footer[2]=close.rect;
                if close.clicked() {
                    self.capture = None;
                    self.settings_open = false;
                    self.flush();
                }
            });
            footer
        });
        Some(response.inner)
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
    fn preferences_roundtrip_recovery_conflict_and_recent_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state/editor.json");
        let mut w = Workspace::at(Some(path.clone()));
        w.prefs.scale = 1.25;
        w.prefs.inspector_side = Side::Left;
        w.prefs.hierarchy_visible = false;
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
        let camera = ruster_render::CameraState {
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
        let pending_camera = ruster_render::CameraState {
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
                    |_| footer = w.settings_ui_with_footer(&ctx),
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
        assert_eq!(w.shortcut_text(Action::FocusRuntime), "Ctrl+Shift+P");
        assert_eq!(
            frame(&mut w, true, key(egui::Key::P, runtime_modifiers), false),
            Some(Action::FocusRuntime)
        );
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
            frame(&mut w, true, key(egui::Key::P, runtime_modifiers), false),
            None
        );
        w.settings_open = false;
        w.capture = Some(Action::FocusRuntime);
        assert_eq!(
            frame(&mut w, true, key(egui::Key::P, runtime_modifiers), false),
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
                ]
                .iter()
                .any(|action| binding["action"] == action.name())
            });
        let upgraded = Preferences::parse(&serde_json::to_vec(&previous).unwrap()).unwrap();
        assert!(upgraded.content_visible);
        assert_eq!(
            upgraded.external_editor,
            Preferences::default().external_editor
        );
        assert_eq!(upgraded.bindings.len(), Action::ALL.len());
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
        // runtime-focus default becomes Unbound, then supports normal remapping.
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
                .find(|binding| binding.action == Action::FocusRuntime)
                .unwrap()
                .key,
            None
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("runtime-shortcut.json");
        let mut remapped = Workspace::at(Some(path.clone()));
        remapped.prefs = migrated_runtime;
        assert_eq!(remapped.shortcut_text(Action::FocusRuntime), "Unbound");
        assert_eq!(
            frame(
                &mut remapped,
                true,
                key(egui::Key::P, runtime_modifiers),
                false
            ),
            Some(Action::Frame)
        );
        remapped.capture = Some(Action::FocusRuntime);
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
            Some(Action::FocusRuntime)
        );
        remapped.flush();
        assert!(remapped.error.is_empty());
        assert_eq!(Workspace::at(Some(path)).prefs, remapped.prefs);
        let mut duplicate = w.prefs.json();
        duplicate["bindings"][13]["key"] = "S".into();
        duplicate["bindings"][13]["ctrl"] = true.into();
        assert!(Preferences::parse(&serde_json::to_vec(&duplicate).unwrap()).is_err());
        for (pressed, modifiers, expected) in [
            (egui::Key::P, runtime_modifiers, Some(Action::FocusRuntime)),
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
}
