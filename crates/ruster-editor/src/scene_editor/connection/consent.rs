//! Frozen, one-shot registration consent; no configuration IO or process launch.

use serde_json::{json, Value};
use std::path::Path;

/// Frozen request plus local provenance. Production execution remains disabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Invocation {
    args: Vec<String>,
    stdin: String,
    #[cfg(target_os = "linux")]
    provenance: Option<super::capsule::Provenance>,
}

impl Invocation {
    #[cfg(target_os = "linux")]
    pub(super) fn provenance(&self) -> Option<&super::capsule::Provenance> {
        self.provenance.as_ref()
    }

    pub(super) fn args(&self) -> &[String] {
        &self.args
    }
    pub(super) fn stdin(&self) -> &str {
        &self.stdin
    }
}

#[derive(PartialEq, Eq)]
enum State {
    Preview,
    Cancelled,
    Dispatched,
    Created,
    Reused,
    Removed,
    Conflict,
    Failed,
}

/// This seam deliberately cannot choose a profile, launch a process, or retry.
pub(super) struct RegistrationConsent {
    invocation: Invocation,
    state: State,
    remove: bool,
    receipt: Option<Value>,
}

impl RegistrationConsent {
    #[cfg(target_os = "linux")]
    pub(super) fn preview_bound(
        provenance: super::capsule::Provenance,
        name: &str,
        locator: &Path,
        selection: Value,
        remove: bool,
    ) -> Result<Self, String> {
        provenance.revalidate()?;
        if selection.as_object().is_none_or(|v| v.len() != 2)
            || selection["present"].as_bool().is_none()
            || !(selection["entry"].is_null() || selection["entry"].is_object())
            || (selection["present"] == false && !selection["entry"].is_null())
            || (remove && selection["present"] != true)
            || locator
                .to_str()
                .is_none_or(|s| s.contains('$') || s.contains('\0'))
        {
            return Err("Requires an exact raw selection and literal locator.".into());
        }
        let expected = selection["entry"]
            .as_object()
            .map(|_| selection["entry"].clone());
        // prepare's legacy removal check requires an object; strict present-null
        // removal is represented separately in the immutable raw selection below.
        let mut consent = Self::prepare(
            provenance.profile(),
            name,
            provenance.adapter(),
            locator,
            expected,
            false,
        )?;
        let mut request: Value = serde_json::from_str(&consent.invocation.stdin)
            .map_err(|_| "Invalid prepared request.")?;
        request["context"] = provenance.context().clone();
        request["expected"] = selection;
        if remove {
            request["entry"] = Value::Null;
        }
        consent.remove = remove;
        consent.invocation.stdin = request.to_string();
        if consent.invocation.stdin.len() > 4096 {
            return Err("Strict request exceeds bounded worker input.".into());
        }
        consent.invocation.args.push("--strict-profile".into());
        consent.invocation.provenance = Some(provenance);
        Ok(consent)
    }

    #[cfg(test)]
    pub(super) fn preview(
        profile: &str,
        name: &str,
        matched_adapter: &Path,
        locator: &Path,
        expected: Option<Value>,
        remove: bool,
    ) -> Result<Self, String> {
        Self::prepare(profile, name, matched_adapter, locator, expected, remove)
    }

    fn prepare(
        profile: &str,
        name: &str,
        matched_adapter: &Path,
        locator: &Path,
        expected: Option<Value>,
        remove: bool,
    ) -> Result<Self, String> {
        let valid_name = |value: &str| {
            !value.is_empty()
                && !value.starts_with('-')
                && value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        };
        if !valid_name(profile) || profile == "default" || !valid_name(name) {
            return Err("Select an explicit non-default named profile and entry.".into());
        }
        if !matched_adapter.is_absolute() || !locator.is_absolute() {
            return Err(
                "Preview requires absolute matched adapter and nonsecret locator paths.".into(),
            );
        }
        let adapter = matched_adapter
            .to_str()
            .ok_or("Adapter path is not UTF-8.")?;
        let locator = locator.to_str().ok_or("Locator path is not UTF-8.")?;
        if expected.as_ref().is_some_and(|value| !value.is_object())
            || (remove && expected.is_none())
        {
            return Err("Expected entry must be an exact object; removal requires one.".into());
        }
        let entry = if remove {
            Value::Null
        } else {
            json!({"command": adapter, "enabled": true, "env": {"RUSTER_MCP_LOCATOR": locator}})
        };
        Ok(Self {
            invocation: Invocation {
                args: ["-p", profile, "mcp", "apply", name, "--request", "-"]
                    .map(str::to_owned)
                    .to_vec(),
                stdin: json!({"expected": expected, "entry": entry}).to_string(),
                #[cfg(target_os = "linux")]
                provenance: None,
            },
            state: State::Preview,
            remove,
            receipt: None,
        })
    }

    pub(super) fn invocation(&self) -> &Invocation {
        &self.invocation
    }

    pub(super) fn cancel(&mut self) {
        if self.state == State::Preview {
            self.state = State::Cancelled;
        }
    }

    pub(super) fn confirm(&mut self) -> Result<Invocation, String> {
        if self.state != State::Preview {
            return Err("Consent is no longer pending.".into());
        }
        #[cfg(target_os = "linux")]
        if let Some(provenance) = &self.invocation.provenance {
            if let Err(error) = provenance.revalidate() {
                self.fail();
                return Err(error);
            }
        }
        self.state = State::Dispatched;
        Ok(self.invocation.clone())
    }

    /// Consume one bounded machine receipt. Failure is terminal, including
    /// malformed output or an exit/receipt mismatch; no implicit re-read/retry.
    pub(super) fn complete(&mut self, exit: i32, stdout: &str) -> Result<(), String> {
        if self.state != State::Dispatched {
            return Err("No authorized request is awaiting a result.".into());
        }
        self.receipt = (stdout.len() <= 4096)
            .then(|| serde_json::from_str(stdout).ok())
            .flatten();
        self.state = match self.receipt.as_ref() {
            Some(value)
                if exit == 3 && value == &json!({"ok":false,"error":"config_target_conflict"}) =>
            {
                State::Conflict
            }
            Some(value)
                if exit == 0
                    && value.as_object().is_some_and(|object| object.len() == 3)
                    && value.get("ok") == Some(&Value::Bool(true))
                    && value.get("changed").and_then(Value::as_bool).is_some()
                    && value.get("tools").and_then(Value::as_u64)
                        == Some(if self.remove { 0 } else { 84 }) =>
            {
                if self.remove {
                    State::Removed
                } else if value["changed"] == true {
                    State::Created
                } else {
                    State::Reused
                }
            }
            _ => State::Failed,
        };
        if self.state == State::Failed {
            self.receipt = None;
        }
        Ok(())
    }

    pub(super) fn succeeded(&self) -> bool {
        matches!(self.state, State::Created | State::Reused | State::Removed)
    }

    pub(super) fn fail(&mut self) {
        self.state = State::Failed;
        self.receipt = None;
    }

    pub(super) fn receipt(&self) -> Option<&Value> {
        self.receipt.as_ref()
    }

    pub(super) fn status(&self) -> &'static str {
        match self.state {
            State::Preview => "Awaiting registration consent",
            State::Cancelled => "Registration cancelled; no request dispatched",
            State::Dispatched => "Registration authorized; execution/result pending",
            State::Created => "Registered; client restart required; editor binding separate",
            State::Reused => {
                "Registration reused; client restart required; editor binding separate"
            }
            State::Removed => {
                "Registration removed; client restart required; editor binding separate"
            }
            State::Conflict => "Registration conflict; no refresh or retry",
            State::Failed => "Registration failed or indeterminate; inspect before a new preview",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn one_shot_receipts_do_not_retry_or_claim_binding() {
        for (code, receipt, status) in [
            (
                0,
                json!({"ok":true,"changed":true,"tools":84}),
                "Registered; client restart required; editor binding separate",
            ),
            (
                0,
                json!({"ok":true,"changed":false,"tools":84}),
                "Registration reused; client restart required; editor binding separate",
            ),
            (
                3,
                json!({"ok":false,"error":"config_target_conflict"}),
                "Registration conflict; no refresh or retry",
            ),
            (
                0,
                json!({"ok":true}),
                "Registration failed or indeterminate; inspect before a new preview",
            ),
            (
                1,
                json!({"ok":true,"changed":true,"tools":84}),
                "Registration failed or indeterminate; inspect before a new preview",
            ),
            (
                0,
                json!({"ok":true,"changed":true,"tools":84,"secret":"never retain unknown fields"}),
                "Registration failed or indeterminate; inspect before a new preview",
            ),
        ] {
            let mut consent = RegistrationConsent::preview(
                "named",
                "rusterize",
                Path::new("/matched/ruster-mcp"),
                Path::new("/locator"),
                None,
                false,
            )
            .unwrap();
            assert!(consent.complete(code, &receipt.to_string()).is_err());
            let frozen = consent.invocation().clone();
            assert_eq!(consent.confirm().unwrap(), frozen);
            consent.cancel(); // Cannot pretend an authorized request was cancelled.
            assert!(consent.confirm().is_err());
            consent.complete(code, &receipt.to_string()).unwrap();
            assert_eq!(consent.status(), status);
            assert_eq!(
                consent.receipt(),
                if consent.state == State::Failed {
                    None
                } else {
                    Some(&receipt)
                }
            );
            assert!(consent.confirm().is_err());
            assert!(consent.complete(code, &receipt.to_string()).is_err());
            assert_eq!(consent.invocation(), &frozen);
        }
        for stdout in ["not JSON".to_owned(), "x".repeat(4097)] {
            let mut consent = RegistrationConsent::preview(
                "named",
                "rusterize",
                Path::new("/matched/ruster-mcp"),
                Path::new("/locator"),
                None,
                false,
            )
            .unwrap();
            consent.confirm().unwrap();
            consent.complete(0, &stdout).unwrap();
            assert!(consent.receipt().is_none());
            assert_eq!(
                consent.status(),
                "Registration failed or indeterminate; inspect before a new preview"
            );
            assert!(consent.confirm().is_err());
        }
        let entry = json!({"command":"/matched/ruster-mcp","enabled":true,"env":{"RUSTER_MCP_LOCATOR":"/locator"}});
        let mut remove = RegistrationConsent::preview(
            "named",
            "rusterize",
            Path::new("/matched/ruster-mcp"),
            Path::new("/locator"),
            Some(entry.clone()),
            true,
        )
        .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&remove.invocation().stdin).unwrap(),
            json!({"expected":entry,"entry":null})
        );
        remove.confirm().unwrap();
        remove
            .complete(0, r#"{"ok":true,"changed":true,"tools":0}"#)
            .unwrap();
        assert_eq!(
            remove.status(),
            "Registration removed; client restart required; editor binding separate"
        );
    }

    #[test]
    fn preview_is_exact_and_cancel_cannot_dispatch() {
        let expected = json!({"command":"/old/adapter","enabled":true});
        let mut consent = RegistrationConsent::preview(
            "disposable-cp2",
            "rusterize",
            Path::new("/matched/ruster-mcp"),
            Path::new("/private/nonsecret-locator.json"),
            Some(expected.clone()),
            false,
        )
        .unwrap();
        assert_eq!(
            consent.invocation().args,
            [
                "-p",
                "disposable-cp2",
                "mcp",
                "apply",
                "rusterize",
                "--request",
                "-"
            ]
        );
        let request: serde_json::Value = serde_json::from_str(&consent.invocation().stdin).unwrap();
        assert_eq!(
            request,
            json!({"expected":expected,"entry":{
                "command":"/matched/ruster-mcp", "enabled":true,
                "env":{"RUSTER_MCP_LOCATOR":"/private/nonsecret-locator.json"}
            }})
        );
        assert_eq!(consent.status(), "Awaiting registration consent");
        consent.cancel();
        assert!(consent.confirm().is_err());
        assert_eq!(
            consent.status(),
            "Registration cancelled; no request dispatched"
        );
        for profile in ["", "default", "../other", "-p"] {
            assert!(RegistrationConsent::preview(
                profile,
                "rusterize",
                Path::new("/matched/ruster-mcp"),
                Path::new("/locator"),
                None,
                false
            )
            .is_err());
        }
        assert!(RegistrationConsent::preview(
            "named",
            "rusterize",
            Path::new("relative"),
            Path::new("/locator"),
            None,
            false
        )
        .is_err());
    }
}
