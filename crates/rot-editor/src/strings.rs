//! The editor's string table (UX-STANDARDS §8–§9, S-08). Each key has Plain
//! wording and, for the few moments where it fits, a Rot line. Themed
//! wording shows only with the Rot style and Themed Wording both on, and
//! never in menus, buttons, errors, warnings, saving or AI permissions.
//!
//! Command names live in `Action::label`; this table holds the rest.
use eframe::egui;

// EmptyHierarchy, OpeningProject, Building and BuildFinished are wired by
// the rest of the wording sweep (BACKLOG S-08 notes).
#[allow(dead_code)]
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
}

impl Text {
    #[cfg(test)]
    pub const ALL: [Self; 14] = [
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

/// The words for `key` in the current style.
pub fn tr(ctx: &egui::Context, key: Text) -> &'static str {
    if themed(ctx) {
        key.rot().unwrap_or(key.plain())
    } else {
        key.plain()
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
        assert_eq!(tr(&ctx, Text::LoadingContent), "Loading project content…");
        for key in Text::ALL {
            assert!(key.plain().len() <= 80, "{key:?} fits on one line");
            if let Some(rot) = key.rot() {
                assert!(rot.len() <= 80);
            }
        }
    }
}
