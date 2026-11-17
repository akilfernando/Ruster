//! One explicit process directory scope, revalidated at its use boundary.
//! Trusted same-user/cooperating filesystem checks, not hostile ancestor-race isolation.
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct DirectoryGrant {
    root: PathBuf,
    #[cfg(unix)]
    identity: (u64, u64),
}
impl DirectoryGrant {
    pub(crate) fn new(path: &Path) -> Result<Self, String> {
        let root = path.canonicalize().map_err(|error| error.to_string())?;
        let metadata = root.metadata().map_err(|error| error.to_string())?;
        if !metadata.is_dir() {
            return Err("Choose an existing directory".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            Ok(Self {
                root,
                identity: (metadata.dev(), metadata.ino()),
            })
        }
        #[cfg(not(unix))]
        Err("Directory grants currently require Unix filesystem identity".into())
    }
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
    pub(crate) fn validate(&self) -> Result<&Path, String> {
        let invalid = || {
            "Granted directory changed or became unavailable; choose it again explicitly"
                .to_string()
        };
        if self.root.canonicalize().map_err(|_| invalid())? != self.root {
            return Err(invalid());
        }
        let metadata = self.root.symlink_metadata().map_err(|_| invalid())?;
        if !metadata.is_dir() {
            return Err(invalid());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if (metadata.dev(), metadata.ino()) != self.identity {
                return Err(invalid());
            }
        }
        Ok(&self.root)
    }
}
