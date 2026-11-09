//! Language-neutral authored behavior data. This module never compiles or executes source.
use std::{
    collections::{BTreeMap, HashSet},
    io::{self, Write},
};

use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};

use crate::{validate_id, validate_name, Scene, SceneError};

pub const MAX_SOURCES: usize = 256;
pub const MAX_BINDINGS: usize = 256;
pub const MAX_SOURCE_BYTES: usize = 16 * 1024;
pub const MAX_PROPERTIES: usize = 64;
pub const MAX_PROPERTY_TEXT_BYTES: usize = 1024;
pub const AUTHORING_PAGE_LIMIT: usize = 16;
pub const MAX_STATE_FIELDS: usize = 64;
pub const MAX_BINDING_STATE_BYTES: usize = 16 * 1024;
pub const MAX_RUNTIME_STATE_BYTES: usize = 1024 * 1024;
pub const MAX_COMMANDS: usize = 16;
pub const MAX_COMMAND_ARGUMENTS: usize = 16;
pub const MAX_COMMAND_ARGUMENT_BYTES: usize = MAX_BINDING_STATE_BYTES;

pub type StateFields = BTreeMap<String, StateValue>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum StateValue {
    Number(f64),
    Bool(bool),
    Text(String),
    Vec3([f64; 3]),
}

impl StateValue {
    pub fn validate(&self) -> Result<(), SceneError> {
        match self {
            Self::Number(value) if !value.is_finite() => {
                Err(SceneError::Invalid("state number must be finite".into()))
            }
            Self::Vec3(value) if !value.iter().all(|component| component.is_finite()) => {
                Err(SceneError::Invalid("state vector must be finite".into()))
            }
            Self::Text(value) if value.len() > MAX_PROPERTY_TEXT_BYTES => Err(SceneError::Invalid(
                "state text exceeds 1024 UTF-8 bytes".into(),
            )),
            _ => Ok(()),
        }
    }
}

/// Validate a bounded field map and count its canonical typed JSON bytes.
/// Empty maps encode as two bytes. Owners omit them from aggregate accounting.
pub fn state_payload_bytes(fields: &StateFields) -> Result<usize, SceneError> {
    if fields.len() > MAX_STATE_FIELDS {
        return Err(SceneError::Invalid("state exceeds 64 fields".into()));
    }
    for (name, value) in fields {
        validate_id(name)?;
        value.validate()?;
    }
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > MAX_BINDING_STATE_BYTES - self.0 {
                return Err(io::Error::other("state payload limit exceeded"));
            }
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, fields).map_err(|_| {
        SceneError::Invalid("binding state exceeds 16 KiB of canonical typed JSON".into())
    })?;
    Ok(counter.0)
}

/// Validate a complete command argument map, including canonical typed JSON size.
pub fn command_argument_payload_bytes(arguments: &StateFields) -> Result<usize, SceneError> {
    if arguments.len() > MAX_COMMAND_ARGUMENTS {
        return Err(SceneError::Invalid("command exceeds 16 arguments".into()));
    }
    state_payload_bytes(arguments)
        .map_err(|error| SceneError::Invalid(format!("invalid command arguments ({error})")))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDefinition {
    pub arguments: StateFields,
}
impl CommandDefinition {
    pub fn validate(&self) -> Result<(), SceneError> {
        command_argument_payload_bytes(&self.arguments).map(|_| ())
    }
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingTarget {
    Object(String),
    Application,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BehaviorBinding {
    pub target: BindingTarget,
    pub source_id: String,
    pub enabled: bool,
    /// Values are retained even when a source no longer declares them, permitting repair.
    pub properties: BTreeMap<String, PropertyValue>,
    pub state_defaults: StateFields,
    pub commands: BTreeMap<String, CommandDefinition>,
}
impl BehaviorBinding {
    pub fn object_id(&self) -> Option<&str> {
        match &self.target {
            BindingTarget::Object(id) => Some(id),
            BindingTarget::Application => None,
        }
    }
    pub fn validate(&self) -> Result<(), SceneError> {
        self.validate_and_state_bytes().map(|_| ())
    }
    fn validate_and_state_bytes(&self) -> Result<usize, SceneError> {
        if let Some(id) = self.object_id() {
            validate_id(id)?;
        }
        validate_id(&self.source_id)?;
        if self.properties.len() > MAX_PROPERTIES {
            return Err(SceneError::Invalid("binding exceeds64 properties".into()));
        }
        for (name, value) in &self.properties {
            validate_id(name)?;
            value.validate()?;
        }
        if self.commands.len() > MAX_COMMANDS {
            return Err(SceneError::Invalid("binding exceeds 16 commands".into()));
        }
        if !self.commands.is_empty() && self.target != BindingTarget::Application {
            return Err(SceneError::Invalid(
                "nonempty commands require an application binding".into(),
            ));
        }
        for (name, definition) in &self.commands {
            validate_id(name)?;
            definition.validate()?;
        }
        state_payload_bytes(&self.state_defaults)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum ApplicationTarget {
    Application {},
}

// Unlike Option<T>, this distinguishes a missing field from explicit null.
#[derive(Default)]
enum Present<T> {
    #[default]
    Missing,
    Value(T),
}
impl<'de, T: Deserialize<'de>> Deserialize<'de> for Present<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::Value)
    }
}

impl<'de> Deserialize<'de> for BehaviorBinding {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            #[serde(default)]
            object_id: Present<String>,
            #[serde(default)]
            target: Present<ApplicationTarget>,
            source_id: String,
            enabled: bool,
            properties: BTreeMap<String, PropertyValue>,
            #[serde(default)]
            state_defaults: StateFields,
            #[serde(default)]
            commands: BTreeMap<String, CommandDefinition>,
        }
        let wire = Wire::deserialize(deserializer)?;
        let target = match (wire.object_id, wire.target) {
            (Present::Value(id), Present::Missing) => BindingTarget::Object(id),
            (Present::Missing, Present::Value(ApplicationTarget::Application {})) => {
                BindingTarget::Application
            }
            _ => {
                return Err(serde::de::Error::custom(
                    "binding requires exactly one object_id or application target",
                ))
            }
        };
        Ok(Self {
            target,
            source_id: wire.source_id,
            enabled: wire.enabled,
            properties: wire.properties,
            state_defaults: wire.state_defaults,
            commands: wire.commands,
        })
    }
}

impl Serialize for BehaviorBinding {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut wire = serializer.serialize_struct(
            "BehaviorBinding",
            4 + usize::from(!self.state_defaults.is_empty())
                + usize::from(!self.commands.is_empty()),
        )?;
        match &self.target {
            BindingTarget::Object(id) => wire.serialize_field("object_id", id)?,
            BindingTarget::Application => {
                wire.serialize_field("target", &ApplicationTarget::Application {})?
            }
        }
        wire.serialize_field("source_id", &self.source_id)?;
        wire.serialize_field("enabled", &self.enabled)?;
        wire.serialize_field("properties", &self.properties)?;
        if !self.state_defaults.is_empty() {
            wire.serialize_field("state_defaults", &self.state_defaults)?;
        }
        if !self.commands.is_empty() {
            wire.serialize_field("commands", &self.commands)?;
        }
        wire.end()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Authoring {
    /// Map keys are stable source IDs, separate from display labels and object owners.
    pub sources: BTreeMap<String, BehaviorSource>,
    /// Map keys are stable binding IDs. Each object and the application have one owner.
    pub bindings: BTreeMap<String, BehaviorBinding>,
    /// Scene-owned keyboard actions; live key state is never authored.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub actions: crate::input::ActionMap,
}
impl Authoring {
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty() && self.bindings.is_empty() && self.actions.is_empty()
    }
    pub fn validate(&self, scene: &Scene) -> Result<(), SceneError> {
        crate::input::validate_action_map(&self.actions)?;
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
        let mut application_owned = false;
        let mut state_bytes = 0_usize;
        for (id, binding) in &self.bindings {
            validate_id(id)?;
            let bytes = binding.validate_and_state_bytes()?;
            match &binding.target {
                BindingTarget::Object(object_id) => {
                    if !scene.objects.iter().any(|object| &object.id == object_id) {
                        return Err(SceneError::Invalid(format!(
                            "binding {id} owner {object_id} does not exist"
                        )));
                    }
                    if !owners.insert(object_id) {
                        return Err(SceneError::Invalid(
                            "only one behavior binding per object is currently supported".into(),
                        ));
                    }
                }
                BindingTarget::Application => {
                    if application_owned {
                        return Err(SceneError::Invalid("only one application behavior binding is supported, including disabled bindings".into()));
                    }
                    application_owned = true;
                }
            }
            if !binding.state_defaults.is_empty() {
                state_bytes = state_bytes
                    .checked_add(bytes)
                    .filter(|bytes| *bytes <= MAX_RUNTIME_STATE_BYTES)
                    .ok_or_else(|| {
                        SceneError::Invalid(
                            "initial state exceeds 1 MiB of canonical typed JSON across bindings"
                                .into(),
                        )
                    })?;
            }
        }
        Ok(())
    }
    pub(crate) fn object_ids(&self) -> impl Iterator<Item = &str> {
        self.bindings.values().flat_map(|b| {
            b.object_id()
                .into_iter()
                .chain(b.properties.values().filter_map(|v| match v {
                    PropertyValue::ObjectRef(Some(id)) => Some(id.as_str()),
                    _ => None,
                }))
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
#[derive(Clone, Debug, PartialEq)]
pub struct BindingSummary {
    pub id: String,
    pub target: BindingTarget,
    pub source_id: String,
    pub enabled: bool,
    pub property_count: usize,
    pub state_field_count: usize,
    pub command_count: usize,
}
impl Serialize for BindingSummary {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut wire = serializer.serialize_struct("BindingSummary", 7)?;
        wire.serialize_field("id", &self.id)?;
        match &self.target {
            BindingTarget::Object(id) => wire.serialize_field("object_id", id)?,
            BindingTarget::Application => {
                wire.serialize_field("target", &ApplicationTarget::Application {})?
            }
        }
        wire.serialize_field("source_id", &self.source_id)?;
        wire.serialize_field("enabled", &self.enabled)?;
        wire.serialize_field("property_count", &self.property_count)?;
        wire.serialize_field("state_field_count", &self.state_field_count)?;
        wire.serialize_field("command_count", &self.command_count)?;
        wire.end()
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CommandSummary {
    pub id: String,
    pub argument_count: usize,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Object;
    use serde_json::json;

    fn payload(bytes: usize) -> StateFields {
        let mut fields: StateFields = (0..16)
            .map(|index| (format!("field-{index:02}"), StateValue::Text(String::new())))
            .collect();
        let mut remaining = bytes - serde_json::to_vec(&fields).unwrap().len();
        for value in fields.values_mut() {
            let length = remaining.min(MAX_PROPERTY_TEXT_BYTES);
            *value = StateValue::Text("x".repeat(length));
            remaining -= length;
        }
        assert_eq!(remaining, 0);
        assert_eq!(serde_json::to_vec(&fields).unwrap().len(), bytes);
        fields
    }

    // Unique boundaries are strict target presence and exact typed-JSON accounting,
    // including escaped UTF-8 data and the aggregate across disabled owners.
    #[test]
    fn binding_target_wire_and_state_budgets_are_strict() {
        let legacy =
            json!({"object_id":"owner","source_id":"source","enabled":true,"properties":{}});
        let object: BehaviorBinding = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(object.object_id(), Some("owner"));
        assert!(object.state_defaults.is_empty());
        assert!(object.commands.is_empty());
        assert_eq!(serde_json::to_value(&object).unwrap(), legacy);
        let app_wire = json!({"target":{"type":"application"},"source_id":"source","enabled":false,"properties":{}});
        let app: BehaviorBinding = serde_json::from_value(app_wire.clone()).unwrap();
        assert_eq!(app.target, BindingTarget::Application);
        assert_eq!(app.object_id(), None);
        assert_eq!(serde_json::to_value(&app).unwrap(), app_wire);
        let mut neither = legacy.clone();
        neither.as_object_mut().unwrap().remove("object_id");
        for invalid in [
            neither,
            json!({"object_id":"owner","target":{"type":"application"},"source_id":"source","enabled":true,"properties":{}}),
            json!({"object_id":null,"source_id":"source","enabled":true,"properties":{}}),
            json!({"target":null,"source_id":"source","enabled":true,"properties":{}}),
            json!({"target":{"type":"object","object_id":"owner"},"source_id":"source","enabled":true,"properties":{}}),
            json!({"target":{"type":"application","extra":true},"source_id":"source","enabled":true,"properties":{}}),
            json!({"object_id":"owner","source_id":"source","enabled":true,"properties":{},"state_defaults":null}),
            json!({"target":{"type":"application"},"source_id":"source","enabled":false,"properties":{},"commands":null}),
            json!({"object_id":"owner","source_id":"source","enabled":true,"properties":{},"unknown":true}),
        ] {
            assert!(serde_json::from_value::<BehaviorBinding>(invalid).is_err());
        }
        assert!(serde_json::from_str::<BehaviorBinding>(r#"{"object_id":"a","object_id":"b","source_id":"source","enabled":true,"properties":{}}"#).is_err());
        assert!(
            serde_json::from_value::<StateValue>(json!({"type":"object_ref","value":"owner"}))
                .is_err()
        );
        for invalid in [
            StateValue::Number(f64::NAN),
            StateValue::Vec3([0.0, f64::INFINITY, 0.0]),
            StateValue::Text("é".repeat(513)),
        ] {
            assert!(invalid.validate().is_err());
        }
        StateValue::Text("é".repeat(512)).validate().unwrap();
        assert_eq!(state_payload_bytes(&StateFields::new()).unwrap(), 2);
        for bytes in [MAX_BINDING_STATE_BYTES - 1, MAX_BINDING_STATE_BYTES] {
            assert_eq!(state_payload_bytes(&payload(bytes)).unwrap(), bytes);
        }
        assert!(state_payload_bytes(&payload(MAX_BINDING_STATE_BYTES + 1)).is_err());
        let escaped: StateFields = (0..3)
            .map(|index| {
                (
                    format!("escaped-{index}"),
                    StateValue::Text("\0".repeat(1024)),
                )
            })
            .collect();
        assert!(
            state_payload_bytes(&escaped).is_err(),
            "JSON escaping participates in the byte cap"
        );
        let too_many = (0..=MAX_STATE_FIELDS)
            .map(|index| (format!("key-{index}"), StateValue::Bool(false)))
            .collect();
        assert!(state_payload_bytes(&too_many).is_err());
        let allowed = (0..MAX_STATE_FIELDS)
            .map(|index| (format!("key-{index}"), StateValue::Bool(false)))
            .collect();
        state_payload_bytes(&allowed).unwrap();

        let empty_command = CommandDefinition {
            arguments: StateFields::new(),
        };
        for invalid in [
            json!({}),
            json!({"arguments":null}),
            json!({"arguments":[]}),
            json!({"arguments":{},"extra":true}),
            json!({"arguments":{"target":{"type":"object_ref","value":"owner"}}}),
        ] {
            assert!(serde_json::from_value::<CommandDefinition>(invalid).is_err());
        }
        let mut catalog = app.clone();
        catalog.commands = (0..MAX_COMMANDS)
            .map(|index| (format!("command-{index}"), empty_command.clone()))
            .collect();
        catalog.validate().unwrap();
        let wire = serde_json::to_value(&catalog).unwrap();
        assert_eq!(wire["commands"]["command-0"], json!({"arguments":{}}));
        assert_eq!(
            serde_json::from_value::<BehaviorBinding>(wire).unwrap(),
            catalog
        );
        let mut object_commands = catalog.clone();
        object_commands.target = BindingTarget::Object("owner".into());
        assert!(
            object_commands.validate().is_err(),
            "disabled objects cannot own commands"
        );
        catalog.commands.insert("crossing".into(), empty_command);
        assert!(catalog.validate().is_err());
        catalog.commands.remove("crossing");
        catalog.commands.insert(
            "bad name".into(),
            CommandDefinition {
                arguments: StateFields::new(),
            },
        );
        catalog.commands.remove("command-0");
        assert!(
            catalog.validate().is_err(),
            "command identifiers are validated independently of count"
        );

        let mut arguments: StateFields = (0..MAX_COMMAND_ARGUMENTS)
            .map(|index| (format!("arg-{index}"), StateValue::Bool(false)))
            .collect();
        command_argument_payload_bytes(&arguments).unwrap();
        arguments.insert("crossing".into(), StateValue::Bool(false));
        assert!(command_argument_payload_bytes(&arguments).is_err());
        assert_eq!(
            command_argument_payload_bytes(&StateFields::new()).unwrap(),
            2
        );
        for bytes in [MAX_COMMAND_ARGUMENT_BYTES - 1, MAX_COMMAND_ARGUMENT_BYTES] {
            assert_eq!(
                command_argument_payload_bytes(&payload(bytes)).unwrap(),
                bytes
            );
        }
        assert!(command_argument_payload_bytes(&payload(MAX_COMMAND_ARGUMENT_BYTES + 1)).is_err());
        assert!(command_argument_payload_bytes(&escaped).is_err());
        for arguments in [
            BTreeMap::from([("bad name".into(), StateValue::Bool(false))]),
            BTreeMap::from([("count".into(), StateValue::Number(f64::NAN))]),
        ] {
            assert!(CommandDefinition { arguments }.validate().is_err());
        }

        let mut scene = Scene::empty("aggregate-state");
        let mut authoring = Authoring::default();
        authoring.bindings.insert("app".into(), app.clone());
        authoring.validate(&scene).unwrap();
        let mut second = app;
        second.enabled = true;
        authoring.bindings.insert("second-app".into(), second);
        assert!(
            authoring.validate(&scene).is_err(),
            "a disabled application binding still owns the target"
        );
        authoring.bindings.remove("second-app");
        authoring.bindings.get_mut("app").unwrap().state_defaults =
            payload(MAX_BINDING_STATE_BYTES);
        for index in 0..63 {
            let id = format!("owner-{index}");
            scene.objects.push(Object::cube(&id, None));
            let mut binding = object.clone();
            binding.target = BindingTarget::Object(id.clone());
            binding.enabled = false;
            binding.state_defaults = payload(MAX_BINDING_STATE_BYTES);
            authoring.bindings.insert(id, binding);
        }
        assert_eq!(
            authoring
                .bindings
                .values()
                .map(|binding| state_payload_bytes(&binding.state_defaults).unwrap())
                .sum::<usize>(),
            MAX_RUNTIME_STATE_BYTES
        );
        authoring.validate(&scene).unwrap();
        scene.objects.push(Object::cube("over-budget", None));
        let mut crossing = object;
        crossing.target = BindingTarget::Object("over-budget".into());
        crossing
            .state_defaults
            .insert("enabled".into(), StateValue::Bool(false));
        authoring.bindings.insert("over-budget".into(), crossing);
        assert!(authoring
            .validate(&scene)
            .unwrap_err()
            .to_string()
            .contains("1 MiB"));
        let mut application = authoring.bindings["app"].clone();
        application.state_defaults.clear();
        let mut empty = authoring.bindings["owner-0"].clone();
        empty.state_defaults.clear();
        authoring.bindings.clear();
        authoring.bindings.insert("app".into(), application);
        scene.objects.clear();
        for index in 0..255 {
            let id = format!("owner-{index}");
            scene.objects.push(Object::cube(&id, None));
            let mut binding = empty.clone();
            binding.target = BindingTarget::Object(id.clone());
            authoring.bindings.insert(id, binding);
        }
        assert_eq!(authoring.bindings.len(), MAX_BINDINGS);
        assert_eq!(scene.objects.len(), MAX_BINDINGS - 1);
        authoring.validate(&scene).unwrap();
        scene.objects.push(Object::cube("extra-owner", None));
        empty.target = BindingTarget::Object("extra-owner".into());
        authoring.bindings.insert("extra-owner".into(), empty);
        assert!(
            authoring.validate(&scene).is_err(),
            "the application binding counts toward the binding cap"
        );
    }
}
