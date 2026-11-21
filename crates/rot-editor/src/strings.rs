//! The editor's string table (UX-STANDARDS §8–§9, S-08). Each key has Plain
//! wording and, for the few moments where it fits, a Rot line. Themed
//! wording shows only with the Rot style and Themed Wording both on, and
//! never in menus, buttons, errors, warnings, saving or AI permissions.
//!
//! Command names live in `Action::label`; this table holds the rest.
use eframe::egui;

// Keep each key and its Plain wording together. Tests use the same list.
macro_rules! editor_text {
    ($($key:ident => $plain:literal,)+) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Text {
            $($key,)+
        }

        impl Text {
            #[cfg(test)]
            pub const ALL: &'static [Self] = &[$(Self::$key,)+];

            pub const fn plain(self) -> &'static str {
                match self {
                    $(Self::$key => $plain,)+
                }
            }
        }
    };
}

editor_text! {
    Welcome => "Welcome to Rot Engine",
    NoRecentProjects => "No recent projects",
    EmptyHierarchy => "This scene is empty. Add an object with Create.",
    NewGroup => "New Group",
    NewGroupHint => "Create an empty group.",
    GroupCreateBlocked => "Couldn't create an empty group. This legacy scene uses {}.",
    GroupCreateFinishEdit => "Finish the current edit before creating a group.",
    GroupCreateWaitForAssets => "Wait for assets to finish before creating a group.",
    GroupCreateSceneFull => "Couldn't create a group. This scene is full.",
    GroupCreated => "Created an empty group.",
    GroupSelectionName => "Group",
    GroupSelectionLocked => "Unlock the objects before grouping them.",
    GroupSelectionWorldTransform => "Grouping would change the objects’ world transforms.",
    GroupSelectionFailed => "Couldn't group selection.",
    GroupBlockerInput => "input actions",
    GroupBlockerHud => "a HUD",
    GroupBlockerScriptSources => "ordinary script sources",
    GroupBlockerObjectScripts => "scripts attached to objects",
    GroupBlockerApplicationScript => "a script attached to the application",
    EmptyAssets => "No assets yet. Drop files here to import.",
    EmptyConsole => "No messages",
    OpeningProject => "Opening project…",
    LoadingContent => "Loading project content…",
    Building => "Building…",
    BuildFinished => "Build finished",
    NoResults => "No results",
    AboutTagline => "",
    NoMatchingCommands => "No matching commands",
    GameCameraNeeded => "Set up the game camera to see what the player sees.",
    EditsRestartPlay => "Edits apply to the scene and restart Play.",
    ComponentFileMenu => "File",
    ComponentEditMenu => "Edit",
    ComponentHierarchy => "Hierarchy",
    ComponentAssets => "Assets",
    ComponentInspector => "Inspector",
    ComponentName => "Name",
    ComponentEnabled => "Enabled",
    ComponentPosition => "Position",
    ComponentAxisX => "X  ",
    ComponentAxisY => "Y  ",
    ComponentAxisZ => "Z  ",
    ComponentCamera => "Camera",
    ComponentAnimator => "Animator",
    ComponentScript => "Script",
    ComponentMeshRenderer => "Mesh Renderer",
    ComponentLight => "Light",
    ComponentCollider => "Collider",
    ComponentApply => "Apply",
    ComponentUnsaved => "Unsaved changes",
    ComponentSaveClose => "Save and Close",
    ComponentDiscardClose => "Discard and Close",
    ComponentCancel => "Cancel",
    ComponentTransform => "Transform",
    ComponentRotation => "Rotation",
    ComponentScale => "Scale",
    ComponentAdd => "Add Component",
    ComponentEnable => "Enable",
    ComponentSearch => "Search Components",
    ComponentRemove => "Remove Component",
    ComponentGeometry => "Geometry",
    ComponentTint => "Tint",
    ComponentMaterial => "Material",
    ComponentNoMaterial => "None",
    MaterialUnapplied => "Unapplied Changes",
    MaterialApply => "Apply Material",
    MaterialApplyHint => "Apply the changes. Enter from a material field.",
    MaterialCancelHint => "Discard the changes. Escape from a material field.",
    MaterialUsedByOne => "Used by 1 object",
    MaterialUsedByMany => "Used by {count} objects",
    MaterialAffectsOne => "Affects 1 object",
    MaterialAffectsMany => "Affects {count} objects",
    MaterialNew => "New Material",
    MaterialNewFor => "New material for {object}",
    MaterialMakeUnique => "Make Unique",
    MaterialUniqueFor => "Unique copy for {object}",
    MaterialUniqueHint => "Copy the material for the selected object.",
    MaterialManage => "Manage Materials",
    MaterialEmpty => "No materials yet.",
    MaterialDelete => "Delete",
    MaterialDeleteHint => "Delete the unused material.",
    MaterialInUseHint => "Clear material assignments before deleting.",
    MaterialCopyName => "{material} copy",
    MaterialApplied => "Material applied.",
    MaterialCancelled => "Material changes discarded.",
    MaterialAssigned => "Material assignment updated.",
    MaterialDeleted => "Material deleted.",
    MaterialTintInvalid => "Enter three finite tint channels from 0 to 1.",
    MaterialRoughnessInvalid => "Enter a roughness from 0.08 to 1.",
    MaterialApplyBlocked => "Finish the other edit before applying the material.",
    MaterialTargetChanged => "Material changed. Cancel this edit and inspect it again.",
    MaterialUnassigned => "Material is no longer assigned.",
    MaterialTextureNeedsUvs => "Choose a mesh with UVs before applying a textured material.",
    ComponentColliderRole => "Role",
    ComponentBlocker => "Blocker",
    ComponentSensor => "Sensor",
    ComponentColliderShape => "Shape",
    ComponentBox => "Box",
    ComponentSphere => "Sphere",
    ComponentCenter => "Center",
    ComponentRadius => "Radius",
    ComponentSize => "Size",
    ComponentBuildApplication => "Build Application",
    ComponentBuildFolder => "Build Folder",
    ComponentBuild => "Build",
    ComponentBuildRun => "Build and Run",
    ComponentSaveBeforeBuild => "Save the scene before building.",
    BuildInvalidDestination => "Couldn't build. The build folder path is invalid.",
    BuildChooseAbsolutePath => "Enter an absolute path for a new folder.",
    BuildDestinationOccupied => "Couldn't build. That path already exists.",
    BuildChooseNewFolder => "Choose a new build folder.",
    BuildParentUnavailable => "Couldn't build. The parent folder is unavailable.",
    BuildChooseWritableParent => "Choose an existing folder you can write to.",
    BuildPlayerUnavailable => "Couldn't build. The Linux player is missing or unusable.",
    BuildInstallPlayer => "Reinstall Rot Engine or rebuild the Linux player.",
    BuildSceneUnavailable => "Couldn't build. The saved scene is unavailable or changed.",
    BuildAssetUnavailable => "Couldn't build. A required asset couldn't be packaged.",
    BuildFixAsset => "Check Error Details, then restore or reimport the asset.",
    BuildFailed => "Couldn't prepare the build.",
    BuildCheckDetails => "Open Error Details, fix the reported issue, and retry.",
    BuildLaunched => "Build finished. Player launched.",
    BuildLaunchFailed => "Build finished, but the player couldn't start.",
    BuildRunManually => "Open Error Details, then try running application from the build folder.",
    BuildCancelling => "Cancelling build…",
    BuildCancelled => "Build cancelled.",
    BuildDetailsTooltip => "Show or hide the technical error",
    BuildCancelShortcut => "Cancel (Esc)",
    BuildCloseShortcut => "Close (Esc)",
    BuildSubmitShortcut => "Build (Enter)",
    ComponentObjectMenu => "Object",
    ComponentCreate => "Create",
    ComponentCreateChild => "Create Child",
    PrefabSave => "Couldn't save the prefab.",
    PrefabPlace => "Couldn't place the prefab.",
    PrefabApply => "Couldn't apply the prefab override.",
    PrefabRevert => "Couldn't revert the prefab override.",
    PrefabRefresh => "Couldn't update linked prefabs.",
    PrefabUnavailable => "Prefab source unavailable",
    PrefabRecovery => "Restore the prefab file and its .rotmeta sidecar to Assets.",
    PrefabInvalid => "Couldn't read the prefab file.",
    ErrorDetails => "Error Details",
    DismissError => "Dismiss",
    CloseErrorDetails => "Close",
    AssetImportFailed => "Couldn't import this file.",
    AssetSourceMissing => "Source file is missing.",
    AssetReplaceSource => "Replace it with a valid file at this path.",
    AssetRestoreSource => "Restore the file to this path.",
    AssetLastGoodAvailable => "Last working version is available.",
    AssetNoLastGood => "No working version is available.",
    AssetImportsNeedAttention => "Some asset imports need attention.",
    Component3dObject => "3D Object",
    ComponentEmptyObject => "Empty Object",
    ComponentParent => "Parent",
    ComponentSceneRoot => "Scene Root",
    ComponentMoveUp => "Move Up",
    ComponentMoveDown => "Move Down",
    ComponentObjectsSelected => "objects selected",
    ComponentMixed => "Mixed",
    ComponentHide => "Hide in Viewport",
    ComponentShow => "Show in Viewport",
    ComponentLockPicking => "Lock Viewport Picking",
    ComponentUnlockPicking => "Unlock Viewport Picking",
    ComponentParentHidden => "Hidden by Parent",
    ComponentParentLocked => "Picking Locked by Parent",
    ComponentPlayValues => "Play Values",
    ComponentSceneValues => "Scene Values",
    ComponentPlayTick => "Tick",
    ComponentWaitingForPlayFrame => "Waiting for the first frame…",
    ComponentNoPlayTransform => "Transform unavailable in this run.",
    ComponentOpenScene => "Open Scene",
    ComponentSceneFile => "Scene File",
    ComponentOpen => "Open",
    ComponentSaveOpen => "Save and Open",
    ComponentDiscardOpen => "Discard and Open",
    ScriptSection => "Scripts",
    ObjectScriptTab => "Object Script",
    ApplicationScriptTab => "Application Script",
    ScriptSelectGroup => "Select a group to add a rotation script.",
    ScriptSelectObject => "Select an object to add a script.",
    ScriptFinishOtherEdits => "Finish other edits before changing the script.",
    ScriptEditSource => "Edit Script…",
    ScriptDetach => "Detach Script",
    ScriptAdd => "Add Script…",
    ScriptMissingSource => "Script is missing. Restore or detach it.",
    ScriptAddProperty => "Add Property",
    ScriptStateDefaults => "State Defaults",
    ScriptDetails => "Script Details",
    ScriptLuaApplication => "Lua Application",
    ScriptLuaFollower => "Lua Follower",
    ScriptLuaGroupRotation => "Lua Group Rotation",
    ScriptLuaKinematic => "Lua Kinematic Mover",
    ScriptAttachExisting => "Attach Existing Script…",
    ScriptEditor => "Script Editor",
    ScriptNoneSelected => "No script selected.",
    ScriptEditorShortcuts => "Ctrl+Enter Apply · Ctrl+S Apply and Save · Esc Close",
    ScriptDisplayName => "Display Name",
    ScriptLanguage => "Language ID",
    ScriptApiVersion => "API Version",
    ScriptTextTooLong => "That text is too long. The previous text is preserved.",
    ScriptApply => "Apply Script",
    ScriptRevert => "Revert Draft",
    ScriptCloseKeepDraft => "Close and Keep Draft",
    ScriptExternalEditor => "External Editor…",
    ScriptExportWorkingCopy => "Export Working Copy",
    ScriptOpenZed => "Open in Zed",
    ScriptRefreshWorkingCopy => "Refresh Working Copy",
    ScriptFinishBeforeExternal => "Apply or revert your drafts first.",
    ScriptWorkingCopyDetails => "Working Copy Details",
    ScriptRecoveredCommandInvalid => "Couldn't restore the command edit. The draft is still saved.",
    ScriptRecoveredStateConflict => "Restore the script or value edit before adding a state field.",
    ScriptRecoveredStateInvalid => "Couldn't restore the state field. The draft is still saved.",
    ScriptRecoveredSourceInvalid => "Couldn't restore the script edit. The draft is still saved.",
    ScriptRecoveredPropertyMissing => "The script is missing. The property draft is still saved.",
    ScriptRecoveredPropertyInvalid => "Couldn't restore the property edit. The draft is still saved.",
    ScriptChangedWithDraft => "The script changed. Revert or copy your draft before retrying.",
    ScriptAttachmentChanged => "The attachment changed. Revert your property draft and try again.",
    ScriptFinishBeforeOpening => "Apply or revert the draft before opening another script.",
    ScriptMissingAttachment => "Script is missing. Restore it or attach a different script.",
    ScriptStateChanged => "The state field changed. Cancel the draft and start again.",
    ScriptCompatibilityWarning => "This script version may not run here. You can still edit it.",
    ScriptObjectDragInvalid => "Couldn't use that object. Drag it again from this scene.",
    ScriptObjectMissing => "Object unavailable. Choose a replacement or None.",
    ScriptObjectDropHint => "Choose an object or drop its Hierarchy row here.",
    ScriptSoundUnavailable => "Sound unavailable. Import a WAV or choose another sound.",
    ScriptAssetUnavailable => "Asset unavailable. Choose a replacement or None.",
    ScriptObjectReference => "Object Reference",
    ScriptSoundAsset => "Sound Asset",
    ScriptAssetReference => "Asset Reference",
    ScriptUpdated => "Script updated.",
    ScriptEditBlocked => "Couldn't edit the script. Finish your current edit first.",
    ScriptKinematicHint => "Use a Blocker Sphere, uniform scale, and a flat scene.",
    AssetNewFolder => "New Folder",
    AssetCreateFolder => "Create Folder",
    AssetRenameHeading => "Rename Asset",
    AssetRename => "Rename",
    AssetDeleteHeading => "Delete Asset",
    AssetMoveToTrash => "Move to Trash",
    AssetTrashQuestion => "Move this file and its import settings to the project trash?",
    AssetSearch => "Search Assets",
    AssetNewFolderCommand => "New Folder…",
    AssetShowInFileManager => "Show in File Manager",
    AssetPlaceInScene => "Place in Scene",
    AssetImportSettings => "Import Settings",
    AssetActions => "Asset Actions",
    AssetImportWarnings => "Import Warnings",
    AssetRenameCommand => "Rename…",
    AssetReimport => "Reimport",
    AssetDeleteCommand => "Delete…",
    AssetOpenFileManagerError => "Couldn't open the file manager.",
    AssetOperationBusy => "Wait for the current asset operation to finish.",
    AssetProjectUnavailable => "Project assets are unavailable.",
    AssetChanged => "The asset changed. Select it and try again.",
    AssetMeshUnavailable => "No mesh is available for this asset.",
    AssetReady => "Ready",
    AssetInvalid => "Invalid",
    AssetMissing => "Missing",
    RecoveryRestart => "Recovery stopped. Restart the editor.",
    RecoveryFinishing => "Recovery is finishing the previous operation.",
    RecoveryHeading => "Recover Unfinished Work",
    RecoveryCopyHint => "Restore opens an unsaved copy. Discard deletes the recovery copy.",
    RecoveryUnreadable => "Unreadable Recovery Copy",
    RecoveryDetails => "Recovery Details",
    RecoveryRestore => "Restore as Unsaved Copy",
    RecoveryDiscard => "Discard Recovery Copy",
    RecoveryNone => "No recovery copies available.",
    RecoveryRefresh => "Refresh Recovery List",
    RecoveryKeep => "Keep for Later",
    RecoverySaveFailed => "Couldn't save a recovery copy. Choose Retry.",
    RecoveryCleanupFailed => "Couldn't clean up recovery copies.",
    ScriptApplicationCommands => "Application Commands",
    AiConnectionKeyChanged => "Only allow the app if you set it up again.",
    ScenarioUnapplied => "Unapplied Changes",
    ScenarioOpenFileHint => "Choose a scenario JSON file.",
    ScenarioSaveAsFileHint => "Choose a new file path. Existing files are protected.",
    WorkingCopySaveRequired => "Save the scene before using a working copy.",
    WorkingCopyInvalid => "Couldn't read the working copy.",
    WorkingCopyBusy => "A working-copy operation is still running.",
    WorkingCopyConflict => "Script and working copy conflict.",
    WorkingCopyMissing => "The working copy is missing.",
    WorkingCopyLimit => "The working copy is too large.",
    WorkingCopyIo => "Couldn't access the working copy.",
    WorkingCopyPreviousScene => "The previous scene's working-copy operation is finishing.",
    WorkingCopyStopped => "The working-copy operation stopped. Try again.",
    WorkingCopySceneChanged => "The script changed during the operation. Choose Refresh.",
    WorkingCopyOpenCancelled => "Opening the external editor was cancelled.",
    WorkingCopyDraftConflict => "Finish your drafts, then import the file or choose Refresh.",
    WorkingCopyReviewConflict => "Script and working copy conflict. Choose a version below.",
    WorkingCopyExportMissing => "The working copy is missing. Choose Export Working Copy.",
    WorkingCopyHeading => "External Working Copy",
    WorkingCopyObservationChanged => "The script changed. Choose Refresh before selecting a version.",
    WorkingCopyExportNew => "Export New Working Copy",
    WorkingCopyOpenFile => "Open File in Zed",
    WorkingCopyImportFile => "Import File",
    WorkingCopyReplaceFile => "Replace File with Scene Script",
    WorkingCopyReady => "Working copy ready.",
    WorkingCopyLanguageChanged => "Script language or API changed. Export a new working copy.",
    WorkingCopyImported => "Imported script. Save the scene to keep it.",
    WorkingCopyClosing => "Finish closing or cancel it before using the external editor.",
    WorkingCopyWaitForEditors => "Wait for an external editor to finish starting, then retry.",
    WorkingCopyScriptMissing => "Couldn't find the script.",
    WorkingCopyRefreshConflict => "Choose Refresh to compare the scene and file again.",
    WorkingCopyRefreshChanged => "The script changed. Choose Refresh before resolving the conflict.",
    WorkingCopyOpening => "Opening the external editor…",
    WorkingCopyFileChanged => "The working copy changed. Choose Refresh before opening it.",
    WorkingCopyInvalidEncoding => "The working copy has no valid UTF-8 text.",
    WorkingCopyOpened => "Opened the working copy.",
    WorkingCopyPrepared => "The working copy is ready to open.",
    WorkingCopySaveRecorded => "Saved the working-copy comparison.",
}

impl Text {
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
