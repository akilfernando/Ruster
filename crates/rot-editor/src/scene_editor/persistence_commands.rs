//! Explicit destination authority for guarded scene saves.
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, (String, String)>;
fn error(code: &str, message: &str) -> (String, String) {
    (code.into(), bounded(message))
}
pub(super) fn bounded(text: &str) -> String {
    text.chars().take(4096).collect()
}
#[cfg(test)]
pub(super) fn document_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Clone)]
pub(super) struct SaveGrant(crate::directory_grant::DirectoryGrant);
impl SaveGrant {
    pub(super) fn new(path: &Path) -> std::result::Result<Self, String> {
        crate::directory_grant::DirectoryGrant::new(path).map(Self)
    }
    pub(super) fn root(&self) -> &Path {
        self.0.root()
    }
    pub(super) fn validate(&self) -> Result<&Path> {
        self.0.validate().map_err(|_| {
            error(
                "save_permission_required",
                "The project folder changed or became unavailable; reconnect the AI app",
            )
        })
    }
}

/// The real folder holding a file, so a scene opened by another path to the
/// same folder still matches the save grant.
pub(super) fn canonical_parent(path: &Path) -> Option<PathBuf> {
    path.parent()?.canonicalize().ok()
}

pub(super) fn reject_symlink(path: &Path) -> Result<()> {
    match path.symlink_metadata() {
        Ok(metadata) if !metadata.is_file() => Err(error(
            "invalid_destination",
            "Scene destination must be a regular file, never a symlink or directory",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error("save_error", &e.to_string())),
    }
}
