//! Native file and folder dialogs backed by `rfd` with per-OS feature selection.
use std::path::{Path, PathBuf};

/// Pick one or more files matching `filter_name` / `extensions`.
pub fn pick_files(title: &str, filter_name: &str, extensions: &[&str]) -> Vec<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter_name, extensions)
        .pick_files()
        .unwrap_or_default()
}

/// Pick a single file matching `filter_name` / `extensions`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn pick_file(title: &str, filter_name: &str, extensions: &[&str]) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter(filter_name, extensions)
        .pick_file()
}

/// Pick a folder, optionally seeded at `directory`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn pick_folder(title: &str, directory: Option<&Path>) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().set_title(title);
    if let Some(directory) = directory {
        dialog = dialog.set_directory(directory);
    }
    dialog.pick_folder()
}

/// Choose a destination save path starting in `directory` with `filename`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn save_file(title: &str, directory: &Path, filename: &str) -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .set_directory(directory)
        .set_file_name(filename)
        .save_file()
}
