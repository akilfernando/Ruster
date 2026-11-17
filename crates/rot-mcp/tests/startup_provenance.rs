//! Actual STDIO, with a private script that can only record its PID and exit.
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

struct Adapter {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    sequence: u32,
}
impl Adapter {
    fn start(registry: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_rot-mcp"))
            .env_remove("ROT_MCP_LOCATOR")
            .env("ROT_MCP_PROJECT_REGISTRY", registry)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut adapter = Self {
            child,
            input,
            output,
            sequence: 0,
        };
        adapter.request("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"headless-provenance","version":"1"}}));
        writeln!(
            adapter.input,
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        adapter
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.sequence += 1;
        writeln!(
            self.input,
            "{}",
            json!({"jsonrpc":"2.0","id":self.sequence,"method":method,"params":params})
        )
        .unwrap();
        self.input.flush().unwrap();
        let mut line = String::new();
        assert_ne!(self.output.read_line(&mut line).unwrap(), 0);
        let value: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(value["id"], self.sequence);
        value
    }
    fn call(&mut self, name: &str, arguments: Value) -> Value {
        self.request("tools/call", json!({"name":name,"arguments":arguments}))["result"]
            ["structuredContent"]
            .clone()
    }
    fn list(&mut self) -> Value {
        self.call("project_list", json!({}))["result"]["startup_diagnostics"].clone()
    }
}
impl Drop for Adapter {
    fn drop(&mut self) {
        // Only this fixture-owned adapter; never a native/editor process.
        self.child.kill().ok();
        self.child.wait().unwrap();
    }
}

#[test]
fn stdio_correlates_fresh_and_cached_failure_without_discovery_side_effects() {
    let root = tempfile::tempdir().unwrap();
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let binary = root.path().join("fake-native");
    let spawned = root.path().join("spawned");
    fs::write(
        &binary,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$$\" >> '{}'\nexit 1\n",
            spawned.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let scene = root.path().join("scene.json");
    fs::write(&scene, "{}").unwrap();
    for name in ["owner.lock", "startup.lock"] {
        fs::write(root.path().join(name), "").unwrap();
        fs::set_permissions(root.path().join(name), fs::Permissions::from_mode(0o600)).unwrap();
    }
    let m = fs::metadata(&binary).unwrap();
    let d = fs::metadata(root.path()).unwrap();
    let registry = root.path().join("registry.json");
    fs::write(&registry, json!({"projects":[{"id":"one","name":"One","scene_id":"scene-one","startup":{
        "scene_path":scene,"native_binary":binary,"state_directory":root.path(),
        "binary_identity":{"dev":m.dev(),"ino":m.ino(),"len":m.len(),"mtime":m.mtime(),"mtime_nsec":m.mtime_nsec()},
        "routing_identity":{"directory_dev":d.dev(),"directory_ino":d.ino(),"owner_ino":fs::metadata(root.path().join("owner.lock")).unwrap().ino(),"startup_ino":fs::metadata(root.path().join("startup.lock")).unwrap().ino()}
    }}]}).to_string()).unwrap();
    fs::set_permissions(&registry, fs::Permissions::from_mode(0o600)).unwrap();
    let mut a = Adapter::start(&registry);
    let initial = a.list();
    assert_eq!(initial["adapter_pid"], a.child.id());
    let process_start = fs::read_to_string(format!("/proc/{}/stat", a.child.id())).unwrap();
    let process_start = process_start
        .rsplit_once(") ")
        .unwrap()
        .1
        .split_whitespace()
        .nth(19)
        .unwrap();
    assert_eq!(initial["process_start"], process_start);
    assert_eq!(initial["prepare_attempted"], false);
    assert_eq!(initial["attempt_sequence"], 0);
    assert_eq!(initial["child_pid"], Value::Null);
    assert_eq!(initial["failure_cached"], false);
    assert_eq!(initial["response_source"], "snapshot");
    assert_eq!(
        initial.as_object().unwrap().len(),
        7,
        "bounded allowlist only"
    );
    assert!(!spawned.exists());
    assert_eq!(
        a.call("project_list", json!({"prepare":true}))["error"]["code"],
        "invalid_arguments"
    );
    assert_eq!(
        a.call("project_connect", json!({"project":"On"}))["error"]["code"],
        "project_not_found"
    );
    assert_eq!(a.list(), initial);

    let first = a.call("project_connect", json!({"project":"One"}));
    assert_eq!(first["error"]["code"], "project_startup_unavailable");
    assert_eq!(
        first["error"]["message"],
        "registered native startup failed"
    );
    let fresh = &first["error"]["startup_diagnostics"];
    assert_eq!(fresh["adapter_pid"], initial["adapter_pid"]);
    assert_eq!(fresh["process_start"], initial["process_start"]);
    assert_eq!(fresh["prepare_attempted"], true);
    assert_eq!(fresh["attempt_sequence"], 1);
    assert_eq!(fresh["response_source"], "fresh_failure");
    assert_eq!(fresh["failure_cached"], true);
    let pids = fs::read_to_string(&spawned).unwrap();
    assert_eq!(pids.lines().count(), 1);
    assert_eq!(
        fresh["child_pid"].as_u64(),
        Some(pids.trim().parse().unwrap())
    );
    let mut cached = fresh.clone();
    cached["response_source"] = json!("cached_failure");
    for name in ["project_connect", "scene_inspect"] {
        let reply = a.call(
            name,
            if name == "project_connect" {
                json!({"project":"one"})
            } else {
                json!({})
            },
        );
        assert_eq!(reply["error"]["startup_diagnostics"], cached);
    }
    let mut snapshot = fresh.clone();
    snapshot["response_source"] = json!("snapshot");
    assert_eq!(a.list(), snapshot);
    assert_eq!(fs::read_to_string(&spawned).unwrap(), pids);

    let mut b = Adapter::start(&registry);
    let unattempted = b.list();
    assert_ne!(unattempted["adapter_pid"], initial["adapter_pid"]);
    assert_eq!(unattempted["prepare_attempted"], false);
    assert_eq!(unattempted["attempt_sequence"], 0);
    assert_eq!(unattempted["child_pid"], Value::Null);
    assert_eq!(unattempted["failure_cached"], false);
    // Hold both real advisory locks: discovery cannot prepare or consume locks.
    let owner = fs::File::open(root.path().join("owner.lock")).unwrap();
    let startup = fs::File::open(root.path().join("startup.lock")).unwrap();
    owner.try_lock().unwrap();
    startup.try_lock().unwrap();
    // Discovery even survives absent native/scene paths: no project access.
    fs::rename(&scene, root.path().join("scene-hidden")).unwrap();
    fs::rename(&binary, root.path().join("native-hidden")).unwrap();
    assert_eq!(b.list(), unattempted);
    fs::rename(root.path().join("scene-hidden"), &scene).unwrap();
    fs::rename(root.path().join("native-hidden"), &binary).unwrap();
    drop(startup);
    let held = b.call("project_connect", json!({"project":"One"}));
    assert_eq!(held["error"]["code"], "project_startup_unavailable");
    assert!(held["error"]["message"]
        .as_str()
        .unwrap()
        .contains("no duplicate launched"));
    assert_eq!(
        held["error"]["startup_diagnostics"]["child_pid"],
        Value::Null
    );
    assert_eq!(held["error"]["startup_diagnostics"]["attempt_sequence"], 1);
    drop(owner);
    let repeated = b.call("project_connect", json!({"project":"One"}));
    assert_eq!(
        repeated["error"]["startup_diagnostics"]["response_source"],
        "cached_failure"
    );
    assert_eq!(
        fs::read_to_string(&spawned).unwrap(),
        pids,
        "owner release must not replay failed prepare"
    );
    eprintln!(
        "{}",
        json!({"adapter_a_initial":initial,"fresh_failure":fresh,"cached_failure":cached,"adapter_b_initial":unattempted,"held_owner_failure":held["error"]["startup_diagnostics"],"fake_native_spawn_count":pids.lines().count()})
    );
}
