//! With no configuration, `rot-mcp` talks to the running editor it finds in
//! the discovery directory. The editor's approval step still decides access.
use super::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const NOT_RUNNING: &str = "Rot Engine is not running. Open the project in Rot Engine, or call project_connect with the project folder or scene file to start it.";

impl Adapter {
    pub(super) fn call_discovered(
        &mut self,
        directory: &Path,
        name: &str,
        arguments: &Value,
    ) -> Value {
        let cwd = std::env::current_dir().ok();
        match name {
            "project_list" => {
                let running = discovery::list(directory);
                let projects: Vec<_> = running
                    .iter()
                    .map(|editor| {
                        let mut entry = editor.describe();
                        entry["connected"] = json!(
                            self.pinned == Some(editor.pid)
                                && self.client.as_ref().is_some_and(|c| c.available())
                        );
                        entry
                    })
                    .collect();
                json!({"ok":true,"result":{"projects":projects}})
            }
            "project_connect" => {
                let target = arguments["project"].as_str().unwrap_or("");
                let running = discovery::list(directory);
                let found = discovery::find(&running, target, cwd.as_deref());
                let editor = match found.as_slice() {
                    [one] => one.clone(),
                    [] => match launch(directory, target, cwd.as_deref()) {
                        Ok(editor) => editor,
                        Err(error) => return error,
                    },
                    several => return several_running("project_ambiguous", several),
                };
                if let Err(error) = self.select(&editor) {
                    return error;
                }
                let mut observed = self
                    .client
                    .as_mut()
                    .unwrap()
                    .call("scene_inspect", &json!({}));
                if observed["ok"] == true {
                    observed["result"]["editor"] = editor.describe();
                    observed["result"]["connection"] = self.binding.clone();
                }
                observed
            }
            "project_disconnect" => {
                if let Some(client) = &mut self.client {
                    client.disconnect();
                }
                self.client = None;
                self.pinned = None;
                self.binding = Value::Null;
                json!({"ok":true,"result":{"disconnected":true}})
            }
            _ => {
                if let Some(binding) = arguments.get("connection") {
                    if binding != &self.binding {
                        return domain_error("connection_changed", "That connection is no longer current. Call project_connect again, or leave out connection.");
                    }
                }
                if let Err(error) = self.reach(directory, cwd.as_deref()) {
                    return error;
                }
                let mut native = arguments.clone();
                native.as_object_mut().unwrap().remove("connection");
                self.client.as_mut().unwrap().call(name, &native)
            }
        }
    }

    /// Keep the current editor connection, or find the editor to use.
    fn reach(&mut self, directory: &Path, cwd: Option<&Path>) -> Result<(), Value> {
        if self.client.as_ref().is_some_and(|c| c.available()) {
            return Ok(());
        }
        let running = discovery::list(directory);
        if let Some(pid) = self.pinned {
            let Some(editor) = running.iter().find(|r| r.pid == pid) else {
                self.pinned = None;
                self.client = None;
                return Err(domain_error("editor_closed", "The Rot Engine editor this app was connected to has closed. Call project_connect to choose another."));
            };
            return self.select(&editor.clone());
        }
        match discovery::choose(running, cwd) {
            discovery::Choice::One(editor) => self.select(&editor),
            discovery::Choice::None => Err(domain_error("rot_not_running", NOT_RUNNING)),
            discovery::Choice::Several(several) => {
                Err(several_running("editor_choice_required", &several))
            }
        }
    }

    fn select(&mut self, editor: &discovery::Running) -> Result<(), Value> {
        let current =
            self.pinned == Some(editor.pid) && self.client.as_ref().is_some_and(|c| c.available());
        if !current {
            let mut client = connection::Client::open(editor.record.clone()).map_err(|_| {
                domain_error("bridge_unavailable", "Rot Engine is still opening or another AI app is connected to it. Try again shortly.")
            })?;
            // Name the app and present its token, so an app the creator
            // allowed always connects without asking again.
            let name = self.client_name.clone().unwrap_or_else(|| "AI app".into());
            match trust::Store::token_for(&trust::path(), &name) {
                Ok(token) => client.identify(json!({"name":name,"token":token})),
                Err(error) => eprintln!("rot-mcp: cannot read trusted AI apps: {error}"),
            }
            self.client = Some(client);
        }
        if self.pinned != Some(editor.pid) || self.binding.is_null() {
            self.binding = json!({"project_id":editor.pid.to_string(),"epoch":connection::unpredictable().expect("local randomness")});
        }
        self.pinned = Some(editor.pid);
        Ok(())
    }
}

fn several_running(code: &str, several: &[discovery::Running]) -> Value {
    let mut error = domain_error(code, "Several Rot Engine editors are open. Call project_connect with one of these project names, paths or IDs.");
    error["error"]["projects"] = several.iter().map(discovery::Running::describe).collect();
    error
}

/// The scene file for a path: the file itself, or a project folder's
/// `scene.json`, or its only `*.scene.json`.
fn scene_file(path: &Path) -> Option<PathBuf> {
    if path.is_file() {
        return Some(path.to_path_buf());
    }
    let main = path.join("scene.json");
    if main.is_file() {
        return Some(main);
    }
    let mut scenes = std::fs::read_dir(path)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.ends_with(".scene.json"))
        });
    let only = scenes.next()?;
    scenes.next().is_none().then_some(only)
}

/// Start the editor on a scene and wait until it can be found.
fn launch(directory: &Path, target: &str, cwd: Option<&Path>) -> Result<discovery::Running, Value> {
    use std::os::unix::process::CommandExt;
    let path = Path::new(target);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.map(|c| c.join(path)).unwrap_or_default()
    };
    let Some(scene) = path.canonicalize().ok().and_then(|p| scene_file(&p)) else {
        return Err(domain_error("project_not_found", &format!("No open Rot Engine project matches \"{target}\", and it is not a scene file or a project folder with one scene. Call project_list to see open projects.")));
    };
    let editor = std::env::current_exe()
        .ok()
        .map(|exe| exe.with_file_name("rot-editor"))
        .filter(|exe| exe.is_file())
        .unwrap_or_else(|| PathBuf::from("rot-editor"));
    let mut command = std::process::Command::new(editor);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("ROT_MCP_") || key == "ROT_PROJECT_STARTUP" {
            command.env_remove(key);
        }
    }
    let mut child = command
        .arg("--scene")
        .arg(&scene)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|e| {
            domain_error(
                "editor_start_failed",
                &format!("Could not start Rot Engine: {e}"),
            )
        })?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(editor) = discovery::list(directory)
            .into_iter()
            .find(|r| r.pid == child.id())
        {
            std::thread::spawn(move || child.wait());
            return Ok(editor);
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(domain_error(
                "editor_start_failed",
                &format!(
                    "Rot Engine exited while opening {} ({status}).",
                    scene.display()
                ),
            ));
        }
        if Instant::now() >= deadline {
            std::thread::spawn(move || child.wait());
            return Err(domain_error(
                "editor_start_failed",
                "Rot Engine is still starting. Try again in a moment.",
            ));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
