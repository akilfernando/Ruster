//! Project-local `.material.json` files and the stable sidecar IDs used by scene links.
use super::*;
use std::sync::Arc;

pub(super) const MAX_MATERIAL_BYTES: usize = 64 * 1024;
pub const MATERIAL_EXTENSION: &str = ".material.json";

#[derive(Clone, Debug)]
pub struct MaterialFile {
    pub id: String,
    pub source: String,
    pub hash: String,
    pub bytes: Arc<[u8]>,
}

impl MaterialFile {
    pub fn from_bytes(id: String, source: String, bytes: Arc<[u8]>) -> Self {
        Self {
            id,
            source,
            hash: digest(&bytes),
            bytes,
        }
    }
}

impl PartialEq for MaterialFile {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id && self.source == other.source && self.hash == other.hash
    }
}
impl Eq for MaterialFile {}

impl AssetStore {
    pub fn read_project_materials(&self) -> (Vec<MaterialFile>, Vec<ProjectFileIssue>) {
        let paths = match material_paths(&self.root) {
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
            let result = (|| -> Result<MaterialFile, AssetError> {
                let path = confined_path(&self.root, &source, false)?;
                let sidecar = sidecar_path(&path);
                let id = match existing_metadata(&sidecar)? {
                    Some(entries) => material_metadata(&entries)?.id.clone(),
                    None => {
                        let _lock = super::super::lock(&self.root)?;
                        if let Some(entries) = existing_metadata(&sidecar)? {
                            material_metadata(&entries)?.id.clone()
                        } else {
                            let stamp = std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_nanos();
                            let id = format!(
                                "material-{}",
                                digest(format!("{stamp}/{source}").as_bytes())
                            );
                            write_metadata(&sidecar, None, &[metadata(id.clone())])?;
                            id
                        }
                    }
                };
                *counts.entry(id.clone()).or_insert(0) += 1;
                let bytes = read_bounded(&path, MAX_MATERIAL_BYTES)?;
                Ok(MaterialFile::from_bytes(id, source.clone(), bytes.into()))
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
                    "Material asset ID is already in use.",
                ));
                false
            } else {
                true
            }
        });
        (files, issues)
    }
}

fn material_paths(root: &Path) -> Result<Vec<String>, AssetError> {
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
            } else if kind.is_file() && name.ends_with(MATERIAL_EXTENSION) {
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

fn material_metadata(entries: &[Metadata]) -> Result<&Metadata, AssetError> {
    match entries {
        [entry] if entry.mesh_normal_policy.is_none() && entry.texture_role.is_none() => Ok(entry),
        _ => Err(error(
            "A material asset requires one sidecar ID without mesh or texture settings.",
        )),
    }
}

fn validate_material_source_path(source: &str) -> Result<(), AssetError> {
    if !source.starts_with("Assets/")
        || !source.ends_with(MATERIAL_EXTENSION)
        || source.split('/').any(|part| part.starts_with('.'))
    {
        return Err(error("Choose a .material.json file in Assets."));
    }
    Ok(())
}

/// Compare an accepted material source and sidecar without writing either file.
pub fn verify_project_material(
    project_root: &Path,
    source: &str,
    id: &str,
    expected: &[u8],
) -> Result<(), AssetError> {
    validate_material_source_path(source)?;
    validate_id(id)?;
    let root = fs::canonicalize(project_root)?;
    let _lock = super::super::lock(&root)?;
    let path = confined_path(&root, source, false)?;
    verify_material_bytes(&path, &sidecar_path(&path), &metadata(id.into()), expected)
}

fn verify_material_bytes(
    path: &Path,
    sidecar: &Path,
    entry: &Metadata,
    expected: &[u8],
) -> Result<(), AssetError> {
    if existing_metadata(sidecar)?.as_deref() != Some(std::slice::from_ref(entry))
        || read_bounded(path, MAX_MATERIAL_BYTES)? != expected
    {
        return Err(error(
            "The material source or its sidecar changed. Reopen or reload it first.",
        ));
    }
    Ok(())
}

/// Compare the source and its stable sidecar before publishing an authoring
/// command or its inverse. The caller has already validated the material data.
pub fn publish_project_material(
    project_root: &Path,
    source: &str,
    id: &str,
    before: Option<&[u8]>,
    after: Option<&[u8]>,
) -> Result<(), AssetError> {
    validate_material_source_path(source)?;
    validate_id(id)?;
    let root = fs::canonicalize(project_root)?;
    let _lock = super::super::lock(&root)?;
    let path = confined_path(&root, source, before.is_none())?;
    let sidecar = sidecar_path(&path);
    let parent = path.parent().expect("validated project directory");
    let entry = metadata(id.into());
    if let Some(expected) = before {
        verify_material_bytes(&path, &sidecar, &entry, expected)?;
    } else if !is_missing(&path)? || !is_missing(&sidecar)? {
        return Err(error(
            "A material file or sidecar with this name already exists.",
        ));
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
            if bytes.len() > MAX_MATERIAL_BYTES {
                return Err(error("Material source is too large."));
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
    if bytes.len() > MAX_MATERIAL_BYTES {
        return Err(error("Material source is too large."));
    }
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_project_materials_assigns_stable_sidecar_and_ignores_plain_json() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Assets/Materials")).unwrap();
        fs::write(root.join("Assets/notes.json"), br#"{"unrelated":true}"#).unwrap();
        let mat_path = root.join("Assets/Materials/Steel.material.json");
        fs::write(&mat_path, br#"{"format":"rot-material","format_version":1,"material":{"name":"Steel","tint":[0.6,0.65,0.7]}}"#).unwrap();

        let store = AssetStore::open(root).unwrap();
        let (first, issues) = store.read_project_materials();
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].source, "Assets/Materials/Steel.material.json");
        assert!(first[0].id.starts_with("material-"));
        assert!(root
            .join("Assets/Materials/Steel.material.json.rotmeta")
            .is_file());

        // Modifying the material file preserves the same sidecar ID and updates the hash.
        fs::write(&mat_path, br#"{"format":"rot-material","format_version":1,"material":{"name":"Steel","tint":[0.5,0.5,0.5]}}"#).unwrap();
        let (second, issues) = store.read_project_materials();
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].id, first[0].id);
        assert_ne!(second[0].hash, first[0].hash);
    }

    #[test]
    fn read_project_materials_rejects_duplicate_ids_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Assets")).unwrap();
        let body = br#"{"format":"rot-material","format_version":1,"material":{"tint":[1,1,1]}}"#;
        fs::write(root.join("Assets/a.material.json"), body).unwrap();
        fs::write(root.join("Assets/b.material.json"), body).unwrap();
        write_metadata(
            &root.join("Assets/a.material.json.rotmeta"),
            None,
            &[metadata("material-dup".into())],
        )
        .unwrap();
        write_metadata(
            &root.join("Assets/b.material.json.rotmeta"),
            None,
            &[metadata("material-dup".into())],
        )
        .unwrap();
        fs::write(
            root.join("Assets/huge.material.json"),
            vec![b' '; MAX_MATERIAL_BYTES + 1],
        )
        .unwrap();

        let store = AssetStore::open(root).unwrap();
        let (files, issues) = store.read_project_materials();
        assert!(files.is_empty());
        assert_eq!(issues.len(), 3);
    }

    #[test]
    fn publish_project_material_creates_updates_guards_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Assets")).unwrap();
        let _store = AssetStore::open(root).unwrap();
        let source = "Assets/Brick.material.json";
        let id = "material-brick";
        let v1 = br#"{"format":"rot-material","format_version":1,"material":{"name":"Brick","tint":[0.7,0.2,0.1]}}"#;
        let v2 = br#"{"format":"rot-material","format_version":1,"material":{"name":"Brick","tint":[0.8,0.2,0.1]}}"#;

        publish_project_material(root, source, id, None, Some(v1)).unwrap();
        verify_project_material(root, source, id, v1).unwrap();

        assert!(publish_project_material(root, source, id, Some(b"stale"), Some(v2)).is_err());
        assert_eq!(fs::read(root.join(source)).unwrap(), v1);

        publish_project_material(root, source, id, Some(v1), Some(v2)).unwrap();
        verify_project_material(root, source, id, v2).unwrap();

        publish_project_material(root, source, id, Some(v2), None).unwrap();
        assert!(!root.join(source).exists());
        assert!(!root.join("Assets/Brick.material.json.rotmeta").exists());
    }
}
