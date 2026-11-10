//! Bounded, read-only HUD resolution shared by native and headless consumers.
use std::fmt::{self, Write};

use serde::Serialize;

use super::{
    BoolCondition, Hud, HudCommandBinding, HudLabel, HudSource, MAX_HUD_ELEMENT_TEXT_BYTES,
    MAX_HUD_LABELS, MAX_HUD_PANELS, MAX_HUD_RESOLVED_LABEL_BYTES,
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
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ResolvedHudPanel {
    pub id: String,
    pub visible: bool,
    pub diagnostic: Option<HudDiagnostic>,
    pub labels: Vec<ResolvedHudLabel>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct ResolvedHud {
    pub panels: Vec<ResolvedHudPanel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ResolvedCondition {
    pub permitted: bool,
    pub diagnostic: Option<HudDiagnostic>,
}

/// Diagnose only the authored condition target. Admission never reads live state.
pub fn bool_condition_diagnostic(
    authoring: &Authoring,
    condition: &BoolCondition,
) -> Option<HudDiagnostic> {
    let Some(binding) = authoring.bindings.get(&condition.binding_id) else {
        return Some(HudDiagnostic::MissingBinding);
    };
    if !binding.enabled {
        return Some(HudDiagnostic::DisabledBinding);
    }
    if !authoring.sources.contains_key(&binding.source_id) {
        return Some(HudDiagnostic::MissingSource);
    }
    match binding.state_defaults.get(&condition.field) {
        None => Some(HudDiagnostic::MissingField),
        Some(StateValue::Bool(_)) => None,
        Some(_) => Some(HudDiagnostic::TypeChanged),
    }
}

/// Resolve a typed Bool condition against the same immutable frame as the HUD.
/// Absence permits; a value mismatch denies quietly; unavailable data denies with a diagnostic.
pub fn resolve_bool_condition<'s>(
    authoring: &Authoring,
    condition: Option<&BoolCondition>,
    mut lookup: impl FnMut(&str, &str) -> Option<&'s StateValue>,
) -> ResolvedCondition {
    let Some(condition) = condition else {
        return ResolvedCondition {
            permitted: true,
            diagnostic: None,
        };
    };
    if let Some(diagnostic) = bool_condition_diagnostic(authoring, condition) {
        return ResolvedCondition {
            permitted: false,
            diagnostic: Some(diagnostic),
        };
    }
    match lookup(&condition.binding_id, &condition.field) {
        Some(StateValue::Bool(value)) => ResolvedCondition {
            permitted: *value == condition.expected,
            diagnostic: None,
        },
        Some(_) => ResolvedCondition {
            permitted: false,
            diagnostic: Some(HudDiagnostic::TypeChanged),
        },
        None => ResolvedCondition {
            permitted: false,
            diagnostic: Some(HudDiagnostic::UnavailableState),
        },
    }
}

#[derive(Default)]
pub struct HudCache {
    entries: Vec<CachedLabel>,
    resolved: ResolvedHud,
    formatted_labels: usize,
    resolved_changed: bool,
}

struct CachedLabel {
    definition: Option<HudLabel>,
    value: Option<StateValue>,
    diagnostic: Option<HudDiagnostic>,
}

impl HudCache {
    pub fn clear(&mut self) {
        self.resolved_changed = !self.resolved.panels.is_empty();
        self.entries.clear();
        self.resolved.panels.clear();
        self.formatted_labels = 0;
    }

    /// Number of labels reformatted by the last prepare, excluding cache hits.
    pub fn formatted_labels(&self) -> usize {
        self.formatted_labels
    }

    /// Any text, panel visibility or row availability changed in the last prepare.
    pub fn resolved_changed(&self) -> bool {
        self.resolved_changed
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
        self.resolved_changed = false;
        if !hud.enabled || hud.panels.is_empty() {
            self.clear();
            return &self.resolved;
        }
        self.entries
            .truncate(hud.elements().count().min(MAX_HUD_LABELS));
        self.resolved_changed |= self.resolved.panels.len() != hud.panels.len().min(MAX_HUD_PANELS);
        self.resolved
            .panels
            .truncate(hud.panels.len().min(MAX_HUD_PANELS));
        let mut index = 0;
        for (panel_index, panel) in hud.panels.iter().take(MAX_HUD_PANELS).enumerate() {
            let visibility =
                resolve_bool_condition(authoring, panel.visible_when.as_ref(), &mut lookup);
            if panel_index == self.resolved.panels.len() {
                self.resolved.panels.push(ResolvedHudPanel {
                    id: panel.id.clone(),
                    visible: visibility.permitted,
                    diagnostic: visibility.diagnostic,
                    labels: Vec::new(),
                });
            }
            let resolved_panel = &mut self.resolved.panels[panel_index];
            self.resolved_changed |= resolved_panel.id != panel.id
                || resolved_panel.visible != visibility.permitted
                || resolved_panel.diagnostic != visibility.diagnostic;
            resolved_panel.id.clone_from(&panel.id);
            resolved_panel.visible = visibility.permitted;
            resolved_panel.diagnostic = visibility.diagnostic;
            let count = panel.elements.len().min(MAX_HUD_LABELS - index);
            self.resolved_changed |= resolved_panel.labels.len() != count;
            resolved_panel.labels.truncate(count);
            for (row_index, label) in panel.elements.iter().take(count).enumerate() {
                let (value, mut source_diagnostic) = reference(label, authoring, &mut lookup);
                let condition =
                    resolve_bool_condition(authoring, label.enabled_when.as_ref(), &mut lookup);
                let bounded_definition = authored_text_bytes(label) <= MAX_HUD_ELEMENT_TEXT_BYTES;
                let bounded_value = value.is_none_or(|value| match value {
                    StateValue::Text(text) => text.len() <= crate::MAX_PROPERTY_TEXT_BYTES,
                    _ => true,
                });
                if !bounded_definition || !bounded_value {
                    source_diagnostic = Some(HudDiagnostic::OutputOverflow);
                }
                let cached = self.entries.get(index);
                let reusable = bounded_definition
                    && bounded_value
                    && resolved_panel
                        .labels
                        .get(row_index)
                        .is_some_and(|old| old.id == label.id)
                    && cached.is_some_and(|cached| {
                        cached.definition.as_ref() == Some(label)
                            && cached.diagnostic == source_diagnostic
                            && same_value(cached.value.as_ref(), value)
                    });
                if !reusable {
                    self.resolved_changed = true;
                    let text = if source_diagnostic == Some(HudDiagnostic::OutputOverflow)
                        || (source_diagnostic.is_some()
                            && !matches!(&label.source, HudSource::Button { .. }))
                    {
                        UNAVAILABLE.to_owned()
                    } else {
                        match format_label(label, value) {
                            Ok(text) => text,
                            Err(_) => {
                                source_diagnostic = Some(HudDiagnostic::OutputOverflow);
                                UNAVAILABLE.to_owned()
                            }
                        }
                    };
                    let entry = CachedLabel {
                        definition: bounded_definition.then(|| label.clone()),
                        value: if bounded_value { value.cloned() } else { None },
                        diagnostic: source_diagnostic,
                    };
                    if index < self.entries.len() {
                        self.entries[index] = entry;
                    } else {
                        self.entries.push(entry);
                    }
                    let resolved = ResolvedHudLabel {
                        id: label.id.clone(),
                        text,
                        diagnostic: None,
                        available: false,
                    };
                    if row_index < resolved_panel.labels.len() {
                        resolved_panel.labels[row_index] = resolved;
                    } else {
                        resolved_panel.labels.push(resolved);
                    }
                    self.formatted_labels += 1;
                }
                let resolved = &mut resolved_panel.labels[row_index];
                let diagnostic = source_diagnostic.or(condition.diagnostic);
                let available =
                    visibility.permitted && condition.permitted && source_diagnostic.is_none();
                self.resolved_changed |=
                    resolved.diagnostic != diagnostic || resolved.available != available;
                resolved.diagnostic = diagnostic;
                resolved.available = available;
                index += 1;
            }
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
        let hud = crate::hud::Hud {
            enabled: true,
            panels: vec![crate::hud::HudPanel {
                elements: vec![HudLabel {
                    enabled_when: None,
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
                ..crate::hud::HudPanel::default()
            }],
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
                cache.prepare(&hud, &authoring, |_, _| Some(&value)).panels[0].labels[0].text,
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
                .panels[0]
                .labels[0]
                .text,
            "Value -0.00 units"
        );
        assert_eq!(cache.formatted_labels(), 1);
        let (mut hud, authoring) = bound(StateValue::Vec3([f64::MAX; 3]), HudValueType::Vec3);
        if let HudSource::State {
            prefix, precision, ..
        } = &mut hud.panels[0].elements[0].source
        {
            *prefix = "x".repeat(1018);
            *precision = 6;
        }
        let value = StateValue::Vec3([f64::MAX; 3]);
        let resolved = resolve_hud(&hud, &authoring, |_, _| Some(&value));
        assert!(resolved.panels[0].labels[0].text.len() <= MAX_HUD_RESOLVED_LABEL_BYTES);
        let mut capped = CappedText(String::new());
        capped
            .write_str(&"é".repeat(MAX_HUD_RESOLVED_LABEL_BYTES / 2))
            .unwrap();
        assert!(capped.write_str("é").is_err());
        assert_eq!(capped.0.len(), MAX_HUD_RESOLVED_LABEL_BYTES);
        let static_hud = crate::hud::Hud {
            enabled: true,
            panels: vec![crate::hud::HudPanel {
                elements: vec![HudLabel {
                    enabled_when: None,
                    id: "intro".into(),
                    source: HudSource::Static {
                        text: "Ready".into(),
                    },
                }],
                ..crate::hud::HudPanel::default()
            }],
        };
        assert_eq!(
            resolve_hud(&static_hud, &Authoring::default(), |_, _| panic!(
                "static text must not read state"
            ))
            .panels[0]
                .labels[0]
                .text,
            "Ready"
        );
        // A rejected oversized lookup cannot cause a large retained text allocation.
        let (hud, authoring) = bound(StateValue::Text(String::new()), HudValueType::Text);
        let value = StateValue::Text("x".repeat(10_000));
        let resolved = resolve_hud(&hud, &authoring, |_, _| Some(&value));
        assert_eq!(
            resolved.panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::OutputOverflow)
        );
        assert_eq!(resolved.panels[0].labels[0].text, UNAVAILABLE);
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
        hud.panels[0].elements[0].source = HudSource::Button {
            caption: "Activate".into(),
            command: command.clone(),
        };
        let resolve = |authoring: &Authoring| {
            resolve_hud(&hud, authoring, |_, _| panic!("buttons never read state"))
        };
        assert_eq!(resolve(&authoring).panels[0].labels[0].text, "Activate");
        assert_eq!(resolve(&authoring).panels[0].labels[0].diagnostic, None);
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
            resolve(&authoring).panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::MissingCommand)
        );
        assert_eq!(resolve(&authoring).panels[0].labels[0].text, "Activate");
        authoring
            .sources
            .get_mut("source")
            .unwrap()
            .host_api_version = 4;
        assert_eq!(
            resolve(&authoring).panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::UnsupportedCommandApi)
        );
        authoring.bindings.get_mut("app").unwrap().target = BindingTarget::Object("object".into());
        assert_eq!(
            resolve(&authoring).panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::NonApplicationBinding)
        );
        authoring.bindings.get_mut("app").unwrap().target = BindingTarget::Application;
        authoring.sources.clear();
        assert_eq!(
            resolve(&authoring).panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::MissingSource)
        );
        authoring.bindings.get_mut("app").unwrap().enabled = false;
        assert_eq!(
            resolve(&authoring).panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::DisabledBinding)
        );
        authoring.bindings.clear();
        assert_eq!(
            resolve(&authoring).panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::MissingBinding)
        );
    }

    // Conditions change presentation metadata without text work. Hidden panels
    // retain ordered rows, and unavailable/retyped data always denies actions.
    #[test]
    fn condition_visibility_and_availability_update_without_text_formatting() {
        use crate::CommandDefinition;
        let (mut hud, mut authoring) = bound(StateValue::Bool(true), HudValueType::Bool);
        let condition = BoolCondition {
            binding_id: "app".into(),
            field: "reading".into(),
            expected: true,
        };
        super::super::validate_bool_condition(&authoring, &condition).unwrap();
        let unconditional = resolve_bool_condition(&authoring, None, |_, _| {
            panic!("absent condition reads no state")
        });
        assert!(unconditional.permitted && unconditional.diagnostic.is_none());
        authoring.bindings.get_mut("app").unwrap().commands.insert(
            "act".into(),
            CommandDefinition {
                arguments: BTreeMap::new(),
            },
        );
        hud.panels[0].visible_when = Some(condition.clone());
        hud.panels[0].elements[0].enabled_when = Some(condition.clone());
        hud.panels[0].elements[0].source = HudSource::Button {
            caption: "Activate".into(),
            command: HudCommandBinding {
                binding_id: "app".into(),
                command_id: "act".into(),
                arguments: BTreeMap::new(),
            },
        };
        let yes = StateValue::Bool(true);
        let no = StateValue::Bool(false);
        let mut cache = HudCache::default();
        assert!(cache.prepare(&hud, &authoring, |_, _| Some(&yes)).panels[0].labels[0].available);
        assert!(cache.resolved_changed());
        cache.prepare(&hud, &authoring, |_, _| Some(&yes));
        assert_eq!(cache.formatted_labels(), 0);
        assert!(!cache.resolved_changed());
        let hidden = cache.prepare(&hud, &authoring, |_, _| Some(&no));
        assert!(!hidden.panels[0].visible && !hidden.panels[0].labels[0].available);
        assert_eq!(hidden.panels[0].labels[0].text, "Activate");
        assert_eq!(hidden.panels[0].diagnostic, None);
        assert_eq!(hidden.panels[0].labels[0].diagnostic, None);
        assert_eq!(cache.formatted_labels(), 0);
        assert!(cache.resolved_changed());
        cache.prepare(&hud, &authoring, |_, _| Some(&no));
        assert!(!cache.resolved_changed());
        hud.panels[0].visible_when.as_mut().unwrap().expected = false;
        let disabled = cache.prepare(&hud, &authoring, |_, _| Some(&no));
        assert!(disabled.panels[0].visible && !disabled.panels[0].labels[0].available);
        assert_eq!(disabled.panels[0].labels[0].diagnostic, None);
        assert_eq!(cache.formatted_labels(), 0);
        assert!(cache.resolved_changed());
        let number = StateValue::Number(1.0);
        let changed = cache.prepare(&hud, &authoring, |_, _| Some(&number));
        assert!(!changed.panels[0].visible && !changed.panels[0].labels[0].available);
        assert_eq!(
            changed.panels[0].diagnostic,
            Some(HudDiagnostic::TypeChanged)
        );
        assert_eq!(
            changed.panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::TypeChanged)
        );
        assert_eq!(changed.panels[0].labels[0].text, "Activate");
        assert_eq!(cache.formatted_labels(), 0);
        assert_eq!(
            cache.prepare(&hud, &authoring, |_, _| None).panels[0].diagnostic,
            Some(HudDiagnostic::UnavailableState)
        );
        authoring
            .bindings
            .get_mut("app")
            .unwrap()
            .state_defaults
            .clear();
        assert_eq!(
            resolve_bool_condition(&authoring, Some(&condition), |_, _| Some(&yes)).diagnostic,
            Some(HudDiagnostic::MissingField)
        );
        assert!(super::super::validate_bool_condition(&authoring, &condition).is_err());
        authoring
            .bindings
            .get_mut("app")
            .unwrap()
            .state_defaults
            .insert("reading".into(), StateValue::Number(0.0));
        assert_eq!(
            resolve_bool_condition(&authoring, Some(&condition), |_, _| Some(&yes)).diagnostic,
            Some(HudDiagnostic::TypeChanged)
        );
        authoring.bindings.get_mut("app").unwrap().enabled = false;
        assert_eq!(
            resolve_bool_condition(&authoring, Some(&condition), |_, _| Some(&yes)).diagnostic,
            Some(HudDiagnostic::DisabledBinding)
        );
        authoring.bindings.get_mut("app").unwrap().enabled = true;
        authoring.sources.clear();
        assert_eq!(
            resolve_bool_condition(&authoring, Some(&condition), |_, _| Some(&yes)).diagnostic,
            Some(HudDiagnostic::MissingSource)
        );
        authoring.bindings.clear();
        assert_eq!(
            resolve_bool_condition(&authoring, Some(&condition), |_, _| Some(&yes)).diagnostic,
            Some(HudDiagnostic::MissingBinding)
        );
        hud.enabled = false;
        assert!(cache
            .prepare(&hud, &authoring, |_, _| Some(&yes))
            .panels
            .is_empty());
        assert!(cache.resolved_changed());
        cache.prepare(&hud, &authoring, |_, _| Some(&yes));
        assert!(!cache.resolved_changed());
    }

    #[test]
    fn broken_references_remain_diagnostic_without_formatting_affixes() {
        let value = StateValue::Number(1.0);
        let (hud, mut authoring) = bound(value.clone(), super::super::HudValueType::Number);
        assert_eq!(
            resolve_hud(&hud, &authoring, |_, _| None).panels[0].labels[0].diagnostic,
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
            resolved.panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::TypeChanged)
        );
        assert_eq!(resolved.panels[0].labels[0].text, UNAVAILABLE);
        authoring.bindings.get_mut("app").unwrap().enabled = false;
        assert_eq!(
            resolve_hud(&hud, &authoring, |_, _| Some(&value)).panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::DisabledBinding)
        );
        authoring.bindings.get_mut("app").unwrap().enabled = true;
        authoring.sources.clear();
        assert_eq!(
            resolve_hud(&hud, &authoring, |_, _| Some(&value)).panels[0].labels[0].diagnostic,
            Some(HudDiagnostic::MissingSource)
        );
    }
}
