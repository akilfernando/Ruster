//! Project-local authoring files and the sidecar identity used by scene links.
use super::*;
use std::sync::Arc;

pub(super) const MAX_PREFAB_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct PrefabFile {
    pub id: String,
    pub source: String,
    pub hash: String,
    pub bytes: Arc<[u8]>,
}

impl PrefabFile {
    pub fn from_bytes(id: String, source: String, bytes: Arc<[u8]>) -> Self {
        Self {
            id,
            source,
            hash: digest(&bytes),
            bytes,
        }
    }
}

impl PartialEq for PrefabFile {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.source == other.source && self.hash == other.hash
    }
}
impl Eq for PrefabFile {}

impl AssetStore {
    pub fn read_project_prefabs(&self) -> (Vec<PrefabFile>, Vec<ProjectFileIssue>) {
        let paths = match prefab_paths(&self.root) {
            Ok(paths) => paths,
            Err(failure) => {
                return (
                    Vec::new(),
                    vec![ProjectFileIssue::invalid("Assets", failure)],
                )
            }
        };
        let mut issues = Vec::new();
        let mut files = Vec::new();
        let mut counts = BTreeMap::new();
        for source in paths {
            let result = (|| -> Result<PrefabFile, AssetError> {
                let path = confined_path(&self.root, &source, false)?;
                let sidecar = sidecar_path(&path);
                let id = match existing_metadata(&sidecar)? {
                    Some(entries) => prefab_metadata(&entries)?.id.clone(),
                    None => {
                        let _lock = super::super::lock(&self.root)?;
                        if let Some(entries) = existing_metadata(&sidecar)? {
                            prefab_metadata(&entries)?.id.clone()
                        } else {
                            let stamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            let id = format!(
                                "prefab-{}",
                                digest(format!("{stamp}/{source}").as_bytes())
                            );
                            write_metadata(&sidecar, None, &[metadata(id.clone())])?;
                            id
                        }
                    }
                };
                *counts.entry(id.clone()).or_insert(0) += 1;
                let bytes = read_bounded(&path, MAX_PREFAB_BYTES)?;
                Ok(PrefabFile::from_bytes(id, source.clone(), bytes.into()))
            })();
            match result {
                Ok(file) => files.push(file),
                Err(failure) => issues.push(ProjectFileIssue::invalid(source, failure)),
            }
        }
        files.retain(|file| {
            if counts[&file.id] != 1 || self.registry.has_asset_id(&file.id) {
                issues.push(ProjectFileIssue::invalid(
                    &file.source,
                    "Prefab asset ID is already in use.",
                ));
                false
            } else {
                true
            }
        });
        (files, issues)
    }
}

fn prefab_paths(root: &Path) -> Result<Vec<String>, AssetError> {
    let mut dirs = vec![PathBuf::from("Assets")];
    let mut paths = Vec::new();
    while let Some(dir) = dirs.pop() {
        let entries = match fs::symlink_metadata(root.join(&dir)) {
            Ok(entry) if entry.is_dir() => fs::read_dir(root.join(&dir))?,
            Ok(_) => continue,
            Err(failure) if failure.kind() == ErrorKind::NotFound => continue,
            Err(failure) => return Err(failure.into()),
        };
        for entry in entries {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str().filter(|name| !name.starts_with('.')) else {
                continue;
            };
            let kind = entry.file_type()?;
            let path = dir.join(name);
            if kind.is_dir() {
                dirs.push(path);
            } else if kind.is_file() && name.ends_with(".prefab") {
                if let Some(path) = path.to_str() {
                    paths.push(path.to_owned());
                }
            }
        }
    }
    paths.sort_unstable();
    Ok(paths)
}

fn metadata(id: String) -> Metadata {
    Metadata {
        id,
        mesh_normal_policy: None,
        texture_role: None,
    }
}

fn prefab_metadata(entries: &[Metadata]) -> Result<&Metadata, AssetError> {
    match entries {
        [entry] if entry.mesh_normal_policy.is_none() && entry.texture_role.is_none() => Ok(entry),
        _ => Err(error(
            "A prefab requires one sidecar ID without mesh or texture settings.",
        )),
    }
}

/// Compare the source and its stable sidecar before publishing an authoring
/// command or its inverse. The caller has already validated the prefab data.
pub fn publish_project_prefab(
    project_root: &Path,
    source: &str,
    id: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Result<(), AssetError> {
    if !source.starts_with("Assets/")
        || !source.ends_with(".prefab")
        || source.split('/').any(|part| part.starts_with('.'))
    {
        return Err(error("Choose a .prefab file in Assets."));
    }
    validate_id(id)?;
    let root = fs::canonicalize(project_root)?;
    let _lock = super::super::lock(&root)?;
    let path = confined_path(&root, source, before.is_none())?;
    let sidecar = sidecar_path(&path);
    let parent = path.parent().expect("validated project directory");
    let entry = metadata(id.into());
    if let Some(expected) = before {
        if existing_metadata(&sidecar)?.as_deref() != Some(std::slice::from_ref(&entry))
            || read_bounded(&path, MAX_PREFAB_BYTES)? != expected
        {
            return Err(error(
                "The prefab source or its sidecar changed. Reopen or reload it first.",
            ));
        }
    } else if !is_missing(&path)? || !is_missing(&sidecar)? {
        return Err(error("A prefab or sidecar with this name already exists."));
    }
    match (before, after) {
        (None, Some(bytes)) => {
            let mut staged = tempfile::NamedTempFile::new_in(parent)?;
            write_bytes(&mut staged, bytes)?;
            write_metadata(&sidecar, None, std::slice::from_ref(&entry))?;
            if let Err(failure) = staged.persist_noclobber(&path) {
                let _ = fs::remove_file(&sidecar);
                return Err(failure.error.into());
            }
        }
        (Some(_), Some(bytes)) => {
            if bytes.len() > MAX_PREFAB_BYTES {
                return Err(error("Prefab source is too large."));
            }
            super::super::atomic_write(parent, &path, bytes)?;
        }
        (Some(_), None) => {
            let staged = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
            fs::rename(&sidecar, &staged)?;
            if let Err(failure) = fs::remove_file(&path) {
                fs::rename(&staged, &sidecar)?;
                return Err(failure.into());
            }
        }
        (None, None) => {}
    }
    Ok(())
}

fn write_bytes(staged: &mut tempfile::NamedTempFile, bytes: &[u8]) -> Result<(), AssetError> {
    if bytes.len() > MAX_PREFAB_BYTES {
        return Err(error("Prefab source is too large."));
    }
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    Ok(())
}
