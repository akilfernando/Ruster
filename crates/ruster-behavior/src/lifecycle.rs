//! State-only initialization and Application transactions on the existing VM owner.
use super::*;
use ruster_scene::{command_argument_payload_bytes, BindingTarget};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct CommandReceipt {
    pub binding_id: String,
    pub command_id: String,
    pub schema_hash: String,
    pub input_sha256: String,
    #[serde(serialize_with = "crate::spatial_host::counter_string")]
    pub previous_commit_revision: u64,
    #[serde(serialize_with = "crate::spatial_host::counter_string")]
    pub commit_revision: u64,
    #[serde(serialize_with = "crate::spatial_host::counter_string")]
    pub tick: u64,
}

/// An action intent bound to one captured HUD row and command catalog.
/// The enclosing runtime owner supplies run identity. Arguments are never caller data.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudActivation {
    pub activation_id: u64,
    pub button_id: String,
    pub definition_hash: String,
    pub command_schema_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct HudActivationReceipt {
    #[serde(serialize_with = "crate::spatial_host::counter_string")]
    pub activation_id: u64,
    pub button_id: String,
    pub command: CommandReceipt,
}

pub(super) enum Invocation<'a> {
    Initialize,
    Command {
        name: &'a str,
        arguments: &'a StateFields,
    },
}
impl Invocation<'_> {
    fn phase(&self) -> Phase {
        match self {
            Self::Initialize => Phase::Initialize,
            Self::Command { .. } => Phase::Command,
        }
    }
}

pub(super) fn catalog_identity(
    authoring: &Authoring,
) -> Result<Option<(String, String)>, Diagnostic> {
    authoring
        .bindings
        .iter()
        .find(|(_, binding)| binding.target == BindingTarget::Application)
        .map(|(id, binding)| {
            let bytes = serde_json::to_vec(&binding.commands).map_err(|error| {
                Diagnostic::plain(
                    DiagnosticCode::InvalidAuthoring,
                    Phase::Setup,
                    &error.to_string(),
                )
            })?;
            Ok((id.clone(), format!("{:x}", Sha256::digest(bytes))))
        })
        .transpose()
}

fn command_error(code: DiagnosticCode, message: &str) -> Diagnostic {
    Diagnostic::plain(code, Phase::Command, message)
}

impl Engine {
    /// Execute an immutable HUD action against the owner's current committed state.
    /// Admission consumes its monotonically increasing identity before VM evaluation,
    /// including failed evaluations. Cancelled queued identities may leave gaps.
    pub fn activate_hud(
        &mut self,
        runtime: &mut Runtime,
        activation: &HudActivation,
    ) -> Result<HudActivationReceipt, Diagnostic> {
        let invalid = |message: &str| command_error(DiagnosticCode::InvalidCommand, message);
        if activation.activation_id == 0 || activation.activation_id <= runtime.last_hud_activation
        {
            return Err(invalid(
                "HUD activation identity was consumed or is invalid",
            ));
        }
        let authoring = Arc::clone(runtime.authoring_snapshot());
        if !authoring.hud.enabled {
            return Err(invalid("HUD is disabled in this run"));
        }
        let panel = authoring
            .hud
            .panel_for_label(&activation.button_id)
            .ok_or_else(|| invalid("HUD button is unavailable in this run"))?;
        let label = panel
            .elements
            .iter()
            .find(|label| label.id == activation.button_id)
            .ok_or_else(|| invalid("HUD button is unavailable in this run"))?;
        let identity = ruster_scene::hud::hud_button_identity(panel, label)
            .map_err(|error| invalid(&error.to_string()))?;
        if identity != activation.definition_hash {
            return Err(invalid("HUD button definition changed"));
        }
        let command = label
            .source
            .command_binding()
            .ok_or_else(|| invalid("HUD element is not a button"))?;
        ruster_scene::hud::validate_hud_button_command(&authoring, command)
            .map_err(|error| invalid(&error.to_string()))?;
        let binding = &authoring.bindings[&command.binding_id];
        if authoring.sources[&binding.source_id].language_id != LANGUAGE_ID {
            return Err(invalid("HUD command backend is unsupported"));
        }
        if runtime.command_schema_hash(&command.binding_id)
            != Some(activation.command_schema_hash.as_str())
        {
            return Err(invalid("HUD command schema changed"));
        }
        for condition in [panel.visible_when.as_ref(), label.enabled_when.as_ref()]
            .into_iter()
            .flatten()
        {
            let eligibility = ruster_scene::hud::resolve_bool_condition(
                &authoring,
                Some(condition),
                |binding, field| runtime.state().binding(binding)?.get(field),
            );
            if !eligibility.permitted {
                return Err(invalid(
                    "HUD button is unavailable in the current committed state",
                ));
            }
        }
        runtime.last_hud_activation = activation.activation_id;
        let revision = runtime.commit_revision();
        let command = self.dispatch_command(
            runtime,
            &command.binding_id,
            revision,
            &command.command_id,
            &command.arguments,
        )?;
        Ok(HudActivationReceipt {
            activation_id: activation.activation_id,
            button_id: activation.button_id.clone(),
            command,
        })
    }

    /// Execute one Application state transaction. The caller owns run/pause authority.
    /// Validation precedes VM work. Success consumes the expected revision, including no-ops.
    pub fn dispatch_command(
        &mut self,
        runtime: &mut Runtime,
        binding_id: &str,
        expected_commit_revision: u64,
        command_id: &str,
        arguments: &StateFields,
    ) -> Result<CommandReceipt, Diagnostic> {
        if runtime.commit_revision != expected_commit_revision {
            return Err(command_error(
                DiagnosticCode::StaleCommit,
                "runtime commit revision changed",
            ));
        }
        let revision = runtime.commit_revision.checked_add(1).ok_or_else(|| {
            command_error(
                DiagnosticCode::CommitExhausted,
                "commit revision exhausted; Reset before continuing",
            )
        })?;
        let authoring = runtime.authoring.clone();
        let binding = authoring.bindings.get(binding_id).ok_or_else(|| {
            command_error(
                DiagnosticCode::InvalidCommand,
                "Application binding is unavailable",
            )
        })?;
        let source = authoring.sources.get(&binding.source_id);
        let result = (|| {
            if binding.target != BindingTarget::Application
                || !binding.enabled
                || source.is_none_or(|source| {
                    source.language_id != LANGUAGE_ID
                        || source.host_api_version != LIFECYCLE_HOST_API_VERSION
                })
            {
                return Err(command_error(
                    DiagnosticCode::InvalidCommand,
                    "command requires an enabled API5 Application binding",
                ));
            }
            let definition = binding.commands.get(command_id).ok_or_else(|| {
                command_error(
                    DiagnosticCode::InvalidCommand,
                    "command is not declared in this run",
                )
            })?;
            // Check supplied values before cloning the bounded defaults. The complete
            // normalized map is validated again after replacing supplied fields.
            command_argument_payload_bytes(arguments).map_err(|error| {
                command_error(DiagnosticCode::InvalidCommand, &error.to_string())
            })?;
            for (name, value) in arguments {
                let default = definition.arguments.get(name).ok_or_else(|| {
                    command_error(DiagnosticCode::InvalidCommand, "unknown command argument")
                })?;
                if std::mem::discriminant(default) != std::mem::discriminant(value) {
                    return Err(command_error(
                        DiagnosticCode::InvalidCommand,
                        "command argument changed type",
                    ));
                }
            }
            let mut normalized = definition.arguments.clone();
            normalized.extend(
                arguments
                    .iter()
                    .map(|(name, value)| (name.clone(), value.clone())),
            );
            command_argument_payload_bytes(&normalized).map_err(|error| {
                command_error(DiagnosticCode::InvalidCommand, &error.to_string())
            })?;
            let encoded = serde_json::to_vec(&normalized).map_err(|error| {
                command_error(DiagnosticCode::InvalidCommand, &error.to_string())
            })?;
            let schema_hash = runtime
                .command_schema_hash(binding_id)
                .ok_or_else(|| {
                    command_error(
                        DiagnosticCode::InvalidCommand,
                        "command schema is unavailable",
                    )
                })?
                .to_owned();
            self.begin()?;
            self.begin_debug(
                runtime.tick,
                runtime.tick,
                runtime.commit_revision,
                revision,
                false,
            );
            let patch = self.evaluate_lifecycle(
                binding_id,
                binding,
                &authoring,
                &runtime.state,
                Invocation::Command {
                    name: command_id,
                    arguments: &normalized,
                },
            )?;
            let mut state = runtime.state.clone();
            RuntimeState::apply(&mut state, binding_id, patch)?;
            state.validate_total()?;
            self.check_cancelled()?;
            let receipt = CommandReceipt {
                binding_id: binding_id.into(),
                command_id: command_id.into(),
                schema_hash,
                input_sha256: format!("{:x}", Sha256::digest(encoded)),
                previous_commit_revision: runtime.commit_revision,
                commit_revision: revision,
                tick: runtime.tick,
            };
            runtime.state = state;
            runtime.commit_revision = revision;
            Ok(receipt)
        })();
        // Borrowing the captured definitions for evaluation ends before publication.
        // Construct context from the immutable captured owner on failure only.
        result.map_err(|error| error.context(binding_id, binding, source))
    }

    pub(super) fn evaluate_lifecycle(
        &mut self,
        id: &str,
        binding: &BehaviorBinding,
        authoring: &Authoring,
        snapshot: &RuntimeState,
        invocation: Invocation<'_>,
    ) -> Result<StateFields, Diagnostic> {
        let source = authoring.sources.get(&binding.source_id);
        let result = (|| {
            let source = source.ok_or_else(|| {
                command_error(DiagnosticCode::MissingSource, "binding source is missing")
            })?;
            let main = self.compile(&binding.source_id, source)?;
            let phase = invocation.phase();
            if let Some(debugger) = self.debugger.borrow_mut().as_mut() {
                debugger.context(debug::Context {
                    source_id: binding.source_id.clone(),
                    source_hash: source_hash(&source.text),
                    binding_id: id.into(),
                    object_id: binding.object_id().map(Into::into),
                    phase,
                });
            }
            self.binding_count.set(0);
            self.check_cancelled()?;
            let result = self.lua.scope(|scope| {
                let environment = base_environment(&self.lua)?;
                let host = self.lua.create_table()?;
                install_state_host(scope, &host, snapshot, id)?;
                main.set_environment(environment)?;
                self.debug_phase(Phase::Module);
                let module: Table = main.call(())?;
                // Reset validates the declared handler without invoking it.
                if !binding.commands.is_empty() {
                    let _: Function = module.raw_get("command")?;
                }
                let props = self.lua.create_table()?;
                for (name, value) in &binding.properties {
                    props.raw_set(name.as_str(), property_value(&self.lua, value)?)?;
                }
                self.debug_phase(phase);
                let output: Value = match invocation {
                    Invocation::Initialize => {
                        let Some(initialize) = module.raw_get::<Option<Function>>("initialize")? else {
                            return Ok(StateFields::new());
                        };
                        initialize.call((props, host))?
                    }
                    Invocation::Command { name, arguments } => {
                        let command: Function = module.raw_get("command")?;
                        let args = self.lua.create_table()?;
                        for (name, value) in arguments {
                            args.raw_set(name.as_str(), state::lua_value(&self.lua, value)?)?;
                        }
                        command.call((name, args, props, host))?
                    }
                };
                let Value::Table(output) = output else {
                    return Err(mlua::Error::runtime("lifecycle callback must return a state envelope"));
                };
                state::plain_table(&output)?;
                for entry in output.pairs::<Value, Value>() {
                    let (key, _) = entry?;
                    if !matches!(key, Value::String(name) if name.as_bytes().as_ref() == b"state") {
                        return Err(mlua::Error::runtime("lifecycle output only permits state"));
                    }
                }
                match output.raw_get::<Value>("state")? {
                    Value::Nil => Ok(StateFields::new()),
                    Value::Table(patch) => state::read_patch(patch,
                        snapshot.binding(id).ok_or_else(|| mlua::Error::runtime("state owner is unavailable"))?),
                    _ => Err(mlua::Error::runtime("lifecycle state patch must be a table")),
                }
            }).map_err(|error| self.error(error, phase, DiagnosticCode::Runtime));
            // Expire scoped captures and clear the cached chunk on every result.
            let cleanup = self
                .lua
                .create_table()
                .and_then(|empty| main.set_environment(empty));
            if let Err(error) = cleanup {
                self.cache.remove(&binding.source_id);
                drop(main);
                return match result {
                    Err(original) => Err(original),
                    Ok(_) => Err(self.error(error, Phase::Module, DiagnosticCode::Runtime)),
                };
            }
            result
        })();
        if let Some(debugger) = self.debugger.borrow_mut().as_mut() {
            debugger.clear();
        }
        if result.is_err() {
            let _ = self.lua.gc_collect();
        }
        result.map_err(|error| error.context(id, binding, source))
    }
}
