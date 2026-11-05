//! Worker-only Save As preparation/publication. UI adoption owns all continuations.
use std::path::{Path, PathBuf};

use ruster_assets::{AssetError, AssetStore};
use ruster_scene::Session;
use sha2::{Digest, Sha256};

use crate::directory_grant::DirectoryGrant;

/// Captured caller state. original_asset_root is the already canonical accepted
/// root, including a recovery origin; do not infer it from the cloned association.
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
) -> Result<(), String> {
    let root = directory.validate()?;
    if let Some(grant) = grant {
        if grant.validate()? != root {
            return Err("Scene destination is outside the granted save directory".into());
        }
    }
    Ok(())
}

fn save_session(
    session: &mut Session,
    path: &Path,
    directory: &DirectoryGrant,
    grant: Option<&DirectoryGrant>,
    fresh_destination: bool,
) -> Result<(), AssetError> {
    // Recheck after acquiring any destination catalog lock. The grant is a
    // cooperating-filesystem identity check, not hostile ancestor-race isolation.
    validate_destination(directory, grant).map_err(AssetError)?;
    validate_fresh_destination(session, path, fresh_destination).map_err(AssetError)?;
    session
        .save(path)
        .map_err(|failure| AssetError(failure.to_string()))
}

fn validate_fresh_destination(
    session: &Session,
    path: &Path,
    fresh_destination: bool,
) -> Result<(), String> {
    if !fresh_destination {
        return Ok(());
    }
    match path.symlink_metadata() {
        Ok(_) => return Err("Save As requires a fresh destination that does not exist".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.to_string()),
    }
    // Reject this identity even when temporarily absent. Otherwise a recreated
    // matching baseline between this check and Session's lock could regain its
    // associated-file overwrite authority. Distinct paths retain Session's
    // atomic persist_noclobber protection as the final publication boundary.
    if session.path() == Some(path) {
        return Err(
            "Fresh Save As requires a different destination from the current association".into(),
        );
    }
    Ok(())
}

impl Request {
    /// Worker-only: canonicalization, snapshot encoding/hashing and optional
    /// destination catalog open. This does not publish the scene document.
    pub fn prepare(self) -> Result<Prepared, String> {
        let parent = self
            .destination
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let directory = DirectoryGrant::new(parent)?;
        let name = self
            .destination
            .file_name()
            .ok_or("Scene destination requires a file name")?;
        let path = directory.root().join(name);
        validate_destination(&directory, self.grant.as_ref())?;
        validate_fresh_destination(&self.session, &path, self.fresh_destination)?;
        let different_root = self.original_asset_root.as_deref() != Some(directory.root());
        if different_root && self.session.has_asset_references() {
            return Err("Save As across asset roots is unavailable while scene or undo/redo history references project assets; save beside the existing project.".into());
        }
        // snapshot_bytes and Session::save use the same canonical encoding,
        // including current-format migration, history, behavior and final newline.
        let bytes = self
            .session
            .snapshot_bytes()
            .map_err(|failure| failure.to_string())?;
        let document_hash = format!("{:x}", Sha256::digest(&bytes));
        let document_bytes = bytes.len();
        let destination_store = if different_root {
            Some(AssetStore::open(directory.root()).map_err(|failure| failure.to_string())?)
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
    pub fn publish(self) -> Result<Saved, String> {
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
            Some(store) => store.with_current_catalog(|| {
                // Never reenter asset mutation/open while this guard is held.
                save_session(
                    &mut session,
                    &path,
                    &directory,
                    grant.as_ref(),
                    fresh_destination,
                )
            }),
            None => save_session(
                &mut session,
                &path,
                &directory,
                grant.as_ref(),
                fresh_destination,
            ),
        }
        .map_err(|failure| failure.to_string())?;
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
        assert!(request(source.clone(), None).prepare().is_err());
        assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
        std::fs::remove_file(&source).unwrap();
        assert!(request(source.clone(), None).prepare().is_err());
        std::fs::write(&source, &source_bytes).unwrap();
        let denied = destination.join("outside-grant.json");
        assert!(request(
            denied.clone(),
            Some(DirectoryGrant::new(&original).unwrap())
        )
        .prepare()
        .is_err());
        assert!(!denied.exists());
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
        assert!(prepared.publish().is_err());
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
        assert!(prepared.publish().is_err());
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
        assert!(rejected.unwrap_err().contains("undo/redo history"));
        assert_eq!(history.snapshot_bytes().unwrap(), history_bytes);
        assert!(!destination.join("history.json").exists());
    }
}
