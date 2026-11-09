//! Immutable, run-owned typed state. Cached Lua functions never own these values.
use std::collections::BTreeMap;
use std::sync::Arc;

use mlua::{Lua, Table, Value};
use ruster_scene::{
    state_payload_bytes, Authoring, BindingTarget, StateFields, StateValue,
    MAX_RUNTIME_STATE_BYTES, MAX_STATE_FIELDS,
};
use sha2::{Digest, Sha256};

use crate::{Diagnostic, DiagnosticCode, Phase, STATE_HOST_API_VERSION};

#[derive(Clone, Debug)]
struct Definition {
    source_hash: Option<String>,
    schema_hash: String,
}

#[derive(Clone, Debug)]
struct Owner {
    definition: Arc<Definition>,
    fields: Arc<StateFields>,
    encoded_bytes: usize,
}

/// Last fully committed state. Clones share immutable owner values until a write.
#[derive(Clone, Debug, Default)]
pub struct RuntimeState {
    owners: BTreeMap<String, Arc<Owner>>,
    encoded_bytes: usize,
}

impl RuntimeState {
    pub fn binding(&self, id: &str) -> Option<&StateFields> {
        self.owners.get(id).map(|owner| owner.fields.as_ref())
    }

    pub fn binding_ids(&self) -> impl Iterator<Item = &str> {
        self.owners.keys().map(String::as_str)
    }

    pub fn source_hash(&self, id: &str) -> Option<&str> {
        self.owners.get(id)?.definition.source_hash.as_deref()
    }

    pub fn schema_hash(&self, id: &str) -> Option<&str> {
        Some(&self.owners.get(id)?.definition.schema_hash)
    }

    pub fn encoded_bytes(&self) -> usize {
        self.encoded_bytes
    }

    pub(crate) fn defaults(authoring: &Authoring) -> Result<Arc<Self>, Diagnostic> {
        let mut state = Self::default();
        for (id, binding) in &authoring.bindings {
            let source = authoring.sources.get(&binding.source_id);
            if binding.state_defaults.is_empty()
                && binding.target != BindingTarget::Application
                && source.is_none_or(|source| source.host_api_version != STATE_HOST_API_VERSION)
            {
                continue;
            }
            let encoded = serde_json::to_vec(&binding.state_defaults).map_err(setup_error)?;
            let encoded_bytes = payload_bytes(&binding.state_defaults).map_err(setup_error)?;
            state.encoded_bytes = state
                .encoded_bytes
                .checked_add(encoded_bytes)
                .filter(|size| *size <= MAX_RUNTIME_STATE_BYTES)
                .ok_or_else(|| setup_error("runtime state exceeds 1 MiB encoded payload"))?;
            state.owners.insert(
                id.clone(),
                Arc::new(Owner {
                    definition: Arc::new(Definition {
                        source_hash: source.map(|source| crate::source_hash(&source.text)),
                        schema_hash: format!("{:x}", Sha256::digest(&encoded)),
                    }),
                    fields: Arc::new(binding.state_defaults.clone()),
                    encoded_bytes,
                }),
            );
        }
        Ok(Arc::new(state))
    }

    pub(crate) fn validate_total(&self) -> Result<(), Diagnostic> {
        if self.encoded_bytes > MAX_RUNTIME_STATE_BYTES {
            return Err(output_error("runtime state exceeds 1 MiB encoded payload"));
        }
        Ok(())
    }

    pub(crate) fn apply(
        state: &mut Arc<Self>,
        binding_id: &str,
        patch: StateFields,
    ) -> Result<(), Diagnostic> {
        if patch.is_empty() {
            return Ok(());
        }
        let owner = state
            .owners
            .get(binding_id)
            .ok_or_else(|| output_error("state owner is unavailable; Reset the runtime"))?;
        if patch
            .iter()
            .all(|(name, value)| owner.fields.get(name) == Some(value))
        {
            return Ok(());
        }
        let mut fields = owner.fields.as_ref().clone();
        for (name, value) in patch {
            let previous = fields
                .get(&name)
                .ok_or_else(|| output_error(format!("unknown state field '{name}'")))?;
            if std::mem::discriminant(previous) != std::mem::discriminant(&value) {
                return Err(output_error(format!("state field '{name}' changed type")));
            }
            fields.insert(name, value);
        }
        let encoded_bytes = payload_bytes(&fields).map_err(output_error)?;
        let total = state
            .encoded_bytes
            .checked_sub(owner.encoded_bytes)
            .and_then(|size| size.checked_add(encoded_bytes))
            .ok_or_else(|| output_error("runtime state accounting overflow"))?;
        let replacement = Arc::new(Owner {
            definition: owner.definition.clone(),
            fields: Arc::new(fields),
            encoded_bytes,
        });
        let candidate = Arc::make_mut(state);
        candidate.owners.insert(binding_id.into(), replacement);
        candidate.encoded_bytes = total;
        Ok(())
    }
}

fn payload_bytes(fields: &StateFields) -> Result<usize, ruster_scene::SceneError> {
    if fields.is_empty() {
        Ok(0)
    } else {
        state_payload_bytes(fields)
    }
}

fn setup_error(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::plain(
        DiagnosticCode::InvalidAuthoring,
        Phase::Setup,
        &error.to_string(),
    )
}

fn output_error(error: impl std::fmt::Display) -> Diagnostic {
    Diagnostic::plain(
        DiagnosticCode::InvalidOutput,
        Phase::Output,
        &error.to_string(),
    )
}

pub(crate) fn lua_value(lua: &Lua, value: &StateValue) -> mlua::Result<Value> {
    Ok(match value {
        StateValue::Number(value) => Value::Number(*value),
        StateValue::Bool(value) => Value::Boolean(*value),
        StateValue::Text(value) => Value::String(lua.create_string(value)?),
        StateValue::Vec3(value) => Value::Table(lua.create_sequence_from(*value)?),
    })
}

fn number(value: Value) -> mlua::Result<f64> {
    let result = match value {
        Value::Number(value) => value,
        Value::Integer(value)
            if (-9_007_199_254_740_992..=9_007_199_254_740_992).contains(&value) =>
        {
            value as f64
        }
        Value::Integer(_) => {
            return Err(mlua::Error::runtime(
                "state integer exceeds exact binary64 range ±2^53",
            ));
        }
        _ => return Err(mlua::Error::runtime("state value requires a number")),
    };
    if !result.is_finite() {
        return Err(mlua::Error::runtime("state numbers must be finite"));
    }
    Ok(result)
}

fn read_value(value: Value, schema: &StateValue) -> mlua::Result<StateValue> {
    let result = match (schema, value) {
        (StateValue::Number(_), value) => StateValue::Number(number(value)?),
        (StateValue::Bool(_), Value::Boolean(value)) => StateValue::Bool(value),
        (StateValue::Text(_), Value::String(value)) => {
            if value.as_bytes().len() > ruster_scene::MAX_PROPERTY_TEXT_BYTES {
                return Err(mlua::Error::runtime("state text exceeds 1024 UTF-8 bytes"));
            }
            StateValue::Text(value.to_str()?.to_string())
        }
        (StateValue::Vec3(_), Value::Table(table)) => {
            plain_table(&table)?;
            let mut values = [0.0; 3];
            let mut count = 0;
            for entry in table.pairs::<Value, Value>() {
                let (key, value) = entry?;
                let Value::Integer(index @ 1..=3) = key else {
                    return Err(mlua::Error::runtime(
                        "state vector requires only indices 1..3",
                    ));
                };
                values[index as usize - 1] = number(value)?;
                count += 1;
            }
            if count != 3 {
                return Err(mlua::Error::runtime(
                    "state vector requires exactly three components",
                ));
            }
            StateValue::Vec3(values)
        }
        _ => {
            return Err(mlua::Error::runtime(
                "state field changed its declared type",
            ))
        }
    };
    result
        .validate()
        .map_err(|error| mlua::Error::runtime(error.to_string()))?;
    Ok(result)
}

pub(crate) fn plain_table(table: &Table) -> mlua::Result<()> {
    if table.metatable().is_some() {
        return Err(mlua::Error::runtime("state output must use plain tables"));
    }
    Ok(())
}

pub(crate) fn read_patch(table: Table, schema: &StateFields) -> mlua::Result<StateFields> {
    plain_table(&table)?;
    let mut patch = StateFields::new();
    for entry in table.pairs::<Value, Value>() {
        if patch.len() >= MAX_STATE_FIELDS {
            return Err(mlua::Error::runtime("state patch exceeds 64 fields"));
        }
        let (key, value) = entry?;
        let Value::String(name) = key else {
            return Err(mlua::Error::runtime("state field names must be strings"));
        };
        if name.as_bytes().len() > 128 {
            return Err(mlua::Error::runtime("state field name exceeds 128 bytes"));
        }
        let name = name.to_str()?;
        let declared = schema
            .get(name.as_ref())
            .ok_or_else(|| mlua::Error::runtime(format!("unknown state field '{name}'")))?;
        patch.insert(name.to_string(), read_value(value, declared)?);
    }
    Ok(patch)
}
