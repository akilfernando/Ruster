//! One accepted GLB source generation is authoritative for all of its subresources.
use super::{glb::DecodedModel, *};

const ACCEPTED_DIRECTORY: &str = ".ruster-models";

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|ch| ch.is_ascii_hexdigit() && !ch.is_ascii_uppercase())
}

fn validate_glb_path(source: &str) -> Result<(), AssetError> {
    validate_project_relative(source)?;
    if Path::new(source).extension().and_then(|ext| ext.to_str()) != Some("glb") {
        return Err(error(
            "static model import requires a project-relative .glb file",
        ));
    }
    Ok(())
}

fn accepted_directory(root: &Path) -> Result<PathBuf, AssetError> {
    let directory = root.join(ACCEPTED_DIRECTORY);
    reject_symlink(&directory)?;
    Ok(directory)
}

fn generation_path(directory: &Path, hash: &str) -> PathBuf {
    debug_assert!(valid_hash(hash));
    directory.join(format!("{hash}.glb"))
}

fn read_candidate(root: &Path, source: &str) -> Result<Vec<u8>, AssetError> {
    validate_glb_path(source)?;
    read_bounded(&confined_path(root, source, false)?, MAX_GLB_SOURCE_BYTES)
}

pub(super) fn read_accepted(root: &Path, info: &ModelInfo) -> Result<DecodedModel, AssetError> {
    let directory = accepted_directory(root)?;
    let bytes = read_bounded(
        &generation_path(&directory, &info.hash),
        MAX_GLB_SOURCE_BYTES,
    )
    .map_err(|e| {
        error(format!(
            "model {:?} accepted GLB generation is unreadable: {e}",
            info.id
        ))
    })?;
    if digest(&bytes) != info.hash {
        return Err(error(format!(
            "model {:?} accepted GLB generation hash mismatch",
            info.id
        )));
    }
    let decoded = glb::decode(&info.id, &info.source, &bytes)?;
    if &decoded.info != info {
        return Err(error(format!(
            "model {:?} accepted GLB structure does not match its catalog",
            info.id
        )));
    }
    Ok(decoded)
}

impl Registry {
    pub(super) fn has_asset_id(&self, id: &str) -> bool {
        self.assets.contains_key(id)
            || self.models.contains_key(id)
            || self.model_resource(id).is_some()
    }

    pub(super) fn asset_count(&self) -> usize {
        self.assets.len()
            + self.models.len()
            + self
                .models
                .values()
                .map(|model| model.resources().count())
                .sum::<usize>()
    }

    pub(super) fn model_resource(&self, id: &str) -> Option<(&ModelInfo, AssetKind)> {
        self.models.values().find_map(|model| {
            model
                .resources()
                .find(|(resource, _)| *resource == id)
                .map(|(_, kind)| (model, kind))
        })
    }

    pub(super) fn validate_models(&self) -> Result<(), AssetError> {
        let mut ids: BTreeSet<_> = self.assets.keys().map(String::as_str).collect();
        let mut sources = BTreeSet::new();
        for (id, model) in &self.models {
            validate_id(id)?;
            validate_glb_path(&model.source)?;
            if !valid_hash(&model.hash)
                || &model.id != id
                || !ids.insert(id)
                || !sources.insert(&model.source)
                || model.nodes.is_empty()
                || model.nodes.len() > MAX_ASSETS
            {
                return Err(error("invalid or duplicate static model catalog identity"));
            }
            for (resource, _) in model.resources() {
                validate_id(resource)?;
                if !ids.insert(resource) {
                    return Err(error("duplicate static model resource ID"));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(super) struct PreparedModel {
    bytes: Vec<u8>,
    decoded: DecodedModel,
    registry: Option<mutations::PreparedRegistry>,
}

impl AssetStore {
    /// Last accepted definitions and stable resource IDs in named-path order.
    pub fn model(&self, id: &str) -> Option<&ModelInfo> {
        self.registry.models.get(id)
    }

    pub fn models(&self) -> impl ExactSizeIterator<Item = &ModelInfo> {
        self.registry.models.values()
    }

    pub fn import_glb_at(
        &mut self,
        expected: u64,
        id: &str,
        source: &str,
    ) -> Result<ModelInfo, AssetError> {
        self.apply_model_sync(
            expected,
            AssetMutation::ImportGlb {
                id: id.into(),
                source: source.into(),
            },
        )
    }

    pub fn refresh_glb_at(&mut self, expected: u64, id: &str) -> Result<ModelInfo, AssetError> {
        self.apply_model_sync(expected, AssetMutation::RefreshGlb { id: id.into() })
    }

    fn apply_model_sync(
        &mut self,
        expected: u64,
        mutation: AssetMutation,
    ) -> Result<ModelInfo, AssetError> {
        match self.apply_sync(expected, mutation)? {
            AssetOutcome::Model(info) => Ok(*info),
            AssetOutcome::Asset(_) => Err(error("expected a static GLB model outcome")),
        }
    }

    pub(super) fn validate_model_request(
        &self,
        id: &str,
        source: Option<&str>,
    ) -> Result<(), AssetError> {
        validate_id(id)?;
        if let Some(source) = source {
            validate_glb_path(source)?;
            if self.registry.has_asset_id(id)
                || self
                    .registry
                    .models
                    .values()
                    .any(|model| model.source == source)
            {
                return Err(error("GLB model ID or source already registered; use the existing model ID to Refresh"));
            }
            if self.registry.asset_count() >= MAX_ASSETS {
                return Err(error("asset catalog limit (64) reached"));
            }
        } else if !self.registry.models.contains_key(id) {
            return Err(error(format!("unknown GLB model ID {id:?}")));
        }
        Ok(())
    }

    pub(super) fn prepare_model(
        &self,
        id: &str,
        source: &str,
    ) -> Result<PreparedModel, AssetError> {
        let prior = self.registry.models.get(id);
        let bytes = read_candidate(&self.root, source)?;
        let decoded = glb::decode(id, source, &bytes)?;
        if let Some(prior) = prior {
            if decoded.info.nodes != prior.nodes || decoded.info.materials != prior.materials {
                return Err(error("GLB structure changed (named paths, local transforms, material assignments or settings); import as a fresh model"));
            }
        }
        let mut next = self.registry.clone();
        next.models.insert(id.into(), decoded.info.clone());
        if next.asset_count() > MAX_ASSETS {
            return Err(error(
                "asset catalog exceeds 64 assets including model subresources",
            ));
        }
        next.validate_models()?;
        self.check_model_budget(&decoded)?;
        let changed = prior.is_none_or(|prior| prior.hash != decoded.info.hash);
        let registry = changed
            .then(|| mutations::encode_registry(next))
            .transpose()?;
        Ok(PreparedModel {
            bytes,
            decoded,
            registry,
        })
    }

    /// Publication runs under the existing exact-catalog lock.
    pub(super) fn publish_model(
        &mut self,
        candidate: PreparedModel,
        catalog_published: &mut bool,
    ) -> Result<ModelInfo, AssetError> {
        self.publish_model_with_registry(candidate, catalog_published, atomic_write)
    }

    fn publish_model_with_registry(
        &mut self,
        candidate: PreparedModel,
        catalog_published: &mut bool,
        publish_registry: impl FnOnce(&Path, &Path, &[u8]) -> Result<(), AssetError>,
    ) -> Result<ModelInfo, AssetError> {
        let PreparedModel {
            bytes,
            decoded,
            registry,
        } = candidate;
        let info = &decoded.info;
        if digest(&read_candidate(&self.root, &info.source)?) != info.hash {
            return Err(error(
                "GLB source changed during preparation; inspect and retry explicitly",
            ));
        }
        let directory = accepted_directory(&self.root)?;
        if !directory.exists() {
            fs::create_dir(&directory)?;
        }
        let generation = generation_path(&directory, &info.hash);
        let retained = read_bounded(&generation, MAX_GLB_SOURCE_BYTES)
            .is_ok_and(|current| digest(&current) == info.hash);
        if !retained {
            atomic_write(&directory, &generation, &bytes)?;
            File::open(&directory)?.sync_all()?;
            File::open(&self.root)?.sync_all()?;
        }
        if let Some(registry) = registry {
            if let Err(failure) =
                publish_registry(&self.root, &self.root.join(REGISTRY), &registry.bytes)
            {
                if !retained
                    && !self
                        .registry
                        .models
                        .values()
                        .any(|model| model.hash == info.hash)
                {
                    let _ = fs::remove_file(generation);
                }
                return Err(failure);
            }
            let old_hash = self
                .registry
                .models
                .get(&info.id)
                .map(|model| model.hash.clone());
            self.install_registry(registry);
            *catalog_published = true;
            if let Some(old_hash) = old_hash {
                let durable = File::open(&self.root)
                    .and_then(|dir| dir.sync_all())
                    .is_ok();
                if durable
                    && !self
                        .registry
                        .models
                        .values()
                        .any(|model| model.hash == old_hash)
                {
                    let _ = fs::remove_file(generation_path(&directory, &old_hash));
                }
            }
        }
        Ok(self.install_model(decoded))
    }

    pub(super) fn restore_models(&mut self) -> Result<(), AssetError> {
        for info in self.registry.models.clone().values() {
            let decoded = read_accepted(&self.root, info)?;
            self.check_model_budget(&decoded)?;
            self.install_model(decoded);
        }
        Ok(())
    }

    fn install_model(&mut self, model: DecodedModel) -> ModelInfo {
        for (id, resource) in model.resources {
            self.install_payload(&id, resource.payload);
        }
        model.info
    }

    fn check_model_budget(&self, model: &DecodedModel) -> Result<(), AssetError> {
        let mut mesh_bytes = self
            .meshes
            .iter()
            .filter(|(id, _)| !model.resources.contains_key(*id))
            .map(|(_, mesh)| mesh.payload_bytes())
            .sum::<usize>();
        let mut texture_bytes = self
            .textures
            .iter()
            .filter(|(id, _)| !model.resources.contains_key(*id))
            .map(|(_, texture)| texture.payload_bytes())
            .sum::<usize>();
        for resource in model.resources.values() {
            match &resource.payload {
                Payload::Mesh(mesh) => mesh_bytes += mesh.payload_bytes(),
                Payload::Texture(texture) => texture_bytes += texture.payload_bytes(),
                Payload::Sound(_) => return Err(error("GLB cannot own WAV audio")),
            }
        }
        if mesh_bytes > MAX_RESOLVED_BYTES || texture_bytes > MAX_TEXTURE_RESOLVED_BYTES {
            return Err(error(
                "GLB resources exceed the project's aggregate mesh or texture payload limits",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires pinned Blender GLB A/B fixtures; injects failure after real generation staging"]
    fn registry_replacement_failure_after_generation_staging_preserves_active_model() {
        let fixture = PathBuf::from(std::env::var_os("RUSTER_TEST_GLB_FIXTURES").unwrap());
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("beacon.glb");
        fs::copy(fixture.join("beacon-A.glb"), &source).unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        let first = store.import_glb_at(0, "beacon", "beacon.glb").unwrap();
        let catalog = fs::read(root.path().join(REGISTRY)).unwrap();
        let old_meshes = store.mesh_snapshots();
        let old_textures = store.texture_snapshots();

        fs::copy(fixture.join("beacon-B-reordered.glb"), &source).unwrap();
        let candidate = store.prepare_model("beacon", "beacon.glb").unwrap();
        let candidate_hash = candidate.decoded.info.hash.clone();
        let generation =
            generation_path(&accepted_directory(root.path()).unwrap(), &candidate_hash);
        assert!(!generation.exists());
        let mut published = false;
        let lock = store.guard(1).unwrap();
        let failure = store
            .publish_model_with_registry(candidate, &mut published, |_, path, _| {
                assert_eq!(fs::read(&generation).unwrap(), fs::read(&source).unwrap());
                assert_eq!(fs::read(path).unwrap(), catalog);
                Err(error("injected registry replacement failure"))
            })
            .unwrap_err();
        drop(lock);
        assert_eq!(failure.to_string(), "injected registry replacement failure");
        assert!(!published);
        assert!(!generation.exists());
        assert_eq!(fs::read(root.path().join(REGISTRY)).unwrap(), catalog);
        assert_eq!(store.revision(), 1);
        assert_eq!(store.model("beacon"), Some(&first));
        assert!(!root.path().join(".ruster-cache").exists());
        let reopened = AssetStore::open(root.path()).unwrap();
        assert_eq!(reopened.model("beacon"), Some(&first));
        assert_eq!(reopened.mesh_snapshots(), old_meshes);
        assert_eq!(reopened.texture_snapshots(), old_textures);

        let second = store.refresh_glb_at(1, "beacon").unwrap();
        assert_eq!(store.revision(), 2);
        assert_eq!(second.hash, candidate_hash);
        assert_eq!(second.nodes, first.nodes);
        assert_eq!(second.materials, first.materials);
        assert_eq!(
            AssetStore::open(root.path()).unwrap().model("beacon"),
            Some(&second)
        );
    }
}
