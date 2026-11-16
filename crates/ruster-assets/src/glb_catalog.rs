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
        self.mutate_model(expected, id, Some(source))
    }

    pub fn refresh_glb_at(&mut self, expected: u64, id: &str) -> Result<ModelInfo, AssetError> {
        self.mutate_model(expected, id, None)
    }

    fn mutate_model(
        &mut self,
        expected: u64,
        id: &str,
        requested_source: Option<&str>,
    ) -> Result<ModelInfo, AssetError> {
        if expected != self.revision() {
            return Err(error(format!("asset revision conflict: expected {expected}, current {}; inspect and retry explicitly", self.revision())));
        }
        validate_id(id)?;
        let prior = self.registry.models.get(id);
        let source = if let Some(source) = requested_source {
            if self.registry.has_asset_id(id)
                || self
                    .registry
                    .models
                    .values()
                    .any(|model| model.source == source)
            {
                return Err(error("GLB model ID or source already registered; use the existing model ID to Refresh"));
            }
            source
        } else {
            &prior
                .ok_or_else(|| error(format!("unknown GLB model ID {id:?}")))?
                .source
        };
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
        let prepared = changed
            .then(|| mutations::encode_registry(next))
            .transpose()?;
        let _lock = self.guard(expected)?;
        if digest(&read_candidate(&self.root, source)?) != decoded.info.hash {
            return Err(error(
                "GLB source changed during preparation; inspect and retry explicitly",
            ));
        }
        if let Some(prepared) = prepared {
            let directory = accepted_directory(&self.root)?;
            if !directory.exists() {
                fs::create_dir(&directory)?;
            }
            let generation = generation_path(&directory, &decoded.info.hash);
            let retain = read_bounded(&generation, MAX_GLB_SOURCE_BYTES)
                .is_ok_and(|current| digest(&current) == decoded.info.hash);
            if !retain {
                atomic_write(&directory, &generation, &bytes)?;
                File::open(&directory)?.sync_all()?;
                File::open(&self.root)?.sync_all()?;
            }
            atomic_write(&self.root, &self.root.join(REGISTRY), &prepared.bytes)?;
            let old_hash = prior.map(|prior| prior.hash.clone());
            self.install_registry(prepared);
            if let Some(old_hash) = old_hash {
                if !self
                    .registry
                    .models
                    .values()
                    .any(|model| model.hash == old_hash)
                {
                    // Old generations are only redundant after the new catalog is durable.
                    if File::open(&self.root)
                        .and_then(|dir| dir.sync_all())
                        .is_ok()
                    {
                        let _ = fs::remove_file(generation_path(&directory, &old_hash));
                    }
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
