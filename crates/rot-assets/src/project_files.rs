//! Project sources and their adjacent metadata are the inputs to the catalog.
use super::{
    confined_path, digest, error, read_bounded, validate_id, AssetError, AssetKind, AssetStatus,
    AssetStore, MeshNormalPolicy, TextureRole,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

const METADATA_EXTENSION: &str = ".rotmeta";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mesh_normal_policy: Option<MeshNormalPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    texture_role: Option<TextureRole>,
}

#[derive(Clone, Copy)]
enum Importer {
    Asset(AssetKind),
    Model,
}

impl Importer {
    fn from_path(path: &str) -> Option<Self> {
        if path.ends_with(".glb") {
            Some(Self::Model)
        } else {
            AssetKind::from_source(path).ok().map(Self::Asset)
        }
    }

    fn metadata(self, id: String) -> Metadata {
        Metadata {
            id,
            mesh_normal_policy: matches!(self, Self::Asset(AssetKind::Mesh))
                .then_some(MeshNormalPolicy::AuthoredWhenComplete),
            texture_role: matches!(self, Self::Asset(AssetKind::Texture))
                .then_some(TextureRole::BaseColor),
        }
    }
}

impl AssetStore {
    /// Open a v23 project and reconcile supported sources from its Assets tree.
    /// Callers must run this as file I/O. Diagnostics name only failed sources.
    pub fn open_project(root: impl AsRef<Path>) -> Result<(Self, Vec<String>), AssetError> {
        let mut store = Self::open(root)?;
        let diagnostics = store.reconcile_project_files();
        Ok((store, diagnostics))
    }

    fn reconcile_project_files(&mut self) -> Vec<String> {
        let mut diagnostics = Vec::new();
        let mut pending = vec![String::from("Assets")];
        let mut sources = Vec::new();
        while let Some(relative) = pending.pop() {
            let path = match confined_path(&self.root, &relative, true) {
                Ok(path) => path,
                Err(err) => {
                    diagnostics.push(format!("{relative}: {err}"));
                    continue;
                }
            };
            let entries = match fs::read_dir(path) {
                Ok(entries) => entries,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => {
                    diagnostics.push(format!("{relative}: {err}"));
                    continue;
                }
            };
            for entry in entries {
                let result = entry.and_then(|entry| Ok((entry.file_name(), entry.file_type()?)));
                let (name, kind) = match result {
                    Ok(result) => result,
                    Err(err) => {
                        diagnostics.push(format!("{relative}: {err}"));
                        continue;
                    }
                };
                let Some(name) = name.to_str() else {
                    continue;
                };
                if name.starts_with('.') {
                    continue;
                }
                let source = format!("{relative}/{name}");
                if kind.is_dir() {
                    pending.push(source);
                } else if kind.is_file() {
                    if let Some(importer) = Importer::from_path(&source) {
                        sources.push((source, importer));
                    }
                }
            }
        }
        sources.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        let mut sidecar_ids = BTreeMap::<String, usize>::new();
        let mut candidates = Vec::with_capacity(sources.len());
        for (source, importer) in sources {
            let metadata = confined_path(&self.root, &source, false)
                .and_then(|path| existing_metadata(&sidecar_path(&path)));
            match metadata {
                Ok(metadata) => {
                    if let Some(metadata) = &metadata {
                        *sidecar_ids.entry(metadata.id.clone()).or_default() += 1;
                    }
                    candidates.push((source, importer, metadata));
                }
                Err(err) => diagnostics.push(format!("{source}: {err}")),
            }
        }
        for (source, importer, metadata) in candidates {
            if let Err(err) = self.reconcile_source(&source, importer, metadata, &sidecar_ids) {
                diagnostics.push(format!("{source}: {err}"));
            }
        }
        diagnostics
    }

    fn reconcile_source(
        &mut self,
        source: &str,
        importer: Importer,
        metadata: Option<Metadata>,
        sidecar_ids: &BTreeMap<String, usize>,
    ) -> Result<(), AssetError> {
        let metadata = if let Some(metadata) = metadata {
            if sidecar_ids[&metadata.id] > 1 {
                return Err(error(format!(
                    "Duplicate sidecar asset ID {}.",
                    metadata.id
                )));
            }
            metadata
        } else {
            // An existing manual interpretation remains authoritative until
            // migrated, including multiple IDs for the same source.
            if self
                .registry
                .assets
                .values()
                .any(|asset| asset.source == source)
                || self
                    .registry
                    .models
                    .values()
                    .any(|model| model.source == source)
            {
                return Ok(());
            }
            let path = confined_path(&self.root, source, false)?;
            let sidecar = sidecar_path(&path);
            let hash = digest(source.as_bytes());
            let base = format!("asset-{}", &hash[..24]);
            let mut id = base.clone();
            for suffix in 1.. {
                if !self.registry.assets.contains_key(&id)
                    && !self.registry.models.contains_key(&id)
                    && !sidecar_ids.contains_key(&id)
                {
                    break;
                }
                id = format!("{base}-{suffix}");
            }
            self.with_current_catalog(|| write_metadata(&sidecar, &importer.metadata(id)))?;
            load_metadata(&sidecar)?
        };
        let revision = self.revision();
        match importer {
            Importer::Asset(kind) => {
                if self.registry.models.contains_key(&metadata.id)
                    || self
                        .registry
                        .assets
                        .get(&metadata.id)
                        .is_some_and(|record| record.source != source || record.kind != kind)
                {
                    return Err(error(format!(
                        "Asset ID {} already belongs to another source.",
                        metadata.id
                    )));
                }
                let (policy, role) = settings(kind, &metadata)?;
                if let Some(record) = self.registry.assets.get(&metadata.id) {
                    if record.mesh_normal_policy != policy || record.texture_role != role {
                        return Err(error(
                            "Sidecar import settings differ from the accepted catalog.",
                        ));
                    }
                    if self.infos[&metadata.id].status != AssetStatus::Ready {
                        self.refresh_at(revision, &metadata.id)?;
                    }
                } else {
                    match kind {
                        AssetKind::Mesh => {
                            self.import_mesh_at(
                                revision,
                                &metadata.id,
                                source,
                                policy.expect("mesh policy"),
                            )?;
                        }
                        AssetKind::Texture => {
                            self.import_texture_at(
                                revision,
                                &metadata.id,
                                source,
                                role.expect("texture role"),
                            )?;
                        }
                        AssetKind::Sound => {
                            self.import_at(revision, &metadata.id, source)?;
                        }
                    }
                }
            }
            Importer::Model => {
                if metadata.mesh_normal_policy.is_some() || metadata.texture_role.is_some() {
                    return Err(error("OBJ and PNG settings do not apply to GLB."));
                }
                match self.registry.models.get(&metadata.id) {
                    Some(model) if model.source == source => {}
                    Some(_) => {
                        return Err(error(format!(
                            "Asset ID {} already belongs to another source.",
                            metadata.id
                        )))
                    }
                    None => {
                        self.import_glb_at(revision, &metadata.id, source)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn settings(
    kind: AssetKind,
    metadata: &Metadata,
) -> Result<(Option<MeshNormalPolicy>, Option<TextureRole>), AssetError> {
    let mesh = kind == AssetKind::Mesh;
    let texture = kind == AssetKind::Texture;
    if metadata.mesh_normal_policy.is_some() && !mesh || metadata.texture_role.is_some() && !texture
    {
        return Err(error(
            "Sidecar import settings do not apply to this source type.",
        ));
    }
    Ok((
        mesh.then(|| {
            metadata
                .mesh_normal_policy
                .unwrap_or(MeshNormalPolicy::AuthoredWhenComplete)
        }),
        texture.then(|| metadata.texture_role.unwrap_or(TextureRole::BaseColor)),
    ))
}

fn sidecar_path(source: &Path) -> PathBuf {
    let mut path = source.as_os_str().to_os_string();
    path.push(METADATA_EXTENSION);
    path.into()
}

fn existing_metadata(path: &Path) -> Result<Option<Metadata>, AssetError> {
    match fs::symlink_metadata(path) {
        Ok(_) => load_metadata(path).map(Some),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn load_metadata(path: &Path) -> Result<Metadata, AssetError> {
    let metadata: Metadata = serde_json::from_slice(&read_bounded(path, 4096)?)?;
    validate_id(&metadata.id)?;
    Ok(metadata)
}

fn write_metadata(path: &Path, metadata: &Metadata) -> Result<(), AssetError> {
    let parent = path
        .parent()
        .ok_or_else(|| error("Missing asset directory."))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, metadata)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    match file.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(failure) if failure.error.kind() == ErrorKind::AlreadyExists => Ok(()),
        Err(failure) => Err(failure.error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    const OBJ: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";

    #[test]
    fn project_sources_rebuild_with_their_sidecar_identity() {
        let root = tempfile::tempdir().unwrap();
        let folders = root.path().join("Assets/Props");
        fs::create_dir_all(&folders).unwrap();
        let source = folders.join("Triangle.obj");
        fs::write(&source, OBJ).unwrap();
        let (store, diagnostics) = AssetStore::open_project(root.path()).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let record = store.infos().next().unwrap().clone();
        assert_eq!(record.source, "Assets/Props/Triangle.obj");
        let mesh = store.mesh(&record.id).unwrap().clone();
        let metadata = fs::read(sidecar_path(&source)).unwrap();
        drop(store);

        fs::remove_file(root.path().join(super::super::REGISTRY)).unwrap();
        fs::remove_dir_all(root.path().join(".rot-cache")).unwrap();
        let (rebuilt, diagnostics) = AssetStore::open_project(root.path()).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(rebuilt.infos().next(), Some(&record));
        assert_eq!(rebuilt.mesh(&record.id), Some(&mesh));
        assert_eq!(fs::read(sidecar_path(&source)).unwrap(), metadata);

        // Rebuilding a derived catalog must not silently choose between copies.
        drop(rebuilt);
        let duplicate = folders.join("Copy.obj");
        fs::write(&duplicate, OBJ).unwrap();
        fs::write(sidecar_path(&duplicate), metadata).unwrap();
        fs::remove_file(root.path().join(super::super::REGISTRY)).unwrap();
        let (ambiguous, diagnostics) = AssetStore::open_project(root.path()).unwrap();
        assert_eq!(ambiguous.infos().len(), 0);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics
            .iter()
            .all(|line| line.contains("Duplicate sidecar asset ID")));
    }

    #[test]
    fn file_discovery_uses_the_existing_image_sound_and_model_importers() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Assets");
        fs::create_dir(&folder).unwrap();
        let mut encoder = png::Encoder::new(File::create(folder.join("pixel.png")).unwrap(), 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[10, 20, 30, 255])
            .unwrap();
        let mut wave = hound::WavWriter::create(
            folder.join("tone.wav"),
            hound::WavSpec {
                channels: 1,
                sample_rate: 48_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        wave.write_sample(42_i16).unwrap();
        wave.finalize().unwrap();
        fs::write(
            folder.join("animated.glb"),
            crate::fixtures::animated_glb(Default::default()),
        )
        .unwrap();

        let (store, diagnostics) = AssetStore::open_project(root.path()).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let id = |name: &str| load_metadata(&sidecar_path(&folder.join(name))).unwrap().id;
        assert!(store.texture(&id("pixel.png")).is_some());
        assert!(store.sound(&id("tone.wav")).is_some());
        assert_eq!(store.model(&id("animated.glb")).unwrap().clips.len(), 1);
    }

    #[test]
    fn invalid_managed_sources_keep_accepted_content_and_do_not_block_valid_files() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Assets");
        fs::create_dir(&folder).unwrap();
        let bad = folder.join("bad.obj");
        fs::write(&bad, OBJ).unwrap();
        let (store, _) = AssetStore::open_project(root.path()).unwrap();
        let accepted = store.infos().next().unwrap().clone();
        let mesh = store.mesh(&accepted.id).unwrap().clone();
        drop(store);

        fs::write(&bad, "unreadable geometry").unwrap();
        fs::write(folder.join("good.obj"), OBJ).unwrap();
        let (store, diagnostics) = AssetStore::open_project(root.path()).unwrap();
        assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
        assert!(diagnostics[0].contains("Assets/bad.obj"));
        let invalid = store.infos().find(|asset| asset.id == accepted.id).unwrap();
        assert_eq!(invalid.status, AssetStatus::Invalid);
        assert_eq!(invalid.hash, accepted.hash);
        assert_eq!(store.mesh(&accepted.id), Some(&mesh));
        assert!(store
            .infos()
            .any(|asset| asset.status == AssetStatus::Ready));
    }
}
