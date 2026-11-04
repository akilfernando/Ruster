//! Language-neutral authored behavior data. This module never compiles or executes source.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{validate_id, validate_name, Scene, SceneError};

pub const MAX_SOURCES: usize = 256;
pub const MAX_BINDINGS: usize = 256;
pub const MAX_SOURCE_BYTES: usize = 16 * 1024;
pub const MAX_PROPERTIES: usize = 64;
pub const MAX_PROPERTY_TEXT_BYTES: usize = 1024;
pub const AUTHORING_PAGE_LIMIT: usize = 16;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorSource {
    pub label: Option<String>,
    /// An opaque backend identifier; unknown languages remain editable and saveable.
    pub language_id: String,
    pub host_api_version: u32,
    /// Embedded text is authoritative. No filesystem path or executable authority is stored.
    pub text: String,
}
impl BehaviorSource {
    pub fn validate(&self) -> Result<(), SceneError> {
        validate_name(self.label.as_deref())?;
        validate_id(&self.language_id)?;
        if self.host_api_version == 0 || self.text.len() > MAX_SOURCE_BYTES {
            return Err(SceneError::Invalid(
                "source requires a positive host API version and at most16KiB UTF-8 text".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum PropertyValue {
    Number(f64),
    Bool(bool),
    Text(String),
    Vec3([f64; 3]),
    ObjectRef(Option<String>),
    AssetRef(Option<String>),
}
impl PropertyValue {
    pub fn validate(&self) -> Result<(), SceneError> {
        match self {
            Self::Number(n) if !n.is_finite() => {
                return Err(SceneError::Invalid("property number must be finite".into()))
            }
            Self::Vec3(v) if !v.iter().all(|n| n.is_finite()) => {
                return Err(SceneError::Invalid("property vector must be finite".into()))
            }
            Self::Text(s) if s.len() > MAX_PROPERTY_TEXT_BYTES => {
                return Err(SceneError::Invalid(
                    "property text exceeds1024 UTF-8 bytes".into(),
                ))
            }
            Self::ObjectRef(Some(id)) | Self::AssetRef(Some(id)) => validate_id(id)?,
            _ => {}
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BehaviorBinding {
    pub object_id: String,
    pub source_id: String,
    pub enabled: bool,
    /// Values are retained even when a source no longer declares them, permitting repair.
    pub properties: BTreeMap<String, PropertyValue>,
}
impl BehaviorBinding {
    pub fn validate(&self) -> Result<(), SceneError> {
        validate_id(&self.object_id)?;
        validate_id(&self.source_id)?;
        if self.properties.len() > MAX_PROPERTIES {
            return Err(SceneError::Invalid("binding exceeds64 properties".into()));
        }
        for (name, value) in &self.properties {
            validate_id(name)?;
            value.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authoring {
    /// Map keys are stable source IDs, separate from display labels and object owners.
    pub sources: BTreeMap<String, BehaviorSource>,
    /// Map keys are stable binding IDs; currently at most one binding owns each object.
    pub bindings: BTreeMap<String, BehaviorBinding>,
}
impl Authoring {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.bindings.is_empty()
    }
    pub fn validate(&self, scene: &Scene) -> Result<(), SceneError> {
        if self.sources.len() > MAX_SOURCES || self.bindings.len() > MAX_BINDINGS {
            return Err(SceneError::Invalid(
                "authoring exceeds256 sources or bindings".into(),
            ));
        }
        for (id, source) in &self.sources {
            validate_id(id)?;
            source.validate()?;
        }
        let mut owners = HashSet::new();
        for (id, binding) in &self.bindings {
            validate_id(id)?;
            binding.validate()?;
            if !scene.objects.iter().any(|o| o.id == binding.object_id) {
                return Err(SceneError::Invalid(format!(
                    "binding {id} owner {} does not exist",
                    binding.object_id
                )));
            }
            if !owners.insert(&binding.object_id) {
                return Err(SceneError::Invalid(
                    "only one behavior binding per object is currently supported".into(),
                ));
            }
        }
        Ok(())
    }
    pub(crate) fn object_ids(&self) -> impl Iterator<Item = &str> {
        self.bindings.values().flat_map(|b| {
            std::iter::once(b.object_id.as_str()).chain(b.properties.values().filter_map(
                |v| match v {
                    PropertyValue::ObjectRef(Some(id)) => Some(id.as_str()),
                    _ => None,
                },
            ))
        })
    }
    pub(crate) fn has_asset_references(&self) -> bool {
        self.bindings.values().any(|b| {
            b.properties
                .values()
                .any(|v| matches!(v, PropertyValue::AssetRef(Some(_))))
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuthoringPage<T> {
    pub items: Vec<T>,
    pub total: usize,
    pub next_offset: Option<usize>,
}
pub(crate) fn page<T>(
    items: impl ExactSizeIterator<Item = T>,
    offset: usize,
    limit: usize,
) -> AuthoringPage<T> {
    let total = items.len();
    let items: Vec<_> = items
        .skip(offset)
        .take(limit.clamp(1, AUTHORING_PAGE_LIMIT))
        .collect();
    let end = offset.saturating_add(items.len());
    AuthoringPage {
        items,
        total,
        next_offset: (end < total).then_some(end),
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceSummary {
    pub id: String,
    pub label: Option<String>,
    pub language_id: String,
    pub host_api_version: u32,
    pub text_bytes: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BindingSummary {
    pub id: String,
    pub object_id: String,
    pub source_id: String,
    pub enabled: bool,
    pub property_count: usize,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoringDiagnosticCode {
    MissingSource,
    MissingObjectReference,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AuthoringDiagnostic {
    pub code: AuthoringDiagnosticCode,
    pub binding_id: String,
    pub property: Option<String>,
    pub reference_id: String,
}
