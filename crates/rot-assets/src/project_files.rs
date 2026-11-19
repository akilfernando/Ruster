//! Project sources and their adjacent metadata are the inputs to the catalog.
use super::{
    atomic_write, confined_path, digest, error, read_bounded, validate_id, validate_registry,
    AssetError, AssetKind, AssetMutation, AssetStatus, AssetStore, MeshNormalPolicy, TextureRole,
    REGISTRY,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
        let diagnostics = store.update_project_files(&BTreeSet::new(), || true);
        Ok((store, diagnostics))
    }

    /// Reconcile source files on the asset worker. A changed directory covers
    /// all files beneath it. The empty set is the initial discovery pass.
    pub fn update_project_files(
        &mut self,
        changed: &BTreeSet<PathBuf>,
        active: impl Fn() -> bool,
    ) -> Vec<String> {
        let changed = |source: &str| {
            let source = Path::new(source);
            changed.iter().any(|path| source.starts_with(path))
        };
        let mut diagnostics = Vec::new();
        let mut pending = vec![String::from("Assets")];
        let mut sources = Vec::new();
        while let Some(relative) = pending.pop() {
            if !active() {
                return diagnostics;
            }
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
            if !active() {
                return diagnostics;
            }
            let refresh = changed(&source);
            if let Err(err) =
                self.reconcile_source(&source, importer, metadata, &sidecar_ids, refresh, &active)
            {
                diagnostics.push(format!("{source}: {err}"));
            }
        }
        let missing: Vec<_> = self
            .registry
            .assets
            .iter()
            .filter(|(_, record)| {
                record.source.starts_with("Assets/")
                    && changed(&record.source)
                    && !self.root.join(&record.source).is_file()
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in missing {
            if !active() {
                break;
            }
            if let Err(err) =
                self.publish_project_change(AssetMutation::Refresh { id: id.clone() }, &active)
            {
                diagnostics.push(format!("{}: {err}", self.registry.assets[&id].source));
            }
        }
        for model in self.registry.models.values() {
            if model.source.starts_with("Assets/") && !self.root.join(&model.source).is_file() {
                diagnostics.push(format!("{}: Source file is missing.", model.source));
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
        refresh: bool,
        active: &impl Fn() -> bool,
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
            if self.awaiting_moved_sidecar(source, importer)? {
                return Err(error("Move the asset's sidecar next to the source."));
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
            self.with_current_catalog(|| {
                if !active() {
                    return Err(error("Project closed during import."));
                }
                write_metadata(&sidecar, &importer.metadata(id))
            })?;
            load_metadata(&sidecar)?
        };
        let mutation = match importer {
            Importer::Asset(kind) => {
                let (policy, role) = settings(kind, &metadata)?;
                if self.registry.models.contains_key(&metadata.id) {
                    return Err(existing_asset_id(&metadata.id));
                }
                if let Some(record) = self.registry.assets.get(&metadata.id) {
                    if record.kind != kind {
                        return Err(existing_asset_id(&metadata.id));
                    }
                    if record.mesh_normal_policy != policy || record.texture_role != role {
                        return Err(error(
                            "Sidecar import settings differ from the accepted catalog.",
                        ));
                    }
                    let moved = record.source != source;
                    if moved {
                        self.relocate_source(&metadata.id, source, importer, active)?;
                    }
                    let record = &self.registry.assets[&metadata.id];
                    let ready = self.infos[&metadata.id].status == AssetStatus::Ready;
                    if ready
                        && (!(refresh || moved)
                            || record.hash.as_ref().is_some_and(|hash| {
                                super::read_source_bytes(&self.root, source, kind)
                                    .is_ok_and(|bytes| digest(&bytes) == *hash)
                            }))
                    {
                        return Ok(());
                    }
                    AssetMutation::Refresh { id: metadata.id }
                } else {
                    match kind {
                        AssetKind::Mesh => AssetMutation::ImportMesh {
                            id: metadata.id,
                            source: source.into(),
                            normal_policy: policy.expect("mesh policy"),
                        },
                        AssetKind::Texture => AssetMutation::ImportTexture {
                            id: metadata.id,
                            source: source.into(),
                            role: role.expect("texture role"),
                        },
                        AssetKind::Sound => AssetMutation::Import {
                            id: metadata.id,
                            source: source.into(),
                        },
                    }
                }
            }
            Importer::Model => {
                if metadata.mesh_normal_policy.is_some() || metadata.texture_role.is_some() {
                    return Err(error("OBJ and PNG settings do not apply to GLB."));
                }
                if let Some(model) = self.registry.models.get(&metadata.id) {
                    if model.source != source {
                        self.relocate_source(&metadata.id, source, importer, active)?;
                    }
                    let path = confined_path(&self.root, source, false)?;
                    if digest(&read_bounded(&path, super::MAX_GLB_SOURCE_BYTES)?)
                        == self.registry.models[&metadata.id].hash
                    {
                        return Ok(());
                    }
                    AssetMutation::RefreshGlb { id: metadata.id }
                } else {
                    AssetMutation::ImportGlb {
                        id: metadata.id,
                        source: source.into(),
                    }
                }
            }
        };
        self.publish_project_change(mutation, active)
    }

    fn awaiting_moved_sidecar(&self, source: &str, importer: Importer) -> Result<bool, AssetError> {
        let orphaned_sidecar = |id: &str, previous: &str| -> Result<bool, AssetError> {
            if !previous.starts_with("Assets/") {
                return Ok(false);
            }
            let old = self.root.join(previous);
            Ok(is_missing(&old)?
                && existing_metadata(&sidecar_path(&old))
                    .ok()
                    .flatten()
                    .is_some_and(|metadata| metadata.id == id))
        };
        let mut orphaned = Vec::new();
        match importer {
            Importer::Asset(kind) => {
                for (id, record) in &self.registry.assets {
                    if record.kind == kind && orphaned_sidecar(id, &record.source)? {
                        orphaned.extend(record.hash.as_deref());
                    }
                }
            }
            Importer::Model => {
                for (id, model) in &self.registry.models {
                    if orphaned_sidecar(id, &model.source)? {
                        orphaned.push(model.hash.as_str());
                    }
                }
            }
        }
        if orphaned.is_empty() {
            return Ok(false);
        }
        let bytes = match importer {
            Importer::Asset(kind) => super::read_source_bytes(&self.root, source, kind)?,
            Importer::Model => {
                let path = confined_path(&self.root, source, false)?;
                read_bounded(&path, super::MAX_GLB_SOURCE_BYTES)?
            }
        };
        Ok(orphaned.contains(&digest(&bytes).as_str()))
    }

    fn relocate_source(
        &mut self,
        id: &str,
        destination: &str,
        importer: Importer,
        active: &impl Fn() -> bool,
    ) -> Result<(), AssetError> {
        let _lock = self.guard(self.revision())?;
        let previous = match importer {
            Importer::Asset(_) => &self.registry.assets[id].source,
            Importer::Model => &self.registry.models[id].source,
        };
        if !previous.starts_with("Assets/") {
            return Err(existing_asset_id(id));
        }
        let old = self.root.join(previous);
        if !is_missing(&old)? || !is_missing(&sidecar_path(&old))? {
            return Err(existing_asset_id(id));
        }
        let new = confined_path(&self.root, destination, false)?;
        if load_metadata(&sidecar_path(&new))?.id != id || !active() {
            return Err(error("The asset or project changed during the move."));
        }
        let mut registry = self.registry.clone();
        match importer {
            Importer::Asset(_) => {
                registry.assets.get_mut(id).expect("existing asset").source = destination.into();
            }
            Importer::Model => {
                registry.models.get_mut(id).expect("existing model").source = destination.into();
            }
        }
        validate_registry(&registry)?;
        let prepared = super::mutations::encode_registry(registry)?;
        if !active() {
            return Err(error("Project closed during the move."));
        }
        atomic_write(&self.root, &self.root.join(REGISTRY), &prepared.bytes)?;
        self.install_registry(prepared);
        if let Some(info) = self.infos.get_mut(id) {
            Arc::make_mut(info).source = destination.into();
        }
        Ok(())
    }

    fn publish_project_change(
        &mut self,
        mutation: AssetMutation,
        active: &impl Fn() -> bool,
    ) -> Result<(), AssetError> {
        let prepared = self.request_at(self.revision(), mutation)?.prepare();
        if !active() {
            return Err(error("Project closed during import."));
        }
        let completion = prepared.publish_if(active);
        self.adopt(completion)
            .map_err(|_| error("Asset state changed during import."))?
            .result
            .map(|_| ())
    }

    /// Read-only editor consumers keep accepted Arc payloads while the project
    /// worker owns mutations. The snapshot itself must not publish changes.
    pub fn project_snapshot(&self) -> Self {
        self.shallow_snapshot()
    }
}

fn existing_asset_id(id: &str) -> AssetError {
    error(format!(
        "Asset ID {id} already belongs to another source. Move both the file and its sidecar."
    ))
}

fn is_missing(path: &Path) -> Result<bool, AssetError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
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
    fn move_requires_both_files_and_preserves_the_accepted_mesh_identity() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("Assets");
        fs::create_dir(&folder).unwrap();
        let source = folder.join("Before.obj");
        let destination = folder.join("After.obj");
        fs::write(&source, OBJ).unwrap();
        let (mut store, _) = AssetStore::open_project(root.path()).unwrap();
        let record = store.infos().next().unwrap().clone();
        let snapshot = store.mesh_snapshots().remove(&record.id).unwrap();
        let paths = [PathBuf::from("Assets")].into_iter().collect();
        fs::rename(&source, &destination).unwrap();
        let pending = store.update_project_files(&paths, || true);
        assert!(pending
            .iter()
            .any(|message| message.contains("Move the asset's sidecar")));
        assert!(is_missing(&sidecar_path(&destination)).unwrap());
        assert_eq!(store.infos().len(), 1);
        fs::copy(sidecar_path(&source), sidecar_path(&destination)).unwrap();
        let collision = store.update_project_files(&paths, || true);
        assert!(
            collision
                .iter()
                .any(|message| message.contains("already belongs")),
            "{collision:?}"
        );
        assert_eq!(store.asset_info(&record.id).unwrap().source, record.source);

        fs::remove_file(sidecar_path(&source)).unwrap();
        let diagnostics = store.update_project_files(&paths, || true);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            store.asset_info(&record.id).unwrap().source,
            "Assets/After.obj"
        );
        assert!(Arc::ptr_eq(&snapshot, &store.mesh_snapshots()[&record.id]));
        assert_eq!(
            AssetStore::open(root.path())
                .unwrap()
                .asset_info(&record.id)
                .unwrap()
                .source,
            "Assets/After.obj"
        );
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
