//! The editor's string table (UX-STANDARDS §8–§9, S-08). Each key has Plain
//! wording and, for the few moments where it fits, a Rot line. Themed
//! wording shows only with the Rot style and Themed Wording both on, and
//! never in menus, buttons, errors, warnings, saving or AI permissions.
//!
//! Command names live in `Action::label`; this table holds the rest.
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text {
    Welcome,
    NoRecentProjects,
    EmptyHierarchy,
    EmptyAssets,
    EmptyConsole,
    OpeningProject,
    LoadingContent,
    Building,
    BuildFinished,
    NoResults,
    AboutTagline,
    NoMatchingCommands,
    GameCameraNeeded,
    EditsRestartPlay,
    ComponentFileMenu,
    ComponentEditMenu,
    ComponentHierarchy,
    ComponentInspector,
    ComponentName,
    ComponentEnabled,
    ComponentPosition,
    ComponentAxisX,
    ComponentAxisY,
    ComponentAxisZ,
    ComponentCamera,
    ComponentAnimator,
    ComponentScript,
    ComponentMeshRenderer,
    ComponentLight,
    ComponentCollider,
    ComponentApply,
    ComponentUnsaved,
    ComponentSaveClose,
    ComponentDiscardClose,
    ComponentCancel,
    ComponentTransform,
    ComponentRotation,
    ComponentScale,
    ComponentAdd,
    ComponentEnable,
    ComponentSearch,
    ComponentRemove,
    ComponentGeometry,
    ComponentTint,
    ComponentMaterial,
    ComponentNoMaterial,
    ComponentColliderRole,
    ComponentBlocker,
    ComponentSensor,
    ComponentColliderShape,
    ComponentBox,
    ComponentSphere,
    ComponentCenter,
    ComponentRadius,
    ComponentSize,

    ComponentBuildApplication,
    ComponentBuildFolder,
    ComponentBuild,
    ComponentBuildRun,
    ComponentSaveBeforeBuild,

    ComponentObjectMenu,
    ComponentCreate,
    Component3dObject,
    ComponentEmptyObject,
    ComponentParent,
    ComponentSceneRoot,
    ComponentMoveUp,
    ComponentMoveDown,

    ComponentOpenScene,
    ComponentSceneFile,
    ComponentOpen,
    ComponentSaveOpen,
    ComponentDiscardOpen,

    ComponentConnectionUnavailable,
}

impl Text {
    #[cfg(test)]
    pub const ALL: [Self; 74] = [
        Self::Welcome,
        Self::NoRecentProjects,
        Self::EmptyHierarchy,
        Self::EmptyAssets,
        Self::EmptyConsole,
        Self::OpeningProject,
        Self::LoadingContent,
        Self::Building,
        Self::BuildFinished,
        Self::NoResults,
        Self::AboutTagline,
        Self::NoMatchingCommands,
        Self::GameCameraNeeded,
        Self::EditsRestartPlay,
        Self::ComponentFileMenu,
        Self::ComponentEditMenu,
        Self::ComponentHierarchy,
        Self::ComponentInspector,
        Self::ComponentName,
        Self::ComponentEnabled,
        Self::ComponentPosition,
        Self::ComponentAxisX,
        Self::ComponentAxisY,
        Self::ComponentAxisZ,
        Self::ComponentCamera,
        Self::ComponentAnimator,
        Self::ComponentScript,
        Self::ComponentMeshRenderer,
        Self::ComponentLight,
        Self::ComponentCollider,
        Self::ComponentApply,
        Self::ComponentUnsaved,
        Self::ComponentSaveClose,
        Self::ComponentDiscardClose,
        Self::ComponentCancel,
        Self::ComponentTransform,
        Self::ComponentRotation,
        Self::ComponentScale,
        Self::ComponentAdd,
        Self::ComponentEnable,
        Self::ComponentSearch,
        Self::ComponentRemove,
        Self::ComponentGeometry,
        Self::ComponentTint,
        Self::ComponentMaterial,
        Self::ComponentNoMaterial,
        Self::ComponentColliderRole,
        Self::ComponentBlocker,
        Self::ComponentSensor,
        Self::ComponentColliderShape,
        Self::ComponentBox,
        Self::ComponentSphere,
        Self::ComponentCenter,
        Self::ComponentRadius,
        Self::ComponentSize,
        Self::ComponentBuildApplication,
        Self::ComponentBuildFolder,
        Self::ComponentBuild,
        Self::ComponentBuildRun,
        Self::ComponentSaveBeforeBuild,
        Self::ComponentObjectMenu,
        Self::ComponentCreate,
        Self::Component3dObject,
        Self::ComponentEmptyObject,
        Self::ComponentParent,
        Self::ComponentSceneRoot,
        Self::ComponentMoveUp,
        Self::ComponentMoveDown,
        Self::ComponentOpenScene,
        Self::ComponentSceneFile,
        Self::ComponentOpen,
        Self::ComponentSaveOpen,
        Self::ComponentDiscardOpen,
        Self::ComponentConnectionUnavailable,
    ];

    pub fn plain(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome to Rot Engine",
            Self::NoRecentProjects => "No recent projects",
            Self::EmptyHierarchy => "This scene is empty. Add an object with Create.",
            Self::EmptyAssets => "No assets yet. Drop files here to import.",
            Self::EmptyConsole => "No messages",
            Self::OpeningProject => "Opening project…",
            Self::LoadingContent => "Loading project content…",
            Self::Building => "Building…",
            Self::BuildFinished => "Build finished",
            Self::NoResults => "No results",
            Self::AboutTagline => "",
            Self::NoMatchingCommands => "No matching commands",
            Self::GameCameraNeeded => "Set up the game camera to see what the player sees.",
            Self::EditsRestartPlay => "Edits apply to the scene and restart Play.",
            Self::ComponentFileMenu => "File",
            Self::ComponentEditMenu => "Edit",
            Self::ComponentHierarchy => "Hierarchy",
            Self::ComponentInspector => "Inspector",
            Self::ComponentName => "Name",
            Self::ComponentEnabled => "Enabled",
            Self::ComponentPosition => "Position",
            Self::ComponentAxisX => "X  ",
            Self::ComponentAxisY => "Y  ",
            Self::ComponentAxisZ => "Z  ",
            Self::ComponentCamera => "Camera",
            Self::ComponentAnimator => "Animator",
            Self::ComponentScript => "Script",
            Self::ComponentMeshRenderer => "Mesh Renderer",
            Self::ComponentLight => "Light",
            Self::ComponentCollider => "Collider",
            Self::ComponentApply => "Apply",
            Self::ComponentUnsaved => "Unsaved changes",
            Self::ComponentSaveClose => "Save and Close",
            Self::ComponentDiscardClose => "Discard and Close",
            Self::ComponentCancel => "Cancel",
            Self::ComponentTransform => "Transform",
            Self::ComponentRotation => "Rotation",
            Self::ComponentScale => "Scale",
            Self::ComponentAdd => "Add Component",
            Self::ComponentEnable => "Enable",
            Self::ComponentSearch => "Search Components",
            Self::ComponentRemove => "Remove Component",
            Self::ComponentGeometry => "Geometry",
            Self::ComponentTint => "Tint",
            Self::ComponentMaterial => "Material",
            Self::ComponentNoMaterial => "None",
            Self::ComponentColliderRole => "Role",
            Self::ComponentBlocker => "Blocker",
            Self::ComponentSensor => "Sensor",
            Self::ComponentColliderShape => "Shape",
            Self::ComponentBox => "Box",
            Self::ComponentSphere => "Sphere",
            Self::ComponentCenter => "Center",
            Self::ComponentRadius => "Radius",
            Self::ComponentSize => "Size",

            Self::ComponentBuildApplication => "Build Application",
            Self::ComponentBuildFolder => "Build Folder",
            Self::ComponentBuild => "Build",
            Self::ComponentBuildRun => "Build and Run",
            Self::ComponentSaveBeforeBuild => "Save the scene before building.",

            Self::ComponentObjectMenu => "Object",
            Self::ComponentCreate => "Create",
            Self::Component3dObject => "3D Object",
            Self::ComponentEmptyObject => "Empty Object",
            Self::ComponentParent => "Parent",
            Self::ComponentSceneRoot => "Scene Root",
            Self::ComponentMoveUp => "Move Up",
            Self::ComponentMoveDown => "Move Down",

            Self::ComponentOpenScene => "Open Scene",
            Self::ComponentSceneFile => "Scene File",
            Self::ComponentOpen => "Open",
            Self::ComponentSaveOpen => "Save and Open",
            Self::ComponentDiscardOpen => "Discard and Open",

            Self::ComponentConnectionUnavailable => {
                "AI connection is unavailable for this scene format."
            }
        }
    }

    /// The Rot line, where the wording table has one.
    pub fn rot(self) -> Option<&'static str> {
        match self {
            Self::Welcome => Some("Welcome to Rot Engine. Let's grow something."),
            Self::NoRecentProjects => Some("Nothing planted yet."),
            Self::EmptyHierarchy => Some("Nothing growing here yet. Add an object with Create."),
            Self::EmptyAssets => Some("Bare soil. Drop files here to import."),
            Self::EmptyConsole => Some("All quiet in the undergrowth."),
            Self::OpeningProject => Some("Spreading mycelium…"),
            Self::Building => Some("Fruiting…"),
            Self::BuildFinished => Some("Fruited."),
            Self::NoResults => Some("Nothing under this log."),
            Self::AboutTagline => Some("Everything grows from rot."),
            _ => None,
        }
    }
}

fn themed_id() -> egui::Id {
    egui::Id::unique("rot_themed_wording")
}

/// Called when the style or the Themed Wording preference changes.
pub fn set_themed(ctx: &egui::Context, themed: bool) {
    ctx.data_mut(|data| data.insert_temp(themed_id(), themed));
}

pub fn themed(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(themed_id()))
        .unwrap_or(false)
}

fn shown_id() -> egui::Id {
    egui::Id::unique("rot_themed_line_frame")
}

/// The words for `key` in the current style. At most one themed line shows
/// per screen (UX-STANDARDS §9): after the first in a frame, the rest of
/// that frame gets Plain wording.
pub fn tr(ctx: &egui::Context, key: Text) -> &'static str {
    let Some(rot) = key.rot().filter(|_| themed(ctx)) else {
        return key.plain();
    };
    let frame = ctx.cumulative_frame_nr();
    let owner = ctx.data(|data| data.get_temp::<(u64, Text)>(shown_id()));
    match owner {
        // Another line already took this frame; the same key may repeat
        // (egui can lay a frame out twice).
        Some((shown, other)) if shown == frame && other != key => key.plain(),
        _ => {
            ctx.data_mut(|data| data.insert_temp(shown_id(), (frame, key)));
            rot
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themed_wording_changes_only_lines_with_a_rot_variant() {
        let ctx = egui::Context::default();
        assert_eq!(tr(&ctx, Text::EmptyConsole), "No messages");
        set_themed(&ctx, true);
        assert_eq!(
            tr(&ctx, Text::EmptyConsole),
            "All quiet in the undergrowth."
        );
        assert_eq!(
            tr(&ctx, Text::EmptyAssets),
            Text::EmptyAssets.plain(),
            "one themed line per screen"
        );
        assert_eq!(tr(&ctx, Text::LoadingContent), "Loading project content…");
        for key in Text::ALL {
            assert!(key.plain().len() <= 80, "{key:?} fits on one line");
            if let Some(rot) = key.rot() {
                assert!(rot.len() <= 80);
            }
        }
    }
}
