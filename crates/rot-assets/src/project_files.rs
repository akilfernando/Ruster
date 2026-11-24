//! Project sources and their adjacent metadata are the inputs to the catalog.
use super::{
    atomic_write, confined_path, digest, error, read_bounded, validate_id,
    validate_project_relative, validate_registry, AssetError, AssetKind, AssetMutation,
    AssetStatus, AssetStore, MeshNormalPolicy, TextureRole, REGISTRY,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const METADATA_EXTENSION: &str = ".rotmeta";
pub const MAX_PROJECT_SIDECAR_BYTES: usize = 4096;

#[path = "project_prefabs.rs"]
mod prefabs;
pub use prefabs::{publish_project_prefab, verify_project_prefab, PrefabFile};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectFileIssue {
    /// The failing interpretation, if other imports can still use this source.
    pub asset_id: Option<String>,
    pub texture_role: Option<TextureRole>,
    pub path: String,
    pub status: AssetStatus,
    pub message: String,
}

impl ProjectFileIssue {
    fn invalid(path: impl Into<String>, error: impl std::fmt::Display) -> Self {
        Self {
            asset_id: None,
            texture_role: None,
            path: path.into(),
            status: AssetStatus::Invalid,
            message: error.to_string(),
        }
    }
}

impl std::fmt::Display for ProjectFileIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Metadata {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mesh_normal_policy: Option<MeshNormalPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    texture_role: Option<TextureRole>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportList {
    imports: Vec<Metadata>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Sidecar {
    Single(Metadata),
    Multiple(ImportList),
}

#[derive(Clone, Copy)]
enum Importer {
    Asset(AssetKind),
    Model,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Reconcile {
    Discover,
    Changed,
    Reimport,
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
    /// Size limit for a project source accepted by the automatic importer.
    pub fn project_source_limit(source: &str) -> Result<usize, AssetError> {
        validate_project_relative(source)?;
        if source.ends_with(".prefab") {
            return Ok(prefabs::MAX_PREFAB_BYTES);
        }
        match Importer::from_path(source) {
            Some(Importer::Asset(kind)) => Ok(kind.source_limit()),
            Some(Importer::Model) => Ok(super::MAX_GLB_SOURCE_BYTES),
            None => Err(error("Rot imports OBJ, PNG, WAV, GLB, and prefab files.")),
        }
    }

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
        self.reconcile_project_files(changed, active)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    pub fn reconcile_project_files(
        &mut self,
        changed: &BTreeSet<PathBuf>,
        active: impl Fn() -> bool,
    ) -> Vec<ProjectFileIssue> {
        self.reconcile_project_files_with_reimport(changed, None, active)
    }

    /// A creator's explicit reimport can adopt a new model interpretation
    /// even when the source hash and accepted dependency IDs are unchanged.
    pub fn reconcile_project_files_with_reimport(
        &mut self,
        changed: &BTreeSet<PathBuf>,
        reimport: Option<&Path>,
        active: impl Fn() -> bool,
    ) -> Vec<ProjectFileIssue> {
        let changed = |source: &str| {
            let source = Path::new(source);
            changed.iter().any(|path| source.starts_with(path))
        };
        let mut diagnostics = Vec::new();
        let mut pending = vec![String::from("Assets")];
        let mut sources = Vec::new();
        let mut sidecars = Vec::new();
        while let Some(relative) = pending.pop() {
            if !active() {
                return diagnostics;
            }
            let path = match confined_path(&self.root, &relative, true) {
                Ok(path) => path,
                Err(err) => {
                    diagnostics.push(ProjectFileIssue::invalid(&relative, err));
                    continue;
                }
            };
            let entries = match fs::read_dir(path) {
                Ok(entries) => entries,
                Err(err) if err.kind() == ErrorKind::NotFound => continue,
                Err(err) => {
                    diagnostics.push(ProjectFileIssue::invalid(&relative, err));
                    continue;
                }
            };
            for entry in entries {
                let result = entry.and_then(|entry| Ok((entry.file_name(), entry.file_type()?)));
                let (name, kind) = match result {
                    Ok(result) => result,
                    Err(err) => {
                        diagnostics.push(ProjectFileIssue::invalid(&relative, err));
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
                    } else if source
                        .strip_suffix(METADATA_EXTENSION)
                        .is_some_and(|path| Importer::from_path(path).is_some())
                    {
                        sidecars.push(source);
                    }
                }
            }
        }
        // Legacy sources can live elsewhere in the project. Give their
        // registered paths a sidecar so the creator can move the pair into
        // Assets without changing scene references.
        let legacy_sources = self
            .registry
            .assets
            .values()
            .map(|record| &record.source)
            .chain(self.registry.models.values().map(|record| &record.source));
        for source in legacy_sources.filter(|source| !source.starts_with("Assets/")) {
            if let Some(importer) = Importer::from_path(source) {
                if self.root.join(source).is_file() {
                    sources.push((source.clone(), importer));
                }
            }
        }
        sources.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        sources.dedup_by(|a, b| a.0 == b.0);
        let mut sidecar_ids = BTreeMap::<String, usize>::new();
        // A sidecar can arrive before its source during a directory move.
        // Reserve its IDs so the old source does not regenerate the sidecar.
        for sidecar in sidecars {
            let source = sidecar
                .strip_suffix(METADATA_EXTENSION)
                .expect("sidecar extension");
            if sources
                .binary_search_by(|entry| entry.0.as_str().cmp(source))
                .is_ok()
            {
                continue;
            }
            match confined_path(&self.root, &sidecar, false).and_then(|path| load_metadata(&path)) {
                Ok(entries) => {
                    for entry in entries {
                        *sidecar_ids.entry(entry.id).or_default() += 1;
                    }
                }
                Err(error) => diagnostics.push(ProjectFileIssue::invalid(sidecar, error)),
            }
        }
        let mut candidates = Vec::with_capacity(sources.len());
        for (source, importer) in sources {
            let metadata = confined_path(&self.root, &source, false)
                .and_then(|path| existing_metadata(&sidecar_path(&path)));
            match metadata {
                Ok(metadata) => {
                    if let Some(metadata) = &metadata {
                        for interpretation in metadata {
                            *sidecar_ids.entry(interpretation.id.clone()).or_default() += 1;
                        }
                    }
                    candidates.push((source, importer, metadata));
                }
                Err(err) => diagnostics.push(ProjectFileIssue::invalid(source, err)),
            }
        }
        for (source, importer, metadata) in candidates {
            if !active() {
                return diagnostics;
            }
            let mode = if reimport == Some(Path::new(&source)) {
                Reconcile::Reimport
            } else if changed(&source) {
                Reconcile::Changed
            } else {
                Reconcile::Discover
            };
            match self.reconcile_source(&source, importer, metadata, &sidecar_ids, mode, &active) {
                Ok(issues) => diagnostics.extend(issues),
                Err(err) => diagnostics.push(ProjectFileIssue::invalid(source, err)),
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
                diagnostics.push(ProjectFileIssue::invalid(
                    &self.registry.assets[&id].source,
                    err,
                ));
            }
        }
        for model in self.registry.models.values() {
            if model.source.starts_with("Assets/") && !self.root.join(&model.source).is_file() {
                diagnostics.push(ProjectFileIssue {
                    asset_id: Some(model.id.clone()),
                    texture_role: None,
                    path: model.source.clone(),
                    status: AssetStatus::Missing,
                    message: "Source file is missing.".into(),
                });
            }
        }
        diagnostics
    }

    fn reconcile_source(
        &mut self,
        source: &str,
        importer: Importer,
        metadata: Option<Vec<Metadata>>,
        sidecar_ids: &BTreeMap<String, usize>,
        mode: Reconcile,
        active: &impl Fn() -> bool,
    ) -> Result<Vec<ProjectFileIssue>, AssetError> {
        let registered = self.registered_interpretations(source);
        let old_metadata = metadata;
        let mut metadata = old_metadata.clone().unwrap_or_default();
        for entry in registered {
            if !metadata.iter().any(|old| old.id == entry.id) {
                if sidecar_ids.contains_key(&entry.id) {
                    return Err(error(format!("Duplicate sidecar asset ID {}.", entry.id)));
                }
                metadata.push(entry);
            }
        }
        if metadata.is_empty() {
            if self.awaiting_moved_sidecar(source, importer)? {
                return Err(error("Move the asset's sidecar next to the source."));
            }
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
            metadata.push(importer.metadata(id));
        }
        let previous = self.verify_sidecar(source, importer, &metadata, sidecar_ids)?;
        let path = confined_path(&self.root, source, false)?;
        let sidecar = sidecar_path(&path);
        if old_metadata.as_ref() != Some(&metadata) {
            if previous.is_some() {
                return Err(error(
                    "The destination has other registered imports. Move to a new source path.",
                ));
            }
            self.with_current_catalog(|| {
                if !active() {
                    return Err(error("Project closed during import."));
                }
                write_metadata(&sidecar, old_metadata.as_deref(), &metadata)
            })?;
        }
        let mode = if let Some(previous) = previous {
            self.relocate_source(&previous, source, importer, &metadata, active)?;
            if mode == Reconcile::Discover {
                Reconcile::Changed
            } else {
                mode
            }
        } else {
            mode
        };

        let mut failures = Vec::new();
        for interpretation in metadata {
            if !active() {
                break;
            }
            let id = interpretation.id.clone();
            let texture_role = match importer {
                Importer::Asset(AssetKind::Texture) => Some(
                    interpretation
                        .texture_role
                        .unwrap_or(TextureRole::BaseColor),
                ),
                _ => None,
            };
            if let Err(error) =
                self.reconcile_interpretation(source, importer, interpretation, mode, active)
            {
                let mut issue = ProjectFileIssue::invalid(source, error);
                issue.asset_id = Some(id);
                issue.texture_role = texture_role;
                failures.push(issue);
            }
        }
        Ok(failures)
    }

    /// Catalog-only hint. A candidate still needs its exact cache and sidecar verified.
    pub fn missing_source_restore_available(&self, id: &str) -> Option<bool> {
        let info = self.asset_info(id)?;
        if info.status != AssetStatus::Missing
            || info.hash.is_none()
            || !Path::new(&info.source).starts_with("Assets")
            || validate_project_relative(&info.source).is_err()
        {
            return None;
        }
        match info.kind {
            AssetKind::Script => Some(self.script(id).is_some()),
            AssetKind::Sound => Some(self.sound(id).is_some()),
            _ => None,
        }
    }

    /// Root-relative location and byte limit for the accepted original encoding.
    pub fn missing_source_restore_cache(&self, id: &str) -> Result<(PathBuf, usize), AssetError> {
        if self.missing_source_restore_available(id) != Some(true) {
            return Err(error(
                "No accepted source bytes are available for this missing asset.",
            ));
        }
        let info = self.asset_info(id).expect("missing asset");
        let hash = info.hash.as_ref().expect("accepted asset hash");
        let (directory, version) = match info.kind {
            AssetKind::Script => ("scripts", 1),
            AssetKind::Sound => ("sounds", super::SOUND_CACHE_VERSION),
            _ => unreachable!("restorable source kind"),
        };
        Ok((
            Path::new(".rot-cache")
                .join(directory)
                .join(format!("v{version}-{hash}.bin")),
            info.kind.source_limit(),
        ))
    }

    /// Validate bytes from descriptor-pinned reads before restoring a source.
    pub fn verify_missing_source_restore(
        &self,
        id: &str,
        sidecar: &[u8],
        source: &[u8],
    ) -> Result<(), AssetError> {
        if self.missing_source_restore_available(id) != Some(true) {
            return Err(error(
                "No accepted source bytes are available for this missing asset.",
            ));
        }
        let info = self.asset_info(id).expect("accepted missing asset");
        let imports = parse_metadata(sidecar)?;
        if !matches!(&imports[..], [metadata] if metadata.id == id && metadata.mesh_normal_policy.is_none() && metadata.texture_role.is_none())
        {
            return Err(error(
                "The asset sidecar does not match the accepted catalog identity.",
            ));
        }
        if source.len() > info.kind.source_limit() || info.hash.as_deref() != Some(&digest(source))
        {
            return Err(error(
                "The cached source failed its accepted SHA-256 check. Restore a backup.",
            ));
        }
        Ok(())
    }

    /// Validate the interpretation identities before moving a registered source.
    /// File actions call this while holding the current catalog lock.
    pub fn validate_movable_sidecar(&self, source: &str) -> Result<(), AssetError> {
        let accepted = self.registered_interpretations(source);
        if accepted.is_empty() {
            return Ok(());
        }
        let path = confined_path(&self.root, source, false)?;
        let entries = load_metadata(&sidecar_path(&path))?;
        if accepted
            .iter()
            .any(|entry| !entries.iter().any(|metadata| metadata.id == entry.id))
        {
            return Err(error("The sidecar is missing a registered asset ID."));
        }
        let importer =
            Importer::from_path(source).ok_or_else(|| error("Unsupported asset source."))?;
        if self
            .verify_sidecar(source, importer, &entries, &BTreeMap::new())?
            .is_some()
        {
            return Err(error("The sidecar belongs to another source."));
        }
        Ok(())
    }

    /// Publish another interpretation of a managed PNG through the same
    /// transaction as a guarded asset import.
    pub fn interpret_project_texture(
        &mut self,
        source: &str,
        role: TextureRole,
        active: impl Fn() -> bool,
    ) -> Result<(), AssetError> {
        Self::project_source_limit(source)?;
        if !source.ends_with(".png") || !Path::new(source).starts_with("Assets") {
            return Err(error("Choose a PNG in the Assets folder."));
        }
        let path = confined_path(&self.root, source, false)?;
        let sidecar = sidecar_path(&path);
        let metadata = existing_metadata(&sidecar)?.unwrap_or_default();
        let id = if let Some(entry) = metadata
            .iter()
            .find(|entry| entry.texture_role == Some(role))
        {
            entry.id.clone()
        } else {
            let mut reserved: BTreeSet<_> = metadata.into_iter().map(|entry| entry.id).collect();
            self.visit_other_sidecar_ids(&sidecar, |id| {
                reserved.insert(id.to_owned());
                Ok(())
            })?;
            let hash = digest(source.as_bytes());
            (1..)
                .map(|suffix| format!("asset-{}-{suffix}", &hash[..24]))
                .find(|id| !self.registry.has_asset_id(id) && !reserved.contains(id))
                .expect("finite catalog")
        };
        let request = self.request_at(
            self.revision(),
            AssetMutation::ImportTexture {
                id,
                source: source.into(),
                role,
            },
        )?;
        self.publish_project_request(request.prepare(), active)
            .map(|_| ())
    }

    /// Clear a failed Base Color reservation beside an accepted Roughness import.
    pub fn clear_failed_base_color(
        &self,
        source: &str,
        id: &str,
        active: impl FnOnce() -> bool,
    ) -> Result<(), AssetError> {
        Self::project_source_limit(source)?;
        if !source.ends_with(".png") || !Path::new(source).starts_with("Assets") {
            return Err(error("Choose a PNG in the Assets folder."));
        }
        let sidecar = sidecar_path(&confined_path(&self.root, source, false)?);
        self.with_current_catalog(|| {
            let unavailable = || error("No failed Base Color import beside a ready Roughness map.");
            if self.registry.has_asset_id(id) {
                return Err(unavailable());
            }
            let previous = existing_metadata(&sidecar)?.ok_or_else(unavailable)?;
            let mut entries = previous.clone();
            let index = entries
                .iter()
                .position(|entry| {
                    entry.id == id
                        && entry.mesh_normal_policy.is_none()
                        && entry.texture_role.unwrap_or(TextureRole::BaseColor)
                            == TextureRole::BaseColor
                })
                .ok_or_else(unavailable)?;
            let bytes = super::read_source_bytes(&self.root, source, AssetKind::Texture)?;
            let hash = digest(&bytes);
            let has_roughness = entries.iter().any(|entry| {
                entry.texture_role == Some(TextureRole::Roughness)
                    && self.asset_info(&entry.id).is_some_and(|info| {
                        info.source == source
                            && info.texture_role == Some(TextureRole::Roughness)
                            && info.status == AssetStatus::Ready
                            && info.hash.as_deref() == Some(hash.as_str())
                    })
            });
            if !has_roughness || super::decode_png(&bytes).is_ok() {
                return Err(unavailable());
            }
            if !active() {
                return Err(error("Project closed before clearing the failed import."));
            }
            entries.remove(index);
            write_metadata(&sidecar, Some(&previous), &entries)
        })
    }

    /// Give a copied source new identities while retaining its accepted import settings.
    pub fn duplicate_import_settings(
        &self,
        source: &str,
        target: &str,
        active: impl Fn() -> bool,
    ) -> Result<(), AssetError> {
        let mut entries = self.registered_interpretations(source);
        if entries.is_empty() {
            return Ok(());
        }
        let hash = digest(target.as_bytes());
        let mut candidates = (1..)
            .map(|suffix| format!("asset-{}-{suffix}", &hash[..24]))
            .filter(|id| !self.registry.has_asset_id(id));
        for entry in &mut entries {
            entry.id = candidates.next().expect("finite catalog");
        }
        let path = sidecar_path(&confined_path(&self.root, target, false)?);
        self.with_current_catalog(|| {
            if !active() {
                return Err(error("Project closed during duplication."));
            }
            write_metadata(&path, None, &entries)
        })
    }

    fn registered_interpretations(&self, source: &str) -> Vec<Metadata> {
        self.registry
            .assets
            .iter()
            .filter(|(_, record)| record.source == source)
            .map(|(id, record)| Metadata {
                id: id.clone(),
                mesh_normal_policy: record.mesh_normal_policy,
                texture_role: record.texture_role,
            })
            .chain(
                self.registry
                    .models
                    .values()
                    .filter(|record| record.source == source)
                    .map(|record| Importer::Model.metadata(record.id.clone())),
            )
            .collect()
    }

    fn verify_sidecar(
        &self,
        source: &str,
        importer: Importer,
        metadata: &[Metadata],
        sidecar_ids: &BTreeMap<String, usize>,
    ) -> Result<Option<String>, AssetError> {
        let mut settings_seen = BTreeSet::new();
        let mut previous = None;
        for entry in metadata {
            if sidecar_ids.get(&entry.id).is_some_and(|count| *count > 1) {
                return Err(error(format!("Duplicate sidecar asset ID {}.", entry.id)));
            }
            let registered_source = match importer {
                Importer::Asset(kind) => {
                    let mode = settings(kind, entry)?;
                    if !settings_seen.insert(mode) {
                        return Err(error("Multiple asset IDs use the same import settings."));
                    }
                    if self.registry.models.contains_key(&entry.id) {
                        return Err(existing_asset_id(&entry.id));
                    }
                    if let Some(record) = self.registry.assets.get(&entry.id) {
                        if record.kind != kind {
                            return Err(existing_asset_id(&entry.id));
                        }
                        if (record.mesh_normal_policy, record.texture_role) != mode {
                            return Err(error(
                                "Sidecar import settings differ from the accepted catalog.",
                            ));
                        }
                        Some(record.source.as_str())
                    } else {
                        None
                    }
                }
                Importer::Model => {
                    if metadata.len() != 1
                        || entry.mesh_normal_policy.is_some()
                        || entry.texture_role.is_some()
                    {
                        return Err(error("GLB requires one ID without OBJ or PNG settings."));
                    }
                    if self.registry.assets.contains_key(&entry.id) {
                        return Err(existing_asset_id(&entry.id));
                    }
                    self.registry
                        .models
                        .get(&entry.id)
                        .map(|model| model.source.as_str())
                }
            };
            if let Some(registered_source) = registered_source.filter(|path| *path != source) {
                if previous.is_some_and(|previous| previous != registered_source) {
                    return Err(error(
                        "One sidecar cannot move IDs from different source files.",
                    ));
                }
                previous = Some(registered_source);
            }
        }
        if let Some(previous) = previous {
            let all_registered = self.registered_interpretations(previous);
            if all_registered
                .iter()
                .any(|registered| !metadata.iter().any(|entry| entry.id == registered.id))
            {
                return Err(error(
                    "The moved sidecar must contain every registered import of its source.",
                ));
            }
        }
        Ok(previous.map(str::to_owned))
    }

    fn reconcile_interpretation(
        &mut self,
        source: &str,
        importer: Importer,
        metadata: Metadata,
        mode: Reconcile,
        active: &impl Fn() -> bool,
    ) -> Result<(), AssetError> {
        let mutation = match importer {
            Importer::Asset(kind) => {
                let (policy, role) = settings(kind, &metadata)?;
                if let Some(record) = self.registry.assets.get(&metadata.id) {
                    let ready = self.infos[&metadata.id].status == AssetStatus::Ready;
                    if ready
                        && (mode == Reconcile::Discover
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
                        AssetKind::Sound | AssetKind::Script => AssetMutation::Import {
                            id: metadata.id,
                            source: source.into(),
                        },
                    }
                }
            }
            Importer::Model => {
                if let Some(model) = self.registry.models.get(&metadata.id) {
                    if mode != Reconcile::Reimport
                        && digest(&super::glb_catalog::read_candidate(&self.root, source)?)
                            == model.hash
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
            let old = self.root.join(previous);
            Ok(is_missing(&old)?
                && existing_metadata(&sidecar_path(&old))
                    .ok()
                    .flatten()
                    .is_some_and(|entries| entries.iter().any(|metadata| metadata.id == id)))
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
            Importer::Model => super::glb_catalog::read_candidate(&self.root, source)?,
        };
        Ok(orphaned.contains(&digest(&bytes).as_str()))
    }

    fn relocate_source(
        &mut self,
        previous: &str,
        destination: &str,
        importer: Importer,
        metadata: &[Metadata],
        active: &impl Fn() -> bool,
    ) -> Result<(), AssetError> {
        let _lock = self.guard(self.revision())?;
        let old = self.root.join(previous);
        if !is_missing(&old)? || !is_missing(&sidecar_path(&old))? {
            return Err(existing_asset_id(&metadata[0].id));
        }
        let new = confined_path(&self.root, destination, false)?;
        if load_metadata(&sidecar_path(&new))? != metadata || !active() {
            return Err(error("The asset or project changed during the move."));
        }
        let mut registry = self.registry.clone();
        for entry in metadata {
            let path = match importer {
                Importer::Asset(_) => registry
                    .assets
                    .get_mut(&entry.id)
                    .map(|asset| &mut asset.source),
                Importer::Model => registry
                    .models
                    .get_mut(&entry.id)
                    .map(|model| &mut model.source),
            };
            if let Some(path) = path.filter(|path| path.as_str() == previous) {
                *path = destination.into();
            }
        }
        validate_registry(&registry)?;
        let prepared = super::mutations::encode_registry(registry)?;
        if !active() {
            return Err(error("Project closed during the move."));
        }
        atomic_write(&self.root, &self.root.join(REGISTRY), &prepared.bytes)?;
        self.install_registry(prepared);
        for entry in metadata {
            if let Some(info) = self.infos.get_mut(&entry.id) {
                Arc::make_mut(info).source = destination.into();
            }
        }
        Ok(())
    }

    // Explicit imports must honor reserved (including as-yet unimported)
    // sidecar IDs and preserve other interpretations of the same source.
    // The publisher calls this with the same catalog lock it uses to commit.
    pub(super) fn validate_project_import_sidecar(
        &self,
        mutation: &AssetMutation,
    ) -> Result<(), AssetError> {
        let (id, source) = match mutation {
            AssetMutation::Import { id, source }
            | AssetMutation::ImportMesh { id, source, .. }
            | AssetMutation::ImportTexture { id, source, .. }
            | AssetMutation::ImportGlb { id, source } => (id, source),
            _ => return Ok(()),
        };
        let importer =
            Importer::from_path(source).ok_or_else(|| error("Unsupported asset source."))?;
        let mut requested = importer.metadata(id.clone());
        if requested.mesh_normal_policy.is_some() {
            requested.mesh_normal_policy = Some(match mutation {
                AssetMutation::ImportMesh { normal_policy, .. } => *normal_policy,
                _ => MeshNormalPolicy::Flat,
            });
        }
        if let AssetMutation::ImportTexture { role, .. } = mutation {
            requested.texture_role = Some(*role);
        }
        let path = confined_path(&self.root, source, false)?;
        let sidecar = sidecar_path(&path);
        let mut entries = existing_metadata(&sidecar)?.unwrap_or_default();
        for registered in self.registered_interpretations(source) {
            if !entries.iter().any(|entry| entry.id == registered.id) {
                entries.push(registered);
            }
        }
        if let Some(entry) = entries.iter().find(|entry| entry.id == *id) {
            let agrees = match importer {
                Importer::Asset(kind) => settings(kind, entry)? == settings(kind, &requested)?,
                Importer::Model => entry == &requested,
            };
            if !agrees {
                return Err(error("Import settings differ from the source sidecar."));
            }
        } else {
            entries.push(requested);
        }
        if self
            .verify_sidecar(source, importer, &entries, &BTreeMap::new())?
            .is_some()
        {
            return Err(error(
                "This sidecar belongs to a moved asset. Let the watcher adopt the move first.",
            ));
        }
        self.visit_other_sidecar_ids(&sidecar, |candidate| {
            if candidate == id {
                return Err(error(format!(
                    "Asset ID {id} is reserved by another sidecar."
                )));
            }
            Ok(())
        })
    }

    fn visit_other_sidecar_ids(
        &self,
        sidecar: &Path,
        mut visit: impl FnMut(&str) -> Result<(), AssetError>,
    ) -> Result<(), AssetError> {
        // Include sidecars whose source is missing. Those IDs are still
        // reserved for the creator's source files.
        let mut dirs = vec![self.root.join("Assets")];
        while let Some(dir) = dirs.pop() {
            let children = match fs::symlink_metadata(&dir) {
                Ok(metadata) if metadata.is_dir() => fs::read_dir(&dir)?,
                Ok(_) => continue,
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            for child in children {
                let child = child?;
                let name = child.file_name();
                let Some(name) = name.to_str().filter(|name| !name.starts_with('.')) else {
                    continue;
                };
                let kind = child.file_type()?;
                let other = child.path();
                if kind.is_dir() {
                    dirs.push(other);
                } else if kind.is_file()
                    && other != sidecar
                    && name
                        .strip_suffix(METADATA_EXTENSION)
                        .is_some_and(|stem| Importer::from_path(stem).is_some())
                {
                    if let Ok(entries) = load_metadata(&other) {
                        for entry in entries {
                            visit(&entry.id)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn publish_project_change(
        &mut self,
        mutation: AssetMutation,
        active: &impl Fn() -> bool,
    ) -> Result<(), AssetError> {
        let prepared = self.request_at(self.revision(), mutation)?.prepare();
        self.adopt_project_request(prepared, active, false)
            .map(|_| ())
    }

    /// Run the same guarded publication used for file imports and reimports.
    /// The caller owns the project worker and any editor request lease.
    pub fn publish_project_request(
        &mut self,
        prepared: super::PreparedAsset,
        active: impl Fn() -> bool,
    ) -> Result<super::AssetOutcome, AssetError> {
        self.adopt_project_request(prepared, active, true)
    }

    fn adopt_project_request(
        &mut self,
        prepared: super::PreparedAsset,
        active: impl Fn() -> bool,
        validate_sidecar: bool,
    ) -> Result<super::AssetOutcome, AssetError> {
        if !self.matches_identity(prepared.state_identity()) {
            return Err(error("Asset state changed during import."));
        }
        if !active() {
            return Err(error("Project closed during import."));
        }
        let completion = prepared.publish_with_project_validation(active, validate_sidecar);
        self.adopt(completion)
            .map_err(|_| error("Asset state changed during import."))?
            .result
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

fn existing_metadata(path: &Path) -> Result<Option<Vec<Metadata>>, AssetError> {
    match fs::symlink_metadata(path) {
        Ok(_) => load_metadata(path).map(Some),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn load_metadata(path: &Path) -> Result<Vec<Metadata>, AssetError> {
    parse_metadata(&read_bounded(path, MAX_PROJECT_SIDECAR_BYTES)?)
}

fn parse_metadata(bytes: &[u8]) -> Result<Vec<Metadata>, AssetError> {
    if bytes.len() > MAX_PROJECT_SIDECAR_BYTES {
        return Err(error("The asset sidecar exceeds its size limit."));
    }
    let imports = match serde_json::from_slice(bytes)? {
        Sidecar::Single(metadata) => vec![metadata],
        Sidecar::Multiple(list) => list.imports,
    };
    if imports.is_empty() {
        return Err(error("A sidecar needs at least one import."));
    }
    let mut ids = BTreeSet::new();
    for metadata in &imports {
        validate_id(&metadata.id)?;
        if !ids.insert(metadata.id.as_str()) {
            return Err(error(format!(
                "Duplicate sidecar asset ID {}.",
                metadata.id
            )));
        }
    }
    Ok(imports)
}

fn write_metadata(
    path: &Path,
    previous: Option<&[Metadata]>,
    metadata: &[Metadata],
) -> Result<(), AssetError> {
    let parent = path
        .parent()
        .ok_or_else(|| error("Missing asset directory."))?;
    let mut bytes = match metadata {
        [entry] => serde_json::to_vec_pretty(entry)?,
        entries => serde_json::to_vec_pretty(&ImportList {
            imports: entries.to_vec(),
        })?,
    };
    bytes.push(b'\n');
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    if existing_metadata(path)?.as_deref() != previous {
        return Err(error("Asset sidecar changed; retry on the next scan."));
    }
    if previous.is_some() {
        file.persist(path).map_err(|failure| failure.error)?;
    } else {
        file.persist_noclobber(path)
            .map_err(|failure| failure.error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    const OBJ: &str = "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n";

    #[test]
    fn roughness_interpretation_preserves_ready_and_reserved_base_color_ids() {
        let dir = tempfile::tempdir().unwrap();
        let assets = dir.path().join("Assets");
        fs::create_dir(&assets).unwrap();
        let gray = "Assets/Gray.png";
        let rgb = "Assets/Rgb.png";
        for (path, mode, pixels) in [
            (gray, png::ColorType::Grayscale, &[128u8][..]),
            (rgb, png::ColorType::Rgb, &[18u8, 99, 207][..]),
        ] {
            let mut png = Vec::new();
            let mut encoder = png::Encoder::new(&mut png, 1, 1);
            encoder.set_color(mode);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(pixels)
                .unwrap();
            fs::write(dir.path().join(path), png).unwrap();
        }
        let hash = digest(gray.as_bytes());
        let previously_reserved = format!("asset-{}-1", &hash[..24]);
        write_metadata(
            &sidecar_path(&assets.join("Missing.png")),
            None,
            &[Importer::Asset(AssetKind::Texture).metadata(previously_reserved.clone())],
        )
        .unwrap();
        let (mut store, diagnostics) = AssetStore::open_project(dir.path()).unwrap();
        assert_eq!(diagnostics.len(), 1);
        let gray_sidecar = sidecar_path(&dir.path().join(gray));
        let reservation = load_metadata(&gray_sidecar).unwrap().remove(0);
        assert_eq!(reservation.texture_role, Some(TextureRole::BaseColor));
        let color_id = store
            .infos()
            .find(|info| info.source == rgb)
            .unwrap()
            .id
            .clone();
        let original = store.texture_snapshots()[&color_id].clone();
        let original_bytes = fs::read(dir.path().join(rgb)).unwrap();
        assert!(store
            .interpret_project_texture(rgb, TextureRole::Roughness, || false)
            .is_err());
        assert_eq!(store.infos().filter(|info| info.source == rgb).count(), 1);
        for path in [rgb, gray] {
            store
                .interpret_project_texture(path, TextureRole::Roughness, || true)
                .unwrap();
        }
        let issues =
            store.reconcile_project_files(&BTreeSet::from([gray.into(), rgb.into()]), || true);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].asset_id.as_deref(), Some(reservation.id.as_str()));
        let gray_roles = load_metadata(&gray_sidecar).unwrap();
        assert!(gray_roles.contains(&reservation));
        let roughness = gray_roles
            .iter()
            .find(|entry| entry.texture_role == Some(TextureRole::Roughness))
            .unwrap();
        assert_ne!(roughness.id, reservation.id);
        assert_ne!(roughness.id, previously_reserved);
        assert_eq!(store.texture(&roughness.id).unwrap().mips[0].bytes, [128]);
        assert!(std::sync::Arc::ptr_eq(
            &store.texture_snapshots()[&color_id],
            &original
        ));
        assert_eq!(fs::read(dir.path().join(rgb)).unwrap(), original_bytes);
        let (mut reopened, _) = AssetStore::open_project(dir.path()).unwrap();
        let issues = reopened.reconcile_project_files(&BTreeSet::new(), || true);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].asset_id.as_deref(), Some(reservation.id.as_str()));
        assert_eq!(
            reopened.asset_info(&roughness.id).unwrap().status,
            AssetStatus::Ready
        );
        let source_bytes = fs::read(dir.path().join(gray)).unwrap();
        let accepted = reopened.texture_snapshots()[&roughness.id].clone();
        let revision = reopened.revision();
        assert!(reopened
            .clear_failed_base_color(gray, &roughness.id, || true)
            .is_err());
        assert!(reopened
            .clear_failed_base_color(gray, &reservation.id, || false)
            .is_err());
        fs::write(dir.path().join(gray), &original_bytes).unwrap();
        assert!(reopened
            .clear_failed_base_color(gray, &reservation.id, || true)
            .is_err());
        fs::write(dir.path().join(gray), &source_bytes).unwrap();
        assert_eq!(load_metadata(&gray_sidecar).unwrap(), gray_roles);
        reopened
            .clear_failed_base_color(gray, &reservation.id, || true)
            .unwrap();
        assert_eq!(
            load_metadata(&gray_sidecar).unwrap(),
            vec![roughness.clone()]
        );
        assert_eq!(reopened.revision(), revision);
        assert!(Arc::ptr_eq(
            &reopened.texture_snapshots()[&roughness.id],
            &accepted
        ));
        assert_eq!(fs::read(dir.path().join(gray)).unwrap(), source_bytes);
        let (clean, diagnostics) = AssetStore::open_project(dir.path()).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(clean.texture(&roughness.id), Some(accepted.as_ref()));
    }

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
                .any(|message| message.contains("Duplicate sidecar asset ID")),
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
    fn migration_does_not_copy_an_id_claimed_by_another_sidecar() {
        let root = tempfile::tempdir().unwrap();
        let assets = root.path().join("Assets");
        fs::create_dir(&assets).unwrap();
        fs::write(assets.join("Original.obj"), OBJ).unwrap();
        fs::write(assets.join("Duplicate.obj"), OBJ).unwrap();
        let mut store = AssetStore::open(root.path()).unwrap();
        let accepted = store.import("original", "Assets/Original.obj").unwrap();
        let duplicate = assets.join("Duplicate.obj.rotmeta");
        let metadata = Metadata {
            id: accepted.id.clone(),
            mesh_normal_policy: Some(MeshNormalPolicy::Flat),
            texture_role: None,
        };
        write_metadata(&duplicate, None, &[metadata]).unwrap();
        let before = fs::read(root.path().join(REGISTRY)).unwrap();

        let (migrated, diagnostics) = AssetStore::open_project(root.path()).unwrap();
        assert!(!assets.join("Original.obj.rotmeta").exists());
        assert_eq!(migrated.asset_info(&accepted.id), Some(&accepted));
        assert_eq!(fs::read(root.path().join(REGISTRY)).unwrap(), before);
        assert!(diagnostics
            .iter()
            .any(|line| line.contains("Duplicate sidecar asset ID")));

        fs::remove_file(duplicate).unwrap();
        fs::remove_file(assets.join("Duplicate.obj")).unwrap();
        let (migrated, diagnostics) = AssetStore::open_project(root.path()).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            load_metadata(&assets.join("Original.obj.rotmeta")).unwrap()[0].id,
            accepted.id
        );
        assert_eq!(migrated.asset_info(&accepted.id), Some(&accepted));
    }

    #[test]
    fn legacy_imports_keep_all_ids_when_migrated_moved_and_rebuilt() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        let folder = root.join("Assets");
        fs::create_dir(&folder).unwrap();
        let obj = folder.join("Legacy.obj");
        fs::write(&obj, OBJ).unwrap();
        let model = root.join("Legacy.glb");
        fs::write(&model, crate::fixtures::animated_glb(Default::default())).unwrap();
        let mut old = AssetStore::open(root).unwrap();
        old.import_mesh("flat-id", "Assets/Legacy.obj", MeshNormalPolicy::Flat)
            .unwrap();
        old.import_mesh(
            "normals-id",
            "Assets/Legacy.obj",
            MeshNormalPolicy::AuthoredWhenComplete,
        )
        .unwrap();
        let accepted_model = old
            .import_glb_at(old.revision(), "animated-id", "Legacy.glb")
            .unwrap();
        drop(old);
        // A project that already has metadata for one interpretation can
        // acquire the remaining catalog IDs without replacing the first.
        let first = Metadata {
            id: "flat-id".into(),
            mesh_normal_policy: Some(MeshNormalPolicy::Flat),
            texture_role: None,
        };
        write_metadata(&sidecar_path(&obj), None, std::slice::from_ref(&first)).unwrap();
        let before = fs::read(root.join(REGISTRY)).unwrap();
        let (mut migrated, diagnostics) = AssetStore::open_project(root).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(fs::read(root.join(REGISTRY)).unwrap(), before);
        let migrated_imports = load_metadata(&sidecar_path(&obj)).unwrap();
        assert_eq!(migrated_imports[0], first);
        assert_eq!(migrated_imports.len(), 2);
        assert_eq!(migrated_imports[1].id, "normals-id");
        assert_eq!(
            load_metadata(&sidecar_path(&model)).unwrap()[0].id,
            "animated-id"
        );

        let destination = folder.join("Moved.obj");
        let changed = [PathBuf::from("Assets")].into_iter().collect();
        fs::rename(sidecar_path(&obj), sidecar_path(&destination)).unwrap();
        let between = migrated.update_project_files(&changed, || true);
        assert!(between
            .iter()
            .any(|message| message.contains("Duplicate sidecar asset ID")));
        assert!(is_missing(&sidecar_path(&obj)).unwrap());
        fs::rename(&obj, &destination).unwrap();
        let moved_model = folder.join("Moved.glb");
        fs::rename(&model, &moved_model).unwrap();
        fs::rename(sidecar_path(&model), sidecar_path(&moved_model)).unwrap();
        let diagnostics = migrated.update_project_files(&changed, || true);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        for id in ["flat-id", "normals-id"] {
            assert_eq!(migrated.asset_info(id).unwrap().source, "Assets/Moved.obj");
        }
        let mut expected_model = accepted_model;
        expected_model.source = "Assets/Moved.glb".into();
        assert_eq!(migrated.model("animated-id"), Some(&expected_model));
        drop(migrated);

        fs::remove_file(root.join(REGISTRY)).unwrap();
        fs::remove_dir_all(root.join(".rot-cache")).unwrap();
        fs::remove_dir_all(root.join(".rot-models")).unwrap();
        let (rebuilt, diagnostics) = AssetStore::open_project(root).unwrap();
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            rebuilt.asset_info("flat-id").unwrap().mesh_normal_policy,
            Some(MeshNormalPolicy::Flat)
        );
        assert_eq!(
            rebuilt.asset_info("normals-id").unwrap().mesh_normal_policy,
            Some(MeshNormalPolicy::AuthoredWhenComplete)
        );
        assert_eq!(rebuilt.model("animated-id"), Some(&expected_model));
        for clip in expected_model.clips.values() {
            assert!(rebuilt.clip(&clip.id).is_some());
        }
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
        let id = |name: &str| {
            load_metadata(&sidecar_path(&folder.join(name)))
                .unwrap()
                .remove(0)
                .id
        };
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
