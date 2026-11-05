//! Worker-only Save As preparation/publication. UI adoption owns all continuations.
use std::path::{Path, PathBuf};

use ruster_assets::AssetStore;
use ruster_scene::Session;
use sha2::{Digest, Sha256};

use crate::directory_grant::DirectoryGrant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SaveFailureKind {
    PermissionRequired,
    InvalidDestination,
    DestinationExists,
    AssetRootMismatch,
    SaveError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SaveFailure {
    pub kind: SaveFailureKind,
    pub message: String,
}

impl SaveFailure {
    fn new(kind: SaveFailureKind, message: impl AsRef<str>) -> Self {
        Self {
            kind,
            message: message.as_ref().chars().take(4096).collect(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self.kind {
            SaveFailureKind::PermissionRequired => "save_permission_required",
            SaveFailureKind::InvalidDestination => "invalid_destination",
            SaveFailureKind::DestinationExists => "destination_exists",
            SaveFailureKind::AssetRootMismatch => "asset_root_mismatch",
            SaveFailureKind::SaveError => "save_error",
        }
    }
}

impl std::fmt::Display for SaveFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SaveFailure {}

fn save_error(error: impl std::fmt::Display) -> SaveFailure {
    SaveFailure::new(SaveFailureKind::SaveError, error.to_string())
}

/// Captured caller state. Compare original_asset_root exactly as supplied; do not
/// infer it from the cloned association or normalize retained recovery provenance.
/// Selecting the accepted root and pinning the original provenance belong to the host.
#[derive(Debug)]
pub(super) struct Request {
    pub session: Session,
    pub destination: PathBuf,
    pub original_asset_root: Option<PathBuf>,
    pub grant: Option<DirectoryGrant>,
    /// True for an explicitly named MCP Save As; never inherit overwrite
    /// authority from the cloned session's existing file association.
    pub fresh_destination: bool,
}

/// Retain on the worker until the caller authorizes publication. Cancellation or
/// failed preparation/publication must destroy this state on that worker too.
#[derive(Debug)]
pub(super) struct Prepared {
    session: Session,
    path: PathBuf,
    directory: DirectoryGrant,
    grant: Option<DirectoryGrant>,
    fresh_destination: bool,
    destination_store: Option<AssetStore>,
    document_hash: String,
    document_bytes: usize,
}

/// A committed cloned session; adopt it without saving again. The hash describes
/// the admitted canonical document, never a post-commit read of mutable disk.
#[derive(Debug)]
pub(super) struct Saved {
    pub session: Session,
    pub destination_store: Option<AssetStore>,
    pub path: PathBuf,
    pub root: PathBuf,
    pub document_hash: String,
    pub document_bytes: usize,
}

fn validate_destination(
    directory: &DirectoryGrant,
    grant: Option<&DirectoryGrant>,
) -> Result<(), SaveFailure> {
    // Validate explicit authority first so a replaced granted directory retains
    // its permission failure instead of becoming a generic destination error.
    if let Some(grant) = grant {
        let granted = grant
            .validate()
            .map_err(|message| SaveFailure::new(SaveFailureKind::PermissionRequired, message))?;
        let root = directory
            .validate()
            .map_err(|message| SaveFailure::new(SaveFailureKind::InvalidDestination, message))?;
        if granted != root {
            return Err(SaveFailure::new(
                SaveFailureKind::PermissionRequired,
                "Scene destination is outside the granted save directory",
            ));
        }
    } else {
        directory
            .validate()
            .map_err(|message| SaveFailure::new(SaveFailureKind::InvalidDestination, message))?;
    }
    Ok(())
}

fn save_session(
    session: &mut Session,
    path: &Path,
    directory: &DirectoryGrant,
    grant: Option<&DirectoryGrant>,
    fresh_destination: bool,
) -> Result<(), SaveFailure> {
    // Recheck after acquiring any destination catalog lock. The grant is a
    // cooperating-filesystem identity check, not hostile ancestor-race isolation.
    validate_destination(directory, grant)?;
    validate_fresh_destination(session, path, fresh_destination)?;
    session.save(path).map_err(|failure| {
        // For a distinct fresh destination Session's final noclobber conflict
        // means another file won after our last absence check.
        if fresh_destination && matches!(failure, ruster_scene::SceneError::Conflict) {
            SaveFailure::new(SaveFailureKind::DestinationExists, failure.to_string())
        } else {
            save_error(failure)
        }
    })
}

fn validate_fresh_destination(
    session: &Session,
    path: &Path,
    fresh_destination: bool,
) -> Result<(), SaveFailure> {
    match path.symlink_metadata() {
        Ok(metadata) if !metadata.is_file() => {
            return Err(SaveFailure::new(
                SaveFailureKind::InvalidDestination,
                "Scene destination must be a regular file, never a symlink or directory",
            ))
        }
        Ok(_) if fresh_destination => {
            return Err(SaveFailure::new(
                SaveFailureKind::DestinationExists,
                "Save As requires a fresh destination that does not exist",
            ))
        }
        Ok(_) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(save_error(error)),
    }
    // Reject this identity even when temporarily absent. Otherwise a recreated
    // matching baseline between this check and Session's lock could regain its
    // associated-file overwrite authority. Distinct paths retain Session's
    // atomic persist_noclobber protection as the final publication boundary.
    if fresh_destination && session.path() == Some(path) {
        return Err(SaveFailure::new(
            SaveFailureKind::DestinationExists,
            "Fresh Save As requires a different destination from the current association",
        ));
    }
    Ok(())
}

impl Request {
    /// Worker-only: canonicalization, snapshot encoding/hashing and optional
    /// destination catalog open. This does not publish the scene document.
    pub fn prepare(self) -> Result<Prepared, SaveFailure> {
        if let Some(grant) = &self.grant {
            grant.validate().map_err(|message| {
                SaveFailure::new(SaveFailureKind::PermissionRequired, message)
            })?;
        }
        let parent = self
            .destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let directory = DirectoryGrant::new(parent)
            .map_err(|message| SaveFailure::new(SaveFailureKind::InvalidDestination, message))?;
        let name = self.destination.file_name().ok_or_else(|| {
            SaveFailure::new(
                SaveFailureKind::InvalidDestination,
                "Scene destination requires a file name",
            )
        })?;
        let path = directory.root().join(name);
        validate_destination(&directory, self.grant.as_ref())?;
        validate_fresh_destination(&self.session, &path, self.fresh_destination)?;
        let different_root = self.original_asset_root.as_deref() != Some(directory.root());
        if different_root && self.session.has_asset_references() {
            return Err(SaveFailure::new(SaveFailureKind::AssetRootMismatch, "Save As across asset roots is unavailable while scene or undo/redo history references project assets; save beside the existing project."));
        }
        // snapshot_bytes and Session::save use the same canonical encoding,
        // including current-format migration, history, behavior and final newline.
        let bytes = self.session.snapshot_bytes().map_err(save_error)?;
        let document_hash = format!("{:x}", Sha256::digest(&bytes));
        let document_bytes = bytes.len();
        let destination_store = if different_root {
            Some(AssetStore::open(directory.root()).map_err(save_error)?)
        } else {
            // Reopening here would add synchronous work and replace independent
            // accepted/last-good catalog state during an ordinary same-root save.
            None
        };
        Ok(Prepared {
            session: self.session,
            path,
            directory,
            grant: self.grant,
            fresh_destination: self.fresh_destination,
            destination_store,
            document_hash,
            document_bytes,
        })
    }
}

impl Prepared {
    /// Worker-only, after the caller's final identity/draft/lease authorization.
    /// Publication preserves Session's existing baseline/fresh-file semantics.
    pub fn publish(self) -> Result<Saved, SaveFailure> {
        let Self {
            mut session,
            path,
            directory,
            grant,
            fresh_destination,
            destination_store,
            document_hash,
            document_bytes,
        } = self;
        validate_destination(&directory, grant.as_ref())?;
        match &destination_store {
            Some(store) => store
                .with_current_catalog(|| {
                    // Never reenter asset mutation/open while this guard is held.
                    Ok(save_session(
                        &mut session,
                        &path,
                        &directory,
                        grant.as_ref(),
                        fresh_destination,
                    ))
                })
                .map_err(save_error)?,
            None => save_session(
                &mut session,
                &path,
                &directory,
                grant.as_ref(),
                fresh_destination,
            ),
        }?;
        // Nothing fallible remains after Session::save reports its disk commit.
        Ok(Saved {
            session,
            path,
            root: directory.root().to_path_buf(),
            destination_store,
            document_hash,
            document_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::Scene;

    // Helper boundary only: preparation cannot publish a scene; clone adoption
    // preserves canonical receipts, save authority and history-root confinement.
    #[test]
    fn save_as_preparation_and_publication_preserve_receipts_and_original_authority() {
        let owner = tempfile::tempdir().unwrap();
        let original = owner.path().join("original");
        let destination = owner.path().join("destination");
        std::fs::create_dir(&original).unwrap();
        std::fs::create_dir(&destination).unwrap();
        let original = original.canonicalize().unwrap();
        let destination = destination.canonicalize().unwrap();
        let source = original.join("scene.json");
        let mut caller = Session::new(Scene::demo()).unwrap();
        caller.save(&source).unwrap();
        let source_bytes = std::fs::read(&source).unwrap();
        caller.set_color(0, "left", [0.2, 0.3, 0.4, 1.0]).unwrap();
        let caller_bytes = caller.snapshot_bytes().unwrap();
        let request = |path: PathBuf, grant| Request {
            session: caller.clone(),
            destination: path,
            original_asset_root: Some(original.clone()),
            grant,
            fresh_destination: true,
        };
        assert_eq!(
            request(source.clone(), None).prepare().unwrap_err().code(),
            "destination_exists"
        );
        assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
        std::fs::remove_file(&source).unwrap();
        assert_eq!(
            request(source.clone(), None).prepare().unwrap_err().code(),
            "destination_exists"
        );
        std::fs::write(&source, &source_bytes).unwrap();
        let denied = destination.join("outside-grant.json");
        assert_eq!(
            request(
                denied.clone(),
                Some(DirectoryGrant::new(&original).unwrap())
            )
            .prepare()
            .unwrap_err()
            .code(),
            "save_permission_required"
        );
        assert!(!denied.exists());
        let directory_target = destination.join("directory.json");
        std::fs::create_dir(&directory_target).unwrap();
        assert_eq!(
            request(directory_target, None)
                .prepare()
                .unwrap_err()
                .code(),
            "invalid_destination"
        );
        #[cfg(unix)]
        {
            let alias = destination.join("alias.json");
            std::os::unix::fs::symlink(&source, &alias).unwrap();
            assert_eq!(
                request(alias, None).prepare().unwrap_err().code(),
                "invalid_destination"
            );
        }
        let path = destination.join("copy.json");
        let prepared = request(
            path.clone(),
            Some(DirectoryGrant::new(&destination).unwrap()),
        )
        .prepare()
        .unwrap();
        assert!(!path.exists());
        assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
        let saved = prepared.publish().unwrap();
        let committed = std::fs::read(&path).unwrap();
        assert_eq!(saved.path, path);
        assert_eq!(saved.root, destination);
        assert_eq!(
            saved.document_hash,
            format!("{:x}", Sha256::digest(&committed))
        );
        assert_eq!(saved.document_bytes, committed.len());
        assert_eq!(saved.session.snapshot_bytes().unwrap(), committed);
        assert_eq!(saved.session.path(), Some(path.as_path()));
        assert!(!saved.session.is_dirty());
        assert_eq!(
            saved.destination_store.as_ref().unwrap().root(),
            destination
        );
        assert_eq!(caller.path(), Some(source.as_path()));
        assert!(caller.is_dirty());
        assert_eq!(caller.snapshot_bytes().unwrap(), caller_bytes);

        // Session's fresh publication still wins over a file appearing after
        // preparation, while the original caller remains dirty and associated.
        let raced = destination.join("raced.json");
        let prepared = request(raced.clone(), None).prepare().unwrap();
        std::fs::write(&raced, b"external file").unwrap();
        assert_eq!(prepared.publish().unwrap_err().code(), "destination_exists");
        assert_eq!(std::fs::read(&raced).unwrap(), b"external file");
        assert_eq!(caller.snapshot_bytes().unwrap(), caller_bytes);
        assert_eq!(std::fs::read(&source).unwrap(), source_bytes);

        // A canonical alias of the original root must not reopen its catalog.
        // The independent invalid catalog therefore cannot block this save.
        std::fs::write(original.join(".ruster-assets.json"), b"invalid catalog").unwrap();
        let same = request(original.join(".").join("same-root.json"), None)
            .prepare()
            .unwrap()
            .publish()
            .unwrap();
        assert!(same.destination_store.is_none());
        assert_eq!(same.root, original);
        assert_eq!(
            same.document_bytes,
            std::fs::read(&same.path).unwrap().len()
        );

        // A prepared destination catalog must still be the same exact baseline
        // when the scene publication callback acquires its catalog guard.
        let catalog_race = destination.join("catalog-race.json");
        let prepared = request(catalog_race.clone(), None).prepare().unwrap();
        std::fs::write(destination.join(".ruster-assets.json"), b"changed catalog").unwrap();
        assert_eq!(prepared.publish().unwrap_err().code(), "save_error");
        assert!(!catalog_race.exists());

        // Ordinary associated Save retains its baseline-checked overwrite
        // semantics; only explicit fresh mode rejects that same destination.
        let mut associated = request(source.clone(), None);
        associated.fresh_destination = false;
        let associated = associated.prepare().unwrap().publish().unwrap();
        assert_eq!(associated.path, source);
        assert!(!associated.session.is_dirty());
        assert_eq!(std::fs::read(&source).unwrap(), caller_bytes);
        assert_eq!(caller.snapshot_bytes().unwrap(), caller_bytes);
        assert!(caller.is_dirty());

        let mut history = caller.clone();
        let revision = history.scene().revision;
        history
            .set_mesh(revision, "left", Some("historic".into()))
            .unwrap();
        history.set_mesh(revision + 1, "left", None).unwrap();
        assert!(history
            .scene()
            .objects
            .iter()
            .all(|object| object.mesh.is_none()));
        let history_bytes = history.snapshot_bytes().unwrap();
        let rejected = Request {
            session: history.clone(),
            destination: destination.join("history.json"),
            original_asset_root: Some(original),
            grant: None,
            fresh_destination: true,
        }
        .prepare();
        assert_eq!(rejected.unwrap_err().code(), "asset_root_mismatch");
        assert_eq!(history.snapshot_bytes().unwrap(), history_bytes);
        assert!(!destination.join("history.json").exists());
        let bounded = SaveFailure::new(SaveFailureKind::SaveError, "é".repeat(5000));
        assert_eq!(bounded.message.chars().count(), 4096);
    }
}
