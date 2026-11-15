//! Explicitly registered routing, never filesystem discovery or access consent.
use super::*;
use serde::Deserialize;
use std::{
    fs, io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Project {
    pub id: String,
    pub name: String,
    pub scene_id: String,
    #[serde(default)]
    pub enrollment: Option<PathBuf>,
    #[serde(default)]
    locator: PathBuf,
    #[serde(default)]
    binding: Value,
    #[serde(default)]
    startup: Option<crate::project_startup::Startup>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Registry {
    projects: Vec<Project>,
}
#[cfg(test)]
#[test]
fn provisioned_registry_and_enrollment_are_accepted_by_production_readers() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    let registry = root.path().join("input.json");
    let output = root.path().join("output");
    let exe = std::fs::read_link("/proc/self/exe").unwrap();
    std::fs::write(&registry, json!({"projects":[{"id":"one","name":"One","scene_id":"scene","locator":"/fixture/connection.json","binding":{"wire":connection::WIRE,"incarnation":"i","generation":"g","connection":"c"}}, {"id":"two","name":"Two","scene_id":"scene-two","startup":{"scene_path":"/fixture/scene.ruster","native_binary":exe,"state_directory":"/fixture/state","binary_identity":crate::project_startup::BinaryIdentity::read(&exe).unwrap(),"routing_identity":{"directory_dev":0,"directory_ino":0,"owner_ino":0,"startup_ino":0}}}]}).to_string()).unwrap();
    std::fs::set_permissions(&registry, std::fs::Permissions::from_mode(0o600)).unwrap();
    let result = std::process::Command::new("python3")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/enroll_local_connector.py"
        ))
        .args([
            "--registry",
            registry.to_str().unwrap(),
            "--adapter",
            exe.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
            "--connector-id",
            "é",
            "--project",
            "one",
        ])
        .output()
        .unwrap();
    assert!(result.status.success(), "provisioner failed");
    let loaded = Registry::load(&output.join("registry.json")).unwrap();
    let project = loaded.resolve("one").unwrap();
    assert!(
        crate::enrollment::Grant::load(project.enrollment.as_ref().unwrap(), "one")
            .unwrap()
            .live()
    );
}

impl Registry {
    pub fn parse(text: &str) -> io::Result<Self> {
        let value = strict_json::parse(text.as_bytes()).map_err(io::Error::other)?;
        let registry: Self = serde_json::from_value(value)?;
        if registry.projects.is_empty() || registry.projects.len() > 32 {
            return Err(io::Error::other(
                "1..=32 explicitly registered projects required",
            ));
        }
        for (index, project) in registry.projects.iter().enumerate() {
            if registry.projects[..index].iter().any(|p| {
                p.id == project.id
                    || p.name == project.name
                    || p.id == project.name
                    || p.name == project.id
            }) {
                return Err(io::Error::other("ambiguous registered project identity"));
            }
            if project.id.is_empty()
                || project.id.len() > 128
                || !project
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || project.name.is_empty()
                || project.name.len() > 128
                || project.name.chars().any(char::is_control)
                || project.scene_id.is_empty()
                || project.scene_id.len() > 128
                || (project.startup.is_none()
                    && (!project.locator.is_absolute()
                        || !project.binding.as_object().is_some_and(|o| {
                            exact_keys(o, &["wire", "incarnation", "generation", "connection"], &[])
                        })
                        || project.binding["wire"] != connection::WIRE
                        || ["incarnation", "generation", "connection"].iter().any(|k| {
                            !project.binding[k]
                                .as_str()
                                .is_some_and(|v| !v.is_empty() && v.len() <= 256)
                        })))
                || project.startup.as_ref().is_some_and(|s| {
                    !s.scene_path.is_absolute()
                        || !s.native_binary.is_absolute()
                        || !s.state_directory.is_absolute()
                        || !project.locator.as_os_str().is_empty()
                        || !project.binding.is_null()
                })
            {
                return Err(io::Error::other("invalid registered project identity"));
            }
        }
        Ok(registry)
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        connection::no_symlinks(path)?;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.mode() & 0o777 != 0o600
            || metadata.uid() != rustix::process::getuid().as_raw()
            || metadata.len() > 262144
        {
            return Err(io::Error::other(
                "private bounded project registry required",
            ));
        }
        Self::parse(&fs::read_to_string(path)?)
    }
    pub fn resolve(&self, exact: &str) -> io::Result<&Project> {
        self.projects
            .iter()
            .find(|p| p.id == exact || p.name == exact)
            .ok_or_else(|| io::Error::other("exact registered project not found"))
    }
    pub fn list(&self) -> Value {
        json!({"ok":true,"result":{"projects":self.projects.iter().map(|p| json!({"id":p.id,"name":p.name,"scene_id":p.scene_id})).collect::<Vec<_>>(),"selection_is_consent":false}})
    }
}
impl Project {
    pub fn workspace_handoff(
        &self,
        descriptor: &Value,
        inspected: &Value,
        native_pid: u32,
    ) -> Value {
        let valid_url = descriptor["url"]
            .as_str()
            .and_then(|s| s.strip_prefix("http://127.0.0.1:"))
            .and_then(|s| s.strip_suffix('/'))
            .is_some_and(|port| {
                !port.is_empty()
                    && port.bytes().all(|b| b.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|n| n != 0)
            });
        if !valid_url
            || descriptor["binding"] != self.binding
            || descriptor["scene_id"] != self.scene_id
            || descriptor["session_token"] != inspected["session_token"]
            || descriptor["native_pid"].as_u64() != Some(native_pid as u64)
            || descriptor["peer_pid"].as_u64().is_none_or(|n| n == 0)
            || !descriptor["attachment"]
                .as_str()
                .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return json!({"status":"native_descriptor_mismatch","message":"Live native workspace identity disagrees with the registered project; no URL published."});
        }
        json!({"status":"native_browser_approval_required","url":descriptor["url"],"attachment":descriptor["attachment"],"message":"Optional browser preview only; native connection is already complete. If requested, open this secret-free URL and explicitly approve its pending browser/project in native. Cookies do not grant browser consent."})
    }

    pub fn prepare(&mut self, child_pid: &mut Option<u32>) -> io::Result<()> {
        let Some(startup) = &self.startup else {
            return Ok(());
        };
        startup.validate()?;
        if let Some(path) = &self.enrollment {
            crate::enrollment::Grant::load(path, &self.id)?;
        }
        let _start = startup.lock("startup.lock")?;
        let owner = match startup.lock("owner.lock") {
            Ok(owner) => Some(owner),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => None,
            Err(error) => return Err(error),
        };
        if let Some(owner) = owner {
            drop(owner);
            // No registered native owner: only this explicitly pinned launch is allowed.
            let mut command = std::process::Command::new(&startup.native_binary);
            for (key, _) in std::env::vars_os() {
                if key.to_string_lossy().starts_with("RUSTER_")
                    && key != "RUSTER_EDITOR_STATE"
                    && key != "RUSTER_RECOVERY_DIR"
                {
                    command.env_remove(key);
                }
            }
            command
                .args([
                    std::ffi::OsStr::new("--scene"),
                    startup.scene_path.as_os_str(),
                ])
                .env("RUSTER_MCP_ENABLE", "1")
                .env("RUSTER_PROJECT_STARTUP", serde_json::to_string(startup)?)
                .env("RUSTER_PROJECT_ID", &self.id)
                .env("RUSTER_PROJECT_SCENE_ID", &self.scene_id)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            if let Some(path) = &self.enrollment {
                command
                    .env("RUSTER_CONNECTOR_ENROLLMENT", path)
                    .env("RUSTER_WORKSPACE", "ai");
            }
            command.env(
                "RUSTER_PROJECT_BOOTSTRAP",
                serde_json::to_string(&crate::project_startup::Bootstrap::capture()?)?,
            );
            let mut child = command.spawn()?;
            *child_pid = Some(child.id());
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
            loop {
                if child.try_wait()?.is_some() {
                    return Err(io::Error::other("registered native startup failed"));
                }
                if let Ok(publication) = startup.read(&self.id, &self.scene_id) {
                    if publication.native_pid == child.id() {
                        self.locator = publication.locator;
                        self.binding = publication.binding;
                        std::thread::spawn(move || {
                            let _ = child.wait();
                        });
                        return Ok(());
                    }
                    // An old publication never becomes the launched owner.
                }
                if std::time::Instant::now() >= deadline {
                    // Do not kill an editor that might contain human work. Retain its
                    // owner lock and report failure; never spawn a replacement.
                    std::thread::spawn(move || {
                        let _ = child.wait();
                    });
                    return Err(io::Error::other("native startup pending/failed; inspect the registered native editor; no duplicate launched"));
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        }
        // A live registered owner, including a disconnected/revoked one, must
        // never be replaced. Its current publication is only routing metadata.
        let deadline = std::time::Instant::now() + crate::connection::DEADLINE;
        let publication = loop {
            match startup.read(&self.id,&self.scene_id) {
                Ok(p)=>break p,
                Err(_) if self.enrollment.is_some() && std::time::Instant::now()<deadline=>std::thread::sleep(crate::connection::POLL),
                Err(_)=>return Err(io::Error::other("existing registered owner unavailable or needs upgrade/manual Connect; no duplicate launched")),
            }
        };
        self.locator = publication.locator;
        self.binding = publication.binding;
        Ok(())
    }

    pub fn open(&self) -> io::Result<connection::Client> {
        let mut client = connection::Client::open(self.locator.clone())?;
        if let Some(path) = &self.enrollment {
            client.enroll(crate::enrollment::Grant::load(path, &self.id)?.proof()?);
        }
        if client.binding() != self.binding {
            return Err(io::Error::other(
                "registered incarnation changed; explicit registration required",
            ));
        }
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_projects_require_unique_unambiguous_selection() {
        let one = json!({"id":"one","name":"One","scene_id":"same-scene","locator":"/private/one.json","binding":{"wire":connection::WIRE,"incarnation":"i","generation":"g","connection":"c"}});
        let mut two = one.clone();
        two["id"] = json!("two");
        two["name"] = json!("Two");
        let registry =
            Registry::parse(&json!({"projects":[one.clone(),two.clone()]}).to_string()).unwrap();
        assert_eq!(registry.resolve("Two").unwrap().id, "two");
        two["name"] = json!("one");
        assert!(Registry::parse(&json!({"projects":[one.clone(),two]}).to_string()).is_err());
        let projects: Vec<_> = (0..33)
            .map(|i| {
                let mut p = one.clone();
                p["id"] = json!(format!("p{i}"));
                p["name"] = json!(format!("Project {i}"));
                p
            })
            .collect();
        assert!(Registry::parse(&json!({"projects":projects}).to_string()).is_err());
    }

    #[test]
    fn private_registration_pins_native_binding_without_opening_owner() {
        use std::os::unix::{fs::PermissionsExt, net::UnixListener};
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.path().join("editor.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600)).unwrap();
        let locator =
            connection::Locator::capture(&socket, "incarnation".into(), "generation".into())
                .unwrap();
        let locator_path = dir.path().join("connection.json");
        fs::write(&locator_path, serde_json::to_vec(&locator).unwrap()).unwrap();
        fs::set_permissions(&locator_path, fs::Permissions::from_mode(0o600)).unwrap();
        let path = dir.path().join("registry.json");
        let mut value = json!({"projects":[{"id":"one","name":"One","scene_id":"scene-one","locator":locator_path,"binding":locator.binding()}]});
        fs::write(&path, value.to_string()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let registry = Registry::load(&path).unwrap();
        assert!(registry.resolve("One").unwrap().open().is_ok());
        assert!(
            listener.accept().is_err(),
            "routing snapshot cannot consume an owner"
        );
        value["projects"][0]["binding"]["incarnation"] = json!("replacement");
        fs::write(&path, value.to_string()).unwrap();
        assert!(
            registry.resolve("One").unwrap().open().is_ok(),
            "loaded registration is immutable"
        );
        assert!(Registry::load(&path)
            .unwrap()
            .resolve("One")
            .unwrap()
            .open()
            .is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(Registry::load(&path).is_err());
        let link = dir.path().join("registry-link.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(Registry::load(&link).is_err());
        assert!(Registry::parse("{\"projects\":[],\"projects\":[]}").is_err());
        assert!(Registry::parse(
            &json!({"projects":[value["projects"][0].clone(),value["projects"][0].clone()]})
                .to_string()
        )
        .is_err());
    }

    #[test]
    fn live_workspace_handoff_rejects_registry_or_native_identity_disagreement() {
        let record = json!({"id":"one","name":"One","scene_id":"scene","locator":"/private/connection.json","binding":{"wire":connection::WIRE,"incarnation":"i","generation":"g","connection":"c"}});
        let registry = Registry::parse(&json!({"projects":[record.clone()]}).to_string()).unwrap();
        let project = registry.resolve("One").unwrap();
        let inspected = json!({"scene_id":"scene","session_token":"native-session"});
        let descriptor = json!({"url":"http://127.0.0.1:34567/","scene_id":"scene","session_token":"native-session","binding":record["binding"],"native_pid":42,"peer_pid":43,"attachment":"a".repeat(64)});
        assert_eq!(
            project.workspace_handoff(&descriptor, &inspected, 42)["url"],
            descriptor["url"]
        );
        for (field, value) in [
            ("url", json!("http://127.0.0.1:34567/?secret=x")),
            ("scene_id", json!("other")),
            ("session_token", json!("old")),
            ("native_pid", json!(43)),
            ("binding", json!({})),
        ] {
            let mut wrong = descriptor.clone();
            wrong[field] = value;
            assert_eq!(
                project.workspace_handoff(&wrong, &inspected, 42)["status"],
                "native_descriptor_mismatch",
                "{wrong}"
            );
        }
    }

    #[test]
    fn registered_saved_project_needs_no_per_run_locator() {
        let record = json!({"id":"one","name":"One","scene_id":"scene-one","startup":{"scene_path":"/private/one.scene.json","native_binary":"/private/ruster-editor","state_directory":"/private/routing","binary_identity":{"dev":1,"ino":2,"len":3,"mtime":4,"mtime_nsec":5},"routing_identity":{"directory_dev":1,"directory_ino":2,"owner_ino":3,"startup_ino":4}}});
        let registry = Registry::parse(&json!({"projects":[record]}).to_string());
        assert!(
            registry.is_ok(),
            "saved-project registration must not require a per-run locator"
        );
    }

    #[test]
    fn cancelled_or_expired_bootstrap_cannot_publish_a_native_owner() {
        let valid = crate::project_startup::Bootstrap::capture().unwrap();
        assert!(valid.validate().is_ok());
        let mut invalid = serde_json::to_value(&valid).unwrap();
        invalid["process_start"] = json!("not-this-adapter");
        let invalid: crate::project_startup::Bootstrap = serde_json::from_value(invalid).unwrap();
        assert!(invalid.validate().is_err());
        let mut expired = serde_json::to_value(&valid).unwrap();
        expired["deadline_ms"] = json!(0);
        let expired: crate::project_startup::Bootstrap = serde_json::from_value(expired).unwrap();
        assert!(expired.validate().is_err());
    }

    #[test]
    fn routing_lock_replacement_cannot_launch_a_second_owner() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let binary = root.path().join("native");
        fs::write(&binary, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
        let scene = root.path().join("scene.json");
        fs::write(&scene, "{}").unwrap();
        let mut raw = json!({"scene_path":scene,"native_binary":binary,"state_directory":root.path(),"binary_identity":crate::project_startup::BinaryIdentity::read(&binary).unwrap()});
        // Registration pins both routing lock files, not just a replaceable path.
        for name in ["owner.lock", "startup.lock"] {
            fs::write(root.path().join(name), "").unwrap();
            fs::set_permissions(root.path().join(name), fs::Permissions::from_mode(0o600)).unwrap();
        }
        raw["routing_identity"] = json!({"directory_dev":fs::metadata(root.path()).unwrap().dev(),"directory_ino":fs::metadata(root.path()).unwrap().ino(),"owner_ino":fs::metadata(root.path().join("owner.lock")).unwrap().ino(),"startup_ino":fs::metadata(root.path().join("startup.lock")).unwrap().ino()});
        let startup: crate::project_startup::Startup =
            serde_json::from_value(raw).expect("registered routing identities must be supported");
        let owner = startup.lock("owner.lock").unwrap();
        assert!(startup.lock("owner.lock").is_err());
        fs::rename(
            root.path().join("owner.lock"),
            root.path().join("original-owner.lock"),
        )
        .unwrap();
        fs::write(root.path().join("owner.lock"), "").unwrap();
        fs::set_permissions(
            root.path().join("owner.lock"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        assert!(
            startup.lock("owner.lock").is_err(),
            "replacement lock must not release the original native owner"
        );
        drop(owner);
    }

    #[test]
    fn exact_registered_project_is_selected_without_fuzzy_fallback() {
        let record = json!({"id":"disposable-one","name":"Disposable One","scene_id":"disposable-scene","locator":"/private/locator.json","binding":{"wire":"ruster-owner-1","incarnation":"i","generation":"g","connection":"c"}});
        let registry = Registry::parse(&json!({"projects":[record]}).to_string()).unwrap();
        assert_eq!(
            registry.resolve("Disposable One").unwrap().id,
            "disposable-one"
        );
        assert_eq!(
            registry.resolve("disposable-one").unwrap().id,
            "disposable-one"
        );
        for unknown in ["disposable", "Disposable", "", "/private/locator.json"] {
            assert!(registry.resolve(unknown).is_err());
        }
        assert!(Registry::parse(&json!({"projects":[]}).to_string()).is_err());
    }
}
