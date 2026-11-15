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
    locator: PathBuf,
    binding: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Registry {
    projects: Vec<Project>,
}
impl Registry {
    pub fn parse(text: &str) -> io::Result<Self> {
        let value = strict_json::parse(text.as_bytes()).map_err(io::Error::other)?;
        let registry: Self = serde_json::from_value(value)?;
        if registry.projects.len() != 1 {
            return Err(io::Error::other(
                "exactly one explicitly registered project required",
            ));
        }
        let project = &registry.projects[0];
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
            || !project.locator.is_absolute()
            || !project.binding.as_object().is_some_and(|o| {
                exact_keys(o, &["wire", "incarnation", "generation", "connection"], &[])
            })
            || project.binding["wire"] != connection::WIRE
            || ["incarnation", "generation", "connection"].iter().any(|k| {
                !project.binding[k]
                    .as_str()
                    .is_some_and(|v| !v.is_empty() && v.len() <= 256)
            })
        {
            return Err(io::Error::other("invalid registered project identity"));
        }
        Ok(registry)
    }
    pub fn load(path: &Path) -> io::Result<Self> {
        connection::no_symlinks(path)?;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.mode() & 0o777 != 0o600
            || metadata.uid() != rustix::process::getuid().as_raw()
            || metadata.len() > 8192
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
        json!({"status":"native_browser_approval_required","url":descriptor["url"],"attachment":descriptor["attachment"],"message":"Open this secret-free workspace URL. Compare its pending browser code in Rusterize AI connection and approve that browser/project once. Cookies do not grant consent."})
    }

    pub fn open(&self) -> io::Result<connection::Client> {
        let client = connection::Client::open(self.locator.clone())?;
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
