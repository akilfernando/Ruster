//! Bounded, read-only HUD resolution shared by native and headless consumers.
use std::fmt::{self, Write};

use serde::Serialize;

use super::{
    Hud, HudCommandBinding, HudLabel, HudSource, MAX_HUD_ELEMENT_TEXT_BYTES, MAX_HUD_LABELS,
    MAX_HUD_RESOLVED_LABEL_BYTES,
};
use crate::{Authoring, BindingTarget, StateValue};

const UNAVAILABLE: &str = "<Unavailable>";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudDiagnostic {
    MissingBinding,
    DisabledBinding,
    MissingSource,
    MissingField,
    TypeChanged,
    UnavailableState,
    OutputOverflow,
    NonApplicationBinding,
    UnsupportedCommandApi,
    MissingCommand,
    CommandArgumentsChanged,
}

impl HudDiagnostic {
    pub fn message(self) -> &'static str {
        match self {
            Self::MissingBinding => "The referenced behavior is missing. Choose a behavior.",
            Self::DisabledBinding => "The referenced behavior is disabled.",
            Self::MissingSource => "The referenced behavior source is missing.",
            Self::MissingField => "The referenced state field is missing.",
            Self::TypeChanged => "The state field type changed. Update this HUD reference.",
            Self::UnavailableState => "Initialize the runtime to read this state value.",
            Self::OutputOverflow => "This value exceeds the HUD text limit.",
            Self::NonApplicationBinding => "Choose an Application behavior for this button.",
            Self::UnsupportedCommandApi => "This button requires an API5 Application behavior.",
            Self::MissingCommand => "The command is missing. Choose a command in the HUD editor.",
            Self::CommandArgumentsChanged => {
                "The command arguments changed. Update this button's constants."
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResolvedHudLabel {
    pub id: String,
    pub text: String,
    pub diagnostic: Option<HudDiagnostic>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ResolvedHud {
    pub labels: Vec<ResolvedHudLabel>,
}

#[derive(Default)]
pub struct HudCache {
    entries: Vec<CachedLabel>,
    resolved: ResolvedHud,
    formatted_labels: usize,
}

struct CachedLabel {
    definition: Option<HudLabel>,
    value: Option<StateValue>,
    diagnostic: Option<HudDiagnostic>,
}

impl HudCache {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.resolved.labels.clear();
        self.formatted_labels = 0;
    }

    /// Number of labels reformatted by the last prepare, excluding cache hits.
    pub fn formatted_labels(&self) -> usize {
        self.formatted_labels
    }

    pub fn resolved(&self) -> &ResolvedHud {
        &self.resolved
    }

    /// Resolve only authored references. Lookup borrows the same immutable state
    /// frame as the consumer's scene. This function never evaluates application code.
    /// Authored documents are validated by their owner before presentation.
    pub fn prepare<'a, 's>(
        &'a mut self,
        hud: &Hud,
        authoring: &Authoring,
        mut lookup: impl FnMut(&str, &str) -> Option<&'s StateValue>,
    ) -> &'a ResolvedHud {
        self.formatted_labels = 0;
        if !hud.enabled || hud.elements.is_empty() {
            self.clear();
            return &self.resolved;
        }
        let count = hud.elements.len().min(MAX_HUD_LABELS);
        self.entries.truncate(count);
        self.resolved.labels.truncate(count);
        for (index, label) in hud.elements.iter().take(count).enumerate() {
            let (value, mut diagnostic) = reference(label, authoring, &mut lookup);
            let bounded_definition = authored_text_bytes(label) <= MAX_HUD_ELEMENT_TEXT_BYTES;
            let bounded_value = value.is_none_or(|value| match value {
                StateValue::Text(text) => text.len() <= crate::MAX_PROPERTY_TEXT_BYTES,
                _ => true,
            });
            if !bounded_definition || !bounded_value {
                diagnostic = Some(HudDiagnostic::OutputOverflow);
            }
            let cached = self.entries.get(index);
            if bounded_definition
                && bounded_value
                && cached.is_some_and(|cached| {
                    cached.definition.as_ref() == Some(label)
                        && cached.diagnostic == diagnostic
                        && same_value(cached.value.as_ref(), value)
                })
            {
                continue;
            }
            let text = if diagnostic == Some(HudDiagnostic::OutputOverflow)
                || (diagnostic.is_some() && !matches!(&label.source, HudSource::Button { .. }))
            {
                UNAVAILABLE.to_owned()
            } else {
                match format_label(label, value) {
                    Ok(text) => text,
                    Err(_) => {
                        diagnostic = Some(HudDiagnostic::OutputOverflow);
                        UNAVAILABLE.to_owned()
                    }
                }
            };
            let entry = CachedLabel {
                definition: bounded_definition.then(|| label.clone()),
                value: if bounded_value { value.cloned() } else { None },
                diagnostic,
            };
            let resolved = ResolvedHudLabel {
                id: label.id.clone(),
                text,
                diagnostic,
            };
            if index < self.entries.len() {
                self.entries[index] = entry;
                self.resolved.labels[index] = resolved;
            } else {
                self.entries.push(entry);
                self.resolved.labels.push(resolved);
            }
            self.formatted_labels += 1;
        }
        &self.resolved
    }
}

pub fn resolve_hud<'s>(
    hud: &Hud,
    authoring: &Authoring,
    lookup: impl FnMut(&str, &str) -> Option<&'s StateValue>,
) -> ResolvedHud {
    let mut cache = HudCache::default();
    cache.prepare(hud, authoring, lookup);
    cache.resolved
}

fn authored_text_bytes(label: &HudLabel) -> usize {
    match &label.source {
        HudSource::Static { text } => text.len(),
        HudSource::Button { caption, .. } => caption.len(),
        HudSource::State { prefix, suffix, .. } => prefix.len().saturating_add(suffix.len()),
    }
}

fn reference<'s>(
    label: &HudLabel,
    authoring: &Authoring,
    lookup: &mut impl FnMut(&str, &str) -> Option<&'s StateValue>,
) -> (Option<&'s StateValue>, Option<HudDiagnostic>) {
    if let HudSource::Button { command, .. } = &label.source {
        return (None, hud_button_command_diagnostic(authoring, command));
    }
    let HudSource::State {
        binding_id,
        field,
        value_type,
        ..
    } = &label.source
    else {
        return (None, None);
    };
    let Some(binding) = authoring.bindings.get(binding_id) else {
        return (None, Some(HudDiagnostic::MissingBinding));
    };
    if !binding.enabled {
        return (None, Some(HudDiagnostic::DisabledBinding));
    }
    if !authoring.sources.contains_key(&binding.source_id) {
        return (None, Some(HudDiagnostic::MissingSource));
    }
    let Some(default) = binding.state_defaults.get(field) else {
        return (None, Some(HudDiagnostic::MissingField));
    };
    if !value_type.matches(default) {
        return (None, Some(HudDiagnostic::TypeChanged));
    }
    let Some(value) = lookup(binding_id, field) else {
        return (None, Some(HudDiagnostic::UnavailableState));
    };
    if !value_type.matches(value) {
        return (None, Some(HudDiagnostic::TypeChanged));
    }
    (Some(value), None)
}

/// Resolve availability against immutable authored definitions without a VM.
/// Structural validation belongs to the authored document's admission owner.
pub fn hud_button_command_diagnostic(
    authoring: &Authoring,
    command: &HudCommandBinding,
) -> Option<HudDiagnostic> {
    let Some(binding) = authoring.bindings.get(&command.binding_id) else {
        return Some(HudDiagnostic::MissingBinding);
    };
    if !binding.enabled {
        return Some(HudDiagnostic::DisabledBinding);
    }
    if binding.target != BindingTarget::Application {
        return Some(HudDiagnostic::NonApplicationBinding);
    }
    let Some(source) = authoring.sources.get(&binding.source_id) else {
        return Some(HudDiagnostic::MissingSource);
    };
    if source.host_api_version != 5 {
        return Some(HudDiagnostic::UnsupportedCommandApi);
    }
    let Some(definition) = binding.commands.get(&command.command_id) else {
        return Some(HudDiagnostic::MissingCommand);
    };
    if definition.arguments.len() != command.arguments.len()
        || !definition.arguments.iter().all(|(name, default)| {
            command.arguments.get(name).is_some_and(|value| {
                std::mem::discriminant(default) == std::mem::discriminant(value)
            })
        })
    {
        return Some(HudDiagnostic::CommandArgumentsChanged);
    }
    None
}

pub(super) fn same_value(left: Option<&StateValue>, right: Option<&StateValue>) -> bool {
    match (left, right) {
        (Some(StateValue::Number(left)), Some(StateValue::Number(right))) => {
            left.to_bits() == right.to_bits()
        }
        (Some(StateValue::Vec3(left)), Some(StateValue::Vec3(right))) => left
            .iter()
            .zip(right)
            .all(|(left, right)| left.to_bits() == right.to_bits()),
        _ => left == right,
    }
}

struct CappedText(String);
impl Write for CappedText {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        if text.len() > MAX_HUD_RESOLVED_LABEL_BYTES - self.0.len() {
            return Err(fmt::Error);
        }
        self.0.push_str(text);
        Ok(())
    }
}

fn format_label(label: &HudLabel, value: Option<&StateValue>) -> Result<String, fmt::Error> {
    let mut output = CappedText(String::new());
    match &label.source {
        HudSource::Static { text } => output.write_str(text)?,
        HudSource::Button { caption, .. } => output.write_str(caption)?,
        HudSource::State {
            prefix,
            suffix,
            precision,
            ..
        } => {
            output.write_str(prefix)?;
            let precision = usize::from((*precision).min(6));
            match value.ok_or(fmt::Error)? {
                StateValue::Number(value) => write!(output, "{value:.precision$}")?,
                StateValue::Bool(value) => write!(output, "{value}")?,
                StateValue::Text(value) => output.write_str(value)?,
                StateValue::Vec3([x, y, z]) => {
                    write!(
                        output,
                        "({x:.precision$}, {y:.precision$}, {z:.precision$})"
                    )?;
                }
            }
            output.write_str(suffix)?;
        }
    }
    Ok(output.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::HudValueType;
    use crate::{BehaviorBinding, BehaviorSource, BindingTarget};
    use std::collections::BTreeMap;

    fn bound(value: StateValue, value_type: HudValueType) -> (Hud, Authoring) {
        let hud = Hud {
            enabled: true,
            elements: vec![HudLabel {
                id: "reading".into(),
                source: HudSource::State {
                    binding_id: "app".into(),
                    field: "reading".into(),
                    value_type,
                    prefix: "Value ".into(),
                    suffix: " units".into(),
                    precision: 2,
                },
            }],
            ..Hud::default()
        };
        let mut authoring = Authoring::default();
        authoring.sources.insert(
            "source".into(),
            BehaviorSource {
                label: None,
                language_id: "lua".into(),
                host_api_version: 5,
                text: String::new(),
            },
        );
        authoring.bindings.insert(
            "app".into(),
            BehaviorBinding {
                target: BindingTarget::Application,
                source_id: "source".into(),
                enabled: true,
                properties: BTreeMap::new(),
                state_defaults: BTreeMap::from([("reading".into(), value)]),
                commands: BTreeMap::new(),
            },
        );
        (hud, authoring)
    }

    #[test]
    fn typed_formatting_and_cache_preserve_negative_zero_and_bound_output() {
        let cases = [
            (
                StateValue::Number(-0.0),
                HudValueType::Number,
                "Value -0.00 units",
            ),
            (
                StateValue::Bool(false),
                HudValueType::Bool,
                "Value false units",
            ),
            (
                StateValue::Text("literal {x}".into()),
                HudValueType::Text,
                "Value literal {x} units",
            ),
            (
                StateValue::Vec3([1.0, -2.5, 0.0]),
                HudValueType::Vec3,
                "Value (1.00, -2.50, 0.00) units",
            ),
        ];
        for (value, value_type, expected) in cases {
            let (hud, authoring) = bound(value.clone(), value_type);
            let mut cache = HudCache::default();
            assert_eq!(
                cache.prepare(&hud, &authoring, |_, _| Some(&value)).labels[0].text,
                expected
            );
            assert_eq!(cache.formatted_labels(), 1);
            cache.prepare(&hud, &authoring, |_, _| Some(&value));
            assert_eq!(cache.formatted_labels(), 0);
        }
        let (hud, authoring) = bound(StateValue::Number(0.0), HudValueType::Number);
        let mut cache = HudCache::default();
        let positive_zero = StateValue::Number(0.0);
        let negative_zero = StateValue::Number(-0.0);
        cache.prepare(&hud, &authoring, |_, _| Some(&positive_zero));
        assert_eq!(
            cache
                .prepare(&hud, &authoring, |_, _| Some(&negative_zero))
                .labels[0]
                .text,
            "Value -0.00 units"
        );
        assert_eq!(cache.formatted_labels(), 1);
        let (mut hud, authoring) = bound(StateValue::Vec3([f64::MAX; 3]), HudValueType::Vec3);
        if let HudSource::State {
            prefix, precision, ..
        } = &mut hud.elements[0].source
        {
            *prefix = "x".repeat(1018);
            *precision = 6;
        }
        let value = StateValue::Vec3([f64::MAX; 3]);
        let resolved = resolve_hud(&hud, &authoring, |_, _| Some(&value));
        assert!(resolved.labels[0].text.len() <= MAX_HUD_RESOLVED_LABEL_BYTES);
        let mut capped = CappedText(String::new());
        capped
            .write_str(&"é".repeat(MAX_HUD_RESOLVED_LABEL_BYTES / 2))
            .unwrap();
        assert!(capped.write_str("é").is_err());
        assert_eq!(capped.0.len(), MAX_HUD_RESOLVED_LABEL_BYTES);
        let static_hud = Hud {
            enabled: true,
            elements: vec![HudLabel {
                id: "intro".into(),
                source: HudSource::Static {
                    text: "Ready".into(),
                },
            }],
            ..Hud::default()
        };
        assert_eq!(
            resolve_hud(&static_hud, &Authoring::default(), |_, _| panic!(
                "static text must not read state"
            ))
            .labels[0]
                .text,
            "Ready"
        );
        // A rejected oversized lookup cannot cause a large retained text allocation.
        let (hud, authoring) = bound(StateValue::Text(String::new()), HudValueType::Text);
        let value = StateValue::Text("x".repeat(10_000));
        let resolved = resolve_hud(&hud, &authoring, |_, _| Some(&value));
        assert_eq!(
            resolved.labels[0].diagnostic,
            Some(HudDiagnostic::OutputOverflow)
        );
        assert_eq!(resolved.labels[0].text, UNAVAILABLE);
    }

    // Button targets are definition-only and require all typed constants. Their
    // captions remain available for disabled presentation while references break.
    #[test]
    fn button_resolution_requires_complete_typed_command_without_state_lookup() {
        use crate::CommandDefinition;
        let (mut hud, mut authoring) = bound(StateValue::Bool(false), HudValueType::Bool);
        let arguments = BTreeMap::from([
            ("n".into(), StateValue::Number(-0.0)),
            ("b".into(), StateValue::Bool(true)),
            ("t".into(), StateValue::Text("literal".into())),
            ("v".into(), StateValue::Vec3([1.0, 2.0, 3.0])),
        ]);
        authoring.bindings.get_mut("app").unwrap().commands.insert(
            "act".into(),
            CommandDefinition {
                arguments: arguments.clone(),
            },
        );
        let command = HudCommandBinding {
            binding_id: "app".into(),
            command_id: "act".into(),
            arguments,
        };
        super::super::validate_hud_button_command(&authoring, &command).unwrap();
        hud.elements[0].source = HudSource::Button {
            caption: "Activate".into(),
            command: command.clone(),
        };
        let resolve = |authoring: &Authoring| {
            resolve_hud(&hud, authoring, |_, _| panic!("buttons never read state"))
        };
        assert_eq!(resolve(&authoring).labels[0].text, "Activate");
        assert_eq!(resolve(&authoring).labels[0].diagnostic, None);
        for (name, value) in [
            ("unknown", Some(StateValue::Bool(true))),
            ("n", None),
            ("n", Some(StateValue::Bool(true))),
        ] {
            let mut changed = command.clone();
            if let Some(value) = value {
                changed.arguments.insert(name.into(), value);
            } else {
                changed.arguments.remove(name);
            }
            assert_eq!(
                hud_button_command_diagnostic(&authoring, &changed),
                Some(HudDiagnostic::CommandArgumentsChanged)
            );
            assert!(super::super::validate_hud_button_command(&authoring, &changed).is_err());
        }
        let mut changed = command.clone();
        changed
            .arguments
            .insert("n".into(), StateValue::Number(0.0));
        assert_ne!(changed, command, "constant identity retains negative zero");
        super::super::validate_hud_button_command(&authoring, &changed).unwrap();
        authoring.bindings.get_mut("app").unwrap().commands.clear();
        assert_eq!(
            resolve(&authoring).labels[0].diagnostic,
            Some(HudDiagnostic::MissingCommand)
        );
        assert_eq!(resolve(&authoring).labels[0].text, "Activate");
        authoring
            .sources
            .get_mut("source")
            .unwrap()
            .host_api_version = 4;
        assert_eq!(
            resolve(&authoring).labels[0].diagnostic,
            Some(HudDiagnostic::UnsupportedCommandApi)
        );
        authoring.bindings.get_mut("app").unwrap().target = BindingTarget::Object("object".into());
        assert_eq!(
            resolve(&authoring).labels[0].diagnostic,
            Some(HudDiagnostic::NonApplicationBinding)
        );
        authoring.bindings.get_mut("app").unwrap().target = BindingTarget::Application;
        authoring.sources.clear();
        assert_eq!(
            resolve(&authoring).labels[0].diagnostic,
            Some(HudDiagnostic::MissingSource)
        );
        authoring.bindings.get_mut("app").unwrap().enabled = false;
        assert_eq!(
            resolve(&authoring).labels[0].diagnostic,
            Some(HudDiagnostic::DisabledBinding)
        );
        authoring.bindings.clear();
        assert_eq!(
            resolve(&authoring).labels[0].diagnostic,
            Some(HudDiagnostic::MissingBinding)
        );
    }

    #[test]
    fn broken_references_remain_diagnostic_without_formatting_affixes() {
        let value = StateValue::Number(1.0);
        let (hud, mut authoring) = bound(value.clone(), super::super::HudValueType::Number);
        assert_eq!(
            resolve_hud(&hud, &authoring, |_, _| None).labels[0].diagnostic,
            Some(HudDiagnostic::UnavailableState)
        );
        authoring
            .bindings
            .get_mut("app")
            .unwrap()
            .state_defaults
            .insert("reading".into(), StateValue::Bool(true));
        let resolved = resolve_hud(&hud, &authoring, |_, _| Some(&value));
        assert_eq!(
            resolved.labels[0].diagnostic,
            Some(HudDiagnostic::TypeChanged)
        );
        assert_eq!(resolved.labels[0].text, UNAVAILABLE);
        authoring.bindings.get_mut("app").unwrap().enabled = false;
        assert_eq!(
            resolve_hud(&hud, &authoring, |_, _| Some(&value)).labels[0].diagnostic,
            Some(HudDiagnostic::DisabledBinding)
        );
        authoring.bindings.get_mut("app").unwrap().enabled = true;
        authoring.sources.clear();
        assert_eq!(
            resolve_hud(&hud, &authoring, |_, _| Some(&value)).labels[0].diagnostic,
            Some(HudDiagnostic::MissingSource)
        );
    }
}
