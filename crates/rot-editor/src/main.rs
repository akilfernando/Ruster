//! Native authoring frontend. egui paints through wgpu's Vulkan backend only.
#[cfg(unix)]
mod ai_apps;
mod asset_save;
mod asset_worker;
mod behavior_external;
mod behavior_files;
mod behavior_ui;
mod behavior_worker;
mod branding;
#[cfg(unix)]
mod bridge;
// Shared wire primitives; each binary uses only its own side.
use rot_support::strict_json;
#[cfg(unix)]
#[allow(unused_imports)]
use rot_support::{connection, discovery, trust};
mod console;
mod creation_export;
mod directory_grant;
mod editor_theme;
mod focus;
mod input_ui;
mod manipulation;
mod recovery;
mod recovery_ui;
mod recovery_worker;
mod scene_editor;
mod strings;
mod ui_kit;
mod workspace;

use std::path::PathBuf;

// Backend diagnostics must remain visible even without an external logger.
struct Diagnostics;
impl log::Log for Diagnostics {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("{} {}: {}", record.level(), record.target(), record.args());
            console::engine_log(
                record.level(),
                format!("{}: {}", record.target(), record.args()),
            );
        }
    }

    fn flush(&self) {}
}

static DIAGNOSTICS: Diagnostics = Diagnostics;

const HELP: &str = "Rot Engine Editor — scene authoring\n\nUsage: rot-editor\n       rot-editor --scene PATH\n       rot-editor --help\n\nDefault: project hub with recent projects, Open Project and New Project.\nRequires a desktop session and Vulkan adapter. Edit → Preferences… (Ctrl+,)\n→ Shortcuts lists and remaps shortcuts.\n";

fn main() -> std::process::ExitCode {
    log::set_logger(&DIAGNOSTICS).expect("binary installs one logger");
    log::set_max_level(log::LevelFilter::Warn);
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("rot-editor: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn scene_argument(args: &[String]) -> Result<Option<Option<PathBuf>>, String> {
    if args.is_empty() {
        return Ok(Some(None));
    }
    if matches!(args, [flag] if flag == "--help" || flag == "-h") {
        return Ok(None);
    }
    if args[0] == "--scene" {
        if args.len() != 2 || args[1].starts_with('-') {
            return Err("--scene requires exactly one scene file path".into());
        }
        return Ok(Some(Some(PathBuf::from(&args[1]))));
    }
    Err(format!(
        "unknown or repeated argument: {}; use --help",
        args[0]
    ))
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = scene_argument(&args)? else {
        print!("{HELP}");
        return Ok(());
    };
    match path {
        Some(path) => scene_editor::run(path),
        None => scene_editor::run_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_fail_before_desktop_initialization() {
        assert_eq!(scene_argument(&[]).unwrap(), Some(None));
        assert_eq!(scene_argument(&["--help".into()]).unwrap(), None);
        assert_eq!(
            scene_argument(&["--scene".into(), "custom.json".into()]).unwrap(),
            Some(Some(PathBuf::from("custom.json")))
        );
        assert!(scene_argument(&["--scene".into()]).is_err());
        assert!(scene_argument(&["--typo".into()]).is_err());
    }
}
