//! One accepted GLB source generation is authoritative for all of its subresources.
use super::{glb::DecodedModel, *};

const ACCEPTED_DIRECTORY: &str = ".rot-models";

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

pub(super) fn read_candidate(root: &Path, source: &str) -> Result<Vec<u8>, AssetError> {
    validate_glb_path(source)?;
    let bytes = read_bounded(&confined_path(root, source, false)?, MAX_GLB_SOURCE_BYTES)?;
    let embedded = glb::embed_external_images(&bytes, |uri| {
        validate_project_relative(uri)?;
        if uri.contains([':', '%', '?', '#'])
            || Path::new(uri).extension().and_then(|ext| ext.to_str()) != Some("png")
        {
            return Err(error("GLB external images require a relative PNG path"));
        }
        let path = source
            .rsplit_once('/')
            .map_or_else(|| uri.to_owned(), |(folder, _)| format!("{folder}/{uri}"));
        read_bounded(
            &confined_path(root, &path, false)?,
            MAX_TEXTURE_SOURCE_BYTES,
        )
    })?;
    Ok(embedded.unwrap_or(bytes))
}

fn old_omitted_roughness(info: &ModelInfo, material: &str) -> bool {
    info.warnings.iter().any(|warning| {
        warning
            .strip_prefix(material)
            .and_then(|warning| warning.strip_prefix(": "))
            .is_some_and(|details| {
                details
                    .split(", ")
                    .any(|part| part == "metallic-roughness map omitted")
            })
    })
}

fn same_material_structure(old: &ModelInfo, new: &ModelInfo) -> bool {
    old.materials.len() == new.materials.len()
        && old
            .materials
            .iter()
            .zip(&new.materials)
            .all(|((name, prior), (next_name, next))| {
                if name != next_name {
                    return false;
                }
                if prior == next {
                    return true;
                }
                if prior.roughness_texture.is_some()
                    || next.roughness_texture.is_none()
                    || !old_omitted_roughness(old, name)
                {
                    return false;
                }
                let mut upgraded = prior.clone();
                upgraded
                    .roughness_texture
                    .clone_from(&next.roughness_texture);
                if prior.base_color_texture.is_none() {
                    upgraded.linear = next.linear;
                    upgraded.repeat = next.repeat;
                }
                &upgraded == next
            })
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
    // Old accepted catalogs recorded exact scalar-only warnings. Recreate
    // their original interpretation until a Refresh publishes the new one.
    let omitted = info
        .materials
        .keys()
        .any(|name| old_omitted_roughness(info, name));
    let decoded = if omitted
        && info
            .materials
            .values()
            .all(|material| material.roughness_texture.is_none())
    {
        glb::decode_without_packed_roughness(&info.id, &info.source, &bytes)?
    } else {
        glb::decode(&info.id, &info.source, &bytes)?
    };
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
            || self.model_clip(id).is_some()
    }

    pub(super) fn model_clip(&self, id: &str) -> Option<(&ModelInfo, &str)> {
        self.models.values().find_map(|model| {
            model
                .clips
                .iter()
                .find(|(_, clip)| clip.id == id)
                .map(|(name, _)| (model, name.as_str()))
        })
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
            let source_nodes = model
                .nodes
                .keys()
                .filter(|path| !ModelInfo::is_primitive_path(path))
                .count();
            if !valid_hash(&model.hash)
                || &model.id != id
                || !ids.insert(id)
                || !sources.insert(&model.source)
                || model.warnings.len() > MAX_MODEL_ITEMS * 3 + 3
                || model.warnings.iter().any(|message| {
                    message.is_empty()
                        || message.len() > 512
                        || message.chars().any(char::is_control)
                })
                || source_nodes == 0
                || source_nodes > MAX_MODEL_ITEMS
                || model.nodes.len() - source_nodes > MAX_MODEL_ITEMS
            {
                return Err(error("invalid or duplicate static model catalog identity"));
            }
            if model.nodes.iter().any(|(path, node)| {
                node.light
                    .is_some_and(|light| ModelInfo::is_primitive_path(path) || !light.is_valid())
            }) {
                return Err(error("invalid static model light catalog entry"));
            }
            if model.nodes.iter().any(|(path, node)| {
                node.camera
                    .is_some_and(|camera| ModelInfo::is_primitive_path(path) || !camera.is_valid())
            }) {
                return Err(error("invalid static model camera catalog entry"));
            }
            for (resource, _) in model.resources() {
                validate_id(resource)?;
                if !ids.insert(resource) {
                    return Err(error("duplicate static model resource ID"));
                }
            }
            if model.clips.len() > glb::MAX_MODEL_CLIPS {
                return Err(error("static model exceeds 8 animation clips"));
            }
            for (name, clip) in &model.clips {
                validate_id(&clip.id)?;
                if clip.id != glb::resource_id(id, "clip", name)
                    || !ids.insert(&clip.id)
                    || clip.targets.is_empty()
                    || !(clip.duration > 0. && clip.duration.is_finite())
                    || clip.targets.windows(2).any(|pair| pair[0] >= pair[1])
                    || clip
                        .targets
                        .iter()
                        .any(|target| !model.nodes.contains_key(&target.node))
                {
                    return Err(error("invalid static model animation clip catalog entry"));
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

    /// Accepted `rot-clip` payload for one model clip resource ID.
    pub fn clip(&self, id: &str) -> Option<&Arc<Vec<u8>>> {
        self.clips.get(id)
    }

    /// Cheap shared snapshot of every accepted clip payload.
    pub fn clip_snapshots(&self) -> BTreeMap<String, Arc<Vec<u8>>> {
        self.clips.clone()
    }

    /// Model ID and clip name owning a clip resource ID.
    pub fn clip_owner(&self, id: &str) -> Option<(&ModelInfo, &str)> {
        self.registry.model_clip(id)
    }

    pub fn model_texture(&self, id: &str) -> Option<(&ModelInfo, TextureRole)> {
        let (model, kind) = self.registry.model_resource(id)?;
        if kind != AssetKind::Texture {
            return None;
        }
        model.materials.values().find_map(|material| {
            [TextureRole::BaseColor, TextureRole::Roughness]
                .into_iter()
                .find(|&role| material.texture(role) == Some(id))
                .map(|role| (model, role))
        })
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
            let same_nodes = decoded.info.nodes.len() == prior.nodes.len()
                && decoded.info.nodes.iter().zip(&prior.nodes).all(
                    |((path, next), (old_path, old))| {
                        path == old_path && next.same_placement_structure(old)
                    },
                );
            if !same_nodes || !same_material_structure(prior, &decoded.info) {
                return Err(error("GLB structure changed (named paths, local transforms, material assignments or settings); import as a fresh model"));
            }
            if !decoded.info.same_clip_structure(prior) {
                return Err(error("GLB animation structure changed (clip names or animated node properties); import as a fresh model. Refresh accepts changed key times and values only"));
            }
        }
        let mut next = self.registry.clone();
        next.models.insert(id.into(), decoded.info.clone());
        next.validate_models()?;
        self.check_model_budget(&decoded)?;
        let changed = prior != Some(&decoded.info);
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
    pub(super) fn publish_model_if(
        &mut self,
        candidate: PreparedModel,
        catalog_published: &mut bool,
        active: impl Fn() -> bool,
    ) -> Result<ModelInfo, AssetError> {
        self.publish_model_with_registry(
            candidate,
            catalog_published,
            |root, destination, bytes| {
                if !active() {
                    return Err(error("Project closed during model preparation."));
                }
                atomic_write(root, destination, bytes)
            },
        )
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
        for (id, bytes) in model.clips {
            self.clips.insert(id, Arc::new(bytes));
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
                Payload::Sound(_) | Payload::Script(_) => {
                    return Err(error("GLB cannot own external source assets"))
                }
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

    fn packed_fixture(
        pixels: &[u8; 12],
        edit: impl FnOnce(&mut serde_json::Value, &mut [u8]),
    ) -> Vec<u8> {
        use crate::fixtures::{animated_glb, pack_glb};
        use serde_json::json;
        let original = animated_glb(Default::default());
        let json_size = u32::from_le_bytes(original[12..16].try_into().unwrap()) as usize;
        let mut doc: serde_json::Value =
            serde_json::from_slice(&original[20..20 + json_size]).unwrap();
        let mut bin = original[28 + json_size..].to_vec();
        let uv_offset = bin.len();
        bin.extend(
            [0_f32, 0., 1., 0., 0., 1.]
                .into_iter()
                .flat_map(f32::to_le_bytes),
        );
        let image_offset = bin.len();
        let mut encoded = Vec::new();
        {
            let mut png = png::Encoder::new(&mut encoded, 2, 2);
            png.set_color(png::ColorType::Rgb);
            png.set_depth(png::BitDepth::Eight);
            png.write_header()
                .unwrap()
                .write_image_data(pixels)
                .unwrap();
        }
        doc["bufferViews"].as_array_mut().unwrap().extend([
            json!({"buffer":0,"byteOffset":uv_offset,"byteLength":24}),
            json!({"buffer":0,"byteOffset":image_offset,"byteLength":encoded.len()}),
        ]);
        doc["accessors"]
            .as_array_mut()
            .unwrap()
            .push(json!({"bufferView":5,"componentType":5126,"count":3,"type":"VEC2"}));
        doc["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_0"] = json!(5);
        doc["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"] = json!({"index":0});
        doc["materials"][0]["pbrMetallicRoughness"]["metallicRoughnessTexture"] =
            json!({"index":0});
        doc["textures"] = json!([{"source":0}]);
        doc["images"] = json!([{"bufferView":6,"mimeType":"image/png"}]);
        edit(&mut doc, &mut encoded);
        bin.extend(encoded);
        doc["buffers"][0]["byteLength"] = json!(bin.len());
        bin.resize(bin.len().next_multiple_of(4), 0);
        pack_glb(&doc, &bin)
    }

    #[test]
    fn packed_glb_roughness_upgrades_old_catalog_and_preserves_exact_green_in_exports() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("paint.glb");
        let pixels = [1, 20, 122, 2, 40, 123, 3, 60, 124, 4, 80, 125];
        let bytes = packed_fixture(&pixels, |_, _| {});
        fs::write(&source, &bytes).unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        let decoded = glb::decode_without_packed_roughness("paint", "paint.glb", &bytes).unwrap();
        let prior = decoded.info.clone();
        let mut registry = store.registry.clone();
        registry.models.insert("paint".into(), prior.clone());
        let registry = mutations::encode_registry(registry).unwrap();
        let mut published = false;
        store
            .publish_model_if(
                PreparedModel {
                    bytes,
                    decoded,
                    registry: Some(registry),
                },
                &mut published,
                || true,
            )
            .unwrap();
        assert!(published);
        let base_id = prior.materials["Paint"].base_color_texture.clone().unwrap();
        assert!(prior.materials["Paint"].roughness_texture.is_none());
        drop(store);

        let mut store = AssetStore::open(root.path()).unwrap();
        assert_eq!(store.model("paint"), Some(&prior));
        let accepted = store.refresh_glb_at(1, "paint").unwrap();
        assert_eq!(store.revision(), 2);
        assert_eq!(accepted.hash, prior.hash);
        assert_eq!(accepted.nodes, prior.nodes);
        assert_eq!(
            accepted.materials["Paint"].base_color_texture.as_ref(),
            Some(&base_id)
        );
        let roughness_id = accepted.materials["Paint"]
            .roughness_texture
            .clone()
            .unwrap();
        assert_ne!(roughness_id, base_id);
        let color = store.texture(&base_id).unwrap();
        assert_eq!(color.role, TextureRole::BaseColor);
        assert_eq!(&color.mips[0].bytes[..4], &[1, 20, 122, 255]);
        let roughness = store.texture(&roughness_id).unwrap().clone();
        assert_eq!(roughness.role, TextureRole::Roughness);
        assert_eq!(roughness.mips[0].bytes, [20, 40, 60, 80]);
        assert_eq!(roughness.mips[1].bytes, [50]);

        let export = store
            .export_snapshot(&BTreeSet::from([base_id.clone(), roughness_id.clone()]))
            .unwrap();
        assert_eq!(
            export.texture_role(&roughness_id),
            Some(TextureRole::Roughness)
        );
        assert_eq!(export.texture_role(&base_id), Some(TextureRole::BaseColor));
        let sources = export.read_sources().unwrap();
        assert_eq!(
            crate::decode_png_with_role(&sources[&roughness_id], TextureRole::Roughness).unwrap(),
            roughness
        );
        assert_eq!(export.read_sources().unwrap(), sources);
        let newer = [1, 80, 122, 2, 60, 123, 3, 40, 124, 4, 20, 125];
        fs::write(&source, packed_fixture(&newer, |_, _| {})).unwrap();
        let refreshed = store.refresh_glb_at(2, "paint").unwrap();
        assert_eq!(refreshed.materials, accepted.materials);
        assert_eq!(
            store.texture(&roughness_id).unwrap().mips[0].bytes,
            [80, 60, 40, 20]
        );

        fs::write(&source, packed_fixture(&pixels, |_, png| png[20] ^= 1)).unwrap();
        assert!(store.refresh_glb_at(3, "paint").is_err());
        assert_eq!(store.revision(), 3);
        assert_eq!(
            AssetStore::open(root.path()).unwrap().model("paint"),
            Some(&refreshed)
        );
    }

    #[test]
    fn packed_glb_roughness_honors_its_sampler_and_skips_unrepresentable_bindings() {
        use serde_json::json;
        let pixels = [1, 20, 122, 2, 40, 123, 3, 60, 124, 4, 80, 125];
        for (mode, expected) in [
            ("roughness-only", None),
            ("incompatible", Some("different sampler from Base Color")),
            ("uv1", Some("requires TEXCOORD_0")),
            ("transform", Some("UV transform is unsupported")),
        ] {
            let bytes = packed_fixture(&pixels, |doc, _| {
                doc["samplers"] =
                    json!([{"magFilter":9728,"minFilter":9984,"wrapS":33071,"wrapT":33071}]);
                doc["textures"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"source":0,"sampler":0}));
                let pbr = &mut doc["materials"][0]["pbrMetallicRoughness"];
                pbr["metallicRoughnessTexture"]["index"] = json!(1);
                match mode {
                    "roughness-only" => {
                        pbr.as_object_mut().unwrap().remove("baseColorTexture");
                    }
                    "uv1" => {
                        pbr["metallicRoughnessTexture"]["texCoord"] = json!(1);
                        doc["meshes"][0]["primitives"][0]["attributes"]["TEXCOORD_1"] = json!(5);
                    }
                    "transform" => {
                        pbr["metallicRoughnessTexture"]["extensions"] =
                            json!({"KHR_texture_transform":{"offset":[0.2,0.0]}})
                    }
                    _ => {}
                }
            });
            let model = glb::decode("paint", "paint.glb", &bytes).unwrap().info;
            let material = &model.materials["Paint"];
            assert_eq!(
                material.roughness_texture.is_some(),
                expected.is_none(),
                "{mode}"
            );
            assert_eq!(
                (material.linear, material.repeat),
                (expected.is_some(), expected.is_some())
            );
            if let Some(expected) = expected {
                assert!(
                    model
                        .warnings
                        .iter()
                        .any(|warning| warning.contains(expected)),
                    "{mode} {:?}",
                    model.warnings
                );
            }
        }
    }

    #[test]
    fn glb_cameras_keep_shared_lenses_and_last_accepted_catalog() {
        use crate::fixtures::punctual_glb;
        use serde_json::json;

        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("director.glb");
        let write = |edit: &dyn Fn(&mut serde_json::Value)| {
            fs::write(&source, punctual_glb(|doc| {
                doc["cameras"] = json!([
                    {"name":"Director","type":"perspective","perspective":{"yfov":f64::from(49).to_radians(),"aspectRatio":1.5,"znear":0.35}},
                    {"name":"Ortho","type":"orthographic","orthographic":{"xmag":1,"ymag":1,"znear":0,"zfar":5}},
                    {"name":"Wide","type":"perspective","perspective":{"yfov":2.25,"znear":2.0,"zfar":2.00000001}},
                ]);
                for (node, camera) in [(1,0),(2,0),(3,1),(4,2)] {
                    doc["nodes"][node]["camera"] = json!(camera);
                }
                edit(doc);
            })).unwrap();
        };
        write(&|_| {});
        let mut store = AssetStore::open(root.path()).unwrap();
        let first = store.import_glb_at(0, "director", "director.glb").unwrap();
        let lens = first.nodes["Rig/Arm"].camera.unwrap();
        assert_eq!(first.nodes["Rig/Base"].camera, Some(lens));
        assert_eq!(
            lens,
            crate::ModelCamera {
                vertical_fov_degrees: 49.,
                aspect_ratio: Some(1.5),
                near: 0.35,
                far: None
            }
        );
        assert!(first.nodes["Rig/Sun"].camera.is_none());
        assert!(first.nodes["Rig/Spot"].camera.is_none());
        assert!(first.warnings[0].contains("skipped orthographic"));
        assert!(first.warnings[1].contains("skipped perspective"));
        for edit in [
            (|doc: &mut serde_json::Value| doc["nodes"][2]["camera"] = json!(3))
                as fn(&mut serde_json::Value),
            |doc| doc["nodes"][2]["camera"] = json!(-1),
            |doc| doc["cameras"][0]["perspective"]["zfar"] = json!(null),
            |doc| doc["cameras"][0]["perspective"]["zfar"] = json!(0.2),
        ] {
            write(&edit);
            assert!(store.refresh_glb_at(store.revision(), "director").is_err());
            assert_eq!(store.model("director"), Some(&first));
        }
        let reopened = AssetStore::open(root.path()).unwrap();
        assert_eq!(reopened.model("director"), Some(&first));
    }

    #[test]
    fn punctual_refresh_keeps_last_good_and_reuses_model_paths() {
        use crate::fixtures::punctual_glb;
        use crate::ModelLight;
        use serde_json::json;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("lantern.glb");
        fs::write(
            &path,
            punctual_glb(|document| {
                document["extensionsUsed"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!("EXT_unknown_optional"));
                document["extensions"]["EXT_unknown_optional"] = json!({"unused": true});
                document["nodes"][1]["extensions"]["EXT_unknown_optional"] =
                    json!({"note":"ignored"});
                let punctual = &mut document["extensions"]["KHR_lights_punctual"];
                punctual["extras"] = json!({"metadata": [1,2,3]});
                punctual["lights"][0]["extras"] = json!({"blender_name":"Paint Lamp"});
                punctual["lights"][2]["spot"]["extras"] = json!({"optional":"metadata"});
            }),
        )
        .unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        let first = store.import_glb_at(0, "lantern", "lantern.glb").unwrap();
        assert_eq!(first.nodes["Rig/Arm"].light, first.nodes["Rig/Base"].light);
        assert_eq!(
            first.nodes["Rig/Arm"].light,
            Some(ModelLight::Point {
                color: [0.25, 0.5, 1.],
                intensity: 16.,
                range: 0.1
            })
        );
        assert_eq!(
            first.nodes["Rig/Sun"].light,
            Some(ModelLight::Directional {
                color: [1.; 3],
                intensity: 0.5
            })
        );
        assert_eq!(first.nodes["Rig/Spot"].translation, [1., 2., 3.]);
        assert!(first.nodes["Rig/Spot"].light.is_none());
        assert!(first
            .nodes
            .iter()
            .filter(|(path, _)| ModelInfo::is_primitive_path(path))
            .all(|(_, node)| node.light.is_none()));
        assert_eq!(
            first
                .warnings
                .iter()
                .filter(|warning| warning.contains("approximate"))
                .count(),
            1
        );
        assert!(first.warnings[1].contains("intensity clamped to 16, range clamped to 0.1"));
        assert!(first.warnings[2].contains("Skipped spot light"));

        for (path_segments, invalid) in [
            (
                &["nodes", "1", "extensions", "KHR_lights_punctual", "light"][..],
                json!(0.5),
            ),
            (
                &["nodes", "1", "extensions", "KHR_lights_punctual", "light"],
                json!(4),
            ),
            (&["extensionsUsed"], json!([])),
            (
                &["extensionsUsed"],
                json!(["KHR_lights_punctual", "KHR_lights_punctual"]),
            ),
            (
                &["extensionsRequired"],
                json!(["KHR_lights_punctual", "KHR_lights_punctual"]),
            ),
            (&["extensionsRequired"], json!(["NOT_supported"])),
            (
                &[
                    "extensions",
                    "KHR_lights_punctual",
                    "lights",
                    "0",
                    "intensity",
                ],
                json!(-1),
            ),
            (
                &["extensions", "KHR_lights_punctual", "lights", "0", "range"],
                json!(0),
            ),
            (
                &["extensions", "KHR_lights_punctual", "lights", "0", "color"],
                json!([1, 1, 2]),
            ),
            (
                &[
                    "extensions",
                    "KHR_lights_punctual",
                    "lights",
                    "2",
                    "spot",
                    "outerConeAngle",
                ],
                json!(2),
            ),
        ] {
            fs::write(
                &path,
                punctual_glb(|value| {
                    let mut field = value;
                    for segment in path_segments {
                        field = if let Ok(index) = segment.parse::<usize>() {
                            &mut field[index]
                        } else {
                            &mut field[*segment]
                        };
                    }
                    *field = invalid;
                }),
            )
            .unwrap();
            let refusal = store.refresh_glb_at(store.revision(), "lantern");
            assert!(
                refusal.is_err(),
                "accepted invalid value at {path_segments:?}"
            );
            assert_eq!(store.model("lantern"), Some(&first));
        }
        let reopened = AssetStore::open(root.path()).unwrap();
        assert_eq!(reopened.model("lantern"), Some(&first));
        fs::write(
            &path,
            punctual_glb(|value| {
                let point = &mut value["extensions"]["KHR_lights_punctual"]["lights"][0];
                point["intensity"] = json!(75);
                point.as_object_mut().unwrap().remove("range");
            }),
        )
        .unwrap();
        let refreshed = store.refresh_glb_at(store.revision(), "lantern").unwrap();
        assert_ne!(first.hash, refreshed.hash);
        assert!(first
            .nodes
            .iter()
            .zip(&refreshed.nodes)
            .all(|((path, old), (next_path, next))| path == next_path
                && old.same_placement_structure(next)));
        assert_eq!(
            refreshed.nodes["Rig/Arm"].light,
            Some(ModelLight::Point {
                color: [0.25, 0.5, 1.],
                intensity: 0.75,
                range: 5.
            })
        );
        assert!(refreshed.warnings[1].contains("range omitted"));
        assert_eq!(
            AssetStore::open(root.path()).unwrap().model("lantern"),
            Some(&refreshed)
        );
    }

    #[test]
    #[ignore = "requires pinned Blender GLB A/B fixtures; injects failure after real generation staging"]
    fn registry_replacement_failure_after_generation_staging_preserves_active_model() {
        let fixture = PathBuf::from(std::env::var_os("ROT_TEST_GLB_FIXTURES").unwrap());
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
        assert!(!root.path().join(".rot-cache").exists());
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
