//! Application-neutral authored objects; no logistics, player or runtime state.
//!
//! Public values permit code-first construction. Call `validate` at ingestion;
//! [`Session`] validates transactional edits and owns bounded persisted history.

use std::collections::{BTreeMap, HashSet};
use std::ffi::OsString;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 16;
pub const MAX_MATERIALS: usize = 64;

mod authoring;
mod collider;
pub mod hud;
pub mod input;
mod lighting;
mod lighting_authoring;
#[path = "../../support/strict_json.rs"]
mod strict_json;
mod surface;
pub use authoring::*;
pub use collider::{Collider, ColliderRole, ColliderShape, ResolvedCollider, ResolvedShape};
pub use lighting::*;
pub use lighting_authoring::LightingEdit;
pub use surface::{DisplayProfile, MaterialResponse};
pub const MAX_OBJECTS: usize = 256;
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
pub const HISTORY_LIMIT: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Euler rotation in radians, fixed-axis X then Y then Z (world qz*qy*qx).
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
        }
    }
}

impl Transform {
    pub fn validate(&self) -> Result<(), SceneError> {
        if !self
            .translation
            .iter()
            .all(|x| x.is_finite() && x.abs() <= 10_000.0)
            || !self
                .rotation
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 10_000.0)
            || !self
                .scale
                .iter()
                .all(|x| x.is_finite() && *x >= 0.001 && *x <= 1_000.0)
        {
            return Err(SceneError::Invalid("transform requires finite translation ±10000, rotation ±10000 radians and scale 0.001..1000".into()));
        }
        Ok(())
    }
}

/// Supported base-color texture sampling. Scene storage owns no image/GPU payload.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureFilter {
    Nearest,
    #[default]
    Linear,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextureAddress {
    #[default]
    Repeat,
    Clamp,
}
/// Reusable scene-owned opaque material. Tint is linear RGB; external texture
/// availability is diagnosed by asset-aware callers, not silently repaired here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpaqueMaterial {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub tint: [f32; 3],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_color_texture: Option<String>,
    /// Linear roughness-data asset; retained when its surface response is inactive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness_texture: Option<String>,
    pub filter: TextureFilter,
    pub address: TextureAddress,
    #[serde(default, skip_serializing_if = "MaterialResponse::is_legacy")]
    pub response: MaterialResponse,
}
impl Default for OpaqueMaterial {
    fn default() -> Self {
        Self {
            name: None,
            tint: [1.0; 3],
            base_color_texture: None,
            roughness_texture: None,
            filter: TextureFilter::Linear,
            address: TextureAddress::Repeat,
            response: MaterialResponse::LegacyDiffuse,
        }
    }
}
impl OpaqueMaterial {
    pub fn validate(&self) -> Result<(), SceneError> {
        validate_name(self.name.as_deref())?;
        if !self
            .tint
            .iter()
            .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
        {
            return Err(SceneError::Invalid(
                "material tint must be finite in 0..=1".into(),
            ));
        }
        if let Some(id) = &self.base_color_texture {
            validate_id(id)?;
        }
        if let Some(id) = &self.roughness_texture {
            validate_id(id)?;
        }
        self.response.validate()?;
        Ok(())
    }
    pub fn display_name<'a>(&'a self, id: &'a str) -> &'a str {
        self.name.as_deref().unwrap_or(id)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Object {
    pub id: String,
    /// Optional display label; identity remains immutable and references use `id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub transform: Transform,
    /// Linear RGBA channels in 0..=1.
    pub color: [f32; 4],
    /// Stable asset ID; None selects the procedural unit cube.
    /// Asset existence is checked by the asset/editor boundary, not this crate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mesh: Option<String>,
    /// Scene-owned material identity; None preserves legacy object color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Explicit primitive query component, independent of the visible mesh.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collider: Option<Collider>,
}

impl Object {
    pub fn cube(id: impl Into<String>, name: Option<String>) -> Self {
        Self {
            id: id.into(),
            name,
            transform: Transform::default(),
            color: [0.6, 0.6, 0.6, 1.0],
            mesh: None,
            material: None,
            collider: None,
        }
    }
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.id)
    }
    pub fn validate(&self) -> Result<(), SceneError> {
        validate_id(&self.id)?;
        validate_name(self.name.as_deref())?;
        if let Some(collider) = self.collider {
            collider.resolve(&self.transform)?;
        } else {
            self.transform.validate()?;
        }
        if let Some(mesh) = &self.mesh {
            validate_id(mesh)?;
        }
        if let Some(id) = &self.material {
            validate_id(id)?;
        }
        validate_color(&self.color)
    }
}

fn validate_color(color: &[f32; 4]) -> Result<(), SceneError> {
    if color
        .iter()
        .all(|x| x.is_finite() && (0.0..=1.0).contains(x))
    {
        Ok(())
    } else {
        Err(SceneError::Invalid(
            "color channels must be finite in 0..=1".into(),
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    pub id: String,
    pub revision: u64,
    pub objects: Vec<Object>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub materials: BTreeMap<String, OpaqueMaterial>,
    #[serde(default, skip_serializing_if = "Lighting::is_legacy")]
    pub lighting: Lighting,
    #[serde(default, skip_serializing_if = "DisplayProfile::is_legacy")]
    pub display: DisplayProfile,
}

impl Scene {
    pub fn empty(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            revision: 0,
            objects: Vec::new(),
            materials: BTreeMap::new(),
            lighting: Lighting::Legacy,
            display: DisplayProfile::LegacySdr,
        }
    }

    /// New editor documents have a visible, editable sun; legacy/code-first
    /// callers of `empty` retain their existing fixed-lighting appearance.
    pub fn authored_empty(id: impl Into<String>) -> Self {
        let mut scene = Self::empty(id);
        scene.lighting = Lighting::authored_default("light-1");
        scene
    }

    pub fn demo() -> Self {
        Self {
            id: "procedural-demo".into(),
            revision: 0,
            materials: BTreeMap::new(),
            lighting: Lighting::Legacy,
            display: DisplayProfile::LegacySdr,
            objects: [
                ("left", [-2.0, 0.0, 0.0], [0.12, 0.58, 0.85, 1.0]),
                ("center", [0.0, 0.0, 0.0], [0.9, 0.4, 0.16, 1.0]),
                ("right", [2.0, 0.0, 0.0], [0.3, 0.75, 0.45, 1.0]),
            ]
            .into_iter()
            .map(|(id, translation, color)| Object {
                id: id.into(),
                name: None,
                transform: Transform {
                    translation,
                    ..Transform::default()
                },
                color,
                mesh: None,
                material: None,
                collider: None,
            })
            .collect(),
        }
    }

    pub fn validate(&self) -> Result<(), SceneError> {
        validate_id(&self.id)?;
        if self.objects.len() > MAX_OBJECTS {
            return Err(SceneError::Invalid(format!(
                "scene exceeds {MAX_OBJECTS} objects"
            )));
        }
        if self.materials.len() > MAX_MATERIALS {
            return Err(SceneError::Invalid(format!(
                "scene exceeds {MAX_MATERIALS} materials"
            )));
        }
        self.lighting.validate()?;
        self.display.validate()?;
        if self.objects.len() + self.lighting.light_count() > MAX_OBJECTS {
            return Err(SceneError::Invalid(format!(
                "scene exceeds {MAX_OBJECTS} geometry and light targets"
            )));
        }
        for (id, material) in &self.materials {
            validate_id(id)?;
            material.validate()?;
        }
        let mut ids = HashSet::new();
        for object in &self.objects {
            validate_id(&object.id)?;
            if !ids.insert(object.id.as_str()) {
                return Err(SceneError::Invalid(format!(
                    "duplicate object id {}",
                    object.id
                )));
            }
            object.validate()?;
            if object
                .material
                .as_ref()
                .is_some_and(|id| !self.materials.contains_key(id))
            {
                return Err(SceneError::Invalid(format!(
                    "object {} references missing material",
                    object.id
                )));
            }
        }
        for (id, _) in self.lighting.lights() {
            if !ids.insert(id) {
                return Err(SceneError::Invalid(format!(
                    "duplicate scene target id {id}"
                )));
            }
        }
        Ok(())
    }

    /// Deterministic noncryptographic FNV-1a checksum of canonical JSON field order.
    /// Fixed-size comparison evidence, never an integrity/security boundary.
    pub fn fingerprint(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("Scene fields serialize to JSON");
        let hash = bytes.into_iter().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        format!("{hash:016x}")
    }

    pub fn target_count(&self) -> usize {
        self.objects.len() + self.lighting.light_count()
    }

    /// A borrowed authoring lookup; geometry storage and runtime instances stay
    /// separate from the small non-geometric light collection.
    pub fn target_transform(&self, id: &str) -> Option<&Transform> {
        self.objects
            .iter()
            .find(|object| object.id == id)
            .map(|object| &object.transform)
            .or_else(|| self.lighting.light(id).map(|light| &light.transform))
    }

    pub fn target_display_name(&self, id: &str) -> Option<&str> {
        self.objects
            .iter()
            .find(|object| object.id == id)
            .map(Object::display_name)
            .or_else(|| {
                self.lighting
                    .lights()
                    .find(|(key, _)| *key == id)
                    .map(|(key, light)| light.display_name(key))
            })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let document = Document::load(path.as_ref())?;
        if !document.authoring.is_empty()
            || document
                .undo
                .iter()
                .chain(&document.redo)
                .any(Change::has_authoring)
        {
            return Err(SceneError::Invalid("document contains authoring; use Session::open and an authoring-aware runtime, or explicitly export Session::scene() as geometry".into()));
        }
        Ok(document.scene)
    }

    /// Atomic replacement; caller manages concurrency. History starts empty.
    /// Prefer Session::save when authoring or sharing a file with other writers.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        Document::new(self.clone()).write(path.as_ref(), false)
    }
}

fn validate_name(name: Option<&str>) -> Result<(), SceneError> {
    if name.is_some_and(|n| {
        n.is_empty() || n.len() > 128 || n.trim() != n || n.chars().any(char::is_control)
    }) {
        return Err(SceneError::Invalid("display name requires 1..128 UTF-8 bytes without surrounding whitespace or control characters".into()));
    }
    Ok(())
}

/// Validate the shared stable object/source/mesh identifier grammar.
/// Display names are separate and may contain Unicode and spaces.
pub fn validate_id(id: &str) -> Result<(), SceneError> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err(SceneError::Invalid(
            "id requires 1..128 ASCII letters, digits, '-', '_' or '.'".into(),
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub enum SceneError {
    Io(std::io::Error),
    Format(serde_json::Error),
    UnsupportedVersion(u64),
    Invalid(String),
    /// The validated authored payload and required history exceed the serialized budget.
    Capacity,
    StaleRevision {
        expected: u64,
        actual: u64,
    },
    RevisionOverflow,
    TickOverflow,
    ObjectNotFound(String),
    LightNotFound(String),
    NoUndo,
    NoRedo,
    Conflict,
    Busy,
}

impl fmt::Display for SceneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "scene I/O failed: {error}"),
            Self::Format(error) => write!(f, "invalid scene JSON: {error}"),
            Self::UnsupportedVersion(version) => write!(
                f,
                "unsupported scene version {version}; supported versions are 1 through {FORMAT_VERSION}"
            ),
            Self::Invalid(reason) => write!(f, "invalid scene: {reason}"),
            Self::Capacity => write!(f, "authored state and required reversible history exceed 1 MiB; reduce content or edit size before retrying"),
            Self::StaleRevision { expected, actual } => write!(
                f,
                "stale revision: expected {expected}, current {actual}; inspect and retry"
            ),
            Self::TickOverflow => write!(f, "preview tick exhausted; no step applied"),
            Self::RevisionOverflow => write!(f, "scene revision exhausted; no change applied"),
            Self::ObjectNotFound(id) => write!(f, "object {id} does not exist"),
            Self::LightNotFound(id) => write!(f, "light {id} does not exist"),
            Self::NoUndo => write!(f, "no authored change to undo"),
            Self::NoRedo => write!(f, "no authored change to redo"),
            Self::Conflict => write!(
                f,
                "scene changed on disk or destination exists; reopen or save to a new path"
            ),
            Self::Busy => write!(
                f,
                "scene is busy in another cooperating writer; retry later"
            ),
        }
    }
}
impl std::error::Error for SceneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Format(e) => Some(e),
            _ => None,
        }
    }
}
impl From<std::io::Error> for SceneError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for SceneError {
    fn from(e: serde_json::Error) -> Self {
        Self::Format(e)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TransformDelta {
    id: String,
    before: Transform,
    after: Transform,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct IndexedObject {
    object: Object,
    index: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SceneDelta {
    Transforms {
        changes: Vec<TransformDelta>,
    },
    Objects {
        created: Vec<IndexedObject>,
        deleted: Vec<IndexedObject>,
    },
    Create {
        object: Object,
        index: usize,
    },
    Delete {
        object: Object,
        index: usize,
    },
    Name {
        id: String,
        before: Option<String>,
        after: Option<String>,
    },
    Transform {
        id: String,
        before: Transform,
        after: Transform,
    },
    Mesh {
        id: String,
        before: Option<String>,
        after: Option<String>,
    },
    Material {
        id: String,
        before: Option<String>,
        after: Option<String>,
    },
    Collider {
        id: String,
        before: Option<Collider>,
        after: Option<Collider>,
    },
    Color {
        id: String,
        before: [f32; 4],
        after: [f32; 4],
    },
    Lighting {
        before: Lighting,
        after: Lighting,
    },
    Display {
        before: DisplayProfile,
        after: DisplayProfile,
    },
}

impl SceneDelta {
    fn references_asset(&self) -> bool {
        match self {
            Self::Mesh { before, after, .. } => before.is_some() || after.is_some(),
            Self::Create { object, .. } | Self::Delete { object, .. } => object.mesh.is_some(),
            Self::Objects { created, deleted } => created
                .iter()
                .chain(deleted)
                .any(|e| e.object.mesh.is_some()),
            _ => false,
        }
    }
    fn contains_id(&self, id: &str) -> bool {
        match self {
            Self::Create { object, .. } | Self::Delete { object, .. } => object.id == id,
            Self::Name { id: old, .. }
            | Self::Transform { id: old, .. }
            | Self::Color { id: old, .. }
            | Self::Mesh { id: old, .. }
            | Self::Material { id: old, .. }
            | Self::Collider { id: old, .. } => old == id,
            Self::Transforms { changes } => changes.iter().any(|e| e.id == id),
            Self::Objects { created, deleted } => {
                created.iter().chain(deleted).any(|e| e.object.id == id)
            }
            Self::Lighting { before, after } => {
                before.light(id).is_some() || after.light(id).is_some()
            }
            Self::Display { .. } => false,
        }
    }
    fn apply(&self, scene: &mut Scene, undo: bool) -> Result<(), SceneError> {
        let inconsistent = || SceneError::Invalid("inconsistent authored history".into());
        match self {
            Self::Collider { id, before, after } => {
                validate_id(id)?;
                for collider in [before, after].into_iter().flatten() {
                    collider.validate()?;
                }
                let object = scene
                    .objects
                    .iter_mut()
                    .find(|object| &object.id == id)
                    .ok_or_else(inconsistent)?;
                let (expected, replacement) = if undo {
                    (after, before)
                } else {
                    (before, after)
                };
                if before == after || &object.collider != expected {
                    return Err(inconsistent());
                }
                // Coupled transform/component edits validate the complete state
                // in Change::apply, preserving atomic grouped history.
                object.collider = *replacement;
            }
            Self::Display { before, after } => {
                before.validate()?;
                after.validate()?;
                let (expected, replacement) = if undo {
                    (after, before)
                } else {
                    (before, after)
                };
                if before == after || &scene.display != expected {
                    return Err(inconsistent());
                }
                scene.display = *replacement;
            }
            Self::Lighting { before, after } => {
                before.validate()?;
                after.validate()?;
                if before.lights().any(|(id, light)| {
                    after.light(id).is_some_and(|other| {
                        std::mem::discriminant(&light.kind) != std::mem::discriminant(&other.kind)
                    })
                }) {
                    return Err(inconsistent());
                }
                let (expected, replacement) = if undo {
                    (after, before)
                } else {
                    (before, after)
                };
                if before == after || &scene.lighting != expected {
                    return Err(inconsistent());
                }
                scene.lighting = replacement.clone();
            }
            Self::Transforms { changes } => {
                if changes.is_empty() || changes.len() > MAX_OBJECTS {
                    return Err(inconsistent());
                }
                let mut ids = HashSet::new();
                for c in changes {
                    if !ids.insert(&c.id) {
                        return Err(inconsistent());
                    }
                    Self::Transform {
                        id: c.id.clone(),
                        before: c.before.clone(),
                        after: c.after.clone(),
                    }
                    .apply(scene, undo)?;
                }
            }
            Self::Objects { created, deleted } => {
                if (created.is_empty() == deleted.is_empty())
                    || created.len() + deleted.len() > MAX_OBJECTS
                {
                    return Err(inconsistent());
                }
                let entries = if created.is_empty() { deleted } else { created };
                let mut ids = HashSet::new();
                for (i, e) in entries.iter().enumerate() {
                    e.object.validate()?;
                    if !ids.insert(&e.object.id) || i > 0 && entries[i - 1].index >= e.index {
                        return Err(inconsistent());
                    }
                }
                let insert = !created.is_empty() != undo;
                if insert {
                    for e in entries {
                        Self::Create {
                            object: e.object.clone(),
                            index: e.index,
                        }
                        .apply(scene, false)?;
                    }
                } else {
                    for e in entries.iter().rev() {
                        Self::Delete {
                            object: e.object.clone(),
                            index: e.index,
                        }
                        .apply(scene, false)?;
                    }
                }
            }
            Self::Create { object, index } | Self::Delete { object, index } => {
                object.validate()?;
                let insert = matches!(self, Self::Create { .. }) != undo;
                if insert {
                    if scene.objects.len() >= MAX_OBJECTS
                        || *index > scene.objects.len()
                        || scene.objects.iter().any(|o| o.id == object.id)
                    {
                        return Err(inconsistent());
                    }
                    scene.objects.insert(*index, object.clone());
                } else {
                    if scene.objects.get(*index) != Some(object) {
                        return Err(inconsistent());
                    }
                    scene.objects.remove(*index);
                }
            }
            Self::Name { id, before, after } => {
                validate_id(id)?;
                validate_name(before.as_deref())?;
                validate_name(after.as_deref())?;
                let object = scene
                    .objects
                    .iter_mut()
                    .find(|o| &o.id == id)
                    .ok_or_else(inconsistent)?;
                let (expected, replacement) = if undo {
                    (after, before)
                } else {
                    (before, after)
                };
                if before == after || &object.name != expected {
                    return Err(inconsistent());
                }
                object.name = replacement.clone();
            }
            Self::Transform { id, before, after } => {
                validate_id(id)?;
                before.validate()?;
                after.validate()?;
                let object = scene
                    .objects
                    .iter_mut()
                    .find(|o| &o.id == id)
                    .ok_or_else(inconsistent)?;
                let (expected, replacement) = if undo {
                    (after, before)
                } else {
                    (before, after)
                };
                if before == after || &object.transform != expected {
                    return Err(inconsistent());
                }
                object.transform = replacement.clone();
            }
            Self::Color { id, before, after } => {
                validate_id(id)?;
                validate_color(before)?;
                validate_color(after)?;
                let object = scene
                    .objects
                    .iter_mut()
                    .find(|o| &o.id == id)
                    .ok_or_else(inconsistent)?;
                let (expected, replacement) = if undo {
                    (after, before)
                } else {
                    (before, after)
                };
                if before == after || &object.color != expected {
                    return Err(inconsistent());
                }
                object.color = *replacement;
            }
            Self::Material { id, before, after } => {
                validate_id(id)?;
                for material in [before, after].into_iter().flatten() {
                    validate_id(material)?;
                }
                let object = scene
                    .objects
                    .iter_mut()
                    .find(|o| &o.id == id)
                    .ok_or_else(inconsistent)?;
                let (expected, replacement) = if undo {
                    (after, before)
                } else {
                    (before, after)
                };
                if before == after || &object.material != expected {
                    return Err(inconsistent());
                }
                object.material = replacement.clone();
            }
            Self::Mesh { id, before, after } => {
                validate_id(id)?;
                for mesh in [before, after].into_iter().flatten() {
                    validate_id(mesh)?;
                }
                let object = scene
                    .objects
                    .iter_mut()
                    .find(|o| &o.id == id)
                    .ok_or_else(inconsistent)?;
                let (expected, replacement) = if undo {
                    (after, before)
                } else {
                    (before, after)
                };
                if before == after || &object.mesh != expected {
                    return Err(inconsistent());
                }
                object.mesh = replacement.clone();
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValueDelta<T> {
    id: String,
    before: Option<T>,
    after: Option<T>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HudDelta {
    before: hud::Hud,
    after: hud::Hud,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    scene: Vec<SceneDelta>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    materials: Vec<ValueDelta<OpaqueMaterial>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    sources: Vec<ValueDelta<BehaviorSource>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bindings: Vec<ValueDelta<BehaviorBinding>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    actions: Vec<ValueDelta<input::ActionDefinition>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hud: Option<HudDelta>,
}
impl From<SceneDelta> for Change {
    fn from(delta: SceneDelta) -> Self {
        Self {
            scene: vec![delta],
            ..Self::default()
        }
    }
}
fn apply_values<T: Clone + PartialEq>(
    deltas: &[ValueDelta<T>],
    values: &mut BTreeMap<String, T>,
    undo: bool,
) -> Result<(), SceneError> {
    let mut ids = HashSet::new();
    for delta in deltas {
        validate_id(&delta.id)?;
        let (expected, replacement) = if undo {
            (&delta.after, &delta.before)
        } else {
            (&delta.before, &delta.after)
        };
        if !ids.insert(&delta.id)
            || delta.before == delta.after
            || values.get(&delta.id) != expected.as_ref()
        {
            return Err(SceneError::Invalid(
                "inconsistent authored value history".into(),
            ));
        }
        if let Some(value) = replacement {
            values.insert(delta.id.clone(), value.clone());
        } else {
            values.remove(&delta.id);
        }
    }
    Ok(())
}
impl Change {
    fn has_authoring(&self) -> bool {
        !self.sources.is_empty()
            || !self.bindings.is_empty()
            || !self.actions.is_empty()
            || self.hud.is_some()
    }
    fn hud_values(&self) -> impl Iterator<Item = &hud::Hud> {
        self.hud
            .iter()
            .flat_map(|delta| [&delta.before, &delta.after])
    }
    fn references_asset(&self) -> bool {
        self.scene.iter().any(SceneDelta::references_asset)
            || self
                .materials
                .iter()
                .flat_map(|d| [&d.before, &d.after])
                .flatten()
                .any(|m| m.base_color_texture.is_some() || m.roughness_texture.is_some())
            || self
                .bindings
                .iter()
                .flat_map(|d| [&d.before, &d.after])
                .flatten()
                .any(|b| {
                    b.properties
                        .values()
                        .any(|v| matches!(v, PropertyValue::AssetRef(Some(_))))
                })
    }
    fn object_ids(&self) -> impl Iterator<Item = &str> {
        self.bindings
            .iter()
            .flat_map(|d| [&d.before, &d.after])
            .flatten()
            .flat_map(|b| {
                b.object_id()
                    .into_iter()
                    .chain(b.properties.values().filter_map(|v| match v {
                        PropertyValue::ObjectRef(Some(id)) => Some(id.as_str()),
                        _ => None,
                    }))
            })
    }
    fn contains_id(&self, id: &str) -> bool {
        self.scene.iter().any(|d| d.contains_id(id)) || self.object_ids().any(|used| used == id)
    }
    fn apply(
        &self,
        scene: &mut Scene,
        authoring: &mut Authoring,
        undo: bool,
    ) -> Result<(), SceneError> {
        if self.scene.is_empty() && self.materials.is_empty() && !self.has_authoring()
            || self.scene.len() > MAX_OBJECTS
            || self.materials.len() > MAX_MATERIALS
            || self.sources.len() > MAX_SOURCES
            || self.bindings.len() > MAX_BINDINGS
            || self.actions.len() > input::MAX_ACTIONS
        {
            return Err(SceneError::Invalid(
                "empty or oversized authored history change".into(),
            ));
        }
        if !undo {
            apply_values(&self.materials, &mut scene.materials, false)?;
        }
        if undo {
            for delta in self.scene.iter().rev() {
                delta.apply(scene, true)?;
            }
        } else {
            for delta in &self.scene {
                delta.apply(scene, false)?;
            }
        }
        if undo {
            apply_values(&self.materials, &mut scene.materials, true)?;
        }
        // All sections form one atomic authored state. Validate only after all apply.
        apply_values(&self.sources, &mut authoring.sources, undo)?;
        apply_values(&self.bindings, &mut authoring.bindings, undo)?;
        apply_values(&self.actions, &mut authoring.actions, undo)?;
        if let Some(delta) = &self.hud {
            delta.before.validate()?;
            delta.after.validate()?;
            let (expected, replacement) = if undo {
                (&delta.after, &delta.before)
            } else {
                (&delta.before, &delta.after)
            };
            if delta.before == delta.after || &authoring.hud != expected {
                return Err(SceneError::Invalid("inconsistent HUD history".into()));
            }
            authoring.hud = replacement.clone();
        }
        scene.validate()?;
        authoring.validate(scene)
    }
}

// Decode v1 through its exact schema: even `mesh: null` must be rejected.
// Its untagged transform history is migrated without losing undo or redo.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyObject {
    id: String,
    transform: Transform,
    color: [f32; 4],
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyScene {
    id: String,
    revision: u64,
    objects: Vec<LegacyObject>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacySceneDelta {
    id: String,
    before: Transform,
    after: Transform,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyDocument {
    format: String,
    format_version: u32,
    scene: LegacyScene,
    undo: Vec<LegacySceneDelta>,
    redo: Vec<LegacySceneDelta>,
}
impl From<LegacyDocument> for Document {
    fn from(old: LegacyDocument) -> Self {
        let migrate = |change: LegacySceneDelta| SceneDelta::Transform {
            id: change.id,
            before: change.before,
            after: change.after,
        };
        Self {
            format: old.format,
            format_version: old.format_version,
            scene: Scene {
                id: old.scene.id,
                revision: old.scene.revision,
                materials: BTreeMap::new(),
                lighting: Lighting::Legacy,
                display: DisplayProfile::LegacySdr,
                objects: old
                    .scene
                    .objects
                    .into_iter()
                    .map(|o| Object {
                        id: o.id,
                        name: None,
                        transform: o.transform,
                        color: o.color,
                        mesh: None,
                        material: None,
                        collider: None,
                    })
                    .collect(),
            },
            authoring: Authoring::default(),
            undo: old
                .undo
                .into_iter()
                .map(migrate)
                .map(Change::from)
                .collect(),
            redo: old
                .redo
                .into_iter()
                .map(migrate)
                .map(Change::from)
                .collect(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Object {
    id: String,
    transform: Transform,
    color: [f32; 4],
    #[serde(default)]
    mesh: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Scene {
    id: String,
    revision: u64,
    objects: Vec<V2Object>,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum V2SceneDelta {
    Transform {
        id: String,
        before: Transform,
        after: Transform,
    },
    Mesh {
        id: String,
        before: Option<String>,
        after: Option<String>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct V2Document {
    format: String,
    format_version: u32,
    scene: V2Scene,
    undo: Vec<V2SceneDelta>,
    redo: Vec<V2SceneDelta>,
}
impl From<V2Document> for Document {
    fn from(old: V2Document) -> Self {
        let migrate = |c| match c {
            V2SceneDelta::Transform { id, before, after } => {
                SceneDelta::Transform { id, before, after }
            }
            V2SceneDelta::Mesh { id, before, after } => SceneDelta::Mesh { id, before, after },
        };
        Self {
            format: old.format,
            format_version: old.format_version,
            scene: Scene {
                id: old.scene.id,
                revision: old.scene.revision,
                materials: BTreeMap::new(),
                lighting: Lighting::Legacy,
                display: DisplayProfile::LegacySdr,
                objects: old
                    .scene
                    .objects
                    .into_iter()
                    .map(|o| Object {
                        id: o.id,
                        name: None,
                        transform: o.transform,
                        color: o.color,
                        mesh: o.mesh,
                        material: None,
                        collider: None,
                    })
                    .collect(),
            },
            authoring: Authoring::default(),
            undo: old
                .undo
                .into_iter()
                .map(migrate)
                .map(Change::from)
                .collect(),
            redo: old
                .redo
                .into_iter()
                .map(migrate)
                .map(Change::from)
                .collect(),
        }
    }
}

// Exact pre-authoring envelope and history: future fields are rejected, including null.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct V4Document {
    format: String,
    format_version: u32,
    scene: Scene,
    undo: Vec<SceneDelta>,
    redo: Vec<SceneDelta>,
}
impl From<V4Document> for Document {
    fn from(old: V4Document) -> Self {
        Self {
            format: old.format,
            format_version: old.format_version,
            scene: old.scene,
            authoring: Authoring::default(),
            undo: old.undo.into_iter().map(Change::from).collect(),
            redo: old.redo.into_iter().map(Change::from).collect(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    format: String,
    format_version: u32,
    scene: Scene,
    #[serde(default, skip_serializing_if = "Authoring::is_empty")]
    authoring: Authoring,
    undo: Vec<Change>,
    redo: Vec<Change>,
}
impl Document {
    fn new(scene: Scene) -> Self {
        Self {
            format: "ruster-scene".into(),
            format_version: FORMAT_VERSION,
            scene,
            authoring: Authoring::default(),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
    fn validate(&self) -> Result<(), SceneError> {
        self.scene.validate()?;
        self.authoring.validate(&self.scene)?;
        if self.undo.len() + self.redo.len() > HISTORY_LIMIT {
            return Err(SceneError::Invalid(format!(
                "history exceeds {HISTORY_LIMIT} changes"
            )));
        }
        check_serialized_size(
            &self.scene,
            &self.authoring,
            &self.undo.iter().collect::<Vec<_>>(),
            &self.redo.iter().collect::<Vec<_>>(),
        )
    }
    fn load(path: &Path) -> Result<Self, SceneError> {
        reject_nonregular(path)?;
        let mut bytes = Vec::new();
        File::open(path)?
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        Self::decode(&bytes)
    }
    fn decode(bytes: &[u8]) -> Result<Self, SceneError> {
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(SceneError::Invalid("file exceeds 1 MiB".into()));
        }
        let value = strict_json::parse(bytes)?;
        if value.get("format").and_then(|v| v.as_str()) != Some("ruster-scene") {
            return Err(SceneError::Invalid("expected ruster-scene format; logistics v1 scenarios remain a separate format and are not migrated".into()));
        }
        let version = value
            .get("format_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| SceneError::Invalid("format_version must be an integer".into()))?;
        if (1..16).contains(&version) {
            reject_pre_v16_hud_buttons(&value)?;
        }
        if (1..15).contains(&version) {
            reject_pre_v15_hud_fields(&value)?;
        }
        if (1..14).contains(&version) {
            reject_pre_v14_command_fields(&value)?;
        }
        if (1..13).contains(&version) {
            reject_pre_v13_binding_fields(&value)?;
        }
        if (1..12).contains(&version) {
            reject_pre_v12_collider_fields(&value, version)?;
        }
        if (1..11).contains(&version) {
            reject_pre_v11_input_fields(&value)?;
        }
        if (1..10).contains(&version) {
            reject_pre_v10_roughness_fields(&value)?;
        }
        if (3..=5).contains(&version) {
            reject_pre_v6_material_fields(&value, version)?;
        }
        if (1..7).contains(&version) {
            reject_pre_v7_lighting_fields(&value, version)?;
        }
        if (1..8).contains(&version) {
            reject_pre_v8_shadow_fields(&value, version)?;
        }
        if (1..9).contains(&version) {
            reject_pre_v9_surface_fields(&value, version)?;
        }
        let document: Self = match version {
            1 => serde_json::from_value::<LegacyDocument>(value)?.into(),
            2 => serde_json::from_value::<V2Document>(value)?.into(),
            3 | 4 => serde_json::from_value::<V4Document>(value)?.into(),
            5..=16 => serde_json::from_value(value)?,
            _ => return Err(SceneError::UnsupportedVersion(version)),
        };
        if version < 5
            && document
                .undo
                .iter()
                .chain(&document.redo)
                .flat_map(|c| &c.scene)
                .any(|d| matches!(d, SceneDelta::Color { .. }))
        {
            return Err(SceneError::Invalid(
                "color history requires scene v5".into(),
            ));
        }
        if version == 3
            && document.undo.iter().chain(&document.redo).any(|c| {
                c.scene.iter().any(|d| {
                    matches!(
                        d,
                        SceneDelta::Transforms { .. } | SceneDelta::Objects { .. }
                    )
                })
            })
        {
            return Err(SceneError::Invalid(
                "scene v3 does not support grouped history".into(),
            ));
        }
        document.validate()?;
        Ok(document)
    }
    fn encode(&self) -> Result<Vec<u8>, SceneError> {
        self.validate()?;
        let mut upgraded = self.clone();
        upgraded.format_version = FORMAT_VERSION;
        let mut bytes = serde_json::to_vec(&upgraded)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(SceneError::Capacity);
        }
        Ok(bytes)
    }
    fn write(&self, path: &Path, create_new: bool) -> Result<(), SceneError> {
        reject_nonregular(path)?;
        let bytes = self.encode()?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(&bytes)?;
        temporary.flush()?;
        temporary.as_file().sync_all()?;
        if create_new {
            temporary.persist_noclobber(path).map_err(|e| {
                if e.error.kind() == std::io::ErrorKind::AlreadyExists {
                    SceneError::Conflict
                } else {
                    SceneError::Io(e.error)
                }
            })?;
        } else {
            temporary
                .persist(path)
                .map_err(|e| SceneError::Io(e.error))?;
        }
        Ok(())
    }
}

// Scene15 retains its exact label schema. Inspect only HUD element owners,
// leaving source strings and arbitrary state/property maps opaque.
fn reject_pre_v16_hud_buttons(value: &serde_json::Value) -> Result<(), SceneError> {
    let has_button = |hud: &serde_json::Value| {
        hud["elements"].as_array().is_some_and(|elements| {
            elements.iter().any(|element| {
                element["source"]["kind"].as_str() == Some("button")
                    || element["source"].get("caption").is_some()
                    || element["source"].get("command").is_some()
            })
        })
    };
    let mut found = has_button(&value["authoring"]["hud"]);
    for stack in ["undo", "redo"] {
        for change in value[stack].as_array().into_iter().flatten() {
            found |= has_button(&change["hud"]["before"]) || has_button(&change["hud"]["after"]);
        }
    }
    if found {
        Err(SceneError::Invalid("HUD buttons require scene v16".into()))
    } else {
        Ok(())
    }
}

// Restrict new fields at their owning positions, including empty or null values.
// Source strings and arbitrary state/property keys named hud retain their meaning.
fn reject_pre_v15_hud_fields(value: &serde_json::Value) -> Result<(), SceneError> {
    let has_hud = value["authoring"].get("hud").is_some()
        || ["undo", "redo"].into_iter().any(|stack| {
            value[stack]
                .as_array()
                .into_iter()
                .flatten()
                .any(|change| change.get("hud").is_some())
        });
    if has_hud {
        Err(SceneError::Invalid(
            "HUD authoring requires scene v15".into(),
        ))
    } else {
        Ok(())
    }
}

// Only binding definitions acquire these fields. Property/source contents with
// the same names retain their existing meaning, including opaque API metadata.
fn reject_pre_v14_command_fields(value: &serde_json::Value) -> Result<(), SceneError> {
    let has_commands = |binding: &serde_json::Value| binding.get("commands").is_some();
    let mut found = value["authoring"]["bindings"]
        .as_object()
        .is_some_and(|bindings| bindings.values().any(has_commands));
    for stack in ["undo", "redo"] {
        for change in value[stack].as_array().into_iter().flatten() {
            for delta in change["bindings"].as_array().into_iter().flatten() {
                found |= has_commands(&delta["before"]) || has_commands(&delta["after"]);
            }
        }
    }
    if found {
        Err(SceneError::Invalid(
            "application commands require scene v14".into(),
        ))
    } else {
        Ok(())
    }
}

fn reject_pre_v13_binding_fields(value: &serde_json::Value) -> Result<(), SceneError> {
    let has_new_fields = |binding: &serde_json::Value| {
        binding.get("target").is_some() || binding.get("state_defaults").is_some()
    };
    let mut found = value["authoring"]["bindings"]
        .as_object()
        .is_some_and(|bindings| bindings.values().any(has_new_fields));
    for stack in ["undo", "redo"] {
        for change in value[stack].as_array().into_iter().flatten() {
            for delta in change["bindings"].as_array().into_iter().flatten() {
                found |= has_new_fields(&delta["before"]) || has_new_fields(&delta["after"]);
            }
        }
    }
    if found {
        Err(SceneError::Invalid(
            "application binding targets and state defaults require scene v13".into(),
        ))
    } else {
        Ok(())
    }
}

// Inspect actual Object owners and collider deltas only. Behavior properties
// and JSON-looking source strings named collider retain their existing meaning.
fn reject_pre_v12_collider_fields(
    value: &serde_json::Value,
    version: u64,
) -> Result<(), SceneError> {
    let has_collider = |object: &serde_json::Value| object.get("collider").is_some();
    let mut found = value["scene"]["objects"]
        .as_array()
        .is_some_and(|objects| objects.iter().any(has_collider));
    for stack in ["undo", "redo"] {
        for change in value[stack].as_array().into_iter().flatten() {
            let deltas = if version <= 4 {
                std::slice::from_ref(change)
            } else {
                change["scene"].as_array().map(Vec::as_slice).unwrap_or(&[])
            };
            for delta in deltas {
                found |= match delta["kind"].as_str() {
                    Some("collider") => true,
                    Some("create" | "delete") => has_collider(&delta["object"]),
                    Some("objects") => ["created", "deleted"].into_iter().any(|section| {
                        delta[section].as_array().is_some_and(|entries| {
                            entries.iter().any(|entry| has_collider(&entry["object"]))
                        })
                    }),
                    _ => false,
                };
            }
        }
    }
    if found {
        Err(SceneError::Invalid(
            "primitive colliders require scene v12".into(),
        ))
    } else {
        Ok(())
    }
}

// Only material definitions acquire the new slot, including reversible values.
// Arbitrary behavior property names do not acquire texture semantics.
fn reject_pre_v10_roughness_fields(value: &serde_json::Value) -> Result<(), SceneError> {
    let has_map = |material: &serde_json::Value| material.get("roughness_texture").is_some();
    let mut found = value["scene"]["materials"]
        .as_object()
        .is_some_and(|materials| materials.values().any(has_map));
    for stack in ["undo", "redo"] {
        for change in value[stack].as_array().into_iter().flatten() {
            if let Some(materials) = change["materials"].as_array() {
                found |= materials
                    .iter()
                    .any(|delta| has_map(&delta["before"]) || has_map(&delta["after"]));
            }
        }
    }
    if found {
        Err(SceneError::Invalid(
            "roughness textures require scene v10".into(),
        ))
    } else {
        Ok(())
    }
}

// Only authored definition/history fields acquire action semantics; Lua source
// text and labels may contain arbitrary JSON-shaped strings without reinterpretation.
fn reject_pre_v11_input_fields(value: &serde_json::Value) -> Result<(), SceneError> {
    let has_actions = value["authoring"].get("actions").is_some()
        || ["undo", "redo"].into_iter().any(|stack| {
            value[stack]
                .as_array()
                .into_iter()
                .flatten()
                .any(|change| change.get("actions").is_some())
        });
    if has_actions {
        Err(SceneError::Invalid(
            "input actions require scene v11".into(),
        ))
    } else {
        Ok(())
    }
}

// Only actual scene/material/history positions acquire surface semantics.
// Behavior properties named display/response remain unrelated authored data.
fn reject_pre_v9_surface_fields(value: &serde_json::Value, version: u64) -> Result<(), SceneError> {
    let has_response = |material: &serde_json::Value| material.get("response").is_some();
    let scene = &value["scene"];
    let mut found = scene.get("display").is_some()
        || scene["materials"]
            .as_object()
            .is_some_and(|materials| materials.values().any(has_response));
    for stack in ["undo", "redo"] {
        for change in value[stack].as_array().into_iter().flatten() {
            if let Some(materials) = change["materials"].as_array() {
                found |= materials
                    .iter()
                    .any(|delta| has_response(&delta["before"]) || has_response(&delta["after"]));
            }
            let deltas = if version <= 4 {
                std::slice::from_ref(change)
            } else {
                change["scene"].as_array().map(Vec::as_slice).unwrap_or(&[])
            };
            found |= deltas.iter().any(|delta| delta["kind"] == "display");
        }
    }
    if found {
        Err(SceneError::Invalid(
            "display profiles and material responses require scene v9".into(),
        ))
    } else {
        Ok(())
    }
}

// Inspect only actual light definitions in the current lighting value and both
// endpoints of lighting deltas. Behavior/application properties named `shadow`
// remain independent data; recursively matching arbitrary keys would reject them.
fn reject_pre_v8_shadow_fields(value: &serde_json::Value, version: u64) -> Result<(), SceneError> {
    let has_shadow = |lighting: &serde_json::Value| {
        lighting["lights"].as_object().is_some_and(|lights| {
            lights
                .values()
                .any(|light| light.get("shadow").is_some() || light["kind"].get("shadow").is_some())
        })
    };
    let mut found = has_shadow(&value["scene"]["lighting"]);
    for stack in ["undo", "redo"] {
        for change in value[stack].as_array().into_iter().flatten() {
            let deltas = if version <= 4 {
                std::slice::from_ref(change)
            } else {
                change["scene"].as_array().map(Vec::as_slice).unwrap_or(&[])
            };
            for delta in deltas {
                if delta["kind"] == "lighting" {
                    found |= has_shadow(&delta["before"]) || has_shadow(&delta["after"]);
                }
            }
        }
    }
    if found {
        Err(SceneError::Invalid("Sun shadows require scene v8".into()))
    } else {
        Ok(())
    }
}

// Old envelopes must not acquire new semantics merely through serde defaults.
fn reject_pre_v7_lighting_fields(
    value: &serde_json::Value,
    version: u64,
) -> Result<(), SceneError> {
    let mut found = value["scene"]
        .as_object()
        .is_some_and(|scene| scene.contains_key("lighting"));
    for stack in ["undo", "redo"] {
        if let Some(entries) = value[stack].as_array() {
            for entry in entries {
                if version <= 4 {
                    found |= entry["kind"] == "lighting";
                } else if let Some(deltas) = entry["scene"].as_array() {
                    found |= deltas.iter().any(|delta| delta["kind"] == "lighting");
                }
            }
        }
    }
    if found {
        Err(SceneError::Invalid("lighting requires scene v7".into()))
    } else {
        Ok(())
    }
}

// V3–V5 used today's scene/object structs. Reject V6 additions at their
// exact bounded schema positions before deserializing defaults, including null/empty.
fn reject_pre_v6_material_fields(
    value: &serde_json::Value,
    version: u64,
) -> Result<(), SceneError> {
    let object_has_material = |object: &serde_json::Value| object.get("material").is_some();
    let scene = &value["scene"];
    let mut has_new = scene.get("materials").is_some()
        || scene["objects"]
            .as_array()
            .is_some_and(|objects| objects.iter().any(object_has_material));
    for stack in ["undo", "redo"] {
        for change in value[stack].as_array().into_iter().flatten() {
            has_new |= version == 5 && change.get("materials").is_some();
            let deltas: &[serde_json::Value] = if version == 5 {
                change["scene"].as_array().map(Vec::as_slice).unwrap_or(&[])
            } else {
                std::slice::from_ref(change)
            };
            for delta in deltas {
                has_new |= delta["kind"].as_str() == Some("material")
                    || delta.get("object").is_some_and(object_has_material);
                for section in ["created", "deleted"] {
                    has_new |= delta[section].as_array().is_some_and(|entries| {
                        entries
                            .iter()
                            .any(|entry| object_has_material(&entry["object"]))
                    });
                }
            }
        }
    }
    if has_new {
        Err(SceneError::Invalid("materials require scene v6".into()))
    } else {
        Ok(())
    }
}

// Count history once and reserve the largest scene reachable through Undo/Redo.
// This includes restored deleted objects/sources and maximum revision width.
// Compact encoding is shared with the writer; concatenating stacks is conservative.
fn check_serialized_size(
    scene: &Scene,
    authoring: &Authoring,
    undo: &[&Change],
    redo: &[&Change],
) -> Result<(), SceneError> {
    #[derive(Serialize)]
    struct SceneView<'a> {
        id: &'a str,
        revision: u64,
        objects: &'a [Object],
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        materials: &'a BTreeMap<String, OpaqueMaterial>,
        #[serde(skip_serializing_if = "Lighting::is_legacy")]
        lighting: &'a Lighting,
        #[serde(skip_serializing_if = "DisplayProfile::is_legacy")]
        display: &'a DisplayProfile,
    }
    impl<'a> From<&'a Scene> for SceneView<'a> {
        fn from(scene: &'a Scene) -> Self {
            Self {
                id: &scene.id,
                revision: u64::MAX,
                objects: &scene.objects,
                materials: &scene.materials,
                lighting: &scene.lighting,
                display: &scene.display,
            }
        }
    }
    #[derive(Serialize)]
    struct SceneOnly<'a> {
        scene: SceneView<'a>,
        #[serde(skip_serializing_if = "Authoring::is_empty")]
        authoring: &'a Authoring,
    }
    #[derive(Serialize)]
    struct View<'a> {
        format: &'static str,
        format_version: u32,
        scene: SceneView<'a>,
        #[serde(skip_serializing_if = "Authoring::is_empty")]
        authoring: &'a Authoring,
        undo: &'a [&'a Change],
        redo: &'a [&'a Change],
    }
    struct Counter {
        bytes: u64,
        exceeded: bool,
    }
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes += bytes.len() as u64;
            if self.bytes > MAX_FILE_BYTES {
                self.exceeded = true;
                return Err(std::io::Error::other("serialized scene exceeds 1 MiB"));
            }
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    fn size(value: &impl Serialize) -> Result<u64, SceneError> {
        let mut counter = Counter {
            bytes: 0,
            exceeded: false,
        };
        if let Err(error) = serde_json::to_writer(&mut counter, value) {
            return Err(if counter.exceeded {
                SceneError::Capacity
            } else {
                SceneError::Format(error)
            });
        }
        Ok(counter.bytes)
    }
    let all: Vec<_> = undo.iter().chain(redo).copied().collect();
    let envelope = size(&View {
        format: "ruster-scene",
        format_version: FORMAT_VERSION,
        scene: scene.into(),
        authoring,
        undo: &all,
        redo: &[],
    })?;
    // Wrapper constant bytes cancel when replacing the current authored payload.
    let current = size(&SceneOnly {
        scene: scene.into(),
        authoring,
    })?;
    let mut largest = current;
    for (history, reversing) in [(undo, true), (redo, false)] {
        let mut candidate = scene.clone();
        let mut candidate_authoring = authoring.clone();
        for change in history.iter().rev() {
            change.apply(&mut candidate, &mut candidate_authoring, reversing)?;
            candidate.validate()?;
            largest = largest.max(size(&SceneOnly {
                scene: (&candidate).into(),
                authoring: &candidate_authoring,
            })?);
        }
    }
    // Splitting compact stacks removes one comma when both are nonempty, so the
    // concatenated envelope bounds every split. Reserve the writer's final newline.
    if envelope - current + largest + 1 > MAX_FILE_BYTES {
        return Err(SceneError::Capacity);
    }
    Ok(())
}

fn allocate_ids(
    prefix: &str,
    count: usize,
    limit: usize,
    used: HashSet<&str>,
) -> Result<Vec<String>, SceneError> {
    if count > limit {
        return Err(SceneError::Invalid(format!("ID request exceeds{limit}")));
    }
    Ok((1..=used.len() + count)
        .map(|n| format!("{prefix}-{n}"))
        .filter(|id| !used.contains(id.as_str()))
        .take(count)
        .collect())
}

/// Authored state with bounded object, name, transform, color and mesh deltas. Runtime state belongs elsewhere.
#[derive(Clone, Debug)]
pub struct Session {
    document: Document,
    path: Option<PathBuf>,
    saved: Option<Document>,
    loaded_format_version: u32,
    history_evictions: usize,
}
impl Session {
    pub fn new(scene: Scene) -> Result<Self, SceneError> {
        scene.validate()?;
        let document = Document::new(scene);
        document.validate()?;
        Ok(Self {
            document,
            path: None,
            saved: None,
            loaded_format_version: FORMAT_VERSION,
            history_evictions: 0,
        })
    }
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SceneError> {
        let lock = SceneLock::acquire(path.as_ref())?;
        let document = Document::load(&lock.path)?;
        Ok(Self {
            loaded_format_version: document.format_version,
            history_evictions: 0,
            saved: Some(document.clone()),
            document,
            path: Some(lock.path.clone()),
        })
    }
    /// Complete admitted authored document without changing save authority or history.
    pub fn snapshot_bytes(&self) -> Result<Vec<u8>, SceneError> {
        self.document.encode()
    }
    /// Restore a validated snapshot as an unsaved copy, never bound to its origin.
    pub fn from_snapshot_bytes(bytes: &[u8]) -> Result<Self, SceneError> {
        let document = Document::decode(bytes)?;
        Ok(Self {
            loaded_format_version: document.format_version,
            document,
            path: None,
            saved: None,
            history_evictions: 0,
        })
    }
    pub fn scene(&self) -> &Scene {
        &self.document.scene
    }
    pub fn authoring(&self) -> &Authoring {
        &self.document.authoring
    }
    pub fn hud(&self) -> &hud::Hud {
        &self.authoring().hud
    }

    /// Replace one HUD as an atomic revision and Undo entry. Existing broken
    /// references remain editable, while new references require a typed field
    /// or an enabled API5 Application command with complete typed arguments.
    pub fn set_hud(&mut self, expected_revision: u64, hud: hud::Hud) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        hud.validate()?;
        if self.hud() == &hud {
            return Ok(self.scene().revision);
        }
        for label in &hud.elements {
            let previous = self.hud().elements.iter().find(|old| old.id == label.id);
            if previous.is_none() && self.contains_hud_label_id(&label.id) {
                return Err(SceneError::Invalid(
                    "HUD label identity is reserved by reversible history; Undo or choose a fresh ID"
                        .into(),
                ));
            }
            if let Some(command) = label.source.command_binding() {
                if previous.and_then(|old| old.source.command_binding()) != Some(command) {
                    hud::validate_hud_button_command(self.authoring(), command)?;
                }
            }
            let Some((binding_id, field, value_type)) = label.source.state_reference() else {
                continue;
            };
            if previous.and_then(|old| old.source.state_reference())
                == Some((binding_id, field, value_type))
            {
                continue;
            }
            let valid = self.binding(binding_id).is_some_and(|binding| {
                binding.enabled
                    && self.source(&binding.source_id).is_some()
                    && binding
                        .state_defaults
                        .get(field)
                        .is_some_and(|value| value_type.matches(value))
            });
            if !valid {
                return Err(SceneError::Invalid(format!(
                    "HUD label {} requires an enabled binding {binding_id} with an existing source and field {field} matching its declared type",
                    label.id
                )));
            }
        }
        self.commit_change(Change {
            hud: Some(HudDelta {
                before: self.hud().clone(),
                after: hud,
            }),
            ..Change::default()
        })
    }

    fn retained_huds(&self) -> impl Iterator<Item = &hud::Hud> {
        std::iter::once(self.hud()).chain(
            self.document
                .undo
                .iter()
                .chain(&self.document.redo)
                .flat_map(Change::hud_values),
        )
    }

    pub fn contains_hud_label_id(&self, id: &str) -> bool {
        self.retained_huds()
            .any(|hud| hud.elements.iter().any(|label| label.id == id))
    }

    pub fn allocate_hud_label_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        let used: HashSet<_> = self
            .retained_huds()
            .flat_map(|hud| hud.elements.iter().map(|label| label.id.as_str()))
            .collect();
        let next = self
            .scene()
            .revision
            .checked_add(1)
            .ok_or(SceneError::RevisionOverflow)?;
        allocate_ids(&format!("hud-{next}"), count, hud::MAX_HUD_LABELS, used)
    }

    pub fn source(&self, id: &str) -> Option<&BehaviorSource> {
        self.authoring().sources.get(id)
    }
    pub fn binding(&self, id: &str) -> Option<&BehaviorBinding> {
        self.authoring().bindings.get(id)
    }
    pub fn inspect_sources(&self, offset: usize, limit: usize) -> AuthoringPage<SourceSummary> {
        authoring::page(
            self.authoring()
                .sources
                .iter()
                .map(|(id, s)| SourceSummary {
                    id: id.clone(),
                    label: s.label.clone(),
                    language_id: s.language_id.clone(),
                    host_api_version: s.host_api_version,
                    text_bytes: s.text.len(),
                }),
            offset,
            limit,
        )
    }
    pub fn inspect_bindings(&self, offset: usize, limit: usize) -> AuthoringPage<BindingSummary> {
        authoring::page(
            self.authoring()
                .bindings
                .iter()
                .map(|(id, b)| BindingSummary {
                    id: id.clone(),
                    target: b.target.clone(),
                    source_id: b.source_id.clone(),
                    enabled: b.enabled,
                    property_count: b.properties.len(),
                    state_field_count: b.state_defaults.len(),
                    command_count: b.commands.len(),
                }),
            offset,
            limit,
        )
    }
    /// List bounded command summaries without copying argument maps.
    pub fn inspect_commands(
        &self,
        binding_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<AuthoringPage<CommandSummary>, SceneError> {
        let binding = self
            .binding(binding_id)
            .ok_or_else(|| SceneError::Invalid(format!("binding {binding_id} does not exist")))?;
        Ok(authoring::page(
            binding
                .commands
                .iter()
                .map(|(id, definition)| CommandSummary {
                    id: id.clone(),
                    argument_count: definition.arguments.len(),
                }),
            offset,
            limit,
        ))
    }
    pub fn command_definition(
        &self,
        binding_id: &str,
        command_id: &str,
    ) -> Option<&CommandDefinition> {
        self.binding(binding_id)?.commands.get(command_id)
    }
    /// Structural diagnostics only. Backend/schema/asset availability belongs to their owning hosts.
    pub fn authoring_diagnostics(
        &self,
        offset: usize,
        limit: usize,
    ) -> AuthoringPage<AuthoringDiagnostic> {
        let mut diagnostics = Vec::new();
        for (id, b) in &self.authoring().bindings {
            if !self.authoring().sources.contains_key(&b.source_id) {
                diagnostics.push(AuthoringDiagnostic {
                    code: AuthoringDiagnosticCode::MissingSource,
                    binding_id: id.clone(),
                    property: None,
                    reference_id: b.source_id.clone(),
                });
            }
            for (name, value) in &b.properties {
                if let PropertyValue::ObjectRef(Some(target)) = value {
                    if !self.scene().objects.iter().any(|o| &o.id == target) {
                        diagnostics.push(AuthoringDiagnostic {
                            code: AuthoringDiagnosticCode::MissingObjectReference,
                            binding_id: id.clone(),
                            property: Some(name.clone()),
                            reference_id: target.clone(),
                        });
                    }
                }
            }
        }
        authoring::page(diagnostics.into_iter(), offset, limit)
    }
    pub fn action(&self, id: &str) -> Option<&input::ActionDefinition> {
        self.authoring().actions.get(id)
    }
    /// Shared, undoable action edit. A deleted reversible identity is restored
    /// through Undo, not silently reused for a different action.
    pub fn set_action(
        &mut self,
        expected_revision: u64,
        id: &str,
        action: Option<input::ActionDefinition>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        let before = self.action(id).cloned();
        if before == action {
            return Ok(self.scene().revision);
        }
        if before.is_none() && action.is_some() && self.contains_action_id(id) {
            return Err(SceneError::Invalid(
                "action identity is reserved by reversible history; Undo or choose a fresh ID"
                    .into(),
            ));
        }
        self.commit_change(Change {
            actions: vec![ValueDelta {
                id: id.into(),
                before,
                after: action,
            }],
            ..Change::default()
        })
    }
    pub fn contains_action_id(&self, id: &str) -> bool {
        self.authoring().actions.contains_key(id)
            || self
                .document
                .undo
                .iter()
                .chain(&self.document.redo)
                .any(|c| c.actions.iter().any(|d| d.id == id))
    }
    pub fn allocate_action_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        let mut used: HashSet<_> = self
            .authoring()
            .actions
            .keys()
            .map(String::as_str)
            .collect();
        used.extend(
            self.document
                .undo
                .iter()
                .chain(&self.document.redo)
                .flat_map(|c| c.actions.iter().map(|d| d.id.as_str())),
        );
        // A monotonic revision prefix also avoids normal UI allocation reusing
        // old raw Lua identifiers after their structural history has aged out.
        let next = self
            .scene()
            .revision
            .checked_add(1)
            .ok_or_else(|| SceneError::Invalid("revision exhausted".into()))?;
        allocate_ids(&format!("action-{next}"), count, input::MAX_ACTIONS, used)
    }

    /// Explicit upsert of a stable source identity; broken/unknown-language text is saveable.
    pub fn set_source(
        &mut self,
        expected_revision: u64,
        id: &str,
        source: Option<BehaviorSource>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        if let Some(source) = &source {
            source.validate()?;
        }
        let before = self.source(id).cloned();
        if before == source {
            return Ok(self.scene().revision);
        }
        self.commit_change(Change {
            sources: vec![ValueDelta {
                id: id.into(),
                before,
                after: source,
            }],
            ..Change::default()
        })
    }
    /// Explicit upsert preserves all supplied property values, independent of source schemas.
    pub fn set_binding(
        &mut self,
        expected_revision: u64,
        id: &str,
        binding: Option<BehaviorBinding>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        if let Some(binding) = &binding {
            binding.validate()?;
        }
        let before = self.binding(id).cloned();
        if before == binding {
            return Ok(self.scene().revision);
        }
        if before.is_none()
            && binding.is_some()
            && self
                .retained_huds()
                .any(|hud| hud.binding_ids().any(|used| used == id))
        {
            return Err(SceneError::Invalid(
                "binding identity is retained by a HUD reference; Undo or choose a fresh ID".into(),
            ));
        }
        self.commit_change(Change {
            bindings: vec![ValueDelta {
                id: id.into(),
                before,
                after: binding,
            }],
            ..Change::default()
        })
    }
    /// Edit one authored default as the same atomic binding/history transaction.
    pub fn set_state_default(
        &mut self,
        expected_revision: u64,
        binding_id: &str,
        name: &str,
        value: Option<StateValue>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(binding_id)?;
        validate_id(name)?;
        if let Some(value) = &value {
            value.validate()?;
        }
        let mut binding = self
            .binding(binding_id)
            .cloned()
            .ok_or_else(|| SceneError::Invalid(format!("binding {binding_id} does not exist")))?;
        if let Some(value) = value {
            binding.state_defaults.insert(name.into(), value);
        } else {
            binding.state_defaults.remove(name);
        }
        self.set_binding(expected_revision, binding_id, Some(binding))
    }
    /// Edit one command definition through the existing binding/history transaction.
    pub fn set_command_definition(
        &mut self,
        expected_revision: u64,
        binding_id: &str,
        command_id: &str,
        definition: Option<CommandDefinition>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(binding_id)?;
        validate_id(command_id)?;
        if let Some(definition) = &definition {
            definition.validate()?;
        }
        let mut binding = self
            .binding(binding_id)
            .cloned()
            .ok_or_else(|| SceneError::Invalid(format!("binding {binding_id} does not exist")))?;
        if let Some(definition) = definition {
            binding.commands.insert(command_id.into(), definition);
        } else {
            binding.commands.remove(command_id);
        }
        self.set_binding(expected_revision, binding_id, Some(binding))
    }
    /// Create a fresh source and its attachment as one revision and Undo step.
    /// Use set_binding to attach an existing source, or set_source for explicit repair.
    pub fn create_behavior(
        &mut self,
        expected_revision: u64,
        source_id: &str,
        source: BehaviorSource,
        binding_id: &str,
        binding: BehaviorBinding,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(source_id)?;
        validate_id(binding_id)?;
        source.validate()?;
        binding.validate()?;
        if self.contains_source_id(source_id)
            || self.contains_binding_id(binding_id)
            || binding.source_id != source_id
        {
            return Err(SceneError::Invalid("Create behavior requires fresh identities and a binding referencing the new source.".into()));
        }
        self.commit_change(Change {
            sources: vec![ValueDelta {
                id: source_id.into(),
                before: None,
                after: Some(source),
            }],
            bindings: vec![ValueDelta {
                id: binding_id.into(),
                before: None,
                after: Some(binding),
            }],
            ..Change::default()
        })
    }
    fn reserved_source_ids(&self) -> HashSet<&str> {
        let mut used: HashSet<_> = self
            .authoring()
            .sources
            .keys()
            .map(String::as_str)
            .collect();
        used.extend(
            self.authoring()
                .bindings
                .values()
                .map(|b| b.source_id.as_str()),
        );
        for change in self.document.undo.iter().chain(&self.document.redo) {
            used.extend(change.sources.iter().map(|d| d.id.as_str()));
            used.extend(
                change
                    .bindings
                    .iter()
                    .flat_map(|d| [&d.before, &d.after])
                    .flatten()
                    .map(|b| b.source_id.as_str()),
            );
        }
        used
    }
    pub fn contains_source_id(&self, id: &str) -> bool {
        self.reserved_source_ids().contains(id)
    }
    pub fn contains_binding_id(&self, id: &str) -> bool {
        self.authoring().bindings.contains_key(id)
            || self
                .retained_huds()
                .any(|hud| hud.binding_ids().any(|used| used == id))
            || self
                .document
                .undo
                .iter()
                .chain(&self.document.redo)
                .any(|c| c.bindings.iter().any(|d| d.id == id))
    }
    pub fn allocate_source_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        allocate_ids("source", count, MAX_SOURCES, self.reserved_source_ids())
    }
    pub fn allocate_binding_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        let mut used: HashSet<_> = self
            .authoring()
            .bindings
            .keys()
            .map(String::as_str)
            .collect();
        used.extend(
            self.document
                .undo
                .iter()
                .chain(&self.document.redo)
                .flat_map(|c| c.bindings.iter().map(|d| d.id.as_str())),
        );
        used.extend(self.retained_huds().flat_map(hud::Hud::binding_ids));
        allocate_ids("binding", count, MAX_BINDINGS, used)
    }
    fn owned_binding_deletions(&self, ids: &[&str]) -> Vec<ValueDelta<BehaviorBinding>> {
        self.authoring()
            .bindings
            .iter()
            .filter(|(_, b)| b.object_id().is_some_and(|id| ids.contains(&id)))
            .map(|(id, b)| ValueDelta {
                id: id.clone(),
                before: Some(b.clone()),
                after: None,
            })
            .collect()
    }
    fn cloned_bindings(
        &self,
        pairs: &[(String, String)],
    ) -> Result<Vec<ValueDelta<BehaviorBinding>>, SceneError> {
        let originals: Vec<_> = pairs
            .iter()
            .flat_map(|(source, target)| {
                self.authoring()
                    .bindings
                    .values()
                    .filter(move |b| b.object_id() == Some(source.as_str()))
                    .map(move |b| (target, b))
            })
            .collect();
        let ids = self.allocate_binding_ids(originals.len())?;
        Ok(originals
            .into_iter()
            .zip(ids)
            .map(|((target, b), id)| {
                let mut binding = b.clone();
                binding.target = BindingTarget::Object(target.clone());
                for value in binding.properties.values_mut() {
                    if let PropertyValue::ObjectRef(Some(reference)) = value {
                        if let Some((_, replacement)) =
                            pairs.iter().find(|(original, _)| original == reference)
                        {
                            reference.clone_from(replacement);
                        }
                    }
                }
                ValueDelta {
                    id,
                    before: None,
                    after: Some(binding),
                }
            })
            .collect())
    }
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
    /// Source format opened, or the current format after a successful save.
    pub fn loaded_format_version(&self) -> u32 {
        self.loaded_format_version
    }
    pub fn is_dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.document)
    }
    /// Cumulative oldest Undo entries trimmed by byte pressure in this session.
    /// Does not count ordinary 32-step age eviction; resets on Open/new Session.
    pub fn history_evictions(&self) -> usize {
        self.history_evictions
    }
    pub fn undo_len(&self) -> usize {
        self.document.undo.len()
    }
    pub fn redo_len(&self) -> usize {
        self.document.redo.len()
    }
    /// True when current state or reversible history can resolve an asset ID.
    /// Hosts use this to avoid relocating a scene away from its catalog.
    pub fn has_asset_references(&self) -> bool {
        self.scene().objects.iter().any(|o| o.mesh.is_some())
            || self
                .scene()
                .materials
                .values()
                .any(|m| m.base_color_texture.is_some() || m.roughness_texture.is_some())
            || self.document.authoring.has_asset_references()
            || self
                .document
                .undo
                .iter()
                .chain(&self.document.redo)
                .any(Change::references_asset)
    }
    /// IDs retained by reversible history are reserved, preventing accidental identity reuse.
    pub fn contains_object_id(&self, id: &str) -> bool {
        self.scene().objects.iter().any(|o| o.id == id)
            || self.scene().lighting.light(id).is_some()
            || self.document.authoring.object_ids().any(|used| used == id)
            || self
                .document
                .undo
                .iter()
                .chain(&self.document.redo)
                .any(|c| c.contains_id(id))
    }

    // Geometry and lights share a namespace, including reversible identities.
    fn reserved_target_ids(&self) -> HashSet<&str> {
        let mut used: HashSet<&str> = self.scene().objects.iter().map(|o| o.id.as_str()).collect();
        used.extend(self.scene().lighting.lights().map(|(id, _)| id));
        used.extend(self.document.authoring.object_ids());
        for change in self.document.undo.iter().chain(&self.document.redo) {
            used.extend(change.object_ids());
            for delta in &change.scene {
                match delta {
                    SceneDelta::Create { object, .. } | SceneDelta::Delete { object, .. } => {
                        used.insert(&object.id);
                    }
                    SceneDelta::Name { id, .. }
                    | SceneDelta::Transform { id, .. }
                    | SceneDelta::Color { id, .. }
                    | SceneDelta::Mesh { id, .. }
                    | SceneDelta::Material { id, .. }
                    | SceneDelta::Collider { id, .. } => {
                        used.insert(id);
                    }
                    SceneDelta::Transforms { changes } => {
                        used.extend(changes.iter().map(|e| e.id.as_str()));
                    }
                    SceneDelta::Objects { created, deleted } => {
                        used.extend(created.iter().chain(deleted).map(|e| e.object.id.as_str()));
                    }
                    SceneDelta::Lighting { before, after } => {
                        used.extend(before.lights().chain(after.lights()).map(|(id, _)| id));
                    }
                    SceneDelta::Display { .. } => {}
                }
            }
        }
        used
    }

    /// Allocate bounded IDs with one transient history scan, including grouped reservations.
    pub fn allocate_object_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        allocate_ids("object", count, MAX_OBJECTS, self.reserved_target_ids())
    }

    pub fn allocate_light_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        allocate_ids("light", count, MAX_LIGHTS, self.reserved_target_ids())
    }

    /// Append a validated object. Its ID cannot collide with current or reversible state.
    pub fn create(&mut self, expected_revision: u64, object: Object) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        self.insert_object(object, self.scene().objects.len())
    }
    fn insert_object(&mut self, object: Object, index: usize) -> Result<u64, SceneError> {
        object.validate()?;
        if self.contains_object_id(&object.id) {
            return Err(SceneError::Invalid(format!(
                "object id {} is already present or reserved by history",
                object.id
            )));
        }
        self.commit_change(SceneDelta::Create { object, index })
    }
    /// Duplicate immediately after the source, preserving its properties and optional name.
    /// A supplied name overrides the source label; object identity is always newly supplied.
    pub fn duplicate(
        &mut self,
        expected_revision: u64,
        source: &str,
        new_id: &str,
        name: Option<String>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if let Some(light) = self.scene().lighting.light(source) {
            let mut light = light.clone();
            if name.is_some() {
                light.name = name;
            }
            return self
                .create_light(expected_revision, new_id, light)
                .map(|edit| edit.revision);
        }
        let index = self
            .scene()
            .objects
            .iter()
            .position(|o| o.id == source)
            .ok_or_else(|| SceneError::ObjectNotFound(source.into()))?;
        let mut object = self.scene().objects[index].clone();
        object.id = new_id.into();
        if name.is_some() {
            object.name = name;
        }
        object.validate()?;
        if self.contains_object_id(new_id) {
            return Err(SceneError::Invalid(
                "duplicate object ID is present or reserved".into(),
            ));
        }
        let mut change: Change = SceneDelta::Create {
            object,
            index: index + 1,
        }
        .into();
        change.bindings = self.cloned_bindings(&[(source.into(), new_id.into())])?;
        self.commit_change(change)
    }
    /// SceneDelta only the displayed name. None restores the immutable ID as its label.
    pub fn rename(
        &mut self,
        expected_revision: u64,
        id: &str,
        name: Option<String>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_name(name.as_deref())?;
        if let Some(light) = self.scene().lighting.light(id) {
            let mut light = light.clone();
            light.name = name;
            return self.set_light(expected_revision, id, light);
        }
        let object = self
            .scene()
            .objects
            .iter()
            .find(|o| o.id == id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.into()))?;
        if object.name == name {
            return Ok(self.scene().revision);
        }
        self.commit_change(SceneDelta::Name {
            id: id.into(),
            before: object.name.clone(),
            after: name,
        })
    }
    pub fn delete(&mut self, expected_revision: u64, id: &str) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if self.scene().lighting.light(id).is_some() {
            return self.remove_light(expected_revision, id);
        }
        let index = self
            .scene()
            .objects
            .iter()
            .position(|o| o.id == id)
            .ok_or_else(|| SceneError::ObjectNotFound(id.into()))?;
        let mut change: Change = SceneDelta::Delete {
            object: self.scene().objects[index].clone(),
            index,
        }
        .into();
        change.bindings = self.owned_binding_deletions(&[id]);
        self.commit_change(change)
    }
    fn commit_change(&mut self, change: impl Into<Change>) -> Result<u64, SceneError> {
        let change = change.into();
        let next = self
            .scene()
            .revision
            .checked_add(1)
            .ok_or(SceneError::RevisionOverflow)?;
        let mut candidate = self.scene().clone();
        let mut authoring = self.document.authoring.clone();
        change.apply(&mut candidate, &mut authoring, false)?;
        candidate.revision = next;
        candidate.validate()?;
        let age_evictions = usize::from(self.document.undo.len() == HISTORY_LIMIT);
        let mut undo: Vec<_> = self.document.undo.iter().skip(age_evictions).collect();
        undo.push(&change);
        let mut byte_evictions = 0;
        loop {
            match check_serialized_size(&candidate, &authoring, &undo[byte_evictions..], &[]) {
                Ok(()) => break,
                Err(SceneError::Capacity) if byte_evictions + 1 < undo.len() => byte_evictions += 1,
                Err(error) => return Err(error),
            }
        }
        // No mutation occurs until a suffix retaining the newest delta is admitted.
        self.document.scene = candidate;
        self.document.authoring = authoring;
        self.document.undo.drain(..age_evictions + byte_evictions);
        self.history_evictions = self.history_evictions.saturating_add(byte_evictions);
        self.document.undo.push(change);
        self.document.redo.clear();
        Ok(next)
    }
    /// One atomic transform gesture: one revision and one undo entry for all targets.
    pub fn edit_transforms(
        &mut self,
        expected_revision: u64,
        edits: &[(String, Transform)],
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if edits.len() > MAX_OBJECTS {
            return Err(SceneError::Invalid(
                "transform group exceeds 256 edits".into(),
            ));
        }
        let mut ids = HashSet::new();
        let mut changes = Vec::new();
        let mut lighting = None;
        for (id, transform) in edits {
            if !ids.insert(id) {
                return Err(SceneError::Invalid("duplicate transform target".into()));
            }
            transform.validate()?;
            let before = self
                .scene()
                .target_transform(id)
                .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))?;
            if self.scene().lighting.light(id).is_some() {
                if transform.scale != [1.; 3] {
                    return Err(SceneError::Invalid(
                        "lights cannot be scaled; use intensity or range".into(),
                    ));
                }
                if before != transform {
                    let after = lighting.get_or_insert_with(|| self.scene().lighting.clone());
                    let Lighting::Authored { lights, .. } = after else {
                        unreachable!()
                    };
                    lights.get_mut(id).expect("resolved light").transform = transform.clone();
                }
            } else if before != transform {
                changes.push(TransformDelta {
                    id: id.clone(),
                    before: before.clone(),
                    after: transform.clone(),
                });
            }
        }
        let mut scene = Vec::new();
        if !changes.is_empty() {
            scene.push(SceneDelta::Transforms { changes });
        }
        if let Some(after) = lighting {
            scene.push(SceneDelta::Lighting {
                before: self.scene().lighting.clone(),
                after,
            });
        }
        if scene.is_empty() {
            return Ok(self.scene().revision);
        }
        self.commit_change(Change {
            scene,
            ..Change::default()
        })
    }
    /// Append a copied group atomically. IDs must be fresh in current and reversible state.
    pub fn create_many(
        &mut self,
        expected_revision: u64,
        objects: Vec<Object>,
    ) -> Result<u64, SceneError> {
        self.create_many_with_materials(expected_revision, objects, BTreeMap::new())
    }
    /// Append copied objects and fresh scene-owned materials as one transaction.
    /// Material identities cannot collide with current or reversible state.
    pub fn create_many_with_materials(
        &mut self,
        expected_revision: u64,
        objects: Vec<Object>,
        materials: BTreeMap<String, OpaqueMaterial>,
    ) -> Result<u64, SceneError> {
        self.create_many_with_materials_and_lights(
            expected_revision,
            objects,
            materials,
            BTreeMap::new(),
        )
        .map(|edit| edit.revision)
    }

    /// Append a clipboard group atomically, preserving scene-global ambient.
    /// Receipt IDs identify incoming lights plus any explicit promoted Sun.
    pub fn create_many_with_materials_and_lights(
        &mut self,
        expected_revision: u64,
        objects: Vec<Object>,
        materials: BTreeMap<String, OpaqueMaterial>,
        lights: BTreeMap<String, Light>,
    ) -> Result<LightingEdit, SceneError> {
        self.check_revision(expected_revision)?;
        if objects.is_empty() && materials.is_empty() && lights.is_empty() {
            return Ok(LightingEdit {
                revision: self.scene().revision,
                created_ids: Vec::new(),
            });
        }
        if objects.len() + lights.len() > MAX_OBJECTS
            || self.scene().target_count() + objects.len() + lights.len() > MAX_OBJECTS
        {
            return Err(SceneError::Invalid(
                "selection exceeds 256 geometry and light targets".into(),
            ));
        }
        if materials.len() > MAX_MATERIALS
            || self.scene().materials.len() + materials.len() > MAX_MATERIALS
        {
            return Err(SceneError::Invalid(
                "material group exceeds 64 materials".into(),
            ));
        }
        for (id, value) in &materials {
            validate_id(id)?;
            value.validate()?;
            if self.contains_material_id(id) {
                return Err(SceneError::Invalid(
                    "new material ID collides with current or reversible state".into(),
                ));
            }
        }
        let mut ids = HashSet::new();
        for object in &objects {
            object.validate()?;
            if !ids.insert(&object.id) || self.contains_object_id(&object.id) {
                return Err(SceneError::Invalid(
                    "new object ID collides with current or reversible state".into(),
                ));
            }
        }
        for (id, light) in &lights {
            validate_id(id)?;
            light.validate()?;
            if !ids.insert(id) || self.contains_object_id(id) {
                return Err(SceneError::Invalid(
                    "new target ID collides with current or reversible state".into(),
                ));
            }
        }
        let (lighting, created_ids) = if lights.is_empty() {
            (None, Vec::new())
        } else {
            let incoming_directional = lights
                .values()
                .any(|light| matches!(light.kind, LightKind::Directional { .. }));
            let incoming_ids = ids.iter().map(|id| id.as_str()).collect::<Vec<_>>();
            let (mut after, mut created_ids) =
                self.promoted_lighting(&incoming_ids, incoming_directional)?;
            let Lighting::Authored { lights: target, .. } = &mut after else {
                unreachable!()
            };
            created_ids.extend(lights.keys().cloned());
            target.extend(lights);
            (Some(after), created_ids)
        };
        let start = self.scene().objects.len();
        let created: Vec<_> = objects
            .into_iter()
            .enumerate()
            .map(|(i, object)| IndexedObject {
                object,
                index: start + i,
            })
            .collect();
        let mut scene = if created.is_empty() {
            Vec::new()
        } else {
            vec![SceneDelta::Objects {
                created,
                deleted: Vec::new(),
            }]
        };
        if let Some(after) = lighting {
            scene.push(SceneDelta::Lighting {
                before: self.scene().lighting.clone(),
                after,
            });
        }
        let revision = self.commit_change(Change {
            scene,
            materials: materials
                .into_iter()
                .map(|(id, value)| ValueDelta {
                    id,
                    before: None,
                    after: Some(value),
                })
                .collect(),
            ..Change::default()
        })?;
        Ok(LightingEdit {
            revision,
            created_ids,
        })
    }
    /// Delete a selection atomically, retaining its original scene order for undo.
    pub fn delete_many(
        &mut self,
        expected_revision: u64,
        ids: &[String],
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if ids.is_empty() {
            return Ok(self.scene().revision);
        }
        if ids.len() > MAX_OBJECTS {
            return Err(SceneError::Invalid("selection exceeds 256 targets".into()));
        }
        let mut unique = HashSet::new();
        let mut deleted = Vec::with_capacity(ids.len());
        let mut lighting = None;
        for id in ids {
            if !unique.insert(id) {
                return Err(SceneError::Invalid("duplicate deletion target".into()));
            }
            if self.scene().lighting.light(id).is_some() {
                let after = lighting.get_or_insert_with(|| self.scene().lighting.clone());
                let Lighting::Authored { lights, .. } = after else {
                    unreachable!()
                };
                lights.remove(id);
            } else {
                let index = self
                    .scene()
                    .objects
                    .iter()
                    .position(|o| &o.id == id)
                    .ok_or_else(|| SceneError::ObjectNotFound(id.clone()))?;
                deleted.push(IndexedObject {
                    object: self.scene().objects[index].clone(),
                    index,
                });
            }
        }
        deleted.sort_unstable_by_key(|e| e.index);
        let mut scene = Vec::new();
        if !deleted.is_empty() {
            scene.push(SceneDelta::Objects {
                created: Vec::new(),
                deleted,
            });
        }
        if let Some(after) = lighting {
            scene.push(SceneDelta::Lighting {
                before: self.scene().lighting.clone(),
                after,
            });
        }
        self.commit_change(Change {
            scene,
            bindings: self
                .owned_binding_deletions(&ids.iter().map(String::as_str).collect::<Vec<_>>()),
            ..Change::default()
        })
    }
    /// Copy selected objects in caller order, appending the copies as one undo step.
    pub fn duplicate_many(
        &mut self,
        expected_revision: u64,
        pairs: &[(String, String)],
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if pairs.len() > MAX_OBJECTS {
            return Err(SceneError::Invalid("selection exceeds 256 targets".into()));
        }
        if pairs.is_empty() {
            return Ok(self.scene().revision);
        }
        let mut sources = HashSet::new();
        let mut fresh_ids = HashSet::new();
        let mut objects = Vec::new();
        let mut lighting = None;
        for (source, id) in pairs {
            if !sources.insert(source) {
                return Err(SceneError::Invalid("duplicate source in selection".into()));
            }
            validate_id(id)?;
            if !fresh_ids.insert(id) || self.contains_object_id(id) {
                return Err(SceneError::Invalid(
                    "new target ID collides with current or reversible state".into(),
                ));
            }
            if let Some(light) = self.scene().lighting.light(source) {
                let after = lighting.get_or_insert_with(|| self.scene().lighting.clone());
                let Lighting::Authored { lights, .. } = after else {
                    unreachable!()
                };
                lights.insert(id.clone(), light.clone());
            } else {
                let mut object = self
                    .scene()
                    .objects
                    .iter()
                    .find(|o| &o.id == source)
                    .ok_or_else(|| SceneError::ObjectNotFound(source.clone()))?
                    .clone();
                object.id.clone_from(id);
                objects.push(object);
            }
        }
        let start = self.scene().objects.len();
        let mut scene = Vec::new();
        if !objects.is_empty() {
            scene.push(SceneDelta::Objects {
                created: objects
                    .into_iter()
                    .enumerate()
                    .map(|(i, object)| IndexedObject {
                        object,
                        index: start + i,
                    })
                    .collect(),
                deleted: Vec::new(),
            });
        }
        if let Some(after) = lighting {
            scene.push(SceneDelta::Lighting {
                before: self.scene().lighting.clone(),
                after,
            });
        }
        self.commit_change(Change {
            scene,
            bindings: self.cloned_bindings(pairs)?,
            ..Change::default()
        })
    }
    fn check_revision(&self, expected: u64) -> Result<(), SceneError> {
        let actual = self.scene().revision;
        if expected != actual {
            return Err(SceneError::StaleRevision { expected, actual });
        }
        Ok(())
    }
    pub fn edit(
        &mut self,
        expected_revision: u64,
        id: &str,
        transform: &Transform,
    ) -> Result<u64, SceneError> {
        self.edit_transforms(expected_revision, &[(id.into(), transform.clone())])
    }

    /// Replace one object's authored RGBA channels with one shared Undo entry.
    /// Invalid, stale and unchanged requests preserve history and save state.
    pub fn set_color(
        &mut self,
        expected_revision: u64,
        object_id: &str,
        color: [f32; 4],
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_color(&color)?;
        let object = self
            .scene()
            .objects
            .iter()
            .find(|o| o.id == object_id)
            .ok_or_else(|| SceneError::ObjectNotFound(object_id.into()))?;
        if object.color == color {
            return Ok(self.scene().revision);
        }
        self.commit_change(SceneDelta::Color {
            id: object_id.into(),
            before: object.color,
            after: color,
        })
    }

    fn reserved_material_ids(&self) -> HashSet<&str> {
        let mut used: HashSet<_> = self.scene().materials.keys().map(String::as_str).collect();
        used.extend(
            self.scene()
                .objects
                .iter()
                .filter_map(|o| o.material.as_deref()),
        );
        for change in self.document.undo.iter().chain(&self.document.redo) {
            used.extend(change.materials.iter().map(|d| d.id.as_str()));
            for delta in &change.scene {
                match delta {
                    SceneDelta::Material { before, after, .. } => {
                        used.extend([before.as_deref(), after.as_deref()].into_iter().flatten());
                    }
                    SceneDelta::Create { object, .. } | SceneDelta::Delete { object, .. } => {
                        used.extend(object.material.as_deref());
                    }
                    SceneDelta::Objects { created, deleted } => {
                        used.extend(
                            created
                                .iter()
                                .chain(deleted)
                                .filter_map(|e| e.object.material.as_deref()),
                        );
                    }
                    _ => {}
                }
            }
        }
        used
    }
    /// Material identity is separate from the object/source namespaces.
    pub fn contains_material_id(&self, id: &str) -> bool {
        self.reserved_material_ids().contains(id)
    }
    pub fn allocate_material_ids(&self, count: usize) -> Result<Vec<String>, SceneError> {
        allocate_ids(
            "material",
            count,
            MAX_MATERIALS,
            self.reserved_material_ids(),
        )
    }
    /// Create a fresh identity or replace a live shared material in one Undo step.
    /// Absent historical identities cannot be recycled until their history expires.
    pub fn set_material(
        &mut self,
        expected_revision: u64,
        id: &str,
        value: OpaqueMaterial,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        value.validate()?;
        let before = self.scene().materials.get(id).cloned();
        if before.as_ref() == Some(&value) {
            return Ok(self.scene().revision);
        }
        if before.is_none() && self.contains_material_id(id) {
            return Err(SceneError::Invalid(
                "material ID is reserved by history".into(),
            ));
        }
        self.commit_change(Change {
            materials: vec![ValueDelta {
                id: id.into(),
                before,
                after: Some(value),
            }],
            ..Change::default()
        })
    }
    /// Copy a material under a fresh identity; None preserves the source label.
    pub fn copy_material(
        &mut self,
        expected_revision: u64,
        source: &str,
        id: &str,
        name: Option<String>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if self.contains_material_id(id) {
            return Err(SceneError::Invalid(
                "material ID is present or reserved".into(),
            ));
        }
        let mut value = self
            .scene()
            .materials
            .get(source)
            .cloned()
            .ok_or_else(|| SceneError::Invalid(format!("material {source} does not exist")))?;
        if name.is_some() {
            value.name = name;
        }
        self.set_material(expected_revision, id, value)
    }
    /// Delete only an unassigned live material. Undo keeps its identity reserved.
    pub fn remove_material(&mut self, expected_revision: u64, id: &str) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        let Some(before) = self.scene().materials.get(id).cloned() else {
            return Ok(self.scene().revision);
        };
        if self
            .scene()
            .objects
            .iter()
            .any(|o| o.material.as_deref() == Some(id))
        {
            return Err(SceneError::Invalid(format!(
                "material {id} is assigned to an object"
            )));
        }
        self.commit_change(Change {
            materials: vec![ValueDelta {
                id: id.into(),
                before: Some(before),
                after: None,
            }],
            ..Change::default()
        })
    }
    /// Assign a scene-owned material, preserving the object's existing RGBA tint.
    pub fn assign_material(
        &mut self,
        expected_revision: u64,
        object_id: &str,
        material: Option<String>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if let Some(id) = &material {
            validate_id(id)?;
            if !self.scene().materials.contains_key(id) {
                return Err(SceneError::Invalid(format!("material {id} does not exist")));
            }
        }
        let object = self
            .scene()
            .objects
            .iter()
            .find(|o| o.id == object_id)
            .ok_or_else(|| SceneError::ObjectNotFound(object_id.into()))?;
        if object.material == material {
            return Ok(self.scene().revision);
        }
        self.commit_change(SceneDelta::Material {
            id: object_id.into(),
            before: object.material.clone(),
            after: material,
        })
    }
    /// Atomically create and assign one fresh material: one revision and Undo entry.
    pub fn create_and_assign_material(
        &mut self,
        expected_revision: u64,
        object_id: &str,
        id: &str,
        value: OpaqueMaterial,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        validate_id(id)?;
        value.validate()?;
        if self.contains_material_id(id) {
            return Err(SceneError::Invalid(
                "material ID is present or reserved".into(),
            ));
        }
        let object = self
            .scene()
            .objects
            .iter()
            .find(|o| o.id == object_id)
            .ok_or_else(|| SceneError::ObjectNotFound(object_id.into()))?;
        self.commit_change(Change {
            materials: vec![ValueDelta {
                id: id.into(),
                before: None,
                after: Some(value),
            }],
            scene: vec![SceneDelta::Material {
                id: object_id.into(),
                before: object.material.clone(),
                after: Some(id.into()),
            }],
            ..Change::default()
        })
    }

    /// Replace or remove one primitive component through shared authored history.
    /// Invalid, stale and unchanged edits preserve revision, history and save state.
    pub fn set_collider(
        &mut self,
        expected_revision: u64,
        object_id: &str,
        collider: Option<Collider>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        let object = self
            .scene()
            .objects
            .iter()
            .find(|object| object.id == object_id)
            .ok_or_else(|| SceneError::ObjectNotFound(object_id.into()))?;
        if let Some(value) = collider {
            value.resolve(&object.transform)?;
        }
        if object.collider == collider {
            return Ok(self.scene().revision);
        }
        self.commit_change(SceneDelta::Collider {
            id: object_id.into(),
            before: object.collider,
            after: collider,
        })
    }

    /// Assign a syntactically valid stable asset ID, or None for a cube.
    /// Registry lookup belongs to the caller; failed/no-op commands preserve history.
    pub fn set_mesh(
        &mut self,
        expected_revision: u64,
        object_id: &str,
        mesh: Option<String>,
    ) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        if let Some(id) = &mesh {
            validate_id(id)?;
        }
        let object = self
            .document
            .scene
            .objects
            .iter()
            .find(|o| o.id == object_id)
            .ok_or_else(|| SceneError::ObjectNotFound(object_id.into()))?;
        if object.mesh == mesh {
            return Ok(self.document.scene.revision);
        }
        self.commit_change(SceneDelta::Mesh {
            id: object_id.into(),
            before: object.mesh.clone(),
            after: mesh,
        })
    }

    pub fn undo(&mut self, expected_revision: u64) -> Result<u64, SceneError> {
        self.history(expected_revision, true)
    }
    pub fn redo(&mut self, expected_revision: u64) -> Result<u64, SceneError> {
        self.history(expected_revision, false)
    }
    fn history(&mut self, expected_revision: u64, undo: bool) -> Result<u64, SceneError> {
        self.check_revision(expected_revision)?;
        let change = if undo {
            self.document.undo.last().ok_or(SceneError::NoUndo)?
        } else {
            self.document.redo.last().ok_or(SceneError::NoRedo)?
        };
        let next = self
            .scene()
            .revision
            .checked_add(1)
            .ok_or(SceneError::RevisionOverflow)?;
        let mut candidate = self.scene().clone();
        let mut authoring = self.document.authoring.clone();
        change.apply(&mut candidate, &mut authoring, undo)?;
        candidate.revision = next;
        candidate.validate()?;
        // Admission/load already bounded every reachable scene with maximum revision
        // width. Moving this same delta between stacks preserves that invariant.
        let change = if undo {
            self.document.undo.pop().ok_or(SceneError::NoUndo)?
        } else {
            self.document.redo.pop().ok_or(SceneError::NoRedo)?
        };
        self.document.scene = candidate;
        self.document.authoring = authoring;
        if undo {
            self.document.redo.push(change);
        } else {
            self.document.undo.push(change);
        }
        self.document.scene.revision = next;
        Ok(next)
    }
    /// Save to the opened file only if the full saved baseline still matches.
    /// New destinations must not exist. Failure preserves all in-memory state.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), SceneError> {
        let lock = SceneLock::acquire(path.as_ref())?;
        let existing = self.path.as_ref() == Some(&lock.path);
        if existing && self.saved.as_ref() != Some(&Document::load(&lock.path)?) {
            return Err(SceneError::Conflict);
        }
        self.document.write(&lock.path, !existing)?;
        self.document.format_version = FORMAT_VERSION;
        self.loaded_format_version = FORMAT_VERSION;
        self.saved = Some(self.document.clone());
        self.path = Some(lock.path.clone());
        Ok(())
    }
}

// Same stable sibling-lock convention as the logistics authoring surface,
// independently implemented to keep scene dependencies domain-neutral.
struct SceneLock {
    path: PathBuf,
    _file: File,
}
impl Drop for SceneLock {
    fn drop(&mut self) {
        // End the lock scope even if a concurrent fork temporarily inherited the FD.
        let _ = fs2::FileExt::unlock(&self._file);
    }
}
impl SceneLock {
    fn acquire(path: &Path) -> Result<Self, SceneError> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()?;
        let name = path
            .file_name()
            .ok_or_else(|| SceneError::Invalid("file name required".into()))?;
        let path = parent.join(name);
        reject_nonregular(&path)?;
        let mut lock_name = OsString::from(".");
        lock_name.push(name);
        lock_name.push(".ruster-lock");
        let lock_path = parent.join(lock_name);
        reject_nonregular(&lock_path)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path)?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(|error| {
            if error.kind() == std::io::ErrorKind::WouldBlock
                || error.raw_os_error() == fs2::lock_contended_error().raw_os_error()
            {
                SceneError::Busy
            } else {
                SceneError::Io(error)
            }
        })?;
        reject_nonregular(&path)?;
        Ok(Self { path, _file: file })
    }
}
fn reject_nonregular(path: &Path) -> Result<(), SceneError> {
    match path.symlink_metadata() {
        Ok(m) if !m.is_file() => Err(SceneError::Invalid(
            "scene file and lock must be regular files".into(),
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// A static configurator preview: ticks are lifecycle observations, not physics.
/// The authored snapshot changes only on explicit reset through this API.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Preview {
    pub scene: Scene,
    pub tick: u64,
}
impl Preview {
    pub fn new(scene: &Scene) -> Self {
        Self {
            scene: scene.clone(),
            tick: 0,
        }
    }
    pub fn reset(&mut self, scene: &Scene) {
        self.scene = scene.clone();
        self.tick = 0;
    }
    pub fn step(&mut self) -> Result<u64, SceneError> {
        let next = self.tick.checked_add(1).ok_or(SceneError::TickOverflow)?;
        self.tick = next;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_value(document: &Document) -> serde_json::Value {
        let mut value = serde_json::to_value(document).unwrap();
        value.as_object_mut().unwrap().remove("authoring");
        for stack in ["undo", "redo"] {
            value[stack] = serde_json::to_value(
                (if stack == "undo" {
                    &document.undo
                } else {
                    &document.redo
                })
                .iter()
                .map(|c| {
                    assert!(!c.has_authoring());
                    assert_eq!(c.scene.len(), 1);
                    &c.scene[0]
                })
                .collect::<Vec<_>>(),
            )
            .unwrap();
        }
        value
    }

    fn source(text: &str) -> BehaviorSource {
        BehaviorSource {
            label: Some("Recoverable draft".into()),
            language_id: "unknown-backend".into(),
            host_api_version: 1,
            text: text.into(),
        }
    }
    fn binding(owner: &str, source: &str) -> BehaviorBinding {
        BehaviorBinding {
            target: BindingTarget::Object(owner.into()),
            source_id: source.into(),
            enabled: true,
            properties: BTreeMap::new(),
            state_defaults: StateFields::new(),
            commands: BTreeMap::new(),
        }
    }

    // One owner exercises HUD-only persistence/history, guarded new references,
    // and repairable references across field deletion and binding identity reuse.
    #[test]
    fn hud_history_roundtrip_and_reference_repair_are_atomic() {
        use hud::{Hud, HudLabel, HudSource, HudValueType};
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("hud.json");
        let mut session = Session::new(Scene::empty("hud-scene")).unwrap();
        let mut hud = Hud {
            enabled: true,
            elements: vec![HudLabel {
                id: "instructions".into(),
                source: HudSource::Static {
                    text: "Ready".into(),
                },
            }],
            ..Hud::default()
        };
        assert_eq!(session.set_hud(0, hud.clone()).unwrap(), 1);
        assert_eq!(session.set_hud(1, hud.clone()).unwrap(), 1);
        assert!(!session.authoring().is_empty());
        assert!(session.set_hud(0, hud.clone()).is_err());
        session.undo(1).unwrap();
        assert!(session.hud().is_default());
        assert!(session.set_hud(2, hud.clone()).is_err());
        session.redo(2).unwrap();
        session.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.hud(), &hud);
        assert!(Scene::load(&path).is_err());

        hud.elements.push(HudLabel {
            id: "score".into(),
            source: HudSource::State {
                binding_id: "application".into(),
                field: "score".into(),
                value_type: HudValueType::Number,
                prefix: "Score ".into(),
                suffix: String::new(),
                precision: 2,
            },
        });
        let original = reopened.snapshot_bytes().unwrap();
        assert!(reopened.set_hud(3, hud.clone()).is_err());
        assert_eq!(reopened.snapshot_bytes().unwrap(), original);
        let mut application = binding("unused", "missing-source");
        application.target = BindingTarget::Application;
        application
            .state_defaults
            .insert("score".into(), StateValue::Number(1.0));
        let mut missing_source = Session::new(Scene::empty("missing-source-hud")).unwrap();
        missing_source
            .set_binding(0, "application", Some(application.clone()))
            .unwrap();
        let missing_snapshot = missing_source.snapshot_bytes().unwrap();
        assert!(missing_source.set_hud(1, hud.clone()).is_err());
        assert_eq!(missing_source.snapshot_bytes().unwrap(), missing_snapshot);
        application.source_id = "hud-source".into();
        reopened
            .create_behavior(
                3,
                "hud-source",
                source("unfinished source"),
                "application",
                application.clone(),
            )
            .unwrap();
        reopened.set_hud(4, hud.clone()).unwrap();
        reopened
            .set_state_default(5, "application", "score", None)
            .unwrap();
        hud.margin = 20.0;
        reopened.set_hud(6, hud.clone()).unwrap();
        reopened.set_binding(7, "application", None).unwrap();
        let broken = reopened.snapshot_bytes().unwrap();
        assert!(reopened
            .set_binding(8, "application", Some(application))
            .is_err());
        assert_eq!(reopened.snapshot_bytes().unwrap(), broken);
        assert!(reopened.contains_binding_id("application"));
        reopened.save(&path).unwrap();
        let mut restored = Session::open(&path).unwrap();
        assert_eq!(restored.hud(), &hud);
        restored.undo(8).unwrap();
        restored.undo(9).unwrap();
        restored.undo(10).unwrap();
        assert_eq!(
            restored.binding("application").unwrap().state_defaults["score"],
            StateValue::Number(1.0)
        );
        assert_eq!(restored.hud().margin, 12.0);
        restored.set_source(11, "hud-source", None).unwrap();
        let mut repairable = restored.hud().clone();
        repairable.margin = 24.0;
        restored.set_hud(12, repairable.clone()).unwrap();
        restored.save(&path).unwrap();
        assert_eq!(Session::open(&path).unwrap().hud(), &repairable);
    }

    // A button adds no code execution authority to scene storage. New constant
    // bindings validate atomically, while broken retained targets remain repairable.
    #[test]
    fn hud_button_history_preserves_broken_targets_and_rejects_changed_constants() {
        use hud::{Hud, HudCommandBinding, HudLabel, HudSource};
        let mut session = Session::new(Scene::empty("buttons")).unwrap();
        let mut api5 = source("unfinished API5 source");
        api5.host_api_version = 5;
        let arguments = StateFields::from([("amount".into(), StateValue::Number(-0.0))]);
        let mut application = binding("unused", "source");
        application.target = BindingTarget::Application;
        application.commands.insert(
            "act".into(),
            CommandDefinition {
                arguments: arguments.clone(),
            },
        );
        session
            .create_behavior(0, "source", api5, "app", application.clone())
            .unwrap();
        let mut hud = Hud {
            enabled: true,
            elements: vec![HudLabel {
                id: "button".into(),
                source: HudSource::Button {
                    caption: "Act".into(),
                    command: HudCommandBinding {
                        binding_id: "app".into(),
                        command_id: "act".into(),
                        arguments,
                    },
                },
            }],
            ..Hud::default()
        };
        let original = session.snapshot_bytes().unwrap();
        let mut invalid = hud.clone();
        if let HudSource::Button { command, .. } = &mut invalid.elements[0].source {
            command.arguments.clear();
        }
        assert!(session.set_hud(1, invalid).is_err());
        assert_eq!(session.snapshot_bytes().unwrap(), original);
        session.set_hud(1, hud.clone()).unwrap();
        assert_eq!(session.set_hud(2, hud.clone()).unwrap(), 2);
        session
            .set_command_definition(2, "app", "act", None)
            .unwrap();
        hud.margin = 20.0;
        if let HudSource::Button { caption, .. } = &mut hud.elements[0].source {
            *caption = "Repair command".into();
        }
        session.set_hud(3, hud.clone()).unwrap();
        let broken = session.snapshot_bytes().unwrap();
        let mut changed = hud.clone();
        if let HudSource::Button { command, .. } = &mut changed.elements[0].source {
            command
                .arguments
                .insert("amount".into(), StateValue::Number(0.0));
        }
        assert!(
            session.set_hud(4, changed).is_err(),
            "even negative-zero changes require revalidation"
        );
        assert_eq!(session.snapshot_bytes().unwrap(), broken);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("buttons.json");
        session.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.hud(), &hud);
        assert_eq!(reopened.loaded_format_version(), 16);
        reopened.undo(4).unwrap();
        reopened.undo(5).unwrap();
        let repaired = hud::resolve_hud(reopened.hud(), reopened.authoring(), |_, _| {
            panic!("no VM/state reads")
        });
        assert_eq!(repaired.labels[0].diagnostic, None);
        reopened.set_binding(6, "app", None).unwrap();
        assert!(reopened.contains_binding_id("app"));
        assert!(reopened.set_binding(7, "app", Some(application)).is_err());
        let mut retained = reopened.hud().clone();
        retained.spacing = 10.0;
        reopened.set_hud(7, retained.clone()).unwrap();
        reopened.save(&path).unwrap();
        assert_eq!(Session::open(&path).unwrap().hud(), &retained);
        reopened.undo(8).unwrap();
        reopened.undo(9).unwrap();
        reopened.undo(10).unwrap();
        assert!(reopened.hud().elements.is_empty());
        assert!(
            reopened.set_hud(11, retained).is_err(),
            "button ID remains reserved by redo"
        );
    }

    // Future HUD owner fields must fail closed for every prior format, even
    // empty/null history entries. Unrelated source/property names stay opaque.
    #[test]
    fn hud_requires_owning_scene_version_in_definition_and_history() {
        let session = Session::new(Scene::empty("old-scene")).unwrap();
        let value = serde_json::to_value(&session.document).unwrap();
        for version in 1..15 {
            let mut legacy = value.clone();
            legacy["format_version"] = version.into();
            assert!(Document::decode(&serde_json::to_vec(&legacy).unwrap()).is_ok());
            for location in ["authoring", "undo", "redo"] {
                for contents in [
                    serde_json::Value::Null,
                    serde_json::json!({}),
                    serde_json::json!([]),
                ] {
                    let mut bad = legacy.clone();
                    if location == "authoring" {
                        bad["authoring"] =
                            serde_json::json!({"sources":{},"bindings":{},"hud":contents});
                    } else {
                        bad[location] = serde_json::json!([{"hud":contents}]);
                    }
                    assert!(Document::decode(&serde_json::to_vec(&bad).unwrap()).is_err());
                }
            }
        }
        // Scene15 static/state HUD documents remain valid and upgrade normally.
        // Buttons are rejected at all actual current/history owner locations.
        let mut labels = Session::new(Scene::empty("v15-hud")).unwrap();
        labels
            .set_hud(
                0,
                hud::Hud {
                    enabled: true,
                    elements: vec![hud::HudLabel {
                        id: "old".into(),
                        source: hud::HudSource::Static {
                            text: "Ready".into(),
                        },
                    }],
                    ..hud::Hud::default()
                },
            )
            .unwrap();
        let mut v15 = serde_json::to_value(&labels.document).unwrap();
        v15["format_version"] = 15.into();
        assert!(Document::decode(&serde_json::to_vec(&v15).unwrap()).is_ok());
        let button = serde_json::json!({"id":"button", "source":{
            "kind":"button", "caption":"Act", "command":{
                "binding_id":"missing", "command_id":"missing", "arguments":{}
            }
        }});
        for version in 1..16 {
            for owner in [
                "current",
                "undo-before",
                "undo-after",
                "redo-before",
                "redo-after",
            ] {
                let mut bad = v15.clone();
                bad["format_version"] = version.into();
                match owner {
                    "current" => {
                        bad["authoring"]["hud"]["elements"] = serde_json::json!([button.clone()])
                    }
                    "undo-before" => {
                        bad["undo"][0]["hud"]["before"]["elements"] =
                            serde_json::json!([button.clone()])
                    }
                    "undo-after" => {
                        bad["undo"][0]["hud"]["after"]["elements"] =
                            serde_json::json!([button.clone()])
                    }
                    "redo-before" => {
                        bad["redo"] = serde_json::json!([{"hud":{"before":{"elements":[button.clone()]}, "after":{}}}])
                    }
                    "redo-after" => {
                        bad["redo"] = serde_json::json!([{"hud":{"before":{}, "after":{"elements":[button.clone()]}}}])
                    }
                    _ => unreachable!(),
                }
                assert!(Document::decode(&serde_json::to_vec(&bad).unwrap())
                    .unwrap_err()
                    .to_string()
                    .contains("scene v16"));
            }
        }
        let disabled = hud::Hud {
            margin: 24.0,
            ..hud::Hud::default()
        };
        let mut retained = session;
        retained.set_hud(0, disabled.clone()).unwrap();
        assert_eq!(
            Session::from_snapshot_bytes(&retained.snapshot_bytes().unwrap())
                .unwrap()
                .hud(),
            &disabled
        );
    }

    // Unique risk: broken source, absent backends/references and obsolete property values
    // must survive actual storage, while geometry readers must never silently strip them.
    #[test]
    fn unfinished_authoring_roundtrips_and_duplicate_keys_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("authoring.json");
        let mut session = Session::new(Scene::demo()).unwrap();
        session
            .set_source(0, "source-1", Some(source("function broken(\n")))
            .unwrap();
        let mut b = binding("left", "missing-source");
        b.properties = BTreeMap::from([
            ("old_number".into(), PropertyValue::Number(f64::MAX)),
            ("enabled".into(), PropertyValue::Bool(false)),
            (
                "title".into(),
                PropertyValue::Text("unfinished\ntext".into()),
            ),
            ("velocity".into(), PropertyValue::Vec3([1.0, 2.0, 3.0])),
            (
                "target".into(),
                PropertyValue::ObjectRef(Some("missing-object".into())),
            ),
            (
                "asset".into(),
                PropertyValue::AssetRef(Some("missing-asset".into())),
            ),
            ("unset".into(), PropertyValue::ObjectRef(None)),
        ]);
        session
            .set_binding(1, "binding-1", Some(b.clone()))
            .unwrap();
        let before = session.document.clone();
        assert_eq!(
            session
                .set_binding(2, "binding-1", Some(b.clone()))
                .unwrap(),
            2
        );
        assert!(session.set_binding(1, "binding-1", None).is_err());
        let mut invalid = b.clone();
        invalid.target = BindingTarget::Object("absent-owner".into());
        assert!(session.set_binding(2, "binding-2", Some(invalid)).is_err());
        assert!(session
            .set_binding(2, "binding-2", Some(b.clone()))
            .is_err());
        let mut invalid = source("draft");
        invalid.text = "x".repeat(MAX_SOURCE_BYTES + 1);
        assert!(session.set_source(2, "oversize", Some(invalid)).is_err());
        let mut invalid = b.clone();
        invalid
            .properties
            .insert("bad".into(), PropertyValue::Number(f64::NAN));
        assert!(session.set_binding(2, "binding-1", Some(invalid)).is_err());
        assert_eq!(session.document, before);
        assert_eq!(session.authoring_diagnostics(0, 16).total, 2);
        assert_eq!(
            session.inspect_sources(0, usize::MAX).items[0].text_bytes,
            "function broken(\n".len()
        );
        assert!(!serde_json::to_string(&session.inspect_sources(0, 16))
            .unwrap()
            .contains("function broken"));
        assert_eq!(session.inspect_bindings(0, 16).items[0].property_count, 7);
        assert!(session.has_asset_references());
        session.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.authoring(), session.authoring());
        assert!(!reopened.is_dirty());
        // Recovery snapshots preserve the whole document while granting no
        // authority to overwrite either the origin or an existing destination.
        let baseline = reopened.saved.clone();
        let original_bytes = std::fs::read(&path).unwrap();
        let snapshot = reopened.snapshot_bytes().unwrap();
        assert_eq!(reopened.saved, baseline);
        assert_eq!(reopened.path(), Some(path.as_path()));
        assert!(!reopened.is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        let mut recovered = Session::from_snapshot_bytes(&snapshot).unwrap();
        assert_eq!(recovered.document, reopened.document);
        assert!(recovered.path().is_none() && recovered.is_dirty());
        assert!(matches!(recovered.save(&path), Err(SceneError::Conflict)));
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        recovered.undo(2).unwrap();
        recovered.redo(3).unwrap();
        assert_eq!(recovered.authoring(), reopened.authoring());
        recovered
            .save(dir.path().join("recovered-copy.json"))
            .unwrap();
        assert!(Scene::load(&path)
            .unwrap_err()
            .to_string()
            .contains("Session::open"));
        reopened.set_binding(2, "binding-1", None).unwrap();
        reopened.set_source(3, "source-1", None).unwrap();
        reopened.save(&path).unwrap();
        assert!(reopened.authoring().is_empty());
        assert!(
            Scene::load(&path).is_err(),
            "reversible behavior must not be discarded either"
        );
        reopened.undo(4).unwrap();
        reopened.undo(5).unwrap();
        assert_eq!(reopened.authoring(), session.authoring());
        // Explicit geometry export is an intentional caller operation.
        let geometry = dir.path().join("geometry.json");
        reopened.scene().save(&geometry).unwrap();
        assert_eq!(Scene::load(&geometry).unwrap(), *reopened.scene());

        let source_json = serde_json::to_string(&source("unfinished (")).unwrap();
        let binding_json = serde_json::to_string(&b).unwrap();
        let scene_json = serde_json::to_string(session.scene()).unwrap();
        let envelope = |authoring: String| {
            format!("{{\"format\":\"ruster-scene\",\"format_version\":5,\"scene\":{scene_json},\"authoring\":{authoring},\"undo\":[],\"redo\":[]}}")
        };
        let good = envelope(format!(
            "{{\"sources\":{{\"s\":{source_json}}},\"bindings\":{{\"b\":{binding_json}}}}}"
        ));
        let duplicates = [
            good.replacen("\"format_version\":5", "\"format_version\":5,\"format_version\":5", 1),
            envelope("{\"sources\":{},\"sources\":{},\"bindings\":{}}".into()),
            envelope(format!("{{\"sources\":{{\"s\":{source_json},\"s\":{source_json}}},\"bindings\":{{}}}}")),
            envelope(format!("{{\"sources\":{{\"s\":{}}},\"bindings\":{{}}}}", source_json.replacen("\"language_id\":", "\"language_id\":\"other\",\"language_id\":", 1))),
            envelope(format!("{{\"sources\":{{}},\"bindings\":{{\"b\":{binding_json},\"b\":{binding_json}}}}}")),
            good.replacen("\"enabled\":true", "\"enabled\":true,\"enabled\":false", 1),
            good.replacen("\"properties\":{", "\"properties\":{\"duplicate\":{\"type\":\"bool\",\"value\":true},\"duplicat\\u0065\":{\"type\":\"bool\",\"value\":false},", 1),
        ];
        for duplicate in duplicates {
            std::fs::write(&path, &duplicate).unwrap();
            assert!(
                Session::open(&path).is_err(),
                "duplicate keys must fail in original bytes"
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), duplicate);
        }
    }

    // New binding fields must respect version labels in current and reversible
    // positions, while default edits share the existing atomic history owner.
    #[test]
    fn state_defaults_history_and_v13_binding_boundaries_are_atomic() {
        let mut session = Session::new(Scene::demo()).unwrap();
        let mut opaque = source("unfinished API4 source (");
        opaque.host_api_version = 4;
        session.set_source(0, "source", Some(opaque)).unwrap();
        let mut original = binding("left", "source");
        original.properties.insert(
            "state_defaults".into(),
            PropertyValue::Text("{\"target\":null}".into()),
        );
        session
            .set_binding(1, "binding", Some(original.clone()))
            .unwrap();
        let legacy =
            serde_json::from_slice::<serde_json::Value>(&session.snapshot_bytes().unwrap())
                .unwrap();
        for version in [5, 12] {
            let mut value = legacy.clone();
            value["format_version"] = version.into();
            let restored =
                Session::from_snapshot_bytes(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(restored.loaded_format_version(), version as u32);
            assert_eq!(restored.source("source").unwrap().host_api_version, 4);
            assert_eq!(restored.binding("binding"), Some(&original));
            for (field, contents) in [
                ("target", serde_json::Value::Null),
                ("target", serde_json::json!({"type":"application"})),
                ("state_defaults", serde_json::Value::Null),
                ("state_defaults", serde_json::json!({})),
            ] {
                let mut bad = value.clone();
                bad["authoring"]["bindings"]["binding"][field] = contents;
                assert!(
                    Session::from_snapshot_bytes(&serde_json::to_vec(&bad).unwrap())
                        .unwrap_err()
                        .to_string()
                        .contains("v13")
                );
            }
        }
        let mut disabled = original.clone();
        disabled.enabled = false;
        session.set_binding(2, "binding", Some(disabled)).unwrap();
        let undo = serde_json::from_slice::<serde_json::Value>(&session.snapshot_bytes().unwrap())
            .unwrap();
        session.undo(3).unwrap();
        let redo = serde_json::from_slice::<serde_json::Value>(&session.snapshot_bytes().unwrap())
            .unwrap();
        for (stack, mut history) in [("undo", undo), ("redo", redo)] {
            history["format_version"] = 12.into();
            Session::from_snapshot_bytes(&serde_json::to_vec(&history).unwrap()).unwrap();
            let last = history[stack].as_array().unwrap().len() - 1;
            for side in ["before", "after"] {
                for field in ["target", "state_defaults"] {
                    let mut bad = history.clone();
                    bad[stack][last]["bindings"][0][side][field] = serde_json::Value::Null;
                    assert!(
                        Session::from_snapshot_bytes(&serde_json::to_vec(&bad).unwrap())
                            .unwrap_err()
                            .to_string()
                            .contains("v13")
                    );
                }
            }
        }
        let before = session.document.clone();
        assert_eq!(
            session
                .set_state_default(4, "binding", "absent", None)
                .unwrap(),
            4
        );
        assert_eq!(session.document, before, "no-op deletion preserves Redo");
        let revision = session
            .set_state_default(4, "binding", "counter", Some(StateValue::Number(1.0)))
            .unwrap();
        let mut expected = original;
        expected
            .state_defaults
            .insert("counter".into(), StateValue::Number(1.0));
        assert_eq!(session.binding("binding"), Some(&expected));
        let changed = session.document.clone();
        assert_eq!(
            session
                .set_state_default(
                    revision,
                    "binding",
                    "counter",
                    Some(StateValue::Number(1.0))
                )
                .unwrap(),
            revision
        );
        assert_eq!(session.document, changed);
        for (name, value) in [
            ("bad name", StateValue::Bool(true)),
            ("counter", StateValue::Number(f64::NAN)),
            ("counter", StateValue::Vec3([f64::INFINITY, 0.0, 0.0])),
            ("counter", StateValue::Text("x".repeat(1025))),
        ] {
            assert!(session
                .set_state_default(revision, "binding", name, Some(value))
                .is_err());
            assert_eq!(session.document, changed);
        }
        assert!(session
            .set_state_default(revision - 1, "binding", "counter", None)
            .is_err());
        assert!(session
            .set_state_default(revision, "missing", "counter", None)
            .is_err());
        assert_eq!(session.document, changed);
        let revision = session.undo(revision).unwrap();
        assert!(session
            .binding("binding")
            .unwrap()
            .state_defaults
            .is_empty());
        let revision = session.redo(revision).unwrap();
        assert_eq!(session.binding("binding"), Some(&expected));
        let revision = session
            .set_state_default(
                revision,
                "binding",
                "escaped-a",
                Some(StateValue::Text("\0".repeat(1024))),
            )
            .unwrap();
        let revision = session
            .set_state_default(
                revision,
                "binding",
                "escaped-b",
                Some(StateValue::Text("\0".repeat(1024))),
            )
            .unwrap();
        let before = session.document.clone();
        assert!(session
            .set_state_default(
                revision,
                "binding",
                "escaped-c",
                Some(StateValue::Text("\0".repeat(1024)))
            )
            .is_err());
        assert_eq!(
            session.document, before,
            "byte-cap failure preserves complete history and binding fields"
        );
        let summary = serde_json::to_value(session.inspect_bindings(0, 16)).unwrap();
        assert_eq!(summary["items"][0]["object_id"], "left");
        assert_eq!(summary["items"][0]["state_field_count"], 3);
        assert!(summary["items"][0].get("target").is_none());
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state-defaults.json");
        session.save(&path).unwrap();
        let restored = Session::open(&path).unwrap();
        assert_eq!(restored.document, session.document);
        assert_eq!(restored.loaded_format_version(), FORMAT_VERSION);
    }

    // Commands add a binding-only version boundary and setter. Reuse the real
    // history/document owner for rollback, strict wire and bounded discovery.
    #[test]
    fn command_definitions_history_and_v14_boundaries_are_atomic() {
        use serde_json::{json, Value};
        let mut session = Session::new(Scene::empty("commands")).unwrap();
        let mut opaque = source("unfinished API5 source (commands)");
        opaque.host_api_version = 5;
        session.set_source(0, "source", Some(opaque)).unwrap();
        let mut original = binding("unused", "source");
        original.target = BindingTarget::Application;
        original
            .properties
            .insert("commands".into(), PropertyValue::Text("opaque".into()));
        original
            .state_defaults
            .insert("count".into(), StateValue::Number(0.0));
        session
            .set_binding(1, "application", Some(original.clone()))
            .unwrap();
        let legacy: Value = serde_json::from_slice(&session.snapshot_bytes().unwrap()).unwrap();
        let mut v13 = legacy.clone();
        v13["format_version"] = json!(13);
        let restored = Session::from_snapshot_bytes(&serde_json::to_vec(&v13).unwrap()).unwrap();
        assert_eq!(restored.source("source").unwrap().host_api_version, 5);
        assert_eq!(restored.binding("application"), Some(&original));
        for version in 1..14 {
            for commands in [Value::Null, json!({}), json!({"restart":{"arguments":{}}})] {
                let mut bad = legacy.clone();
                bad["format_version"] = json!(version);
                bad["authoring"]["bindings"]["application"]["commands"] = commands;
                assert!(
                    Session::from_snapshot_bytes(&serde_json::to_vec(&bad).unwrap())
                        .unwrap_err()
                        .to_string()
                        .contains("v14")
                );
            }
        }
        let mut disabled = original.clone();
        disabled.enabled = false;
        session
            .set_binding(2, "application", Some(disabled))
            .unwrap();
        let undo: Value = serde_json::from_slice(&session.snapshot_bytes().unwrap()).unwrap();
        session.undo(3).unwrap();
        let redo: Value = serde_json::from_slice(&session.snapshot_bytes().unwrap()).unwrap();
        for (stack, mut history) in [("undo", undo), ("redo", redo)] {
            history["format_version"] = json!(13);
            Session::from_snapshot_bytes(&serde_json::to_vec(&history).unwrap()).unwrap();
            let last = history[stack].as_array().unwrap().len() - 1;
            for side in ["before", "after"] {
                for commands in [Value::Null, json!({}), json!({"restart":{"arguments":{}}})] {
                    let mut bad = history.clone();
                    bad[stack][last]["bindings"][0][side]["commands"] = commands;
                    assert!(
                        Session::from_snapshot_bytes(&serde_json::to_vec(&bad).unwrap())
                            .unwrap_err()
                            .to_string()
                            .contains("v14")
                    );
                }
            }
        }
        let before = session.document.clone();
        assert_eq!(
            session
                .set_command_definition(4, "application", "absent", None)
                .unwrap(),
            4
        );
        assert_eq!(session.document, before, "absent removal retains Redo");
        let first = CommandDefinition {
            arguments: BTreeMap::from([("amount".into(), StateValue::Number(1.0))]),
        };
        let revision = session
            .set_command_definition(4, "application", "restart", Some(first.clone()))
            .unwrap();
        let mut expected = original;
        expected.commands.insert("restart".into(), first.clone());
        assert_eq!(session.binding("application"), Some(&expected));
        let before = session.document.clone();
        assert_eq!(
            session
                .set_command_definition(revision, "application", "restart", Some(first))
                .unwrap(),
            revision
        );
        assert_eq!(session.document, before, "equal definition is a no-op");
        let edited = CommandDefinition {
            arguments: BTreeMap::from([("amount".into(), StateValue::Number(2.0))]),
        };
        let revision = session
            .set_command_definition(revision, "application", "restart", Some(edited.clone()))
            .unwrap();
        let revision = session
            .set_command_definition(revision, "application", "restart", None)
            .unwrap();
        assert!(session.binding("application").unwrap().commands.is_empty());
        let revision = session.undo(revision).unwrap();
        assert_eq!(
            session.command_definition("application", "restart"),
            Some(&edited)
        );
        let revision = session.redo(revision).unwrap();
        assert!(session
            .command_definition("application", "restart")
            .is_none());
        let mut revision = session.undo(revision).unwrap();
        for index in 0..MAX_COMMANDS - 1 {
            revision = session
                .set_command_definition(
                    revision,
                    "application",
                    &format!("command-{index:02}"),
                    Some(CommandDefinition {
                        arguments: StateFields::new(),
                    }),
                )
                .unwrap();
        }
        let before = session.document.clone();
        let evictions = session.history_evictions();
        for (expected_revision, owner, name, definition) in [
            (revision - 1, "application", "restart", None),
            (revision, "missing", "restart", None),
            (revision, "application", "bad name", None),
            (revision, "application", "crossing", Some(edited.clone())),
            (
                revision,
                "application",
                "restart",
                Some(CommandDefinition {
                    arguments: (0..=MAX_COMMAND_ARGUMENTS)
                        .map(|index| (format!("arg-{index}"), StateValue::Bool(false)))
                        .collect(),
                }),
            ),
        ] {
            assert!(session
                .set_command_definition(expected_revision, owner, name, definition)
                .is_err());
            assert_eq!(session.document, before);
            assert_eq!(session.history_evictions(), evictions);
        }
        let page = session.inspect_commands("application", 0, 1).unwrap();
        assert_eq!(page.total, MAX_COMMANDS);
        assert_eq!(
            page.items[0],
            CommandSummary {
                id: "command-00".into(),
                argument_count: 0
            }
        );
        assert_eq!(page.next_offset, Some(1));
        assert_eq!(
            session
                .inspect_commands("application", 0, usize::MAX)
                .unwrap()
                .items
                .len(),
            MAX_COMMANDS
        );
        assert!(session
            .inspect_commands("application", usize::MAX, 16)
            .unwrap()
            .items
            .is_empty());
        assert!(session.inspect_commands("missing", 0, 16).is_err());
        assert!(session.command_definition("missing", "restart").is_none());
        assert_eq!(
            serde_json::to_value(session.inspect_bindings(0, 16)).unwrap()["items"][0]
                ["command_count"],
            MAX_COMMANDS
        );

        let bytes = session.snapshot_bytes().unwrap();
        let raw = String::from_utf8(bytes).unwrap();
        for duplicate in [
            raw.replacen("\"commands\":{\"command-00\":", "\"commands\":{},\"commands\":{\"command-00\":", 1),
            raw.replacen("\"commands\":{\"command-00\":", "\"commands\":{\"command-00\":{\"arguments\":{}},\"command-00\":", 1),
            raw.replacen("\"amount\":{\"type\":\"number\",\"value\":2.0}",
                "\"amount\":{\"type\":\"number\",\"value\":2.0},\"amou\\u006et\":{\"type\":\"number\",\"value\":2.0}", 1),
        ] {
            assert_ne!(duplicate, raw, "duplicate fixture must change actual command bytes");
            assert!(Session::from_snapshot_bytes(duplicate.as_bytes()).unwrap_err().to_string().contains("duplicate JSON key"));
        }
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("commands.json");
        session.save(&path).unwrap();
        let restored = Session::open(&path).unwrap();
        assert_eq!(restored.document, session.document);
        assert_eq!(restored.loaded_format_version(), FORMAT_VERSION);
    }

    // Unique risk: deleting/duplicating objects must transact owned bindings with the
    // geometry and remap only references inside the copied selection, including self refs.
    #[test]
    fn behavior_lifecycle_is_one_undo_and_reserves_dangling_references() {
        // New source + attachment must never become two independent Undo steps
        // or leave a source behind when the owner's attachment is rejected.
        let mut attached = Session::new(Scene::demo()).unwrap();
        attached
            .create_behavior(
                0,
                "source-1",
                source("unfinished("),
                "binding-1",
                binding("left", "source-1"),
            )
            .unwrap();
        assert_eq!((attached.scene().revision, attached.undo_len()), (1, 1));
        attached.undo(1).unwrap();
        assert!(attached.authoring().is_empty());
        assert!(
            attached.contains_source_id("source-1") && attached.contains_binding_id("binding-1")
        );
        attached.redo(2).unwrap();
        let before = attached.document.clone();
        for (revision, source_id, binding_id, owner, referenced_source) in [
            (0, "source-2", "binding-2", "right", "source-2"),
            (3, "source-1", "binding-2", "right", "source-1"),
            (3, "source-2", "binding-1", "right", "source-2"),
            (3, "source-2", "binding-2", "left", "source-2"),
            (3, "source-2", "binding-2", "absent", "source-2"),
            (3, "source-2", "binding-2", "right", "other-source"),
        ] {
            assert!(attached
                .create_behavior(
                    revision,
                    source_id,
                    source("unfinished("),
                    binding_id,
                    binding(owner, referenced_source)
                )
                .is_err());
            assert_eq!(attached.document, before);
            assert_eq!(attached.history_evictions(), 0);
        }
        let mut session = Session::new(Scene::demo()).unwrap();
        let mut a = binding("left", "source-1");
        a.state_defaults
            .insert("phase".into(), StateValue::Text("idle".into()));
        a.properties.insert(
            "internal".into(),
            PropertyValue::ObjectRef(Some("right".into())),
        );
        a.properties.insert(
            "external".into(),
            PropertyValue::ObjectRef(Some("center".into())),
        );
        let mut b = binding("right", "source-1");
        b.properties.insert(
            "self_ref".into(),
            PropertyValue::ObjectRef(Some("right".into())),
        );
        b.properties.insert(
            "dangling".into(),
            PropertyValue::ObjectRef(Some("object-1".into())),
        );
        session.set_binding(0, "binding-1", Some(a)).unwrap();
        session.set_binding(1, "binding-2", Some(b)).unwrap();
        assert_eq!(session.allocate_object_ids(1).unwrap(), ["object-2"]);
        assert_eq!(session.allocate_source_ids(1).unwrap(), ["source-2"]);
        let original = session.document.clone();
        let pairs = [
            ("left".into(), "copy-left".into()),
            ("right".into(), "copy-right".into()),
        ];
        session.duplicate_many(2, &pairs).unwrap();
        assert_eq!(session.undo_len(), 3);
        let copy = session
            .authoring()
            .bindings
            .values()
            .find(|b| b.object_id() == Some("copy-left"))
            .unwrap();
        assert_eq!(
            copy.state_defaults["phase"],
            StateValue::Text("idle".into())
        );
        assert_eq!(
            copy.properties["internal"],
            PropertyValue::ObjectRef(Some("copy-right".into()))
        );
        assert_eq!(
            copy.properties["external"],
            PropertyValue::ObjectRef(Some("center".into()))
        );
        let copy = session
            .authoring()
            .bindings
            .values()
            .find(|b| b.object_id() == Some("copy-right"))
            .unwrap();
        assert_eq!(
            copy.properties["self_ref"],
            PropertyValue::ObjectRef(Some("copy-right".into()))
        );
        assert_eq!(
            copy.properties["dangling"],
            PropertyValue::ObjectRef(Some("object-1".into()))
        );
        session.undo(3).unwrap();
        assert_eq!(session.authoring(), &original.authoring);
        assert_eq!(session.scene().objects, original.scene.objects);
        session.redo(4).unwrap();
        let copied = session.authoring().clone();
        session
            .delete_many(5, &["left".into(), "copy-right".into()])
            .unwrap();
        assert_eq!(session.authoring().bindings.len(), 2);
        session.undo(6).unwrap();
        assert_eq!(session.authoring(), &copied);
        session.delete(7, "right").unwrap();
        session.undo(8).unwrap();
        assert_eq!(session.authoring(), &copied);
        session.duplicate(9, "right", "single-copy", None).unwrap();
        assert!(session.authoring().bindings.values().any(|b| b.object_id()
            == Some("single-copy")
            && b.properties["self_ref"] == PropertyValue::ObjectRef(Some("single-copy".into()))));
        session.set_binding(10, "binding-2", None).unwrap();
        assert!(
            session.contains_object_id("object-1"),
            "history references reserve dangling identities"
        );
        assert!(session.create(11, Object::cube("object-1", None)).is_err());
        assert!(session.contains_source_id("source-1"));
        // Explicit source upsert repairs a referred identity instead of reallocating it.
        session
            .set_source(11, "source-1", Some(source("broken draft")))
            .unwrap();
        assert!(session.source("source-1").is_some());

        let mut application = binding("unused", "app-source");
        application.target = BindingTarget::Application;
        application.enabled = false;
        application
            .state_defaults
            .insert("score".into(), StateValue::Number(0.0));
        application.commands.insert(
            "restart".into(),
            CommandDefinition {
                arguments: StateFields::new(),
            },
        );
        application.properties.insert(
            "reference".into(),
            PropertyValue::ObjectRef(Some("future-object".into())),
        );
        let mut session = Session::new(Scene::demo()).unwrap();
        session
            .set_binding(0, "application", Some(application.clone()))
            .unwrap();
        assert!(
            !session.contains_object_id("application"),
            "the application target reserves no geometry identity"
        );
        assert!(
            session.contains_object_id("future-object"),
            "application properties still reserve referenced objects"
        );
        let summary = serde_json::to_value(session.inspect_bindings(0, 16)).unwrap();
        assert_eq!(
            summary["items"][0]["target"],
            serde_json::json!({"type":"application"})
        );
        assert_eq!(summary["items"][0]["state_field_count"], 1);
        assert!(summary["items"][0].get("object_id").is_none());
        session.duplicate(1, "left", "application", None).unwrap();
        assert_eq!(
            session.authoring().bindings.len(),
            1,
            "geometry duplication never clones the application binding"
        );
        session.delete(2, "application").unwrap();
        assert_eq!(session.binding("application"), Some(&application));
        session.undo(3).unwrap();
        session.redo(4).unwrap();
        assert_eq!(
            session.binding("application"),
            Some(&application),
            "geometry lifecycle preserves application defaults"
        );
    }

    // Unique risk: v4 grouped histories must migrate without rewriting, and old
    // versions cannot silently accept new authoring envelope/history fields, even null.
    #[test]
    fn v4_history_migrates_strictly_and_preserves_open_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v4.json");
        let mut old = Session::new(Scene::demo()).unwrap();
        old.duplicate_many(0, &[("left".into(), "copy".into())])
            .unwrap();
        old.delete_many(1, &["left".into(), "right".into()])
            .unwrap();
        old.undo(2).unwrap();
        old.document.format_version = 4;
        let value = legacy_value(&old.document);
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert!(!reopened.is_dirty());
        assert_eq!(reopened.loaded_format_version(), 4);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!((reopened.undo_len(), reopened.redo_len()), (1, 1));
        reopened.redo(3).unwrap();
        reopened.undo(4).unwrap();
        reopened.undo(5).unwrap();
        assert_eq!(reopened.scene().objects, Scene::demo().objects);
        reopened.save(&path).unwrap();
        assert_eq!(reopened.loaded_format_version(), FORMAT_VERSION);
        assert_eq!(Session::open(&path).unwrap().redo_len(), 2);
        for version in [3, 4] {
            for location in ["envelope", "history"] {
                let mut bad = value.clone();
                bad["format_version"] = version.into();
                if location == "envelope" {
                    bad["authoring"] = serde_json::Value::Null;
                } else {
                    bad["undo"][0]["sources"] = serde_json::Value::Null;
                }
                std::fs::write(&path, serde_json::to_vec(&bad).unwrap()).unwrap();
                assert!(Session::open(&path).is_err());
            }
        }
    }

    // Unique risk: a near-cap file admitted by the actual v4 writer must retain
    // every old Undo/Redo entry, rather than fail due to new wrapper indentation.
    #[test]
    fn actual_near_cap_v4_writer_recipe_migrates_without_history_loss() {
        let mut scene = Scene::empty("v4-near-cap");
        scene.objects = (0..256)
            .map(|n| Object::cube(format!("object-{n:03}"), None))
            .collect();
        let mut session = Session::new(scene).unwrap();
        // Historical aaacef9's writer admits five256-object groups, then27 scalar
        // groups to32 entries. The sixth large group was actually rejected there.
        for step in 0..32 {
            let count = if step < 5 { 256 } else { 1 };
            let edits = session
                .scene()
                .objects
                .iter()
                .take(count)
                .map(|o| {
                    let mut transform = o.transform.clone();
                    transform.translation[0] = if transform.translation[0] == 0.0 {
                        0.001
                    } else {
                        0.0
                    };
                    (o.id.clone(), transform)
                })
                .collect::<Vec<_>>();
            session
                .edit_transforms(session.scene().revision, &edits)
                .unwrap();
        }
        session.undo(session.scene().revision).unwrap();
        let old = V4Document {
            format: "ruster-scene".into(),
            format_version: 4,
            scene: session.scene().clone(),
            undo: session
                .document
                .undo
                .iter()
                .map(|c| {
                    assert_eq!(c.scene.len(), 1);
                    c.scene[0].clone()
                })
                .collect(),
            redo: session
                .document
                .redo
                .iter()
                .map(|c| c.scene[0].clone())
                .collect(),
        };
        let mut bytes = serde_json::to_vec_pretty(&old).unwrap();
        bytes.push(b'\n');
        // This exact byte count/hash was independently matched to the actual old
        // Session::save output, retained compressed with provenance in evidence.
        assert_eq!(bytes.len(), 1_028_346);
        let hash = bytes.iter().fold(0xcbf29ce484222325u64, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        });
        assert_eq!(hash, 0x392c41b401123383);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v4.json");
        std::fs::write(&path, &bytes).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!((reopened.undo_len(), reopened.redo_len()), (31, 1));
        assert_eq!(reopened.history_evictions(), 0);
        assert_eq!(reopened.loaded_format_version(), 4);
        assert!(!reopened.is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(
            serde_json::to_vec_pretty(&reopened.document).unwrap().len() > MAX_FILE_BYTES as usize,
            "pretty v5 wrappers reproduce the migration overflow"
        );
        let objects = reopened.scene().objects.clone();
        reopened.redo(33).unwrap();
        reopened.undo(34).unwrap();
        assert_eq!(reopened.scene().objects, objects);
        reopened.save(&path).unwrap();
        assert_eq!(reopened.loaded_format_version(), FORMAT_VERSION);
        assert_eq!((reopened.undo_len(), reopened.redo_len()), (31, 1));
        let compact = std::fs::read(&path).unwrap();
        assert!(compact.len() <= MAX_FILE_BYTES as usize);
        assert_eq!(compact.iter().filter(|b| **b == b'\n').count(), 1);
        let reopened = Session::open(&path).unwrap();
        assert_eq!((reopened.undo_len(), reopened.redo_len()), (31, 1));
        assert_eq!(reopened.history_evictions(), 0);
    }

    // Unique risk: actions-only content and structural history must not disappear
    // through geometry-only loading, old schemas, or deleted-ID reuse.
    #[test]
    fn action_authoring_roundtrip_history_and_owning_version_guards() {
        use input::{ActionDefinition, ActionKind, InputKey};
        let action = ActionDefinition {
            label: Some("Traverse".into()),
            kind: ActionKind::Axis1d {
                negative: vec![InputKey::ArrowLeft],
                positive: vec![InputKey::ArrowRight],
            },
        };
        let mut session = Session::new(Scene::demo()).unwrap();
        session
            .set_action(0, "travel", Some(action.clone()))
            .unwrap();
        assert!(!session.authoring().is_empty());
        session.set_action(1, "travel", None).unwrap();
        let before = session.document.clone();
        assert!(session
            .set_action(2, "travel", Some(action.clone()))
            .is_err());
        assert_eq!(session.document, before);
        assert!(session.contains_action_id("travel"));
        session.undo(2).unwrap();
        assert_eq!(session.action("travel"), Some(&action));
        session.redo(3).unwrap();
        assert!(session.action("travel").is_none());
        session.undo(4).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("actions.json");
        session.save(&path).unwrap();
        assert!(Scene::load(&path).is_err());
        let loaded = Session::open(&path).unwrap();
        assert_eq!(loaded.authoring(), session.authoring());
        assert_eq!(
            (loaded.undo_len(), loaded.redo_len()),
            (session.undo_len(), session.redo_len())
        );
        assert_eq!(
            loaded.allocate_action_ids(1).unwrap(),
            session.allocate_action_ids(1).unwrap()
        );
        // JSON-looking source text is not a schema-owning action location.
        let mut old = Session::new(Scene::demo()).unwrap();
        old.set_source(0, "source", Some(source("-- {\"actions\":null}")))
            .unwrap();
        let mut legacy = serde_json::to_value(&old.document).unwrap();
        legacy["format_version"] = 10.into();
        assert!(Document::decode(&serde_json::to_vec(&legacy).unwrap()).is_ok());
        for version in 1..=10 {
            for location in ["authoring", "undo", "redo"] {
                for payload in [
                    serde_json::Value::Null,
                    serde_json::json!([]),
                    serde_json::json!({}),
                ] {
                    let mut bad = legacy.clone();
                    bad["format_version"] = version.into();
                    if location == "authoring" {
                        bad["authoring"]["actions"] = payload;
                    } else {
                        bad[location] = serde_json::json!([{ "actions": payload }]);
                    }
                    let error = Document::decode(&serde_json::to_vec(&bad).unwrap()).unwrap_err();
                    assert!(
                        error
                            .to_string()
                            .contains("input actions require scene v11"),
                        "{version}/{location}: {error}"
                    );
                }
            }
        }
        let mut invalid = action;
        invalid.kind = ActionKind::Button {
            keys: vec![InputKey::Escape],
        };
        let before = session.document.clone();
        assert!(session
            .set_action(session.scene().revision, "invalid", Some(invalid))
            .is_err());
        assert!(session
            .set_action(session.scene().revision - 1, "stale", None)
            .is_err());
        assert_eq!(session.document, before);
    }

    // Unique risk: source deletion can shrink current state while Undo grows it;
    // admission must reserve all reachable authored payloads plus retained history once.
    #[test]
    fn source_history_byte_admission_preserves_saveable_undo_and_failure_atomicity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bounded.json");
        let mut session = Session::new(Scene::empty("bounded")).unwrap();
        let mut application = binding("unused", "missing-source");
        application.target = BindingTarget::Application;
        application.enabled = false;
        application.commands.insert(
            "set_mode".into(),
            CommandDefinition {
                arguments: BTreeMap::from([("mode".into(), StateValue::Text("ready".into()))]),
            },
        );
        session
            .set_binding(0, "application", Some(application))
            .unwrap();
        // Reuse this reachable-payload byte oracle with the new authoring field,
        // rather than adding a second near-cap suite for action history.
        for n in 0..input::MAX_ACTIONS {
            session
                .set_action(
                    session.scene().revision,
                    &format!("action-{n}"),
                    Some(input::ActionDefinition {
                        label: Some("a".repeat(128)),
                        ..input::ActionDefinition::default()
                    }),
                )
                .unwrap();
        }
        for n in 0..50 {
            let text = format!("{}{n:03}", "\u{0001}".repeat(MAX_SOURCE_BYTES - 3));
            session
                .set_source(session.scene().revision, "source-1", Some(source(&text)))
                .unwrap();
        }
        assert!(
            session.history_evictions() > 0,
            "source editing must continue by trimming oldest history"
        );
        assert!(session.undo_len() < HISTORY_LIMIT);
        let latest = session.source("source-1").unwrap().clone();
        let evictions = session.history_evictions();
        let before = session.document.clone();
        assert!(session
            .set_source(session.scene().revision - 1, "source-1", None)
            .is_err());
        assert_eq!(
            session
                .set_source(session.scene().revision, "source-1", Some(latest.clone()))
                .unwrap(),
            session.scene().revision
        );
        let mut invalid = latest.clone();
        invalid.text.push('x');
        assert!(session
            .set_source(session.scene().revision, "source-1", Some(invalid))
            .is_err());
        assert_eq!(session.document, before);
        assert_eq!(session.history_evictions(), evictions);
        session.undo(session.scene().revision).unwrap();
        assert!(session.source("source-1").unwrap().text.ends_with("048"));
        assert_eq!(session.history_evictions(), evictions);
        session.redo(session.scene().revision).unwrap();
        assert_eq!(session.source("source-1"), Some(&latest));
        assert_eq!(session.history_evictions(), evictions);
        session
            .set_source(session.scene().revision, "source-1", None)
            .unwrap();
        session.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(
            reopened.history_evictions(),
            0,
            "byte-pressure reporting is session-local"
        );
        reopened.document.scene.revision = 98;
        reopened.undo(98).unwrap();
        reopened.redo(99).unwrap();
        reopened.save(&path).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() <= MAX_FILE_BYTES);
        assert_eq!(
            Session::open(&path).unwrap().authoring(),
            reopened.authoring()
        );
        // A valid group whose new payload plus its own newest delta cannot fit
        // remains rejected atomically, even when every older Undo could be trimmed.
        let mut scene = Scene::empty("oversized-group");
        scene.objects = (0..8)
            .map(|n| Object::cube(format!("owner-{n}"), None))
            .collect();
        let mut group = Session::new(scene).unwrap();
        for n in 0..8 {
            let mut binding = binding(&format!("owner-{n}"), "missing-source");
            binding.properties = (0..MAX_PROPERTIES)
                .map(|p| {
                    (
                        format!("property-{p}"),
                        PropertyValue::Text("x".repeat(MAX_PROPERTY_TEXT_BYTES)),
                    )
                })
                .collect();
            group
                .set_binding(
                    group.scene().revision,
                    &format!("binding-{n}"),
                    Some(binding),
                )
                .unwrap();
        }
        let before = group.document.clone();
        let evictions = group.history_evictions();
        let pairs = (0..8)
            .map(|n| (format!("owner-{n}"), format!("copy-{n}")))
            .collect::<Vec<_>>();
        assert!(matches!(
            group.duplicate_many(group.scene().revision, &pairs),
            Err(SceneError::Capacity)
        ));
        assert_eq!(group.document, before);
        assert_eq!(group.history_evictions(), evictions);
        // A catalog edit must reserve its complete binding before/after delta,
        // even when every individual definition remains within its own limit.
        let mut application = binding("unused", "missing-source");
        application.target = BindingTarget::Application;
        application.enabled = false;
        group
            .set_binding(group.scene().revision, "application", Some(application))
            .unwrap();
        let definition = CommandDefinition {
            arguments: (0..15)
                .map(|index| {
                    (
                        format!("arg-{index}"),
                        StateValue::Text("x".repeat(MAX_PROPERTY_TEXT_BYTES)),
                    )
                })
                .collect(),
        };
        definition.validate().unwrap();
        let mut capacity_rejected = false;
        for index in 0..MAX_COMMANDS {
            let before = group.document.clone();
            let evictions = group.history_evictions();
            match group.set_command_definition(
                group.scene().revision,
                "application",
                &format!("command-{index}"),
                Some(definition.clone()),
            ) {
                Ok(_) => {}
                Err(SceneError::Capacity) => {
                    assert_eq!(group.document, before);
                    assert_eq!(group.history_evictions(), evictions);
                    capacity_rejected = true;
                    break;
                }
                Err(error) => panic!("unexpected command admission failure {error}"),
            }
        }
        assert!(
            capacity_rejected,
            "catalog history must participate in whole-document capacity"
        );
        let mut pages = Session::new(Scene::empty("pages")).unwrap();
        for i in 0..17 {
            pages
                .set_source(i, &format!("source-{i}"), Some(source("")))
                .unwrap();
        }
        assert_eq!(pages.inspect_sources(0, usize::MAX).items.len(), 16);
        assert_eq!(pages.inspect_sources(0, usize::MAX).next_offset, Some(16));
        assert_eq!(pages.inspect_sources(16, 16).items.len(), 1);
        assert!(pages.inspect_sources(usize::MAX, 16).items.is_empty());
    }

    // Unique risk: partial multi-object mutations and per-object undo steps break a
    // gesture's atomicity; byte-limited recent history must keep every accepted state saveable.
    #[test]
    fn grouped_edits_are_atomic_one_step_and_byte_bounded() {
        let mut session = Session::new(Scene::demo()).unwrap();
        let first = session.scene().objects[0].transform.clone();
        let mut moved = first.clone();
        moved.translation[0] += 1.0;
        let before = session.document.clone();
        assert!(session
            .edit_transforms(
                0,
                &[
                    ("left".into(), moved.clone()),
                    ("missing".into(), moved.clone())
                ]
            )
            .is_err());
        assert_eq!(session.document, before);
        assert!(session
            .edit_transforms(
                0,
                &[
                    ("left".into(), moved.clone()),
                    ("left".into(), moved.clone())
                ]
            )
            .is_err());
        assert_eq!(session.document, before);
        session
            .edit_transforms(
                0,
                &[("left".into(), moved.clone()), ("right".into(), moved)],
            )
            .unwrap();
        assert_eq!(session.undo_len(), 1);
        session.undo(1).unwrap();
        assert_eq!(session.scene().objects, before.scene.objects);
        let before = session.document.clone();
        assert_eq!(
            session
                .edit_transforms(2, &[("left".into(), first)])
                .unwrap(),
            2
        );
        assert_eq!(session.document, before);
        session
            .duplicate_many(
                2,
                &[
                    ("left".into(), "left-copy".into()),
                    ("right".into(), "right-copy".into()),
                ],
            )
            .unwrap();
        assert_eq!(session.scene().objects.len(), 5);
        assert_eq!(session.undo_len(), 1);
        session
            .delete_many(3, &["left".into(), "right-copy".into()])
            .unwrap();
        session.undo(4).unwrap();
        assert_eq!(
            session
                .scene()
                .objects
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            ["left", "center", "right", "left-copy", "right-copy"]
        );
        session.undo(5).unwrap();
        assert_eq!(session.scene().objects, before.scene.objects);

        let mut large = Session::new(Scene::empty("large")).unwrap();
        large
            .create_many(
                0,
                (0..MAX_OBJECTS)
                    .map(|i| {
                        Object::cube(
                            format!("object-{i}-{}", "a".repeat(110)),
                            Some("n".repeat(128)),
                        )
                    })
                    .map(|mut object| {
                        // Retained components share this reachable-state byte oracle.
                        object.collider = Some(Collider::default());
                        object
                    })
                    .collect(),
            )
            .unwrap();
        for n in 1..=HISTORY_LIMIT + 4 {
            let edits: Vec<_> = large
                .scene()
                .objects
                .iter()
                .map(|o| {
                    let mut t = o.transform.clone();
                    t.translation = [n as f32; 3];
                    (o.id.clone(), t)
                })
                .collect();
            large
                .edit_transforms(large.scene().revision, &edits)
                .unwrap();
        }
        assert!(
            large.history_evictions() > 0,
            "large group edits must keep the newest step and trim older history when needed"
        );
        assert!(large.document.validate().is_ok());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("large.json");
        large.save(&path).unwrap();
        assert!(std::fs::metadata(&path).unwrap().len() <= MAX_FILE_BYTES);
        assert_eq!(Session::open(&path).unwrap().scene(), large.scene());
        // An admitted near-cap transaction remains undoable across revision digit growth.
        let objects = large.scene().objects.clone();
        large.document.scene.revision = 98;
        large.undo(98).unwrap();
        large.redo(99).unwrap();
        assert_eq!(large.scene().revision, 100);
        assert_eq!(large.scene().objects, objects);
        // Group history can reserve far more than the old289-ID editor probe.
        let mut ids = Session::new(Scene::empty("ids")).unwrap();
        for (start, count) in [(1, 256), (257, 44)] {
            let objects: Vec<_> = (start..start + count)
                .map(|n| Object::cube(format!("object-{n}"), None))
                .collect();
            let names: Vec<_> = objects.iter().map(|o| o.id.clone()).collect();
            ids.create_many(ids.scene().revision, objects).unwrap();
            ids.delete_many(ids.scene().revision, &names).unwrap();
        }
        assert_eq!(
            ids.allocate_object_ids(2).unwrap(),
            vec!["object-301", "object-302"]
        );
        assert!(ids.allocate_object_ids(MAX_OBJECTS + 1).is_err());
    }

    // Unique risk: a v3 label can silently accept grouped v4 semantics, or opening
    // a supported v3 document loses name/lifecycle undo and redo state.
    #[test]
    fn v3_migrates_scalar_history_but_rejects_grouped_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v3.json");
        let mut doc = Document::new(Scene::demo());
        doc.format_version = 3;
        doc.scene.objects[0].name = Some("Named".into());
        doc.undo.push(
            SceneDelta::Name {
                id: "left".into(),
                before: None,
                after: Some("Named".into()),
            }
            .into(),
        );
        doc.redo.push(
            SceneDelta::Delete {
                object: doc.scene.objects[1].clone(),
                index: 1,
            }
            .into(),
        );
        std::fs::write(&path, serde_json::to_vec(&legacy_value(&doc)).unwrap()).unwrap();
        let mut session = Session::open(&path).unwrap();
        assert_eq!(session.loaded_format_version(), 3);
        session.redo(0).unwrap();
        session.undo(1).unwrap();
        session.undo(2).unwrap();
        assert!(session.scene().objects[0].name.is_none());
        session.save(&path).unwrap();
        assert_eq!(session.loaded_format_version(), FORMAT_VERSION);
        doc.undo = vec![SceneDelta::Transforms {
            changes: vec![TransformDelta {
                id: "left".into(),
                before: Transform::default(),
                after: doc.scene.objects[0].transform.clone(),
            }],
        }
        .into()];
        std::fs::write(&path, serde_json::to_vec(&legacy_value(&doc)).unwrap()).unwrap();
        assert!(Session::open(&path).is_err());
    }

    // Unique risk: a FIFO or other nonregular source/lock blocks before bounded
    // ingestion. Metadata rejection must occur before any open call.
    #[cfg(unix)]
    #[test]
    fn nonregular_sources_and_locks_are_rejected_before_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fifo.scene");
        assert!(std::process::Command::new("mkfifo")
            .arg(&path)
            .status()
            .unwrap()
            .success());
        assert!(matches!(Scene::load(&path), Err(SceneError::Invalid(_))));
        assert!(matches!(
            Scene::demo().save(&path),
            Err(SceneError::Invalid(_))
        ));
        assert!(matches!(Session::open(&path), Err(SceneError::Invalid(_))));
        let path = dir.path().join("ordinary.json");
        let lock = dir.path().join(".ordinary.json.ruster-lock");
        assert!(std::process::Command::new("mkfifo")
            .arg(lock)
            .status()
            .unwrap()
            .success());
        assert!(matches!(
            Session::new(Scene::demo()).unwrap().save(&path),
            Err(SceneError::Invalid(_))
        ));
        assert!(!path.exists());
    }

    // Unique risk: lifecycle deltas preserve order/identity/assets through storage and
    // interleaved edits while failed commands and no-op naming preserve redo.
    #[test]
    fn lifecycle_history_roundtrip_preserves_order_identity_and_assets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut session = Session::new(Scene::empty("owned")).unwrap();
        let mut cube = Object::cube("stable-a", Some("First object".into()));
        cube.mesh = Some("owned-mesh".into());
        cube.collider = Some(Collider {
            enabled: true,
            role: ColliderRole::Sensor,
            ..Collider::default()
        });
        session.create(0, cube.clone()).unwrap();
        session.duplicate(1, "stable-a", "stable-b", None).unwrap();
        session
            .rename(2, "stable-a", Some("Renamed ✨".into()))
            .unwrap();
        assert_eq!(session.scene().objects[0].id, "stable-a");
        assert_eq!(session.scene().objects[1].display_name(), "First object");
        assert_eq!(session.scene().objects[1].collider, cube.collider);
        session.delete(3, "stable-a").unwrap();
        session.delete(4, "stable-b").unwrap();
        assert!(session.scene().objects.is_empty());
        assert!(
            session.has_asset_references(),
            "deleted payloads retain mesh roots"
        );
        session.save(&path).unwrap();
        let mut session = Session::open(&path).unwrap();
        let before = session.document.clone();
        assert!(
            session.create(5, cube.clone()).is_err(),
            "history IDs stay reserved"
        );
        assert_eq!(session.document, before);
        session.undo(5).unwrap();
        session.undo(6).unwrap();
        assert_eq!(
            session
                .scene()
                .objects
                .iter()
                .map(|o| o.id.as_str())
                .collect::<Vec<_>>(),
            ["stable-a", "stable-b"]
        );
        assert_eq!(session.scene().objects[0].display_name(), "Renamed ✨");
        assert!(session
            .scene()
            .objects
            .iter()
            .all(|object| object.collider == cube.collider));
        let before = session.document.clone();
        assert_eq!(
            session
                .rename(7, "stable-a", Some("Renamed ✨".into()))
                .unwrap(),
            7
        );
        assert!(session.rename(7, "stable-a", Some(" bad ".into())).is_err());
        assert!(session.delete(6, "stable-a").is_err());
        assert_eq!(session.document, before);
        session.save(&path).unwrap();
        let mut session = Session::open(&path).unwrap();
        session.redo(7).unwrap();
        session.redo(8).unwrap();
        for rev in 9..14 {
            session.undo(rev).unwrap();
        }
        assert!(session.scene().objects.is_empty());
        assert_eq!(session.redo_len(), 5);
        session
            .create(14, Object::cube("new-identity", None))
            .unwrap();
        assert_eq!(session.redo_len(), 0);
        assert_eq!(session.undo_len(), 1);
    }

    // Unique risk: old formats silently accept new semantics, or malformed lifecycle
    // history introduces invalid intermediate scenes or non-atomic failed commands.
    #[test]
    fn lifecycle_limits_and_strict_v2_migration() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v2.json");
        let mut value = serde_json::to_value(Document::new(Scene::demo())).unwrap();
        value["format_version"] = 2.into();
        value["scene"]["objects"][0]["mesh"] = "asset".into();
        value["undo"] =
            serde_json::json!([{"kind":"mesh","id":"left","before":null,"after":"asset"}]);
        value["redo"] = serde_json::json!([{"kind":"transform","id":"center","before":Transform::default(),"after":Transform { translation: [1.0,0.0,0.0], ..Transform::default() }}]);
        let bytes = serde_json::to_vec(&value).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let mut old = Session::open(&path).unwrap();
        assert_eq!(old.loaded_format_version(), 2);
        assert!(!old.is_dirty());
        old.redo(0).unwrap();
        old.undo(1).unwrap();
        old.undo(2).unwrap();
        old.save(&path).unwrap();
        assert_eq!(old.loaded_format_version(), FORMAT_VERSION);
        assert!(Session::open(&path).unwrap().scene().objects[0]
            .mesh
            .is_none());
        for version in [1, 2] {
            let mut mislabeled = if version == 1 {
                serde_json::from_slice(include_bytes!("../tests/fixtures/legacy-v1.scene.json"))
                    .unwrap()
            } else {
                value.clone()
            };
            mislabeled["scene"]["objects"][0]["name"] = serde_json::Value::Null;
            std::fs::write(&path, serde_json::to_vec(&mislabeled).unwrap()).unwrap();
            assert!(Session::open(&path).is_err());
        }
        let mut mislabeled = value;
        mislabeled["undo"] =
            serde_json::json!([{"kind":"name","id":"left","before":null,"after":"A"}]);
        std::fs::write(&path, serde_json::to_vec(&mislabeled).unwrap()).unwrap();
        assert!(Session::open(&path).is_err());
        let mut session = Session::new(Scene::empty("limits")).unwrap();
        for i in 0..MAX_OBJECTS {
            session
                .create(i as u64, Object::cube(format!("object-{i}"), None))
                .unwrap();
        }
        let before = session.document.clone();
        assert!(session
            .create(MAX_OBJECTS as u64, Object::cube("overflow", None))
            .is_err());
        assert_eq!(session.document, before);
        assert_eq!(session.undo_len(), HISTORY_LIMIT);
        session.document.scene.revision = u64::MAX;
        let before = session.document.clone();
        assert!(session.delete(u64::MAX, "object-0").is_err());
        assert_eq!(session.document, before);
        let mut corrupt = Document::new(Scene::empty("bad-history"));
        corrupt.redo = vec![SceneDelta::Create {
            object: Object::cube("same", None),
            index: 1,
        }
        .into()];
        assert!(corrupt.validate().is_err());
        corrupt.redo = vec![SceneDelta::Delete {
            object: Object::cube("absent", None),
            index: 0,
        }
        .into()];
        assert!(corrupt.validate().is_err());
    }

    // Unique risk: commands/history must survive the actual storage boundary.
    #[test]
    fn edit_undo_redo_save_reopen_preserves_identity_and_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut session = Session::new(Scene::demo()).unwrap();
        let initial = session.scene().clone();
        let mut transform = initial.objects[0].transform.clone();
        transform.translation[1] = 3.0;
        assert_eq!(session.edit(0, "left", &transform).unwrap(), 1);
        session.save(&path).unwrap();
        assert!(!session.is_dirty());
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.scene(), session.scene());
        assert_eq!(reopened.undo(1).unwrap(), 2);
        assert_eq!(reopened.scene().objects, initial.objects);
        reopened.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.redo(2).unwrap(), 3);
        assert_eq!(reopened.scene().objects[0].transform, transform);
        assert_eq!(reopened.scene().id, initial.id);
        assert_eq!(reopened.undo(3).unwrap(), 4);
        transform.translation[1] = 5.0;
        reopened.edit(4, "left", &transform).unwrap();
        assert!(matches!(reopened.redo(5), Err(SceneError::NoRedo)));
    }

    // Unique risk: invalid/stale/no-op commands and revision exhaustion mutate
    // neither current state nor the history/dirty baseline.
    #[test]
    fn commands_are_atomic_and_history_is_bounded() {
        let mut session = Session::new(Scene::demo()).unwrap();
        let initial = session.document.clone();
        let transform = session.scene().objects[0].transform.clone();
        assert_eq!(session.edit(0, "left", &transform).unwrap(), 0);
        assert_eq!(session.document, initial);
        assert!(matches!(
            session.edit(1, "left", &transform),
            Err(SceneError::StaleRevision { .. })
        ));
        assert!(matches!(
            session.undo(1),
            Err(SceneError::StaleRevision { .. })
        ));
        assert!(matches!(
            session.redo(1),
            Err(SceneError::StaleRevision { .. })
        ));
        let mut invalid = transform.clone();
        invalid.scale[0] = f32::NAN;
        assert!(session.edit(0, "left", &invalid).is_err());
        assert!(matches!(
            session.edit(0, "absent", &transform),
            Err(SceneError::ObjectNotFound(_))
        ));
        assert!(matches!(session.undo(0), Err(SceneError::NoUndo)));
        assert_eq!(session.document, initial);
        for i in 1..=40 {
            let mut change = transform.clone();
            change.translation[1] = i as f32;
            session.edit(i - 1, "left", &change).unwrap();
        }
        assert_eq!(session.undo_len(), HISTORY_LIMIT);
        assert_eq!(
            session.history_evictions(),
            0,
            "normal age eviction is not byte-pressure eviction"
        );
        for i in 0..HISTORY_LIMIT {
            session.undo(40 + i as u64).unwrap();
        }
        assert_eq!(session.scene().objects[0].transform.translation[1], 8.0);
        assert_eq!(session.redo_len(), HISTORY_LIMIT);
        session.document.scene.revision = u64::MAX;
        let before = session.document.clone();
        assert!(matches!(
            session.redo(u64::MAX),
            Err(SceneError::RevisionOverflow)
        ));
        assert!(matches!(
            session.edit(u64::MAX, "left", &transform),
            Err(SceneError::RevisionOverflow)
        ));
        assert_eq!(session.document, before);
    }

    // Unique risk: legacy undo AND redo survive an explicit v1-to-current save,
    // including an unchanged save; absent mesh fields preserve old fingerprints.
    #[test]
    fn legacy_v1_history_migrates_without_dropping_or_rewriting_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        let bytes = include_bytes!("../tests/fixtures/legacy-v1.scene.json");
        std::fs::write(&path, bytes).unwrap();
        let mut session = Session::open(&path).unwrap();
        assert_eq!(session.loaded_format_version(), 1);
        assert!(!session.is_dirty());
        assert_eq!((session.undo_len(), session.redo_len()), (1, 1));
        assert!(session.scene().objects[0].mesh.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let legacy: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        let serialized: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(session.scene()).unwrap()).unwrap();
        assert_eq!(serialized, legacy["scene"]);
        let fingerprint = session.scene().fingerprint();
        // Fixed from the v1 struct's canonical field order, before mesh existed.
        assert_eq!(fingerprint, "65601f5c06cf65d9");
        let occupied = dir.path().join("occupied.json");
        std::fs::write(&occupied, b"external work").unwrap();
        assert!(matches!(session.save(&occupied), Err(SceneError::Conflict)));
        assert_eq!(session.loaded_format_version(), 1);
        assert!(!session.is_dirty());
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        session.save(&path).unwrap();
        assert_eq!(session.loaded_format_version(), FORMAT_VERSION);
        assert!(!session.is_dirty());
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored["format_version"], FORMAT_VERSION);
        assert_eq!(stored["undo"][0]["scene"][0]["kind"], "transform");
        assert_eq!(stored["redo"][0]["scene"][0]["kind"], "transform");
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.scene().fingerprint(), fingerprint);
        reopened.redo(3).unwrap();
        assert_eq!(reopened.scene().objects[0].transform.translation[0], 2.0);
        reopened.undo(4).unwrap();
        reopened.undo(5).unwrap();
        assert_eq!(reopened.scene().objects[0].transform.translation[0], 0.0);
        reopened.redo(6).unwrap();
        assert_eq!(reopened.scene().objects[0].transform.translation[0], 1.0);
    }

    // Unique risk: a mislabeled v1 document must never silently discard v2 mesh
    // assignments or accept v2 tagged history, including explicit mesh null.
    #[test]
    fn legacy_v1_rejects_mesh_fields_and_v2_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("legacy.json");
        let legacy: serde_json::Value =
            serde_json::from_slice(include_bytes!("../tests/fixtures/legacy-v1.scene.json"))
                .unwrap();
        for mesh in [serde_json::Value::Null, "owned-mesh".into()] {
            let mut value = legacy.clone();
            value["scene"]["objects"][0]["mesh"] = mesh;
            let bytes = serde_json::to_vec(&value).unwrap();
            std::fs::write(&path, &bytes).unwrap();
            assert!(matches!(Session::open(&path), Err(SceneError::Format(_))));
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        let mut value = legacy;
        value["undo"][0]["kind"] = "transform".into();
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Session::open(&path).is_err());
    }

    // Unique risk: reusable material edits, assignment and atomic creation must
    // share ordinary history/storage while preserving tint and distinct ID namespaces.
    #[test]
    fn material_authoring_shared_history_roundtrip_and_atomic_failures() {
        let mut session = Session::new(Scene::demo()).unwrap();
        let original_color = session.scene().objects[0].color;
        let original_authoring = session.authoring().clone();
        let material = OpaqueMaterial {
            base_color_texture: Some("unavailable-texture".into()),
            roughness_texture: Some("unavailable-roughness".into()),
            response: MaterialResponse::Dielectric { roughness: 0.08 },
            ..OpaqueMaterial::default()
        };
        let mut references = Session::new(Scene::empty("unused-map")).unwrap();
        references
            .set_material(
                0,
                "unused",
                OpaqueMaterial {
                    base_color_texture: None,
                    ..material.clone()
                },
            )
            .unwrap();
        assert!(
            references.has_asset_references(),
            "Unused current maps retain the root guard"
        );
        references.undo(1).unwrap();
        assert!(
            references.has_asset_references(),
            "Redo-only maps retain the root guard"
        );
        references.redo(2).unwrap();
        references
            .set_material(3, "unused", OpaqueMaterial::default())
            .unwrap();
        assert!(
            references.has_asset_references(),
            "Undo-only maps retain the root guard"
        );
        session
            .create_and_assign_material(0, "left", "left", material.clone())
            .unwrap();
        assert_eq!(session.undo_len(), 1);
        assert_eq!(session.scene().objects[0].color, original_color);
        assert_eq!(session.authoring(), &original_authoring);
        assert!(session.contains_object_id("left") && session.contains_material_id("left"));
        let before = session.document.clone();
        assert!(session.remove_material(1, "left").is_err());
        assert!(session
            .assign_material(1, "right", Some("absent".into()))
            .is_err());
        assert!(session
            .create_and_assign_material(1, "absent", "new", material.clone())
            .is_err());
        assert!(session.set_material(0, "new", material.clone()).is_err());
        let invalid = OpaqueMaterial {
            tint: [f32::NAN, 0.0, 1.0],
            ..material.clone()
        };
        assert!(session.set_material(1, "left", invalid).is_err());
        let invalid = OpaqueMaterial {
            roughness_texture: Some("folder/roughness".into()),
            ..material.clone()
        };
        assert!(session.set_material(1, "left", invalid).is_err());
        assert_eq!(session.document, before);
        session.undo(1).unwrap();
        assert!(
            session.scene().materials.is_empty() && session.scene().objects[0].material.is_none()
        );
        assert!(
            session.has_asset_references(),
            "Redo material textures retain the root guard"
        );
        assert!(session.contains_material_id("left"));
        assert!(!session.contains_object_id("material-1"));
        let before = session.document.clone();
        assert!(session.set_material(2, "left", material.clone()).is_err());
        assert_eq!(session.assign_material(2, "left", None).unwrap(), 2);
        assert_eq!(session.document, before);
        session.redo(2).unwrap();
        session
            .assign_material(3, "right", Some("left".into()))
            .unwrap();
        let edited = OpaqueMaterial {
            tint: [0.25, 0.5, 1.0],
            response: MaterialResponse::LegacyDiffuse,
            roughness_texture: Some("inactive-roughness".into()),
            ..material.clone()
        };
        session.set_material(4, "left", edited.clone()).unwrap();
        assert_eq!(session.scene().materials["left"], edited);
        session.duplicate(5, "left", "copy", None).unwrap();
        assert_eq!(session.scene().objects[1].material.as_deref(), Some("left"));
        assert_eq!(session.scene().materials.len(), 1);
        session
            .copy_material(6, "left", "material-1", Some("Copied".into()))
            .unwrap();
        session.remove_material(7, "material-1").unwrap();
        assert!(session.contains_material_id("material-1"));
        assert!(!session.contains_object_id("material-1"));
        assert_eq!(
            session.allocate_material_ids(1).unwrap(),
            vec!["material-2"]
        );
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("materials.json");
        session.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.document, session.document);
        reopened.undo(8).unwrap();
        assert_eq!(
            reopened.scene().materials["material-1"].name.as_deref(),
            Some("Copied")
        );
        reopened.redo(9).unwrap();
        let recovered = Session::from_snapshot_bytes(&reopened.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(recovered.document, reopened.document);
        assert!(recovered.is_dirty() && recovered.path().is_none());
        let preview = Preview::new(reopened.scene());
        reopened.set_material(10, "left", material).unwrap();
        assert_eq!(preview.scene.materials["left"], edited);
        assert_eq!(Scene::load(&path).unwrap().materials["left"], edited);
        let before = reopened.document.clone();
        let mut objects = vec![Object::cube("bulk-1", None), Object::cube("bulk-2", None)];
        for object in &mut objects {
            object.material = Some("bulk-material".into());
        }
        assert!(reopened
            .create_many_with_materials(10, objects.clone(), BTreeMap::new())
            .is_err());
        assert!(reopened
            .create_many_with_materials(11, objects.clone(), BTreeMap::new())
            .is_err());
        assert!(reopened
            .create_many_with_materials(
                11,
                objects.clone(),
                BTreeMap::from([("material-1".into(), OpaqueMaterial::default())])
            )
            .is_err());
        assert_eq!(reopened.document, before);
        assert_eq!(
            reopened
                .create_many_with_materials(11, Vec::new(), BTreeMap::new())
                .unwrap(),
            11
        );
        reopened
            .create_many_with_materials(
                11,
                objects,
                BTreeMap::from([("bulk-material".into(), OpaqueMaterial::default())]),
            )
            .unwrap();
        assert_eq!(reopened.undo_len(), before.undo.len() + 1);
        reopened.undo(12).unwrap();
        assert_eq!(reopened.scene().objects, before.scene.objects);
        assert_eq!(reopened.scene().materials, before.scene.materials);
        reopened.redo(13).unwrap();
        let restored = Session::from_snapshot_bytes(&reopened.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(restored.document, reopened.document);
    }

    // Unique risk: old structs using the live Scene/Change types accidentally
    // accept new empty/null fields, nested lifecycle references or material deltas.
    #[test]
    fn pre_v6_schemas_reject_material_fields_at_every_owning_boundary() {
        let clean = serde_json::to_value(Document::new(Scene::demo())).unwrap();
        for version in 1..=5 {
            let mut old = clean.clone();
            old["format_version"] = version.into();
            let bytes = serde_json::to_vec(&old).unwrap();
            let loaded = Document::decode(&bytes).unwrap();
            assert_eq!(serde_json::to_value(loaded.scene).unwrap(), clean["scene"]);
            for field in ["map", "ref"] {
                let mut invalid = old.clone();
                if field == "map" {
                    invalid["scene"]["materials"] = serde_json::json!({});
                } else {
                    invalid["scene"]["objects"][0]["material"] = serde_json::Value::Null;
                }
                assert!(Document::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
            }
        }
        for version in 3..=5 {
            for stack in ["undo", "redo"] {
                let mut old = clean.clone();
                old["format_version"] = version.into();
                let mut object = old["scene"]["objects"][0].clone();
                object["material"] = serde_json::Value::Null;
                for delta in [
                    serde_json::json!({"kind":"create","object":object,"index":0}),
                    serde_json::json!({"kind":"objects","created":[{"object":object,"index":0}],"deleted":[]}),
                    serde_json::json!({"kind":"material","id":"left","before":null,"after":"mat"}),
                ] {
                    old[stack] = if version == 5 {
                        serde_json::json!([{"scene":[delta]}])
                    } else {
                        serde_json::json!([delta])
                    };
                    let error = Document::decode(&serde_json::to_vec(&old).unwrap()).unwrap_err();
                    assert!(
                        error.to_string().contains("materials require scene v6"),
                        "{error}"
                    );
                }
            }
        }
        let mut old = clean.clone();
        old["format_version"] = 5.into();
        old["undo"] = serde_json::json!([{"materials":[]}]);
        assert!(Document::decode(&serde_json::to_vec(&old).unwrap())
            .unwrap_err()
            .to_string()
            .contains("materials require scene v6"));
        let mut invalid = clean;
        invalid["scene"]["objects"][0]["material"] = "absent".into();
        assert!(Document::decode(&serde_json::to_vec(&invalid).unwrap()).is_err());
    }

    // Unique risk: shared live light types must not smuggle new shadow semantics
    // into Scene1–7, including either historical endpoint in a mixed change.
    #[test]
    fn scene8_shadow_migration_guards_exact_light_boundaries_only() {
        use serde_json::json;
        let clean = serde_json::to_value(Document::new(Scene::demo())).unwrap();
        for version in 1..=7 {
            let mut old = clean.clone();
            old["format_version"] = version.into();
            let migrated = Document::decode(&serde_json::to_vec(&old).unwrap()).unwrap();
            assert!(migrated.scene.lighting.is_legacy());
            let saved: serde_json::Value =
                serde_json::from_slice(&migrated.encode().unwrap()).unwrap();
            assert_eq!(saved["format_version"], FORMAT_VERSION);
        }
        let mut scene = Scene::demo();
        scene.lighting = Lighting::authored_default("sun");
        let mut session = Session::new(scene).unwrap();
        let mut geometry = session.scene().objects[0].transform.clone();
        geometry.translation[0] += 1.0;
        let mut sun = session
            .scene()
            .lighting
            .light("sun")
            .unwrap()
            .transform
            .clone();
        sun.translation[0] += 1.0;
        session
            .edit_transforms(0, &[("left".into(), geometry), ("sun".into(), sun)])
            .unwrap();
        let shadows = [
            serde_json::to_value(SunShadow::default()).unwrap(),
            serde_json::to_value(SunShadow {
                quality: ShadowQuality::High,
                ..SunShadow::default()
            })
            .unwrap(),
            serde_json::Value::Null,
        ];
        for stack in ["undo", "redo"] {
            let mut old: serde_json::Value =
                serde_json::from_slice(&session.snapshot_bytes().unwrap()).unwrap();
            old["format_version"] = 7.into();
            Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()).unwrap();
            for shadow in &shadows {
                let mut current = old.clone();
                current["scene"]["lighting"]["lights"]["sun"]["kind"]["shadow"] = shadow.clone();
                assert!(Document::decode(&serde_json::to_vec(&current).unwrap())
                    .unwrap_err()
                    .to_string()
                    .contains("Sun shadows require scene v8"));
                for endpoint in ["before", "after"] {
                    let mut historical = old.clone();
                    historical[stack][0]["scene"][1][endpoint]["lights"]["sun"]["kind"]["shadow"] =
                        shadow.clone();
                    assert!(Document::decode(&serde_json::to_vec(&historical).unwrap())
                        .unwrap_err()
                        .to_string()
                        .contains("Sun shadows require scene v8"));
                }
            }
            if stack == "undo" {
                session.undo(1).unwrap();
            }
        }
        let mut behavior = Session::new(Scene::demo()).unwrap();
        let mut b = binding("left", "missing-source");
        b.properties
            .insert("shadow".into(), PropertyValue::Bool(true));
        behavior.set_binding(0, "binding", Some(b.clone())).unwrap();
        b.properties
            .insert("shadow".into(), PropertyValue::Bool(false));
        behavior.set_binding(1, "binding", Some(b)).unwrap();
        behavior.undo(2).unwrap();
        for version in 5..=7 {
            let mut old: serde_json::Value =
                serde_json::from_slice(&behavior.snapshot_bytes().unwrap()).unwrap();
            old["format_version"] = json!(version);
            let migrated =
                Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()).unwrap();
            assert_eq!(migrated.authoring(), behavior.authoring());
            assert_eq!(migrated.undo_len(), 1);
            assert_eq!(migrated.redo_len(), 1);
        }
    }

    // Unique risk: material maps escape current/reachable byte accounting, or
    // material count overflow/create-and-assign leaves a half-applied reference.
    #[test]
    fn material_bounds_and_reachable_payload_admission_are_atomic() {
        let mut scene = Scene::demo();
        scene.materials = (0..MAX_MATERIALS)
            .map(|n| {
                (
                    format!("mat-{n}"),
                    OpaqueMaterial {
                        name: Some("n".repeat(128)),
                        base_color_texture: Some("t".repeat(128)),
                        roughness_texture: Some("r".repeat(128)),
                        ..OpaqueMaterial::default()
                    },
                )
            })
            .collect();
        let mut session = Session::new(scene).unwrap();
        let before = session.document.clone();
        assert!(session
            .create_and_assign_material(0, "left", "overflow", OpaqueMaterial::default())
            .is_err());
        assert_eq!(session.document, before);
        // Leave a narrow but valid compact-save margin. The newest removal
        // requires reserving the larger pre-removal material map plus its delta.
        let target = MAX_FILE_BYTES as usize - 64;
        for n in 0..MAX_SOURCES {
            let key = format!("source-{n}");
            session
                .document
                .authoring
                .sources
                .insert(key.clone(), source(""));
            let size = serde_json::to_vec(&session.document).unwrap().len() + 1;
            assert!(size < target);
            let count = (target - size).min(MAX_SOURCE_BYTES);
            session
                .document
                .authoring
                .sources
                .get_mut(&key)
                .unwrap()
                .text = "x".repeat(count);
            if count < MAX_SOURCE_BYTES {
                break;
            }
        }
        session.document.validate().unwrap();
        let before = session.document.clone();
        assert!(matches!(
            session.remove_material(0, "mat-0"),
            Err(SceneError::Capacity)
        ));
        assert_eq!(session.document, before);
        assert!(matches!(
            session.set_display(0, DisplayProfile::surface_hdr()),
            Err(SceneError::Capacity)
        ));
        assert_eq!(session.document, before);
        assert_eq!(session.history_evictions(), 0);
    }

    // Unique risk: display deltas must share ordinary chronology/recovery and
    // admission without granting future semantics to old current/history keys.
    #[test]
    fn display_history_and_surface_version_boundaries_are_atomic() {
        use serde_json::{json, Value};
        let mut session = Session::new(Scene::demo()).unwrap();
        let legacy_scene = serde_json::to_value(session.scene()).unwrap();
        assert!(legacy_scene.get("display").is_none());
        assert!(serde_json::to_value(OpaqueMaterial::default())
            .unwrap()
            .get("response")
            .is_none());
        assert!(serde_json::to_value(OpaqueMaterial::default())
            .unwrap()
            .get("roughness_texture")
            .is_none());
        let current = session.snapshot_bytes().unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&current).unwrap()["format_version"],
            FORMAT_VERSION
        );
        assert_eq!(
            Session::from_snapshot_bytes(&current).unwrap().scene(),
            session.scene()
        );
        let before = session.document.clone();
        assert_eq!(
            session.set_display(0, DisplayProfile::LegacySdr).unwrap(),
            0
        );
        assert_eq!(session.document, before);
        let display = DisplayProfile::SurfaceHdr {
            exposure_stops: 2.0,
        };
        session.set_display(0, display).unwrap();
        session
            .set_material(
                1,
                "surface",
                OpaqueMaterial {
                    response: MaterialResponse::Dielectric { roughness: 0.08 },
                    ..Default::default()
                },
            )
            .unwrap();
        session.undo(2).unwrap();
        assert!(session.scene().materials.is_empty());
        assert_eq!(session.scene().display, display);
        session.undo(3).unwrap();
        assert_eq!(session.scene().display, DisplayProfile::LegacySdr);
        session.redo(4).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("surface.json");
        session.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.document, session.document);
        reopened.redo(5).unwrap();
        let restored = Session::from_snapshot_bytes(&reopened.snapshot_bytes().unwrap()).unwrap();
        assert_eq!(restored.document, reopened.document);
        assert!(restored.path().is_none() && restored.is_dirty());
        let before = reopened.document.clone();
        for invalid in [f32::NAN, f32::INFINITY, -8.1, 8.1] {
            assert!(reopened
                .set_display(
                    6,
                    DisplayProfile::SurfaceHdr {
                        exposure_stops: invalid
                    }
                )
                .is_err());
        }
        assert!(matches!(
            reopened.set_display(5, DisplayProfile::LegacySdr),
            Err(SceneError::StaleRevision { .. })
        ));
        assert_eq!(reopened.document, before);
        let mut corrupt = serde_json::to_value(&reopened.document).unwrap();
        corrupt["undo"][0]["scene"][0]["after"]["exposure_stops"] = json!(8.0000000001);
        assert!(Session::from_snapshot_bytes(&serde_json::to_vec(&corrupt).unwrap()).is_err());
        // Isolate actual current and material-history positions, including null
        // and explicit legacy values that would otherwise deserialize/default.
        let baseline = serde_json::to_value(Document::new(Scene::empty("old"))).unwrap();
        for version in 1..=9 {
            let mut old = baseline.clone();
            old["format_version"] = json!(version);
            let loaded = Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()).unwrap();
            assert_eq!(loaded.scene(), &Scene::empty("old"));
            assert_eq!(loaded.loaded_format_version(), version);
            old["scene"]["materials"] = json!({"surface":{"tint":[1,1,1],"filter":"linear","address":"repeat","roughness_texture":null}});
            assert!(
                matches!(Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()), Err(SceneError::Invalid(message)) if message.contains("v10"))
            );
        }
        // Both endpoints and both stacks are owning positions even when the
        // future field is null. They must reject before history consistency.
        for (stack, endpoint, texture) in [
            ("undo", "before", Value::Null),
            ("undo", "after", json!("roughness")),
            ("redo", "before", json!("roughness")),
            ("redo", "after", Value::Null),
        ] {
            let mut old = baseline.clone();
            old["format_version"] = json!(9);
            old[stack] = json!([{"materials":[{"id":"surface","before":null,"after":null}]}]);
            old[stack][0]["materials"][0][endpoint] = json!({"tint":[1,1,1],"filter":"linear","address":"repeat","roughness_texture":texture});
            assert!(
                matches!(Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()), Err(SceneError::Invalid(message)) if message.contains("v10"))
            );
        }
        // Genuine Scene9 display/response values retain their interpretation;
        // an absent new slot is not required to be inserted on Open.
        let mut v9_surface = serde_json::to_value(&reopened.document).unwrap();
        v9_surface["format_version"] = json!(9);
        let loaded =
            Session::from_snapshot_bytes(&serde_json::to_vec(&v9_surface).unwrap()).unwrap();
        assert_eq!(loaded.document.scene, reopened.document.scene);
        assert_eq!(loaded.document.undo, reopened.document.undo);
        for version in 1..9 {
            for field in [Value::Null, json!({"type":"legacy_sdr"})] {
                let mut old = baseline.clone();
                old["format_version"] = json!(version);
                old["scene"]["display"] = field;
                assert!(
                    matches!(Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()), Err(SceneError::Invalid(message)) if message.contains("v9"))
                );
            }
            for stack in ["undo", "redo"] {
                let mut old = baseline.clone();
                old["format_version"] = json!(version);
                let delta = json!({"kind":"display","before":{"type":"legacy_sdr"},"after":{"type":"surface_hdr","exposure_stops":0}});
                old[stack] = if version <= 4 {
                    json!([delta])
                } else {
                    json!([{"scene":[delta]}])
                };
                assert!(
                    matches!(Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()), Err(SceneError::Invalid(message)) if message.contains("v9"))
                );
            }
        }
        for version in 6..9 {
            for response in [Value::Null, json!({"type":"legacy_diffuse"})] {
                let mut old = baseline.clone();
                old["format_version"] = json!(version);
                old["scene"]["materials"] = json!({"surface":{"tint":[1,1,1],"filter":"linear","address":"repeat","response":response}});
                assert!(
                    matches!(Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()), Err(SceneError::Invalid(message)) if message.contains("v9"))
                );
                for (stack, endpoint) in [("undo", "before"), ("redo", "after")] {
                    let mut old = baseline.clone();
                    old["format_version"] = json!(version);
                    old[stack] =
                        json!([{"materials":[{"id":"surface","before":null,"after":null}]}]);
                    old[stack][0]["materials"][0][endpoint] = json!({"tint":[1,1,1],"filter":"linear","address":"repeat","response":response});
                    assert!(
                        matches!(Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()), Err(SceneError::Invalid(message)) if message.contains("v9"))
                    );
                }
            }
        }
        let mut behavior = Session::new(Scene::demo()).unwrap();
        behavior
            .set_source(0, "source", Some(source("-- {\"collider\":null}")))
            .unwrap();
        let mut b = binding("left", "source");
        b.properties
            .insert("display".into(), PropertyValue::Text("unrelated".into()));
        b.properties
            .insert("response".into(), PropertyValue::Text("unrelated".into()));
        b.properties.insert(
            "roughness_texture".into(),
            PropertyValue::Text("unrelated".into()),
        );
        b.properties
            .insert("collider".into(), PropertyValue::Text("unrelated".into()));
        behavior.set_binding(1, "binding", Some(b)).unwrap();
        behavior.undo(2).unwrap();
        for version in 5..=11 {
            let mut old = serde_json::to_value(&behavior.document).unwrap();
            old["format_version"] = json!(version);
            let loaded = Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()).unwrap();
            assert_eq!(loaded.document.authoring, behavior.document.authoring);
            assert_eq!((loaded.undo_len(), loaded.redo_len()), (1, 1));
        }
    }

    // Unique risk: color deltas must retain reversible persisted history without
    // admitting future deltas under legacy format labels or mutating on rejection.
    #[test]
    fn color_authoring_roundtrips_and_rejects_atomically() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("color.json");
        let mut session = Session::new(Scene::demo()).unwrap();
        let original = session.scene().objects[0].color;
        let color = [0.0, 0.25, 1.0, 0.75];
        session.set_color(0, "left", color).unwrap();
        assert_eq!(session.undo_len(), 1);
        session.undo(1).unwrap();
        session.save(&path).unwrap();
        let before = session.document.clone();
        assert_eq!(session.set_color(2, "left", original).unwrap(), 2);
        assert!(matches!(
            session.set_color(1, "left", color),
            Err(SceneError::StaleRevision { .. })
        ));
        assert!(matches!(
            session.set_color(2, "absent", color),
            Err(SceneError::ObjectNotFound(_))
        ));
        for channel in [f32::NAN, f32::INFINITY, -0.01, 1.01] {
            assert!(session
                .set_color(2, "left", [channel, 0.0, 0.0, 1.0])
                .is_err());
        }
        assert_eq!(session.document, before);
        assert!(!session.is_dirty());
        let mut reopened = Session::open(&path).unwrap();
        reopened.redo(2).unwrap();
        assert_eq!(reopened.scene().objects[0].color, color);
        reopened.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        reopened.undo(3).unwrap();
        assert_eq!(reopened.scene().objects[0].color, original);
        reopened.redo(4).unwrap();
        assert_eq!(reopened.scene().objects[0].color, color);
        let mut value = serde_json::to_value(&reopened.document).unwrap();
        value["undo"][0]["scene"][0]["before"][0] = serde_json::json!(2.0);
        assert!(Session::from_snapshot_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
        for version in [3, 4] {
            let mut value = legacy_value(&reopened.document);
            value["format_version"] = serde_json::json!(version);
            assert!(Session::from_snapshot_bytes(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }

    // Unique risk: component and transform history must preserve valid sphere
    // scaling atomically, while old labels reject only actual collider owners.
    #[test]
    fn collider_history_scale_and_version_guards_are_atomic() {
        use serde_json::{json, Value};
        // Raw JSON must preserve the next representable f64 above the maximum
        // until collider validation, before any Scene-f32 narrowing.
        for shape in [
            ColliderShape::Sphere { radius: 1000.0 },
            ColliderShape::Box {
                half_extents: [1.0, 1000.0, 1.0],
            },
        ] {
            let mut scene = Scene::demo();
            scene.objects[0].collider = Some(Collider {
                shape,
                ..Default::default()
            });
            let session = Session::new(scene).unwrap();
            let raw = String::from_utf8(session.snapshot_bytes().unwrap()).unwrap();
            assert_eq!(raw.matches("1000.0").count(), 1);
            assert_eq!(
                Session::from_snapshot_bytes(raw.as_bytes())
                    .unwrap()
                    .scene(),
                session.scene()
            );
            let outside = raw.replacen("1000.0", "1000.0000000000001", 1);
            assert!(
                Session::from_snapshot_bytes(outside.as_bytes()).is_err(),
                "{shape:?}"
            );
        }
        let sphere = Collider {
            enabled: true,
            shape: ColliderShape::Sphere { radius: 0.25 },
            ..Collider::default()
        };
        let mut session = Session::new(Scene::demo()).unwrap();
        session.set_collider(0, "left", Some(sphere)).unwrap();
        session.set_collider(1, "left", None).unwrap();
        session.undo(2).unwrap();
        let before = session.document.clone();
        assert_eq!(session.set_collider(3, "left", Some(sphere)).unwrap(), 3);
        assert!(matches!(
            session.set_collider(2, "left", None),
            Err(SceneError::StaleRevision { .. })
        ));
        assert!(matches!(
            session.set_collider(3, "absent", None),
            Err(SceneError::ObjectNotFound(_))
        ));
        assert!(session
            .set_collider(
                3,
                "left",
                Some(Collider {
                    offset: [f32::NAN, 0.0, 0.0],
                    ..sphere
                })
            )
            .is_err());
        let mut nonuniform = session.scene().objects[0].transform.clone();
        nonuniform.scale = [1.0, 2.0, 1.0];
        let mut moved = session.scene().objects[2].transform.clone();
        moved.translation[1] = 2.0;
        assert!(session
            .edit_transforms(
                3,
                &[("right".into(), moved), ("left".into(), nonuniform.clone())]
            )
            .is_err());
        assert_eq!(session.document, before);
        assert_eq!(session.redo_len(), 1);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("collider.json");
        session.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.document, session.document);
        assert_eq!(
            Session::from_snapshot_bytes(&reopened.snapshot_bytes().unwrap())
                .unwrap()
                .document,
            reopened.document
        );
        reopened.redo(3).unwrap();
        assert!(reopened.scene().objects[0].collider.is_none());
        reopened.undo(4).unwrap();
        let mut scaled = reopened.scene().objects[0].transform.clone();
        scaled.scale = [2.0; 3];
        reopened.edit(5, "left", &scaled).unwrap();
        assert_eq!(
            reopened.scene().objects[0]
                .collider
                .unwrap()
                .resolve(&scaled)
                .unwrap()
                .unwrap()
                .shape,
            ResolvedShape::Sphere { radius: 0.5 }
        );
        reopened
            .set_collider(
                6,
                "left",
                Some(Collider {
                    enabled: false,
                    ..sphere
                }),
            )
            .unwrap();
        reopened.edit(7, "left", &nonuniform).unwrap();
        let before = reopened.document.clone();
        assert!(reopened.set_collider(8, "left", Some(sphere)).is_err());
        assert_eq!(reopened.document, before);
        reopened.document.validate().unwrap();
        let mut corrupt = serde_json::to_value(&session.document).unwrap();
        corrupt["undo"][0]["scene"][0]["after"]["shape"]["radius"] = json!(0.5);
        assert!(Session::from_snapshot_bytes(&serde_json::to_vec(&corrupt).unwrap()).is_err());

        let baseline = serde_json::to_value(Document::new(Scene::demo())).unwrap();
        for version in 1..=11 {
            let mut old = baseline.clone();
            old["format_version"] = json!(version);
            let loaded = Session::from_snapshot_bytes(&serde_json::to_vec(&old).unwrap()).unwrap();
            assert_eq!(loaded.scene(), &Scene::demo());
            assert_eq!(loaded.loaded_format_version(), version);
            for payload in [
                Value::Null,
                serde_json::to_value(Collider::default()).unwrap(),
            ] {
                let mut bad = old.clone();
                bad["scene"]["objects"][0]["collider"] = payload;
                assert!(
                    matches!(Document::decode(&serde_json::to_vec(&bad).unwrap()),
                    Err(SceneError::Invalid(message)) if message.contains("colliders require scene v12"))
                );
            }
        }
        // Representative scalar/grouped envelopes cover both stacks and both
        // lifecycle payload lists. Null remains a field-presence violation.
        let mut object = serde_json::to_value(Object::cube("held", None)).unwrap();
        object["collider"] = Value::Null;
        for (version, stack, delta) in [
            (
                3,
                "undo",
                json!({"kind":"create","object":object,"index":0}),
            ),
            (
                4,
                "redo",
                json!({"kind":"delete","object":object,"index":0}),
            ),
            (
                4,
                "undo",
                json!({"kind":"objects","created":[{"object":object,"index":0}],"deleted":[]}),
            ),
            (
                5,
                "redo",
                json!({"kind":"objects","created":[],"deleted":[{"object":object,"index":0}]}),
            ),
            (
                11,
                "undo",
                json!({"kind":"collider","id":"left","before":null,"after":null}),
            ),
            (
                11,
                "redo",
                json!({"kind":"collider","id":"left","before":null,"after":null}),
            ),
        ] {
            let mut bad = baseline.clone();
            bad["format_version"] = json!(version);
            bad[stack] = if version <= 4 {
                json!([delta])
            } else {
                json!([{"scene":[delta]}])
            };
            assert!(
                matches!(Document::decode(&serde_json::to_vec(&bad).unwrap()),
                Err(SceneError::Invalid(message)) if message.contains("colliders require scene v12")),
                "{version}/{stack}"
            );
        }
    }

    // Unique risk: mesh and transform edits share one chronological persisted
    // history while invalid/stale/no-op assignments preserve redo and dirty state.
    #[test]
    fn mesh_assignments_share_history_and_failed_commands_are_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mesh.json");
        let mut session = Session::new(Scene::demo()).unwrap();
        let mesh = Some("owned_mesh.01".into());
        session.set_mesh(0, "left", mesh.clone()).unwrap();
        let mut transform = session.scene().objects[0].transform.clone();
        transform.translation[1] = 5.0;
        session.edit(1, "left", &transform).unwrap();
        session.undo(2).unwrap();
        session.save(&path).unwrap();
        let before = session.document.clone();
        assert_eq!(session.set_mesh(3, "left", mesh.clone()).unwrap(), 3);
        assert!(matches!(
            session.set_mesh(2, "left", mesh.clone()),
            Err(SceneError::StaleRevision { .. })
        ));
        assert!(matches!(
            session.set_mesh(3, "absent", None),
            Err(SceneError::ObjectNotFound(_))
        ));
        for invalid in [
            "",
            "../source.obj",
            "mesh/name",
            "mesh name",
            &"x".repeat(129),
        ] {
            assert!(session.set_mesh(3, "left", Some(invalid.into())).is_err());
        }
        assert_eq!(session.document, before);
        assert!(!session.is_dirty());
        let mut reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.redo(3).unwrap(), 4);
        assert_eq!(reopened.scene().objects[0].transform, transform);
        reopened.undo(4).unwrap();
        reopened.undo(5).unwrap();
        assert_eq!(reopened.scene().objects[0].mesh, None);
        reopened.save(&path).unwrap();
        let mut reopened = Session::open(&path).unwrap();
        reopened.redo(6).unwrap();
        assert_eq!(reopened.scene().objects[0].mesh, mesh);
        reopened.set_mesh(7, "left", None).unwrap();
        assert_eq!(reopened.redo_len(), 0);
        reopened.undo(8).unwrap();
        assert_eq!(reopened.scene().objects[0].mesh, mesh);
        reopened.document.scene.revision = u64::MAX;
        let before = reopened.document.clone();
        assert!(matches!(
            reopened.set_mesh(u64::MAX, "left", None),
            Err(SceneError::RevisionOverflow)
        ));
        assert_eq!(reopened.document, before);
        // Bounded history applies equally to mesh changes.
        let mut bounded = Session::new(Scene::demo()).unwrap();
        for i in 0..40 {
            bounded
                .set_mesh(i, "left", Some(format!("mesh-{i}")))
                .unwrap();
        }
        assert_eq!(bounded.undo_len(), HISTORY_LIMIT);
        bounded.document.validate().unwrap();
        // Persisted mesh chains must match the current field, just like transforms.
        bounded.document.scene.objects[0].mesh = None;
        assert!(bounded.document.validate().is_err());
    }

    // Unique risk: public code-first values bypass constructor validation.
    #[test]
    fn public_value_validation_rejects_representative_limits() {
        let demo = Scene::demo();
        let mut cases = Vec::new();
        let mut scene = demo.clone();
        scene.id = "x".repeat(129);
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].mesh = Some("mesh/source.obj".into());
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[1].id = "left".into();
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].transform.translation[0] = 10_001.0;
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].transform.rotation[0] = f32::INFINITY;
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].transform.scale[0] = 0.0;
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects[0].color[0] = 1.01;
        cases.push(scene);
        let mut scene = demo.clone();
        scene.objects = vec![scene.objects[0].clone(); MAX_OBJECTS + 1];
        cases.push(scene);
        for scene in cases {
            assert!(Session::new(scene).is_err());
        }
        let mut boundary = demo;
        boundary.objects[0].transform = Transform {
            translation: [10_000.0; 3],
            rotation: [-10_000.0; 3],
            scale: [0.001, 1_000.0, 1.0],
        };
        assert!(boundary.validate().is_ok());
    }

    // Unique risk: malformed/foreign/versioned files or inconsistent persisted
    // history are accepted and later corrupt undo, or exhaust bounded ingestion.
    #[test]
    fn file_boundary_rejects_corruption_versions_history_and_foreign_formats() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let good = serde_json::to_value(Document::new(Scene::demo())).unwrap();
        let mut cases = vec![
            b"{".to_vec(),
            br#"{"format_version":1,"id":"logistics","config":{}}"#.to_vec(),
        ];
        let mut value = good.clone();
        value["format_version"] = (FORMAT_VERSION + 1).into();
        cases.push(serde_json::to_vec(&value).unwrap());
        let mut value = good.clone();
        value["unexpected"] = true.into();
        cases.push(serde_json::to_vec(&value).unwrap());
        let mut value = good.clone();
        value["undo"] =
            serde_json::json!([{"kind":"mesh","id":"left","before":null,"after":"owned-mesh"}]);
        cases.push(serde_json::to_vec(&value).unwrap());
        let mut value = good.clone();
        value["undo"] = serde_json::json!([{"kind":"unrecognized","id":"left"}]);
        cases.push(serde_json::to_vec(&value).unwrap());
        let mut value = good;
        value["undo"] = serde_json::json!([{"id":"left","before":Transform::default(),"after":Transform::default()}]);
        cases.push(serde_json::to_vec(&value).unwrap());
        cases.push(vec![b' '; MAX_FILE_BYTES as usize + 1]);
        for bytes in cases {
            std::fs::write(&path, &bytes).unwrap();
            assert!(Session::open(&path).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
        let mut session = Session::new(Scene::demo()).unwrap();
        session.document.undo = vec![
            SceneDelta::Transform {
                id: "left".into(),
                before: Transform::default(),
                after: Transform::default()
            }
            .into();
            HISTORY_LIMIT + 1
        ];
        assert!(session.document.validate().is_err());
    }

    // Unique risk: failed atomic saves, full-baseline conflicts and lock/path
    // aliases overwrite external work or mark unsaved work as clean.
    #[test]
    fn persistence_failure_conflict_and_cooperating_lock_preserve_work() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scene.json");
        let mut session = Session::new(Scene::demo()).unwrap();
        session.save(&path).unwrap();
        let original = std::fs::read(&path).unwrap();
        let mut invalid = Scene::demo();
        invalid.objects[0].transform.scale[0] = 0.0;
        assert!(invalid.save(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let directory = dir.path().join("directory");
        std::fs::create_dir(&directory).unwrap();
        assert!(session.scene().save(&directory).is_err());
        assert!(directory.is_dir());
        let lock = SceneLock::acquire(&path).unwrap();
        assert!(matches!(
            session.save(dir.path().join("./scene.json")),
            Err(SceneError::Busy)
        ));
        // Model a descriptor inherited by another process: closing just this File
        // would retain flock until every duplicate closes, beyond the guard scope.
        let inherited = lock._file.try_clone().unwrap();
        drop(lock);
        let reacquired = SceneLock::acquire(&path).unwrap();
        drop(reacquired);
        drop(inherited);
        let mut external = Document::load(&path).unwrap();
        external.scene.objects[0].color[0] = 0.5;
        external.write(&path, false).unwrap();
        let mut transform = session.scene().objects[0].transform.clone();
        transform.translation[2] = 9.0;
        session.edit(0, "left", &transform).unwrap();
        let before = session.document.clone();
        assert!(matches!(session.save(&path), Err(SceneError::Conflict)));
        assert_eq!(session.document, before);
        assert!(session.is_dirty());
        assert_eq!(Document::load(&path).unwrap(), external);
        let mut fresh = Session::new(Scene::demo()).unwrap();
        assert!(matches!(fresh.save(&path), Err(SceneError::Conflict)));
        assert!(fresh.path().is_none());
        #[cfg(unix)]
        {
            let alias = dir.path().join("alias.json");
            std::os::unix::fs::symlink(&path, &alias).unwrap();
            assert!(matches!(Session::open(alias), Err(SceneError::Invalid(_))));
        }
    }

    // Unique risk: preview ticks/editing leak into each other's snapshots.
    #[test]
    fn static_preview_isolated_until_reset_and_tick_overflow_atomic() {
        let mut scene = Scene::demo();
        let mut preview = Preview::new(&scene);
        scene.objects[0].transform.translation[0] = 9.0;
        assert_eq!(preview.step().unwrap(), 1);
        assert_ne!(preview.scene, scene);
        assert_ne!(preview.scene.fingerprint(), scene.fingerprint());
        preview.reset(&scene);
        assert_eq!(preview.scene, scene);
        assert_eq!(preview.tick, 0);
        assert_eq!(preview.scene.fingerprint(), scene.fingerprint());
        preview.tick = u64::MAX;
        let before = preview.clone();
        assert!(matches!(preview.step(), Err(SceneError::TickOverflow)));
        assert_eq!(preview, before);
    }
}
