//! Local immutable provenance, not authorization to run Hermes or a same-UID sandbox.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Clone, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Artifact {
    pub(super) path: PathBuf,
    /// Digest from a caller-qualified matched build, never learned from this file.
    pub(super) sha256: String,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LaunchSpec {
    pub(super) launcher: Artifact,
    pub(super) interpreter: Artifact,
    pub(super) build: Artifact,
    pub(super) adapter: Artifact,
    pub(super) runtime: Vec<Artifact>,
    pub(super) cwd: PathBuf,
    pub(super) environment: BTreeMap<String, String>,
    /// Supported strict inspection's context, not resolved `config get` output.
    pub(super) context: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Pin {
    path: PathBuf,
    device: u64,
    inode: u64,
    owner: u32,
    mode: u32,
    sha256: Option<String>,
}

fn literal(path: &Path) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .to_str()
            .is_none_or(|p| p.contains('$') || p.contains('\0'))
    {
        return Err("Invocation paths must be absolute and literal.".into());
    }
    Ok(())
}

impl Pin {
    fn capture(path: &Path, directory: bool) -> Result<Self, String> {
        literal(path)?;
        // Reject symlink/relative components, not just a symlink at the leaf.
        if path
            .canonicalize()
            .map_err(|_| "Unavailable provenance path.")?
            != path
        {
            return Err("Provenance paths must be canonical, without symlinks.".into());
        }
        let mut file = File::open(path).map_err(|_| "Cannot open provenance path.")?;
        let before = file
            .metadata()
            .map_err(|_| "Cannot inspect provenance path.")?;
        if (directory && !before.is_dir()) || (!directory && !before.is_file()) {
            return Err("Wrong provenance path type.".into());
        }
        let sha256 = if directory {
            None
        } else {
            let mut hash = Sha256::new();
            let mut buffer = [0; 65536];
            loop {
                let n = file
                    .read(&mut buffer)
                    .map_err(|_| "Cannot hash provenance file.")?;
                if n == 0 {
                    break;
                }
                hash.update(&buffer[..n]);
            }
            Some(format!("{:x}", hash.finalize()))
        };
        let after = file
            .metadata()
            .map_err(|_| "Cannot recheck provenance file.")?;
        let named = std::fs::symlink_metadata(path).map_err(|_| "Provenance path disappeared.")?;
        if before.dev() != named.dev()
            || before.ino() != named.ino()
            || named.file_type().is_symlink()
            || before.size() != after.size()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.mtime() != after.mtime()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
        {
            return Err("Provenance changed while inspected.".into());
        }
        Ok(Self {
            path: path.into(),
            device: before.dev(),
            inode: before.ino(),
            owner: before.uid(),
            mode: before.mode(),
            sha256,
        })
    }

    fn artifact(artifact: &Artifact) -> Result<Self, String> {
        let pin = Self::capture(&artifact.path, false)?;
        if pin.sha256.as_ref() != Some(&artifact.sha256) {
            return Err("Artifact does not match the qualified build.".into());
        }
        Ok(pin)
    }
}

/// Frozen before preview. Fields have no mutation/replacement API. This local
/// capsule does not qualify the strict API or permit dispatch in production.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Provenance {
    launcher: Pin,
    interpreter: Pin,
    build: Pin,
    adapter: Pin,
    runtime: Vec<Pin>,
    cwd: Pin,
    environment: BTreeMap<String, String>,
    context: Value,
    directories: Vec<Pin>,
}

impl Provenance {
    /// Import an explicitly selected qualification receipt, never discover or
    /// self-qualify a launcher. The digest must come from the owner's independent
    /// qualification. Its strict-inspection snapshot is an exact CAS precondition,
    /// not a claim that the current entry still equals that snapshot.
    pub(super) fn from_companion_manifest(
        path: &Path,
        digest: &str,
        profile: &str,
        locator: &Path,
        remove: bool,
    ) -> Result<super::consent::RegistrationConsent, String> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Companion {
            version: u32,
            editor_sha256: String,
            launch: LaunchSpec,
            inspection: Value,
        }
        literal(path)?;
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Err("Requires an explicit lowercase SHA256 qualification digest.".into());
        }
        let manifest = Artifact {
            path: path.into(),
            sha256: digest.into(),
        };
        Pin::artifact(&manifest)?;
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|_| "Cannot open companion manifest.")?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Cannot read companion manifest.")?;
        if bytes.len() > 1024 * 1024 || format!("{:x}", Sha256::digest(&bytes)) != digest {
            return Err("Companion manifest changed or exceeds the bounded input.".into());
        }
        let mut companion: Companion =
            serde_json::from_slice(&bytes).map_err(|_| "Invalid qualified companion manifest.")?;
        if companion.version != 1
            || companion.launch.context["profile"].as_str() != Some(profile)
            || companion.inspection
                != serde_json::json!({"ok":true,"context":companion.launch.context,
                "present":companion.inspection["present"],"entry":companion.inspection["entry"]})
        {
            return Err(
                "Selected named profile or exact strict inspection does not match the manifest."
                    .into(),
            );
        }
        let editor = Artifact {
            path: std::env::current_exe().map_err(|_| "Cannot identify running editor.")?,
            sha256: companion.editor_sha256,
        };
        Pin::artifact(&editor)?;
        companion.launch.runtime.extend([manifest, editor]);
        let selection = serde_json::json!({"present":companion.inspection["present"],"entry":companion.inspection["entry"]});
        super::consent::RegistrationConsent::preview_bound(
            Self::freeze(companion.launch)?,
            "rusterize",
            locator,
            selection,
            remove,
        )
    }

    pub(super) fn freeze(spec: LaunchSpec) -> Result<Self, String> {
        if spec
            .environment
            .keys()
            .any(|key| !matches!(key.as_str(), "HOME" | "PATH" | "LANG" | "LC_ALL" | "TMPDIR"))
            || !spec.environment.contains_key("HOME")
            || !spec.environment.contains_key("PATH")
            || spec.environment.values().any(|v| v.contains('\0'))
            || spec.runtime.is_empty()
        {
            return Err("Requires explicit sanitized environment and runtime artifacts.".into());
        }
        let context = &spec.context;
        let profile = context["profile"]
            .as_str()
            .ok_or("Missing strict profile.")?;
        if profile == "default"
            || profile.is_empty()
            || profile.starts_with('-')
            || !profile
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
            || context["version"] != 1
            || context["backend"] != "file-raw-cas-v1"
            || context["runtime"]["python"].as_str() != spec.interpreter.path.to_str()
            || context["runtime"]["python_sha256"].as_str()
                != Some(spec.interpreter.sha256.as_str())
        {
            return Err("Unsupported strict profile/runtime context.".into());
        }
        let mut directories = Vec::new();
        for key in ["root", "profiles", "home"] {
            let identity = &context[key];
            let pin = Pin::capture(
                Path::new(identity["path"].as_str().ok_or("Missing strict path.")?),
                true,
            )?;
            if identity["device"].as_u64() != Some(pin.device)
                || identity["inode"].as_u64() != Some(pin.inode)
                || identity["owner"].as_u64() != Some(u64::from(pin.owner))
            {
                return Err("Strict directory context changed.".into());
            }
            directories.push(pin);
        }
        if directories[1].path != directories[0].path.join("profiles")
            || directories[2].path != directories[1].path.join(profile)
        {
            return Err("Strict profile hierarchy mismatch.".into());
        }
        // Hermes discovery legitimately secures its profile directory to 0700.
        // Freeze only an already-secured destination so this cannot first surface
        // as a provenance failure AFTER a successful configuration mutation.
        if directories[2].mode & 0o777 != 0o700 {
            return Err("Selected Hermes profile must already have private 0700 permissions before preview; no permissions were changed.".into());
        }
        let this = Self {
            launcher: Pin::artifact(&spec.launcher)?,
            interpreter: Pin::artifact(&spec.interpreter)?,
            build: Pin::artifact(&spec.build)?,
            adapter: Pin::artifact(&spec.adapter)?,
            runtime: spec
                .runtime
                .iter()
                .map(Pin::artifact)
                .collect::<Result<_, _>>()?,
            cwd: Pin::capture(&spec.cwd, true)?,
            environment: spec.environment,
            context: spec.context,
            directories,
        };
        this.validate_config()?;
        Ok(this)
    }

    fn validate_config(&self) -> Result<(), String> {
        let identity = &self.context["config"];
        let path = Path::new(identity["path"].as_str().ok_or("Missing strict config.")?);
        if path != self.directories[2].path.join("config.yaml") {
            return Err("Strict config destination mismatch.".into());
        }
        let pin = Pin::capture(path, false)?;
        // The strict API intentionally permits unrelated atomic config updates.
        if identity["device"].as_u64() != Some(pin.device)
            || identity["owner"].as_u64() != Some(u64::from(pin.owner))
        {
            return Err("Strict config identity changed.".into());
        }
        Ok(())
    }

    pub(super) fn launcher(&self) -> &Path {
        &self.launcher.path
    }
    pub(super) fn cwd(&self) -> &Path {
        &self.cwd.path
    }

    pub(super) fn environment(&self) -> &BTreeMap<String, String> {
        &self.environment
    }
    pub(super) fn profile(&self) -> &str {
        self.context["profile"].as_str().expect("validated")
    }
    pub(super) fn adapter(&self) -> &Path {
        &self.adapter.path
    }
    pub(super) fn context(&self) -> &Value {
        &self.context
    }
    pub(super) fn revalidate(&self) -> Result<(), String> {
        for pin in [
            &self.launcher,
            &self.interpreter,
            &self.build,
            &self.adapter,
        ]
        .into_iter()
        .chain(self.runtime.iter())
        .chain([&self.cwd])
        .chain(self.directories.iter())
        {
            if Pin::capture(&pin.path, pin.sha256.is_none())? != *pin {
                return Err(
                    "Invocation provenance changed; create a new preview. No retry.".into(),
                );
            }
        }
        self.validate_config()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn artifact(path: &std::path::Path, body: &[u8]) -> Artifact {
        std::fs::write(path, body).unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
        Artifact {
            path: path.into(),
            sha256: format!("{:x}", sha2::Sha256::digest(body)),
        }
    }

    pub(crate) fn fixture(root: &std::path::Path) -> LaunchSpec {
        let launcher = artifact(&root.join("launcher"), b"never execute");
        let interpreter = artifact(&root.join("python"), b"interpreter");
        let build = artifact(&root.join("build"), b"matched manifest");
        let adapter = artifact(&root.join("adapter"), b"matched adapter");
        let runtime = artifact(&root.join("runtime"), b"strict protocol");
        std::fs::create_dir(root.join("work")).unwrap();
        let home = root.join("profiles/named");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::write(home.join("config.yaml"), b"unrelated: unchanged").unwrap();
        let identity = |p: &std::path::Path, directory: bool| {
            let m = std::fs::metadata(p).unwrap();
            let mut v = json!({"path":p,"device":m.dev(),"owner":m.uid()});
            if directory {
                v["inode"] = json!(m.ino());
            }
            v
        };
        LaunchSpec {
            launcher,
            interpreter: interpreter.clone(),
            build,
            adapter,
            runtime: vec![runtime],
            cwd: root.join("work"),
            environment: [
                ("HOME".into(), root.to_string_lossy().into_owned()),
                ("PATH".into(), "/usr/bin:/bin".into()),
            ]
            .into(),
            context: json!({"version":1,"backend":"file-raw-cas-v1","profile":"named",
                "root":identity(root,true),"profiles":identity(&root.join("profiles"),true),
                "home":identity(&home,true),"config":identity(&home.join("config.yaml"),false),
                "runtime":{"python":interpreter.path,"python_sha256":interpreter.sha256}}),
        }
    }

    #[test]
    fn insecure_profile_is_rejected_before_preview_or_runtime_normalization() {
        let dir = tempfile::tempdir().unwrap();
        let spec = fixture(dir.path());
        let home = dir.path().join("profiles/named");
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Provenance::freeze(spec).is_err(), "Hermes discovery secures the profile to 0700; refuse BEFORE any mutation, not during readback");
        assert_eq!(std::fs::metadata(home).unwrap().mode() & 0o777, 0o755);
    }

    #[test]
    fn strict_worker_uses_frozen_environment_cwd_and_exact_raw_readback() {
        use super::super::{consent::RegistrationConsent, worker::Worker};
        for mismatch in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut spec = fixture(dir.path());
            let entry = json!({"command": spec.adapter.path, "enabled": true,
                "env": {"RUSTER_MCP_LOCATOR": dir.path().join("locator")}});
            let readback = json!({"ok":true,"context":spec.context,
                "present":true,"entry":if mismatch { Value::Null } else { entry }});
            let script = format!("#!/bin/sh\nprintf 'probe startup diagnostic\\n' >&2\n[ \"$PWD\" = '{}' ] || exit 9\n[ -z \"${{HERMES_HOME+x}}${{PYTHONPATH+x}}${{RUSTER_MCP_ENABLE+x}}\" ] || exit 8\n/bin/cat >/dev/null\nif [ \"$4\" = apply ]; then printf '%s' '{{\"ok\":true,\"changed\":true,\"tools\":84}}'; else printf '%s' '{}'; fi\n", spec.cwd.display(), readback);
            spec.launcher = artifact(&spec.launcher.path, script.as_bytes());
            let provenance = Provenance::freeze(spec).unwrap();
            let consent = RegistrationConsent::preview_bound(
                provenance,
                "rusterize",
                &dir.path().join("locator"),
                json!({"present":false,"entry":null}),
                false,
            )
            .unwrap();
            let worker = Worker::start_strict_candidate(consent).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let result = loop {
                if let Some(result) = worker.poll().unwrap() {
                    break result;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            assert_eq!(result.succeeded(), !mismatch);
        }
    }

    #[test]
    #[ignore = "explicit isolated strict-API/companion fixture; no installed runtime cutover"]
    fn actual_strict_cli_worker_create_reuse_conflict_remove() {
        use super::super::{consent::RegistrationConsent, worker::Worker};
        let manifest =
            std::env::var("E2A_STRICT_WORKER_FIXTURE").expect("explicit fixture required");
        let value: Value = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
        let pinned = |key: &str| Artifact {
            path: value[key]["path"].as_str().unwrap().into(),
            sha256: value[key]["sha256"].as_str().unwrap().into(),
        };
        let launcher = pinned("launcher");
        let environment: BTreeMap<String, String> =
            serde_json::from_value(value["environment"].clone()).unwrap();
        let cwd = PathBuf::from(value["cwd"].as_str().unwrap());
        let inspect = || {
            let output = std::process::Command::new(&launcher.path)
                .args([
                    "-p",
                    "selected",
                    "mcp",
                    "inspect",
                    "rusterize",
                    "--json",
                    "--strict-profile",
                ])
                .env_clear()
                .envs(&environment)
                .current_dir(&cwd)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "strict inspect failed: {:?}",
                output
            );
            serde_json::from_slice::<Value>(&output.stdout).unwrap()
        };
        let default = PathBuf::from(value["default_config"].as_str().unwrap());
        let before = std::fs::read(&default).unwrap();
        let prepare = |raw: &Value, remove| {
            let spec = LaunchSpec {
                launcher: launcher.clone(),
                interpreter: pinned("interpreter"),
                build: pinned("build"),
                adapter: pinned("adapter"),
                runtime: value["runtime"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| Artifact {
                        path: v["path"].as_str().unwrap().into(),
                        sha256: v["sha256"].as_str().unwrap().into(),
                    })
                    .collect(),
                cwd: cwd.clone(),
                environment: environment.clone(),
                context: raw["context"].clone(),
            };
            RegistrationConsent::preview_bound(
                Provenance::freeze(spec).unwrap(),
                "rusterize",
                Path::new(value["locator"].as_str().unwrap()),
                json!({"present":raw["present"],"entry":raw["entry"]}),
                remove,
            )
            .unwrap()
        };
        let wait = |consent| {
            let worker = Worker::start_strict_candidate(consent).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(100);
            loop {
                if let Some(result) = worker.poll().unwrap() {
                    break result;
                }
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        };
        let empty = inspect();
        assert_eq!(empty["present"], false);
        let stale = prepare(&empty, false);
        assert!(wait(prepare(&empty, false)).succeeded());
        assert!(wait(prepare(&inspect(), false)).succeeded());
        assert_eq!(
            wait(stale).status(),
            "Registration conflict; no refresh or retry"
        );
        assert!(wait(prepare(&inspect(), true)).succeeded());
        assert_eq!(inspect()["present"], false);
        assert_eq!(std::fs::read(default).unwrap(), before);
        assert!(!Path::new(value["locator"].as_str().unwrap()).exists());
    }

    #[test]
    fn preview_binds_strict_request_and_environment_without_native_effects() {
        use super::super::consent::RegistrationConsent;
        let dir = tempfile::tempdir().unwrap();
        let mut spec = fixture(dir.path());
        let original_env = spec.environment.clone();
        for key in [
            "PYTHONPATH",
            "HERMES_HOME",
            "HERMES_PROFILE",
            "RUSTER_MCP_ENABLE",
            "LD_PRELOAD",
        ] {
            spec.environment.insert(key.into(), "injection".into());
            assert!(
                Provenance::freeze(LaunchSpec {
                    launcher: spec.launcher.clone(),
                    interpreter: spec.interpreter.clone(),
                    build: spec.build.clone(),
                    adapter: spec.adapter.clone(),
                    runtime: spec.runtime.clone(),
                    cwd: spec.cwd.clone(),
                    environment: spec.environment.clone(),
                    context: spec.context.clone(),
                })
                .is_err(),
                "{key}"
            );
            spec.environment.remove(key);
        }
        let context = spec.context.clone();
        let provenance = Provenance::freeze(spec).unwrap();
        assert_eq!(provenance.environment(), &original_env);
        let selection = json!({"present":true,"entry":null});
        let mut consent = RegistrationConsent::preview_bound(
            provenance,
            "rusterize",
            &dir.path().join("locator"),
            selection.clone(),
            false,
        )
        .unwrap();
        let frozen = consent.invocation().provenance().unwrap();
        assert_eq!(frozen.environment(), &original_env);
        assert_eq!(frozen.launcher(), dir.path().join("launcher"));
        assert_eq!(frozen.cwd(), dir.path().join("work"));
        let request: Value = serde_json::from_str(consent.invocation().stdin()).unwrap();
        assert_eq!(request["context"], context);
        assert_eq!(request["expected"], selection);
        assert!(consent
            .invocation()
            .args()
            .iter()
            .any(|arg| arg == "--strict-profile"));
        // Changing a qualified launcher AFTER preview terminally consumes consent.
        std::fs::write(dir.path().join("launcher"), b"changed after preview").unwrap();
        assert!(consent.confirm().is_err());
        std::fs::write(dir.path().join("launcher"), b"never execute").unwrap();
        assert!(consent.confirm().is_err(), "no repair/retry of old consent");
        assert_eq!(
            consent.status(),
            "Registration failed or indeterminate; inspect before a new preview"
        );
        assert_eq!(
            std::fs::read(dir.path().join("profiles/named/config.yaml")).unwrap(),
            b"unrelated: unchanged"
        );
    }

    #[test]
    fn raw_selection_literal_paths_and_matched_build_are_preserved() {
        use super::super::consent::RegistrationConsent;
        let dir = tempfile::tempdir().unwrap();
        let mut spec = fixture(dir.path());
        let expected_adapter = spec.adapter.sha256.clone();
        spec.adapter.sha256 = "wrong matched build".into();
        assert!(Provenance::freeze(LaunchSpec {
            launcher: spec.launcher.clone(),
            interpreter: spec.interpreter.clone(),
            build: spec.build.clone(),
            adapter: spec.adapter.clone(),
            runtime: spec.runtime.clone(),
            cwd: spec.cwd.clone(),
            environment: spec.environment.clone(),
            context: spec.context.clone(),
        })
        .is_err());
        spec.adapter.sha256 = expected_adapter;
        let provenance = Provenance::freeze(spec).unwrap();
        for selection in [
            json!({"present":false,"entry":null}),
            json!({"present":true,"entry":null}),
            json!({"present":true,"entry":{"command":"${OLD_RAW_COMMAND}"}}),
        ] {
            let mut consent = RegistrationConsent::preview_bound(
                provenance.clone(),
                "rusterize",
                &dir.path().join("locator"),
                selection.clone(),
                false,
            )
            .unwrap();
            // An unrelated atomic config update is allowed, not a stale whole-file CAS.
            let replacement = dir.path().join("replacement.yaml");
            std::fs::write(&replacement, "unrelated: changed").unwrap();
            std::fs::rename(replacement, dir.path().join("profiles/named/config.yaml")).unwrap();
            let invocation = consent.confirm().unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(invocation.stdin()).unwrap()["expected"],
                selection
            );
            assert!(consent.confirm().is_err());
        }
        assert!(RegistrationConsent::preview_bound(
            provenance.clone(),
            "rusterize",
            &dir.path().join("${LOCATOR}"),
            json!({"present":false,"entry":null}),
            false
        )
        .is_err());
        for selection in [
            json!({"entry":null}),
            json!({"present":false,"entry":{}}),
            json!({"present":1,"entry":null}),
        ] {
            assert!(RegistrationConsent::preview_bound(
                provenance.clone(),
                "rusterize",
                &dir.path().join("locator"),
                selection,
                false
            )
            .is_err());
        }
        let consent = RegistrationConsent::preview_bound(
            provenance,
            "rusterize",
            &dir.path().join("locator"),
            json!({"present":false,"entry":null}),
            false,
        )
        .unwrap();
        assert!(
            super::super::worker::Worker::start(consent).is_err(),
            "even a bound capsule cannot promote execution"
        );
    }

    #[test]
    fn changed_provenance_is_terminal_before_any_dispatch() {
        for changed in [
            "launcher", "python", "build", "adapter", "runtime", "cwd", "profile",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let spec = fixture(dir.path());
            let capsule = Provenance::freeze(spec).unwrap();
            if changed == "cwd" || changed == "profile" {
                let p = if changed == "cwd" {
                    dir.path().join("work")
                } else {
                    dir.path().join("profiles/named")
                };
                // Retain the old inode: recreation cannot accidentally reuse it.
                std::fs::rename(&p, p.with_extension("old")).unwrap();
                std::fs::create_dir(&p).unwrap();
            } else {
                std::fs::write(dir.path().join(changed), b"replacement").unwrap();
            }
            assert!(capsule.revalidate().is_err(), "{changed}");
        }
    }
}
