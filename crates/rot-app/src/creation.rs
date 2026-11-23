//! A small read-only launch descriptor for exported creator content.
//! The supported scene document owns authored state; this is not a second project editor.
use crate::{MeshMap, ResourceRoot, TextureMap};
use rot_assets::TextureRole;
use rot_scene::{
    component_scene::{self, Component, ComponentScene, Geometry},
    OpaqueMaterial, Scene, SceneError, Session, SoundAssetReferences,
};
use serde::{de, Deserialize, Deserializer, Serialize};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{self, BufReader, Cursor, Read, Seek},
    path::Path,
    sync::Arc,
    time::Duration,
};

pub const DESCRIPTOR_NAME: &str = "app.json";
pub const FORMAT: &str = "rot-application";
pub const NO_AUDIO_VERSION: u32 = 5;
/// The audio-capable descriptor without imported transform animation.
pub const NO_ANIMATION_VERSION: u32 = 6;
pub const VERSION: u32 = 7;
pub const MAX_DESCRIPTOR_BYTES: usize = 8 * 1024 * 1024;
pub const STARTUP_TRANSITION_NAME: &str = "startup-transition.json";
pub const STARTUP_TARGET_DIRECTORY: &str = "transition";
const STARTUP_TARGET_MANIFEST: &str = "transition/app.json";
const STARTUP_TRANSITION_BYTES: usize = 4096;

/// Optional once-per-launch metadata for the single packaged Start Button edge.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartupTransition {
    version: u32,
    pub source_scene_id: String,
    pub action_id: String,
    pub target_scene_id: String,
    target_manifest: String,
}

impl StartupTransition {
    pub fn new(source_scene_id: String, action_id: String, target_scene_id: String) -> Self {
        Self {
            version: 1,
            source_scene_id,
            action_id,
            target_scene_id,
            target_manifest: STARTUP_TARGET_MANIFEST.into(),
        }
    }

    pub fn load(root: &ResourceRoot, source: &RuntimeScene) -> Result<Option<Self>, String> {
        let Some(bytes) = root.read_optional(STARTUP_TRANSITION_NAME, STARTUP_TRANSITION_BYTES)?
        else {
            return Ok(None);
        };
        let route: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid {STARTUP_TRANSITION_NAME}: {error}"))?;
        route.validate()?;
        let scene = source.draw_scene();
        if scene.id != route.source_scene_id {
            return Err("Start Transition source scene ID differs from the startup scene".into());
        }
        if !matches!(source, RuntimeScene::Behavior(session)
            if session.authoring().actions.get(&route.action_id).is_some_and(|action|
                matches!(action.kind, rot_scene::input::ActionKind::Button { .. })))
        {
            return Err(format!(
                "Start Button {:?} is missing or is not a Button in the startup scene",
                route.action_id
            ));
        }
        Ok(Some(route))
    }

    pub fn load_destination(
        &self,
        root: &ResourceRoot,
    ) -> Result<(ResourceRoot, CreationManifest, RuntimeScene), String> {
        self.validate()?;
        let child = root.resolve(STARTUP_TARGET_DIRECTORY)?;
        let path = root.resolve(&self.target_manifest)?;
        if path.parent() != Some(child.as_path()) {
            return Err("Start Transition manifest escapes its scene resource directory".into());
        }
        let resources = ResourceRoot::new(child)?;
        let manifest = CreationManifest::load(&resources)?;
        let snapshot = SceneSnapshot::from_file(resources.open_regular(&manifest.scene)?)
            .map_err(snapshot_error)?;
        let SceneSnapshot::Components(target) = snapshot else {
            return Err("Start Transition requires a packaged component scene".into());
        };
        if target.id != self.target_scene_id {
            return Err(
                "Start Transition destination scene ID differs from its configured ID".into(),
            );
        }
        let scene = manifest.runtime_scene(SceneSnapshot::Components(target))?;
        let dependencies = scene.draw_dependencies()?;
        if !manifest.meshes.keys().eq(dependencies.meshes.iter())
            || manifest.texture_roles.as_ref() != Some(&dependencies.texture_roles)
        {
            return Err(
                "Start Transition resources differ from the destination scene dependencies".into(),
            );
        }
        Ok((resources, manifest, scene))
    }

    fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.target_manifest != STARTUP_TARGET_MANIFEST {
            return Err("Unsupported Start Transition descriptor or destination manifest".into());
        }
        for id in [
            &self.source_scene_id,
            &self.action_id,
            &self.target_scene_id,
        ] {
            rot_scene::validate_id(id).map_err(|error| error.to_string())?;
        }
        if self.source_scene_id == self.target_scene_id {
            return Err("Start Transition destination must be a different scene".into());
        }
        Ok(())
    }
}

/// Launch data for an application. Static hierarchies outside the behavior
/// adapter's capabilities retain their component objects and draw matrices.
pub enum RuntimeScene {
    Behavior(Box<Session>),
    StaticHierarchy {
        scene: Box<ComponentScene>,
        preview: Arc<rot_render::component_preview::ComponentPreview>,
    },
}

enum SceneSnapshot {
    Legacy(Box<Session>),
    Components(Box<ComponentScene>),
}

impl SceneSnapshot {
    fn from_bytes(bytes: &[u8]) -> Result<Self, SceneError> {
        if bytes.len() as u64 > rot_scene::MAX_FILE_BYTES {
            return Self::read_large(Cursor::new(bytes));
        }
        match Session::from_snapshot_bytes(bytes) {
            Ok(session) => Ok(Self::Legacy(Box::new(session))),
            Err(SceneError::UnsupportedVersion(version))
                if version == u64::from(component_scene::FORMAT_VERSION) =>
            {
                ComponentScene::read_json(bytes)
                    .map(Box::new)
                    .map(Self::Components)
            }
            Err(error) => Err(error),
        }
    }

    fn from_file(mut file: File) -> Result<Self, SceneError> {
        if file.metadata()?.len() <= rot_scene::MAX_FILE_BYTES {
            let mut bytes = Vec::new();
            (&mut file)
                .take(rot_scene::MAX_FILE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 <= rot_scene::MAX_FILE_BYTES {
                return Self::from_bytes(&bytes);
            }
            file.rewind()?;
        }
        Self::read_large(file)
    }

    fn read_large(mut input: impl Read + Seek) -> Result<Self, SceneError> {
        match probe_large_scene(BufReader::new(&mut input))? {
            OversizedFormat::Legacy => Err(SceneError::Invalid("file exceeds 1 MiB".into())),
            OversizedFormat::Future(version) => Err(SceneError::UnsupportedVersion(version)),
            OversizedFormat::Components => {
                input.rewind()?;
                // The strict decoder checks every field and duplicate. A
                // failed component decode never retries as legacy.
                ComponentScene::read_json(input)
                    .map(Box::new)
                    .map(Self::Components)
            }
        }
    }
}

#[derive(Clone, Copy)]
enum OversizedFormat {
    Legacy,
    Future(u64),
    Components,
}

fn probe_large_scene(source: impl Read) -> Result<OversizedFormat, SceneError> {
    let token_budget = Cell::new(Some(0));
    let decision = Cell::new(None);
    let mut parser = serde_json::Deserializer::from_reader(ProbeReader {
        source,
        token_budget: &token_budget,
        decision: &decision,
    });
    let result = parser.deserialize_map(OversizedEnvelope {
        token_budget: &token_budget,
        decision: &decision,
    });
    if let Some(decision) = decision.get() {
        return Ok(decision);
    }
    result?;
    Err(SceneError::Invalid("expected a rot-scene document".into()))
}

struct OversizedEnvelope<'a> {
    token_budget: &'a Cell<Option<usize>>,
    decision: &'a Cell<Option<OversizedFormat>>,
}

impl<'de> de::Visitor<'de> for OversizedEnvelope<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a rot-scene envelope")
    }

    fn visit_map<M: de::MapAccess<'de>>(self, mut fields: M) -> Result<(), M::Error> {
        use serde::de::Error;

        let mut known_format = false;
        let mut known_version = false;
        loop {
            self.token_budget.set(Some(0));
            let field = fields.next_key::<String>()?;
            self.token_budget.set(None);
            match field.as_deref() {
                Some("format") => {
                    self.token_budget.set(Some(0));
                    let format = fields.next_value::<String>()?;
                    self.token_budget.set(None);
                    if format != "rot-scene" || known_format {
                        return Err(M::Error::custom("invalid rot-scene format"));
                    }
                    known_format = true;
                }
                Some("format_version") => {
                    if known_version {
                        return Err(M::Error::duplicate_field("format_version"));
                    }
                    self.token_budget.set(Some(0));
                    let version = fields.next_value::<u64>()?;
                    self.token_budget.set(None);
                    match version.cmp(&u64::from(component_scene::FORMAT_VERSION)) {
                        std::cmp::Ordering::Less => {
                            self.decision.set(Some(OversizedFormat::Legacy))
                        }
                        std::cmp::Ordering::Greater => {
                            self.decision.set(Some(OversizedFormat::Future(version)))
                        }
                        std::cmp::Ordering::Equal => known_version = true,
                    }
                }
                Some(_) => {
                    fields.next_value::<de::IgnoredAny>()?;
                }
                None => return Err(M::Error::custom("missing rot-scene format or version")),
            }
            if known_format && known_version {
                self.decision.set(Some(OversizedFormat::Components));
            }
            if self.decision.get().is_some() {
                // Stop serde before it reads any remaining fields. For a
                // v23 candidate the authoritative parse will read them.
                return Err(M::Error::custom("scene format identified"));
            }
        }
    }
}

struct ProbeReader<'a, R> {
    source: R,
    token_budget: &'a Cell<Option<usize>>,
    decision: &'a Cell<Option<OversizedFormat>>,
}

impl<R: Read> Read for ProbeReader<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.decision.get().is_some() {
            return Ok(0);
        }
        let Some(consumed) = self.token_budget.get() else {
            return self.source.read(bytes);
        };
        // Enough for the longest envelope key written as Unicode escapes.
        if consumed == 128 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized scene envelope token",
            ));
        }
        let len = bytes.len().min(1);
        let read = self.source.read(&mut bytes[..len])?;
        if read == 1
            && (consumed != 0 || !matches!(bytes[0], b' ' | b'\r' | b'\n' | b'\t' | b':' | b','))
        {
            self.token_budget.set(Some(consumed + 1));
        }
        Ok(read)
    }
}

impl RuntimeScene {
    pub fn from_snapshot(bytes: &[u8]) -> Result<Self, String> {
        Self::from_document(SceneSnapshot::from_bytes(bytes).map_err(snapshot_error)?)
    }

    fn from_document(snapshot: SceneSnapshot) -> Result<Self, String> {
        match snapshot {
            SceneSnapshot::Legacy(session) => Ok(Self::Behavior(session)),
            SceneSnapshot::Components(scene) => Self::from_components(scene),
        }
    }

    pub fn from_components(scene: Box<ComponentScene>) -> Result<Self, String> {
        Self::from_components_checked(scene).map_err(|error| error.to_string())
    }

    pub fn from_components_checked(scene: Box<ComponentScene>) -> Result<Self, SceneError> {
        let Some(preview) = static_component_preview(&scene)? else {
            return Session::from_component_runtime(&scene)
                .map(Box::new)
                .map(Self::Behavior);
        };
        Ok(Self::StaticHierarchy { scene, preview })
    }

    /// Current draw references, for dependency capture and package validation.
    pub fn draw_scene(&self) -> &Scene {
        match self {
            Self::Behavior(session) => session.scene(),
            Self::StaticHierarchy { preview, .. } => preview.scene(),
        }
    }

    pub fn draw_dependencies(&self) -> Result<DrawDependencies, String> {
        match self {
            Self::Behavior(session) => DrawDependencies::from_scene(session.scene()),
            Self::StaticHierarchy { scene, .. } => DrawDependencies::from_components(scene),
        }
    }

    pub fn sound_asset_references(&self) -> SoundAssetReferences {
        match self {
            Self::Behavior(session) => session.sound_asset_references(),
            Self::StaticHierarchy { .. } => SoundAssetReferences::default(),
        }
    }
}

fn snapshot_error(error: SceneError) -> String {
    match error {
        SceneError::UnsupportedVersion(version) => format!(
            "unsupported scene version {version}. The player supports versions 1 through {}.",
            component_scene::FORMAT_VERSION
        ),
        error => error.to_string(),
    }
}

/// Return the same authored rendering snapshot for Game view and a packaged player.
pub fn static_component_preview(
    scene: &ComponentScene,
) -> Result<Option<Arc<rot_render::component_preview::ComponentPreview>>, SceneError> {
    if !supports_static_preview(scene) {
        return Ok(None);
    }
    if let Some((index, component, view)) = scene.designated_camera() {
        if component.enabled
            && view.followed_target().is_some()
            && rot_scene::component_scene::ObjectHierarchy::build(&scene.objects)?
                .effectively_enabled[index]
        {
            return Ok(None);
        }
    }
    if scene.gameplay_camera.is_some()
        || scene.objects.len() > rot_scene::MAX_OBJECTS
        || scene.objects.iter().any(|object| {
            !object.enabled
                || object.parent.is_some()
                || object
                    .components
                    .iter()
                    .any(|component| match component.kind {
                        Component::Light { .. } => true,
                        Component::MeshRenderer { .. } => !component.enabled,
                        _ => false,
                    })
        })
    {
        return rot_render::component_preview::project(scene)
            .map(Arc::new)
            .map(Some);
    }
    Ok(None)
}

fn supports_static_preview(scene: &ComponentScene) -> bool {
    scene.sources.is_empty()
        && scene.actions == Default::default()
        && scene.hud.is_default()
        && scene.objects.iter().all(|object| {
            object.components.iter().all(|component| {
                !matches!(
                    component.kind,
                    Component::Script { .. } | Component::Animator { .. }
                )
            })
        })
}

/// Assets referenced by mesh renderers, including static meshes that start disabled.
pub struct DrawDependencies {
    pub meshes: BTreeSet<String>,
    pub texture_roles: BTreeMap<String, TextureRole>,
}

impl DrawDependencies {
    pub fn from_scene(scene: &Scene) -> Result<Self, String> {
        let meshes = scene
            .objects
            .iter()
            .filter_map(|object| object.mesh.clone());
        let materials = scene
            .objects
            .iter()
            .filter_map(|object| object.material.as_ref())
            .filter_map(|id| scene.materials.get(id));
        Self::collect(meshes, materials)
    }

    fn from_components(scene: &ComponentScene) -> Result<Self, String> {
        let renderers = || {
            scene.objects.iter().flat_map(|object| {
                object
                    .components
                    .iter()
                    .filter_map(|component| match &component.kind {
                        Component::MeshRenderer {
                            geometry, material, ..
                        } => Some((geometry, material)),
                        _ => None,
                    })
            })
        };
        let meshes = renderers().filter_map(|(geometry, _)| match geometry {
            Geometry::Imported { asset } => Some(asset.clone()),
            _ => None,
        });
        let materials = renderers()
            .filter_map(|(_, material)| material.as_ref())
            .filter_map(|id| scene.materials.get(id));
        Self::collect(meshes, materials)
    }

    fn collect<'a>(
        meshes: impl Iterator<Item = String>,
        materials: impl Iterator<Item = &'a OpaqueMaterial>,
    ) -> Result<Self, String> {
        let meshes: BTreeSet<_> = meshes.collect();
        let texture_roles = texture_roles(materials)?;
        if texture_roles.keys().any(|id| meshes.contains(id)) {
            return Err("Mesh and texture asset IDs must be distinct before export.".into());
        }
        Ok(Self {
            meshes,
            texture_roles,
        })
    }
}

pub fn texture_roles<'a>(
    materials: impl Iterator<Item = &'a OpaqueMaterial>,
) -> Result<BTreeMap<String, TextureRole>, String> {
    let mut roles = BTreeMap::new();
    for material in materials {
        // Inactive slots are still authored launch dependencies.
        for (id, role) in [
            (&material.base_color_texture, TextureRole::BaseColor),
            (&material.roughness_texture, TextureRole::Roughness),
        ] {
            if let Some(id) = id {
                if roles
                    .insert(id.clone(), role)
                    .is_some_and(|old| old != role)
                {
                    return Err(
                        "One texture identity cannot serve conflicting material roles.".into(),
                    );
                }
            }
        }
    }
    Ok(roles)
}

/// Explicit launch framing. Navigation remains interactive after initialization.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InitialCamera {
    pub target: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub orthographic: bool,
}

impl InitialCamera {
    pub fn camera_state(self) -> Result<rot_render::CameraState, String> {
        let state = rot_render::CameraState {
            target: self.target,
            yaw: self.yaw,
            pitch: self.pitch,
            distance: self.distance,
            orthographic: self.orthographic,
        };
        state
            .is_valid()
            .then_some(state)
            .ok_or_else(|| "Invalid application initial_camera".into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreationManifest {
    pub format: String,
    pub version: u32,
    pub title: String,
    pub scene: String,
    #[serde(deserialize_with = "unique_meshes")]
    pub meshes: BTreeMap<String, String>,
    /// Descriptor3 preserves the immutable import interpretation per mesh.
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_normal_policies"
    )]
    pub mesh_normal_policies: BTreeMap<String, rot_assets::MeshNormalPolicy>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "unique_meshes"
    )]
    pub textures: BTreeMap<String, String>,
    /// Descriptor4 requires an explicit interpretation for every texture ID,
    /// including an empty map when there are no textures. Older descriptors omit it.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "unique_optional_mapping"
    )]
    pub texture_roles: Option<BTreeMap<String, rot_assets::TextureRole>>,
    /// Descriptor6 carries the exact authored and retained-history WAV closure.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "unique_optional_mapping"
    )]
    pub sounds: Option<BTreeMap<String, String>>,
    /// Descriptor7 carries the accepted `rot-clip` payload for every clip a
    /// current Scene22 animation binding uses, including an empty map.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "unique_optional_mapping"
    )]
    pub animations: Option<BTreeMap<String, String>>,
    /// Descriptors1–4 keep their original integer-millisecond contract.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_tick"
    )]
    pub tick_millis: Option<u64>,
    /// Descriptor5 and later carry the exact editor/export fixed quantum.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_tick"
    )]
    pub tick_nanos: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_camera: Option<InitialCamera>,
}

// A present null is invalid, rather than becoming an omitted time unit.
fn present_tick<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    u64::deserialize(deserializer).map(Some)
}

fn unique_meshes<'de, D>(deserializer: D) -> Result<BTreeMap<String, String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    unique_mapping(deserializer)
}

fn unique_normal_policies<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, rot_assets::MeshNormalPolicy>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    unique_mapping(deserializer)
}

fn unique_optional_mapping<'de, D, T>(
    deserializer: D,
) -> Result<Option<BTreeMap<String, T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    // A present null must not become an omitted old-descriptor field.
    unique_mapping(deserializer).map(Some)
}

fn unique_mapping<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Unique<T> {
        type Value = BTreeMap<String, T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("unique resource IDs")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> Result<Self::Value, M::Error> {
            let mut values = BTreeMap::new();
            while let Some((id, source)) = map.next_entry::<String, T>()? {
                if values.insert(id, source).is_some() {
                    return Err(serde::de::Error::custom("duplicate resource ID"));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Unique(std::marker::PhantomData))
}

fn resource_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 256
        || Path::new(path).is_absolute()
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(
            "Application resources require bounded relative paths without traversal".into(),
        );
    }
    Ok(())
}

impl CreationManifest {
    pub fn validate(&self) -> Result<(), String> {
        if self.format != FORMAT || !(1..=VERSION).contains(&self.version) {
            return Err(
                "Unsupported application descriptor; expected rot-application version 1 through 7"
                    .into(),
            );
        }
        if self.version == 1 && !self.textures.is_empty() {
            return Err("Application textures require descriptor version 2".into());
        }
        if self.version < 3 && !self.mesh_normal_policies.is_empty() {
            return Err("Mesh normal policies require descriptor version 3".into());
        }
        if self.version >= 3 && !self.meshes.keys().eq(self.mesh_normal_policies.keys()) {
            return Err("Descriptor3 and later require one normal policy per mesh".into());
        }
        if self.version < 4 && self.texture_roles.is_some() {
            return Err("Texture roles require descriptor version 4".into());
        }
        if self.version >= 4 {
            let roles = self
                .texture_roles
                .as_ref()
                .ok_or("Descriptor4 and later require texture_roles, including an empty map")?;
            if !self.textures.keys().eq(roles.keys()) {
                return Err("Descriptor4 and later require exactly one role per texture".into());
            }
        }
        if (self.version == NO_ANIMATION_VERSION && self.sounds.is_none())
            || (self.version < NO_ANIMATION_VERSION && self.sounds.is_some())
        {
            return Err(
                "Descriptor6 requires sounds, including an empty map; descriptors1–5 omit it"
                    .into(),
            );
        }
        if (self.version == VERSION) != self.animations.is_some() {
            return Err(
                "Descriptor7 requires animations, including an empty map; descriptors1–6 omit it"
                    .into(),
            );
        }
        if self.title.trim().is_empty()
            || self.title.trim() != self.title
            || self.title.len() > 128
            || self.title.chars().any(char::is_control)
        {
            return Err("Application title must be 1–128 UTF-8 bytes without controls or surrounding whitespace".into());
        }
        match (self.version, self.tick_millis, self.tick_nanos) {
            (1..=4, Some(millis), None) if (1..=1000).contains(&millis) => {},
            (5..=VERSION, None, Some(nanos)) if (1_000_000..=1_000_000_000).contains(&nanos) => {},
            _ => return Err("Descriptors1–4 require only tick_millis (1–1000); Descriptors5–7 require only tick_nanos (1000000–1000000000).".into()),
        }
        if let Some(camera) = self.initial_camera {
            camera.camera_state()?;
        }
        resource_path(&self.scene)?;
        for (id, path) in self
            .meshes
            .iter()
            .chain(&self.textures)
            .chain(self.sounds.iter().flatten())
            .chain(self.animations.iter().flatten())
        {
            // Use the scene owner's identity grammar rather than a new dialect.
            rot_scene::validate_id(id).map_err(|error| error.to_string())?;
            resource_path(path)?;
        }
        if let Some(sounds) = &self.sounds {
            let mut paths = std::collections::BTreeSet::new();
            for (id, path) in sounds {
                if self.meshes.contains_key(id) || self.textures.contains_key(id) {
                    return Err("Sound IDs cannot overlap mesh or texture mappings".into());
                }
                if !path.ends_with(".wav") || !paths.insert(path) {
                    return Err("Application sounds require unique WAV resource paths".into());
                }
            }
        }
        if let Some(animations) = &self.animations {
            let mut paths = std::collections::BTreeSet::new();
            for (id, path) in animations {
                if self.meshes.contains_key(id)
                    || self.textures.contains_key(id)
                    || self
                        .sounds
                        .as_ref()
                        .is_some_and(|sounds| sounds.contains_key(id))
                {
                    return Err(
                        "Animation clip IDs cannot overlap mesh, texture or sound mappings".into(),
                    );
                }
                if !path.ends_with(".json") || !paths.insert(path) {
                    return Err(
                        "Application animations require unique JSON clip resource paths".into(),
                    );
                }
            }
        }
        Ok(())
    }

    pub fn load(root: &ResourceRoot) -> Result<Self, String> {
        let bytes = root.read(DESCRIPTOR_NAME, MAX_DESCRIPTOR_BYTES)?;
        let raw: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid {DESCRIPTOR_NAME}: {error}"))?;
        if raw["version"].as_u64().is_some_and(|version| version < 4)
            && raw.get("texture_roles").is_some()
        {
            return Err("Texture roles require descriptor version 4".into());
        }
        if raw["version"].as_u64().is_some_and(|version| version < 3)
            && raw.get("mesh_normal_policies").is_some()
        {
            return Err("Mesh normal policies require descriptor version 3".into());
        }
        if raw["version"].as_u64().is_some_and(|version| version < 5)
            && raw.get("tick_nanos").is_some()
        {
            return Err("Nanosecond ticks require descriptor version 5".into());
        }
        if raw["version"].as_u64().is_some_and(|version| version >= 5)
            && raw.get("tick_millis").is_some()
        {
            return Err("Descriptors5–7 use tick_nanos; tick_millis must be absent".into());
        }
        let manifest: Self = serde_json::from_slice(&bytes)
            .map_err(|error| format!("Invalid {DESCRIPTOR_NAME}: {error}"))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn tick_duration(&self) -> Duration {
        match (self.version, self.tick_millis, self.tick_nanos) {
            (1..=4, Some(millis), None) => Duration::from_millis(millis),
            (5..=VERSION, None, Some(nanos)) => Duration::from_nanos(nanos),
            // Invalid caller-constructed DTOs cannot select a fallback unit;
            // load/validate rejects them, and runtime scheduling rejects zero.
            _ => Duration::ZERO,
        }
    }

    pub fn load_runtime_scene(&self, root: &ResourceRoot) -> Result<RuntimeScene, String> {
        self.validate()?;
        let snapshot =
            SceneSnapshot::from_file(root.open_regular(&self.scene)?).map_err(snapshot_error)?;
        self.runtime_scene(snapshot)
    }

    fn runtime_scene(&self, snapshot: SceneSnapshot) -> Result<RuntimeScene, String> {
        let scene = RuntimeScene::from_document(snapshot)?;
        match &scene {
            RuntimeScene::Behavior(session) => self.validate_behavior_scene(session)?,
            RuntimeScene::StaticHierarchy { scene, preview } => {
                if self.initial_camera.is_some() && preview.has_player_camera() {
                    return Err(
                        "Remove legacy initial camera framing when the scene owns a player view."
                            .into(),
                    );
                }
                if self.version != NO_AUDIO_VERSION {
                    return Err("Static component hierarchies require descriptor version 5".into());
                }
                let dependencies = DrawDependencies::from_components(scene)?;
                for id in dependencies.meshes {
                    if !self.meshes.contains_key(&id) {
                        return Err(format!("Application is missing the required mesh {id}."));
                    }
                }
                let roles = self
                    .texture_roles
                    .as_ref()
                    .expect("descriptor roles validated");
                for (id, role) in dependencies.texture_roles {
                    match roles.get(&id) {
                        Some(actual) if *actual == role => {}
                        Some(_) => {
                            return Err(format!(
                            "Application texture {id} has the wrong role for its material slot."
                        ))
                        }
                        None => {
                            return Err(format!(
                                "Application is missing the required texture {id}."
                            ))
                        }
                    }
                }
            }
        }
        Ok(scene)
    }

    pub fn load_session(&self, root: &ResourceRoot) -> Result<Session, String> {
        self.validate()?;
        let snapshot =
            SceneSnapshot::from_file(root.open_regular(&self.scene)?).map_err(snapshot_error)?;
        let session = match snapshot {
            SceneSnapshot::Legacy(session) => *session,
            SceneSnapshot::Components(scene) => {
                Session::from_component_runtime(&scene).map_err(|error| error.to_string())?
            }
        };
        self.validate_behavior_scene(&session)?;
        Ok(session)
    }

    fn validate_behavior_scene(&self, session: &Session) -> Result<(), String> {
        let animated = session.loaded_format_version() >= rot_scene::ANIMATION_FORMAT_VERSION;
        if animated != (self.version >= VERSION) {
            return Err("Scene22 imported_transform_animation requires descriptor version 7, and descriptor7 requires Scene22".into());
        }
        if let Some(animations) = &self.animations {
            let clips: std::collections::BTreeSet<&str> = session
                .scene()
                .animations
                .values()
                .map(|binding| binding.clip.as_str())
                .collect();
            if !animations.keys().map(String::as_str).eq(clips) {
                return Err(
                    "Application animation clips differ from the scene's animation bindings".into(),
                );
            }
        }
        let referenced = session.sound_asset_references();
        match &self.sounds {
            Some(sounds) if referenced.uses_sound && sounds.keys().eq(referenced.assets.iter()) => {
            }
            None if !referenced.uses_sound => {}
            _ => {
                return Err(
                    "Application sound mappings differ from the scene and its retained history"
                        .into(),
                )
            }
        }
        if self.initial_camera.is_some() && session.scene().player_view.is_some() {
            return Err(
                "Remove legacy initial camera framing when the scene owns a player view".into(),
            );
        }
        if session.loaded_format_version() >= 17 && self.version < 5 {
            return Err("Scene17 runtime_hud_panels requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 16 && self.version < 5 {
            return Err("Scene16 runtime_hud_controls requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 15 && self.version < 5 {
            return Err("Scene15 runtime_hud requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 14 && self.version < 5 {
            return Err("Scene14 lifecycle_commands requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 13 && self.version < 5 {
            return Err("Scene13 runtime_state requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 12 && self.version < 5 {
            return Err("Scene12 spatial_queries requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 11 && self.version < 5 {
            return Err("Scene11 input_actions requires descriptor version 5".into());
        }
        if session.loaded_format_version() >= 10 && self.version < 4 {
            return Err("Scene10 roughness_maps requires descriptor version 4".into());
        }
        Ok(())
    }

    pub fn load_sounds(
        &self,
        root: &ResourceRoot,
    ) -> Result<BTreeMap<String, Arc<rot_assets::AudioClip>>, String> {
        self.validate()?;
        let mut sounds = BTreeMap::new();
        let mut total_bytes = 0;
        for (id, path) in self.sounds.iter().flatten() {
            let source = root.read(path, rot_assets::MAX_SOUND_SOURCE_BYTES)?;
            let clip =
                rot_assets::decode_wav(&source).map_err(|error| format!("sound {id}: {error}"))?;
            total_bytes += clip.payload_bytes();
            if total_bytes > rot_assets::MAX_SOUND_RESOLVED_BYTES {
                return Err("Application WAV samples exceed 64 MiB".into());
            }
            sounds.insert(id.clone(), Arc::new(clip));
        }
        Ok(sounds)
    }

    /// Read and strictly validate each accepted `rot-clip` payload. The exact
    /// bytes are returned so callers can both decode and pin them.
    pub fn load_animations(
        &self,
        root: &ResourceRoot,
    ) -> Result<BTreeMap<String, Arc<Vec<u8>>>, String> {
        self.validate()?;
        let mut clips = BTreeMap::new();
        for (id, path) in self.animations.iter().flatten() {
            let bytes = root.read(path, rot_scene::MAX_CLIP_BYTES)?;
            rot_scene::Clip::decode(&bytes)
                .map_err(|error| format!("animation clip {id}: {error}"))?;
            clips.insert(id.clone(), Arc::new(bytes));
        }
        Ok(clips)
    }

    pub fn load_textures(&self, root: &ResourceRoot) -> Result<TextureMap, String> {
        self.validate()?;
        let mut textures = TextureMap::new();
        let mut bytes = 0;
        for (id, path) in &self.textures {
            let role = self
                .texture_roles
                .as_ref()
                .and_then(|roles| roles.get(id))
                .copied()
                .unwrap_or(rot_assets::TextureRole::BaseColor);
            let texture = root.texture_with_role(path, role)?;
            bytes =
                super::checked_texture_bytes(bytes, texture.payload_bytes()).map_err(|error| {
                    format!("texture {id}: {error}; reduce the creation's texture content")
                })?;
            textures.insert(id.clone(), texture);
        }
        Ok(textures)
    }

    pub fn load_meshes(&self, root: &ResourceRoot) -> Result<MeshMap, String> {
        self.validate()?;
        let mut meshes = MeshMap::new();
        let mut bytes = 0;
        for (id, path) in &self.meshes {
            let mesh = root.mesh_with_normal_policy(
                path,
                self.mesh_normal_policies
                    .get(id)
                    .copied()
                    .unwrap_or(rot_assets::MeshNormalPolicy::Flat),
            )?;
            bytes = super::checked_mesh_bytes(bytes, mesh.payload_bytes()).map_err(|error| {
                format!("mesh {id}: {error}; reduce the creation's mesh content")
            })?;
            meshes.insert(id.clone(), mesh);
        }
        Ok(meshes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Content;

    #[test]
    fn behavior_accepts_active_and_dormant_transform_lenses() {
        use rot_scene::{
            component_scene::{CameraView, ComponentInstance},
            BehaviorSource,
        };
        let mut scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.sources.insert(
            "source".into(),
            BehaviorSource {
                label: None,
                language_id: "lua54".into(),
                host_api_version: 1,
                text: "return {}".into(),
            },
        );
        let owner = &mut scene.objects[0];
        owner.components.push(ComponentInstance {
            id: "object-script".into(),
            enabled: true,
            kind: Component::Script {
                source_kind: None,
                binding: rot_scene::BehaviorBinding {
                    target: rot_scene::BindingTarget::Object(owner.id.clone()),
                    source_id: "source".into(),
                    enabled: true,
                    properties: Default::default(),
                    state_defaults: Default::default(),
                    commands: Default::default(),
                },
            },
        });
        scene.objects.push(rot_scene::component_scene::ObjectNode {
            id: "detached-camera".into(),
            name: "Detached Camera".into(),
            enabled: true,
            parent: None,
            transform: rot_scene::Transform {
                translation: [1., 2., 3.],
                ..Default::default()
            },
            components: vec![ComponentInstance {
                id: "lens".into(),
                enabled: true,
                kind: Component::Camera {
                    view: CameraView::transform_perspective(45.),
                },
            }],
        });
        assert!(matches!(
            RuntimeScene::from_components(Box::new(scene.clone())).unwrap(),
            RuntimeScene::Behavior(session) if !session.scene().hierarchy.groups.contains_key("detached-camera")
        ));
        scene.gameplay_camera = Some("lens".into());
        let RuntimeScene::Behavior(session) =
            RuntimeScene::from_components(Box::new(scene.clone())).unwrap()
        else {
            panic!("expected behavior scene");
        };
        assert_eq!(
            session
                .transform_camera_binding()
                .unwrap()
                .resolve(session.scene())
                .unwrap()
                .eye,
            [1., 2., 3.]
        );
        assert!(session.scene().hierarchy.is_empty());
        scene.objects.last_mut().unwrap().components[0].enabled = false;
        assert!(
            matches!(RuntimeScene::from_components(Box::new(scene)).unwrap(),
            RuntimeScene::Behavior(session) if session.transform_camera_binding().is_none() && session.scene().player_view.is_none())
        );
    }

    #[test]
    fn oversized_package_scene_keeps_strict_version_dispatch() {
        let project = tempfile::tempdir().unwrap();
        let root = ResourceRoot::new(project.path()).unwrap();
        let path = project.path().join("scene.json");
        let manifest: CreationManifest = serde_json::from_value(serde_json::json!({
            "format": FORMAT, "version": NO_AUDIO_VERSION, "title": "Roof",
            "scene": "scene.json", "tick_nanos": 16_666_667,
            "meshes": {}, "texture_roles": {}
        }))
        .unwrap();
        let legacy = Session::new(Scene::demo()).unwrap();
        let mut authored = ComponentScene::from_legacy(&legacy).unwrap();
        authored.objects[0].enabled = false;
        let body = serde_json::to_string(&authored).unwrap();
        let limit = rot_scene::MAX_FILE_BYTES as usize;
        let test = |json: String, length: usize| {
            let mut bytes = json.into_bytes();
            bytes.resize(length, b' ');
            std::fs::write(&path, &bytes).unwrap();
            let snapshot = RuntimeScene::from_snapshot(&bytes);
            let packaged = manifest.load_runtime_scene(&root);
            assert_eq!(snapshot.is_ok(), packaged.is_ok());
            packaged
        };
        let envelope = format!(r#"{{"scene":{body},"format_version":23,"format":"rot-scene"}}"#);
        let loaded = test(envelope.clone(), limit + 1).unwrap();
        let RuntimeScene::StaticHierarchy { scene, .. } = loaded else {
            panic!("expected component projection");
        };
        assert_eq!(*scene, authored);
        let legacy_json = String::from_utf8(legacy.snapshot_bytes().unwrap()).unwrap();
        assert!(test(legacy_json.clone(), limit).is_ok());
        assert!(matches!(test(legacy_json, limit + 1), Err(error) if error.contains("1 MiB")));
        assert!(matches!(test(envelope.replace("23,", "24,"), limit + 1),
            Err(error) if error.contains("unsupported scene version 24") && error.contains("1 through 23")));
        assert!(test(
            format!(r#"{{"format_version":23,{}"#, &envelope[1..]),
            limit + 1
        )
        .is_err());
        assert!(test(format!(r#"{{"unknown":true,{}"#, &envelope[1..]), limit + 1).is_err());
        assert!(test(envelope[..envelope.len() - 1].into(), limit + 1).is_err());
    }

    #[test]
    fn oversized_scene_probe_stops_at_the_discriminator() {
        let legacy_prefix = br#"{"format":"rot-scene","format_version":22,"scene":"#;
        let prefix_only =
            |prefix: &[u8]| probe_large_scene(prefix.chain(std::io::repeat(b'x').take(8192)));
        assert!(matches!(
            prefix_only(legacy_prefix),
            Ok(OversizedFormat::Legacy)
        ));
        for prefix in [
            br#"{"format":""#.as_slice(),
            br#"{"format":"rot-scene","format_version":""#,
            br#"""#,
        ] {
            assert!(matches!(prefix_only(prefix), Err(SceneError::Format(error))
                if error.to_string().contains("oversized scene envelope token")
            ));
        }
        let escaped = br#"{"f\u006frmat" :  "r\u006ft-scene", "format_version":23,"scene":"#;
        assert!(matches!(
            prefix_only(escaped),
            Ok(OversizedFormat::Components)
        ));
        let whitespace = " \t\r\n".repeat(512);
        let spaced = format!(
            r#"{whitespace}{{"format":"rot-scene","format_version":{whitespace}23{whitespace},"scene":"#
        );
        assert!(matches!(
            prefix_only(spaced.as_bytes()),
            Ok(OversizedFormat::Components)
        ));
    }

    #[test]
    fn descriptor6_requires_exact_sound_history_and_valid_wav() {
        use rot_scene::{
            BehaviorBinding, BindingTarget, Object, PropertyValue, Scene, Session, SoundRef,
        };
        let project = tempfile::tempdir().unwrap();
        let root = ResourceRoot::new(project.path()).unwrap();
        let scene_path = project.path().join("scene.json");
        let descriptor_path = project.path().join(DESCRIPTOR_NAME);
        let source = project.path().join("cue.wav");
        let mut scene = Scene::empty("audio");
        scene.objects.push(Object::cube("speaker", None));
        let mut session = Session::new(scene).unwrap();
        let mut raw = serde_json::json!({
            "format": FORMAT, "version": NO_AUDIO_VERSION, "title": "Audio",
            "scene": "scene.json", "tick_nanos": 16_666_667,
            "meshes": {}, "texture_roles": {}
        });
        let write = |value: &serde_json::Value| {
            std::fs::write(&descriptor_path, serde_json::to_vec(value).unwrap()).unwrap();
        };
        session.save(&scene_path).unwrap();
        write(&raw);
        let legacy = CreationManifest::load(&root).unwrap();
        assert!(legacy.load_sounds(&root).unwrap().is_empty());
        legacy.load_session(&root).unwrap();
        raw["sounds"] = serde_json::json!({});
        write(&raw);
        assert!(CreationManifest::load(&root).is_err());
        raw["version"] = NO_ANIMATION_VERSION.into();
        write(&raw);
        assert!(CreationManifest::load(&root)
            .unwrap()
            .load_session(&root)
            .is_err());

        let mut binding = BehaviorBinding {
            target: BindingTarget::Object("speaker".into()),
            source_id: "unbound".into(),
            enabled: false,
            properties: BTreeMap::from([(
                "cue".into(),
                PropertyValue::SoundRef(SoundRef {
                    asset_id: None,
                    gain: 1.,
                }),
            )]),
            commands: Default::default(),
            state_defaults: Default::default(),
        };
        let revision = session
            .set_binding(0, "binding", Some(binding.clone()))
            .unwrap();
        session.save(&scene_path).unwrap();
        CreationManifest::load(&root)
            .unwrap()
            .load_session(&root)
            .unwrap();
        assert!(legacy.load_session(&root).is_err());

        binding.properties.insert(
            "cue".into(),
            PropertyValue::SoundRef(SoundRef {
                asset_id: Some("beep".into()),
                gain: 0.5,
            }),
        );
        let revision = session
            .set_binding(revision, "binding", Some(binding))
            .unwrap();
        session.undo(revision).unwrap();
        session.save(&scene_path).unwrap();
        assert!(CreationManifest::load(&root)
            .unwrap()
            .load_session(&root)
            .is_err());
        raw["sounds"] = serde_json::json!({"beep": "cue.wav"});
        write(&raw);
        let manifest = CreationManifest::load(&root).unwrap();
        manifest.load_session(&root).unwrap();
        assert!(manifest.load_sounds(&root).is_err());
        std::fs::write(&source, b"invalid").unwrap();
        assert!(manifest.load_sounds(&root).is_err());
        // PCM16 mono at 8000 Hz, one zero-valued frame.
        std::fs::write(source, b"RIFF\x26\0\0\0WAVEfmt \x10\0\0\0\x01\0\x01\0\x40\x1f\0\0\x80\x3e\0\0\x02\0\x10\0data\x02\0\0\0\0\0").unwrap();
        assert_eq!(
            manifest.load_sounds(&root).unwrap()["beep"].frames.as_ref(),
            [[0.; 2]]
        );
        raw["sounds"] = serde_json::Value::Null;
        write(&raw);
        assert!(CreationManifest::load(&root).is_err());
    }

    // Unique risk: a descriptor hides clips under an older version, delivers a
    // closure that differs from the bindings or an invalid clip payload.
    #[test]
    fn descriptor7_delivers_exact_valid_animation_closure() {
        use rot_scene::{AnimationBinding, Group, Object, Scene, Session, Transform};
        let project = tempfile::tempdir().unwrap();
        let root = ResourceRoot::new(project.path()).unwrap();
        let mut scene = Scene::empty("animated");
        scene.objects.push(Object::cube("arm", None));
        scene.hierarchy.groups.insert(
            "owner".into(),
            Group {
                name: None,
                transform: Transform::default(),
            },
        );
        scene.hierarchy.parents.insert("arm".into(), "owner".into());
        let mut session = Session::new(scene).unwrap();
        let binding = AnimationBinding {
            clip: "glb-clip-swing".into(),
            looping: true,
            targets: BTreeMap::from([("Rig/Arm".to_owned(), "arm".to_owned())]),
        };
        session.set_animation(0, "owner", Some(binding)).unwrap();
        session.save(project.path().join("scene.json")).unwrap();
        let clip = br#"{"format":"rot-clip","version":1,"duration":2.0,"tracks":[{"node":"Rig/Arm","property":"translation","interpolation":"linear","times":[0.0,2.0],"values":[[0.0,0.0,0.0],[2.0,0.0,0.0]]}]}"#;
        std::fs::create_dir(project.path().join("animations")).unwrap();
        std::fs::write(project.path().join("animations/0000.json"), clip).unwrap();
        let mut raw = serde_json::json!({
            "format": FORMAT, "version": VERSION, "title": "Animated",
            "scene": "scene.json", "tick_nanos": 16_666_667,
            "meshes": {}, "texture_roles": {},
            "animations": {"glb-clip-swing": "animations/0000.json"}
        });
        let write = |value: &serde_json::Value| {
            std::fs::write(
                project.path().join(DESCRIPTOR_NAME),
                serde_json::to_vec(value).unwrap(),
            )
            .unwrap();
        };
        write(&raw);
        let manifest = CreationManifest::load(&root).unwrap();
        let reloaded: CreationManifest =
            serde_json::from_slice(&serde_json::to_vec(&manifest).unwrap()).unwrap();
        assert_eq!(reloaded.animations, manifest.animations);
        manifest.load_session(&root).unwrap();
        assert_eq!(
            manifest.load_animations(&root).unwrap()["glb-clip-swing"].as_slice(),
            clip
        );

        raw["version"] = NO_ANIMATION_VERSION.into();
        raw["sounds"] = serde_json::json!({});
        write(&raw);
        assert!(CreationManifest::load(&root)
            .unwrap_err()
            .contains("Descriptor7 requires animations"));
        raw.as_object_mut().unwrap().remove("animations");
        write(&raw);
        assert!(CreationManifest::load(&root)
            .unwrap()
            .load_session(&root)
            .unwrap_err()
            .contains("Scene22"));

        raw["version"] = VERSION.into();
        raw["animations"] = serde_json::json!({});
        write(&raw);
        assert!(CreationManifest::load(&root)
            .unwrap()
            .load_session(&root)
            .unwrap_err()
            .contains("animation clips differ"));
        raw["animations"] = serde_json::json!({"glb-clip-swing": "animations/0000.json"});
        write(&raw);
        std::fs::write(
            project.path().join("animations/0000.json"),
            br#"{"format":"rot-clip","version":1,"duration":2.0,"tracks":[]}"#,
        )
        .unwrap();
        let error = CreationManifest::load(&root)
            .unwrap()
            .load_animations(&root)
            .unwrap_err();
        assert!(error.contains("animation clip glb-clip-swing"), "{error}");
    }

    // Protect descriptor interpretation/resource confinement, not the existing
    // scene, OBJ parser or Lua evaluation matrices.
    #[test]
    fn creator_descriptor_roundtrip_and_read_only_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let scene_path = directory.path().join("scene.json");
        let mut scene = rot_scene::Scene::demo();
        scene.lighting = rot_scene::Lighting::authored_default("sun");
        scene.objects[0].mesh = Some("marker".into());
        let mut session = rot_scene::Session::new(scene).unwrap();
        session.save(&scene_path).unwrap();
        let original = std::fs::read(&scene_path).unwrap();
        std::fs::write(
            directory.path().join("marker.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let manifest = CreationManifest {
            format: FORMAT.into(),
            version: NO_AUDIO_VERSION,
            title: "My creation 世界".into(),
            scene: "scene.json".into(),
            meshes: [("marker".into(), "marker.obj".into())].into(),
            mesh_normal_policies: [("marker".into(), rot_assets::MeshNormalPolicy::Flat)].into(),
            textures: BTreeMap::new(),
            texture_roles: Some(BTreeMap::new()),
            sounds: None,
            animations: None,
            tick_millis: None,
            tick_nanos: Some(16_666_667),
            initial_camera: None,
        };
        std::fs::write(
            directory.path().join(DESCRIPTOR_NAME),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        let root = ResourceRoot::new(directory.path()).unwrap();
        let loaded = CreationManifest::load(&root).unwrap();
        assert_eq!(loaded.title, manifest.title);
        assert!(loaded.initial_camera.is_none());
        assert!(serde_json::to_value(&manifest)
            .unwrap()
            .get("initial_camera")
            .is_none());
        assert_eq!(loaded.tick_duration(), Duration::from_nanos(16_666_667));
        // Descriptor5 remains the nanos carrier for Scene12, including disabled
        // components. Earlier descriptor versions cannot relabel this content.
        let mut spatial: serde_json::Value = serde_json::from_slice(&original).unwrap();
        spatial["format_version"] = serde_json::json!(12);
        spatial["scene"]["objects"][0]["collider"] =
            serde_json::to_value(rot_scene::Collider::default()).unwrap();
        std::fs::write(&scene_path, serde_json::to_vec(&spatial).unwrap()).unwrap();
        assert_eq!(
            loaded.load_session(&root).unwrap().scene().objects[0].collider,
            Some(rot_scene::Collider::default())
        );
        let mut old_spatial = manifest.clone();
        old_spatial.version = 4;
        old_spatial.tick_millis = Some(100);
        old_spatial.tick_nanos = None;
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene12 spatial_queries"));
        let mut state: serde_json::Value = serde_json::from_slice(&original).unwrap();
        state["format_version"] = serde_json::json!(13);
        state["authoring"] = serde_json::json!({"sources":{},"bindings":{
            "application":{"target":{"type":"application"},"source_id":"unfinished",
                "enabled":false,"properties":{},"state_defaults":{
                    "mode":{"type":"text","value":"ready"}}}
        }});
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        let state_session = loaded.load_session(&root).unwrap();
        assert_eq!(state_session.authoring().bindings.len(), 1);
        assert_eq!(state_session.scene(), session.scene());
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene13 runtime_state"));
        state["format_version"] = serde_json::json!(14);
        state["authoring"]["bindings"]["application"]["commands"] =
            serde_json::json!({"restart":{"arguments":{}}});
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(
            loaded
                .load_session(&root)
                .unwrap()
                .binding("application")
                .unwrap()
                .commands
                .len(),
            1
        );
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene14 lifecycle_commands"));
        state["format_version"] = serde_json::json!(15);
        state["authoring"]["hud"] = serde_json::json!({"enabled":true,"elements":[
            {"id":"instructions","source":{"kind":"static","text":"Creator instructions 世界"}}
        ]});
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        let hud_session = loaded.load_session(&root).unwrap();
        assert!(hud_session.authoring().hud.enabled);
        assert_eq!(hud_session.authoring().hud.elements().count(), 1);
        assert_eq!(loaded.version, 5);
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene15 runtime_hud"));
        state["format_version"] = serde_json::json!(16);
        state["authoring"]["hud"]["elements"][0]["source"] = serde_json::json!({
            "kind":"button","caption":"Restart","command":{
                "binding_id":"application","command_id":"restart","arguments":{}}
        });
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(loaded.load_session(&root).unwrap().authoring().hud.enabled);
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene16 runtime_hud_controls"));
        let migrated = loaded.load_session(&root).unwrap();
        state["format_version"] = serde_json::json!(17);
        state["authoring"]["hud"] = serde_json::to_value(&migrated.authoring().hud).unwrap();
        state["authoring"]["hud"]["panels"][0]["anchor"] = serde_json::json!("center");
        std::fs::write(&scene_path, serde_json::to_vec(&state).unwrap()).unwrap();
        let panels = loaded.load_session(&root).unwrap();
        assert_eq!(
            panels.authoring().hud.panels[0].anchor,
            rot_scene::hud::HudAnchor::Center
        );
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene17 runtime_hud_panels"));
        let mut input_scene: serde_json::Value = serde_json::from_slice(&original).unwrap();
        input_scene["format_version"] = serde_json::json!(11);
        std::fs::write(&scene_path, serde_json::to_vec(&input_scene).unwrap()).unwrap();
        loaded.load_session(&root).unwrap();
        assert!(old_spatial
            .load_session(&root)
            .unwrap_err()
            .contains("Scene11 input_actions"));
        std::fs::write(&scene_path, &original).unwrap();
        for version in 1..=4 {
            let mut old = manifest.clone();
            old.version = version;
            old.tick_millis = Some(100);
            old.tick_nanos = None;
            if version < 4 {
                old.texture_roles = None;
            }
            if version < 3 {
                old.mesh_normal_policies.clear();
            }
            old.validate().unwrap();
            assert_eq!(old.tick_duration(), Duration::from_millis(100));
            let mut old_scene: serde_json::Value = serde_json::from_slice(&original).unwrap();
            old_scene["format_version"] = serde_json::json!(if version == 4 { 10 } else { 9 });
            std::fs::write(&scene_path, serde_json::to_vec(&old_scene).unwrap()).unwrap();
            old.load_session(&root).unwrap();
            std::fs::write(&scene_path, &original).unwrap();
            for value in [serde_json::json!(null), serde_json::json!(16_666_667)] {
                let mut raw = serde_json::to_value(&old).unwrap();
                raw["tick_nanos"] = value;
                std::fs::write(
                    directory.path().join(DESCRIPTOR_NAME),
                    serde_json::to_vec(&raw).unwrap(),
                )
                .unwrap();
                assert!(CreationManifest::load(&root).is_err());
            }
        }
        for bad in [
            "millis-null",
            "millis",
            "both",
            "missing",
            "null",
            "fractional",
            "negative",
            "low",
            "high",
        ] {
            let mut raw = serde_json::to_value(&manifest).unwrap();
            match bad {
                "millis-null" => raw["tick_millis"] = serde_json::Value::Null,
                "millis" => {
                    raw.as_object_mut().unwrap().remove("tick_nanos");
                    raw["tick_millis"] = serde_json::json!(100);
                }
                "both" => raw["tick_millis"] = serde_json::json!(100),
                "missing" => {
                    raw.as_object_mut().unwrap().remove("tick_nanos");
                }
                "null" => raw["tick_nanos"] = serde_json::Value::Null,
                "fractional" => raw["tick_nanos"] = serde_json::json!(16_666_667.5),
                "negative" => raw["tick_nanos"] = serde_json::json!(-1),
                "low" => raw["tick_nanos"] = serde_json::json!(999_999),
                _ => raw["tick_nanos"] = serde_json::json!(1_000_000_001),
            }
            std::fs::write(
                directory.path().join(DESCRIPTOR_NAME),
                serde_json::to_vec(&raw).unwrap(),
            )
            .unwrap();
            assert!(CreationManifest::load(&root).is_err(), "{bad}");
        }
        let mut framed = manifest.clone();
        framed.initial_camera = Some(InitialCamera {
            target: [1.0, 2.0, 3.0],
            yaw: 0.25,
            pitch: -0.5,
            distance: 8.0,
            orthographic: true,
        });
        let roundtrip: CreationManifest =
            serde_json::from_slice(&serde_json::to_vec(&framed).unwrap()).unwrap();
        roundtrip.validate().unwrap();
        assert_eq!(roundtrip.initial_camera, framed.initial_camera);
        for distance in [0.0, f32::INFINITY, 1.1e7] {
            framed.initial_camera.as_mut().unwrap().distance = distance;
            assert!(framed.validate().is_err());
        }
        let input = loaded.load_session(&root).unwrap();
        assert_eq!(input.scene().lighting, session.scene().lighting);
        assert!(
            input.path().is_none(),
            "runtime resources confer no save association"
        );
        Content::new(input.scene().clone(), loaded.load_meshes(&root).unwrap()).unwrap();
        assert_eq!(std::fs::read(&scene_path).unwrap(), original);
        // The actual runtime resource path must preserve the frozen import policy.
        std::fs::write(
            directory.path().join("marker.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nvn 0 2 0\nf 1//1 2//1 3//1\n",
        )
        .unwrap();
        let mut smooth = manifest.clone();
        smooth.mesh_normal_policies.insert(
            "marker".into(),
            rot_assets::MeshNormalPolicy::AuthoredWhenComplete,
        );
        assert_eq!(
            smooth.load_meshes(&root).unwrap()["marker"].vertices[0].normal,
            [0., 1., 0.]
        );
        let mut legacy = manifest.clone();
        legacy.version = 2;
        legacy.tick_millis = Some(100);
        legacy.tick_nanos = None;
        legacy.mesh_normal_policies.clear();
        legacy.texture_roles = None;
        assert_eq!(
            legacy.load_meshes(&root).unwrap()["marker"].vertices[0].normal,
            [0., 0., 1.]
        );
        for bad in [
            serde_json::json!(null),
            serde_json::json!({}),
            serde_json::json!({"marker":"flat"}),
        ] {
            let mut raw = serde_json::to_value(&legacy).unwrap();
            raw["mesh_normal_policies"] = bad;
            std::fs::write(
                directory.path().join(DESCRIPTOR_NAME),
                serde_json::to_vec(&raw).unwrap(),
            )
            .unwrap();
            assert!(CreationManifest::load(&root)
                .unwrap_err()
                .contains("version 3"));
        }
        smooth.mesh_normal_policies.clear();
        assert!(smooth.validate().is_err());

        for version in 1..=3 {
            let mut old = manifest.clone();
            old.version = version;
            old.tick_millis = Some(100);
            old.tick_nanos = None;
            old.texture_roles = None;
            if version < 3 {
                old.mesh_normal_policies.clear();
            }
            old.validate().unwrap();
            assert!(old
                .load_session(&root)
                .unwrap_err()
                .contains("Scene17 runtime_hud_panels"));
            // Genuine old documents still load through old descriptors.
            let mut old_scene: serde_json::Value = serde_json::from_slice(&original).unwrap();
            old_scene["format_version"] = serde_json::json!(9);
            std::fs::write(&scene_path, serde_json::to_vec(&old_scene).unwrap()).unwrap();
            old.load_session(&root).unwrap();
            std::fs::write(&scene_path, &original).unwrap();
            for roles in [serde_json::json!(null), serde_json::json!({})] {
                let mut raw = serde_json::to_value(&old).unwrap();
                raw["texture_roles"] = roles;
                std::fs::write(
                    directory.path().join(DESCRIPTOR_NAME),
                    serde_json::to_vec(&raw).unwrap(),
                )
                .unwrap();
                assert!(CreationManifest::load(&root)
                    .unwrap_err()
                    .contains("version 4"));
            }
        }
        let mut missing_roles = manifest.clone();
        missing_roles.texture_roles = None;
        assert!(missing_roles.validate().is_err());
        let mut extra_role = manifest.clone();
        extra_role
            .texture_roles
            .as_mut()
            .unwrap()
            .insert("extra".into(), rot_assets::TextureRole::Roughness);
        assert!(extra_role.validate().is_err());
        let mut textured = manifest.clone();
        textured
            .textures
            .insert("roughness".into(), "roughness.png".into());
        assert!(textured.validate().is_err());
        textured
            .texture_roles
            .as_mut()
            .unwrap()
            .insert("roughness".into(), rot_assets::TextureRole::Roughness);
        textured.validate().unwrap();
        // One RGB PNG exercises descriptor-controlled interpretation through the
        // actual read-only resource loader, with no catalog or cache available.
        let png = b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a\x00\x00\x00\x0d\x49\x48\x44\x52\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90\x77\x53\xde\x00\x00\x00\x0c\x49\x44\x41\x54\x78\x9c\x63\x68\x50\x70\x00\x00\x02\x04\x00\xe1\xa5\xdf\x7f\x0d\x00\x00\x00\x00\x49\x45\x4e\x44\xae\x42\x60\x82";
        std::fs::write(directory.path().join("roughness.png"), png).unwrap();
        let textures = textured.load_textures(&root).unwrap();
        assert_eq!(
            textures["roughness"].role,
            rot_assets::TextureRole::Roughness
        );
        assert_eq!(textures["roughness"].mips[0].bytes, [128]);
        let mut old_color = textured.clone();
        old_color.version = 3;
        old_color.tick_millis = Some(100);
        old_color.tick_nanos = None;
        old_color.texture_roles = None;
        assert_eq!(
            old_color.load_textures(&root).unwrap()["roughness"].role,
            rot_assets::TextureRole::BaseColor
        );
        let serialized = serde_json::to_string(&textured).unwrap();
        let duplicate = serialized.replace(
            "\"roughness\":\"roughness_linear\"",
            "\"roughness\":\"roughness_linear\",\"roughness\":\"base_color_srgb\"",
        );
        assert!(serde_json::from_str::<CreationManifest>(&duplicate).is_err());

        // Each loader must reject its crossing resource before reaching a
        // later resource failure. The public admission fixture journeys cover
        // both suffix variants and observe their actual read/decode sequence.
        let mut large_png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut large_png, 2048, 2048);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&vec![128; 2048 * 2048 * 3])
                .unwrap();
        }
        assert!(large_png.len() <= rot_assets::MAX_TEXTURE_SOURCE_BYTES);
        std::fs::write(directory.path().join("large.png"), large_png).unwrap();
        std::fs::write(directory.path().join("after-budget.png"), b"malformed PNG").unwrap();
        let mut over_textures = manifest.clone();
        over_textures.meshes.clear();
        over_textures.mesh_normal_policies.clear();
        over_textures.textures = (1..=4)
            .map(|index| (format!("t{index:02}"), "large.png".into()))
            .collect();
        over_textures
            .textures
            .insert("t05".into(), "after-budget.png".into());
        over_textures.texture_roles = Some(
            over_textures
                .textures
                .keys()
                .map(|id| (id.clone(), rot_assets::TextureRole::BaseColor))
                .collect(),
        );
        let texture_error = over_textures.load_textures(&root).unwrap_err();

        let large_obj = "v 0 0 0\nv 1 0 0\nv 0 1 0\nvt 0 0\nvt 1 0\nvt 0 1\n".to_owned()
            + &"f 1/1 2/2 3/3\n".repeat(rot_assets::MAX_TRIANGLES);
        assert!(large_obj.len() <= rot_assets::MAX_SOURCE_BYTES);
        std::fs::write(directory.path().join("large.obj"), large_obj).unwrap();
        let mut over_meshes = manifest.clone();
        over_meshes.textures.clear();
        over_meshes.texture_roles = Some(BTreeMap::new());
        over_meshes.meshes = (1..=15)
            .map(|index| (format!("m{index:02}"), "large.obj".into()))
            .collect();
        over_meshes
            .meshes
            .insert("m16".into(), "after-budget.obj".into());
        over_meshes.mesh_normal_policies = over_meshes
            .meshes
            .keys()
            .map(|id| (id.clone(), rot_assets::MeshNormalPolicy::Flat))
            .collect();
        let mesh_error = over_meshes.load_meshes(&root).unwrap_err();
        assert!(
            texture_error.contains("64 MiB")
                && texture_error.contains("t04")
                && mesh_error.contains("48 MiB")
                && mesh_error.contains("m15"),
            "first-crossing texture error {texture_error:?}; mesh error {mesh_error:?}"
        );
        assert!(!directory.path().join(".rot-assets.json").exists());
        assert!(!directory.path().join(".rot-cache").exists());

        for path in [
            "../outside.json",
            "/outside.json",
            "./scene.json",
            "x/../scene.json",
            "x\\scene.json",
        ] {
            let mut invalid = manifest.clone();
            invalid.scene = path.into();
            assert!(invalid.validate().is_err(), "{path}");
        }
        for tick in [0, 999_999, 1_000_000_001] {
            let mut invalid = manifest.clone();
            invalid.tick_nanos = Some(tick);
            assert!(invalid.validate().is_err());
        }
        let mut invalid = manifest.clone();
        invalid.version = VERSION + 1;
        assert!(invalid.validate().is_err());
        let mut legacy = manifest.clone();
        legacy.version = 1;
        legacy.tick_millis = Some(100);
        legacy.tick_nanos = None;
        legacy.mesh_normal_policies.clear();
        legacy.texture_roles = None;
        legacy
            .validate()
            .expect("old descriptors still read without textures");
        legacy
            .textures
            .insert("base-color".into(), "color.png".into());
        assert!(
            legacy.validate().is_err(),
            "texture resources must not hide under descriptor1"
        );
        legacy.version = NO_AUDIO_VERSION;
        legacy.tick_millis = None;
        legacy.tick_nanos = manifest.tick_nanos;
        legacy.mesh_normal_policies = manifest.mesh_normal_policies.clone();
        legacy.texture_roles =
            Some([("base-color".into(), rot_assets::TextureRole::BaseColor)].into());
        legacy.validate().unwrap();
        let json = serde_json::to_string(&manifest).unwrap();
        let duplicate = json.replace(
            "\"marker\":\"marker.obj\"",
            "\"marker\":\"marker.obj\",\"marker\":\"other.obj\"",
        );
        assert!(serde_json::from_str::<CreationManifest>(&duplicate).is_err());
        let unknown = json.trim_end_matches('}').to_owned() + ",\"unknown\":true}";
        assert!(serde_json::from_str::<CreationManifest>(&unknown).is_err());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::fs::write(outside.path().join("escape.obj"), b"not a mesh").unwrap();
            std::os::unix::fs::symlink(
                outside.path().join("escape.obj"),
                directory.path().join("alias.obj"),
            )
            .unwrap();
            let mut escape = manifest;
            escape.meshes.insert("marker".into(), "alias.obj".into());
            assert!(escape.load_meshes(&root).is_err());
        }
    }
}
