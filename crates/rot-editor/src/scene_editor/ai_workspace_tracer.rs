//! Disposable headless owner: real startup/Host/Session/Unix transport/STDIO,
//! no window, renderer, native input, browser or human-acceptance claim.
use super::*;
use std::io::{BufRead, Write};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[test]
fn headless_native_fixture_owner() {
    if std::env::var_os("NATIVE_AI_FIXTURE_CHILD").is_none() {
        return;
    }
    let startup: crate::project_startup::Startup =
        serde_json::from_str(&std::env::var("ROT_PROJECT_STARTUP").unwrap()).unwrap();
    let bootstrap: crate::project_startup::Bootstrap =
        serde_json::from_str(&std::env::var("ROT_PROJECT_BOOTSTRAP").unwrap()).unwrap();
    bootstrap.validate().unwrap();
    let _owner = startup.lock("owner.lock").unwrap();
    let ctx = egui::Context::default();
    let mut host = Host {
        editor: SceneEditor::new(Session::open(&startup.scene_path).unwrap()),
        input: Default::default(),
        bridge: None,
        workspace: None,
        workspace_enabled: false,
    };
    // S-07c: beside an AI app the editor starts in the Side by Side layout.
    assert_eq!(std::env::var("ROT_WORKSPACE").as_deref(), Ok("ai"));
    host.editor.layout_override = Some(super::panels::LayoutKind::SideBySide);
    host.editor.save_grant =
        Some(persistence_commands::SaveGrant::new(startup.scene_path.parent().unwrap()).unwrap());
    assert!(host.editor.layout_override.is_some());
    host.connect_adapter(&ctx).unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline && !startup.state_directory.join("stop-fixture").exists() {
        host.input.observe(&egui::RawInput::default(), &ctx);
        host.logic_tick(&ctx);
        std::thread::sleep(Duration::from_millis(1));
    }
    host.disconnect_adapter();
    for _ in 0..100 {
        host.poll_connection(&ctx);
        if host.bridge.is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

struct Adapter {
    child: std::process::Child,
    output: std::io::BufReader<std::process::ChildStdout>,
}
impl Adapter {
    fn spawn(binary: &std::path::Path, registry: &std::path::Path) -> Self {
        let mut child = std::process::Command::new(binary)
            .env_remove("ROT_MCP_LOCATOR")
            .env("ROT_MCP_PROJECT_REGISTRY", registry)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let output = std::io::BufReader::new(child.stdout.take().unwrap());
        let mut adapter = Self { child, output };
        let init=adapter.request("initialize",json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"not-an-authentication-claim","version":"1"}}));
        assert!(init["result"].is_object());
        writeln!(
            adapter.child.stdin.as_mut().unwrap(),
            "{}",
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .unwrap();
        adapter
    }
    fn request(&mut self, method: &str, params: Value) -> Value {
        writeln!(
            self.child.stdin.as_mut().unwrap(),
            "{}",
            json!({"jsonrpc":"2.0","id":1,"method":method,"params":params})
        )
        .unwrap();
        self.child.stdin.as_mut().unwrap().flush().unwrap();
        let mut raw = String::new();
        self.output.read_line(&mut raw).unwrap();
        serde_json::from_str::<Value>(&raw).unwrap()
    }
    fn call(&mut self, name: &str, args: Value) -> Value {
        self.request("tools/call", json!({"name":name,"arguments":args}))["result"]
            ["structuredContent"]
            .clone()
    }
}
impl Drop for Adapter {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
struct FixtureRoots(Vec<PathBuf>);
impl Drop for FixtureRoots {
    fn drop(&mut self) {
        let owners: Vec<_> = self
            .0
            .iter()
            .filter_map(|root| {
                let p: Value =
                    serde_json::from_slice(&std::fs::read(root.join("native.json")).ok()?).ok()?;
                Some((
                    p["native_pid"].as_u64()? as u32,
                    p["process_start"].as_str()?.to_string(),
                    PathBuf::from(p["locator"].as_str()?),
                ))
            })
            .collect();
        for root in &self.0 {
            let _ = std::fs::write(root.join("stop-fixture"), "");
        }
        let deadline = Instant::now() + Duration::from_secs(4);
        for (pid, start, locator) in owners {
            while crate::project_startup::process_start(pid).ok().as_ref() == Some(&start)
                && Instant::now() < deadline
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            if !std::thread::panicking() {
                assert_ne!(
                    crate::project_startup::process_start(pid).ok().as_ref(),
                    Some(&start),
                    "owned headless native fixture failed to stop"
                );
                assert!(
                    !locator.exists() && !locator.with_file_name("editor.sock").exists(),
                    "owned transport cleanup incomplete"
                );
            }
        }
    }
}

#[test]
#[ignore = "explicit source-matched ROT_TEST_MCP_BINARY; disposable local transport, never native acceptance"]
fn trusted_cold_start_attach_reopen_switch_revoke_stdio() {
    let binary = std::fs::canonicalize(
        std::env::var_os("ROT_TEST_MCP_BINARY").expect("source-matched adapter"),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut roots = FixtureRoots(Vec::new());
    let executable = dir.path().join("headless-native");
    let test_binary = std::env::current_exe().unwrap();
    let fixture_log =
        std::env::var("NATIVE_AI_FIXTURE_LOG").expect("audit-local fixture diagnostic path");
    std::fs::write(&executable,format!("#!/bin/sh\nexport NATIVE_AI_FIXTURE_CHILD=1\nexec '{}' --exact scene_editor::ai_workspace_tracer::headless_native_fixture_owner --nocapture >> '{}' 2>&1\n",test_binary.display(),fixture_log)).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    let enrollment = dir.path().join("enrollment.json");
    std::fs::write(&enrollment,json!({"version":1,"connector_id":"disposable-stdio","executable":binary,"binary_identity":crate::project_startup::BinaryIdentity::read(&binary).unwrap(),"secret":crate::connection::unpredictable().unwrap(),"generation":1,"projects":[{"id":"one","generation":1,"allowed":true},{"id":"two","generation":1,"allowed":true}]}).to_string()).unwrap();
    std::fs::set_permissions(&enrollment, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut projects = Vec::new();
    for id in ["one", "two"] {
        let root = dir.path().join(id);
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        roots.0.push(root.clone());
        let scene = root.join("scene.json");
        Session::new(Scene::empty("same-scene"))
            .unwrap()
            .save(&scene)
            .unwrap();
        for name in ["owner.lock", "startup.lock"] {
            std::fs::write(root.join(name), "").unwrap();
            std::fs::set_permissions(root.join(name), std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }
        projects.push(json!({"id":id,"name":format!("Project {id}"),"scene_id":"same-scene","enrollment":enrollment,"startup":{"scene_path":scene,"native_binary":executable,"binary_identity":crate::project_startup::BinaryIdentity::read(&executable).unwrap(),"state_directory":root,"routing_identity":{"directory_dev":std::fs::metadata(&root).unwrap().dev(),"directory_ino":std::fs::metadata(&root).unwrap().ino(),"owner_ino":std::fs::metadata(root.join("owner.lock")).unwrap().ino(),"startup_ino":std::fs::metadata(root.join("startup.lock")).unwrap().ino()}}}));
    }
    let registry = dir.path().join("registry.json");
    std::fs::write(&registry, json!({"projects":projects}).to_string()).unwrap();
    std::fs::set_permissions(&registry, std::fs::Permissions::from_mode(0o600)).unwrap();
    let mut adapter = Adapter::spawn(&binary, &registry);
    assert_eq!(
        adapter.call("project_list", json!({}))["result"]["projects"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!roots.0[0].join("native.json").exists());
    let connected = adapter.call("project_connect", json!({"project":"one"}));
    assert_eq!(connected["ok"], true, "{connected}");
    assert_eq!(
        connected["result"]["workspace_handoff"]["status"],
        "workspace_unavailable"
    );
    let first = connected["result"]["connection"].clone();
    let publication: Value =
        serde_json::from_slice(&std::fs::read(roots.0[0].join("native.json")).unwrap()).unwrap();
    let pid = publication["native_pid"].clone();
    let repeated = adapter.call("project_connect", json!({"project":"one"}));
    assert_eq!(repeated["result"]["connection"], first);
    let inspected = adapter.call("scene_inspect", json!({"connection":first}));
    assert_eq!(inspected["ok"], true);
    let created=adapter.call("scene_create",json!({"connection":first,"expected_session_token":inspected["result"]["session_token"],"expected_revision":inspected["result"]["revision"],"object_id":"same-object","name":"Created through STDIO"}));
    assert_eq!(created["ok"], true, "{created}");
    let changed = adapter.call("scene_inspect", json!({"connection":first}));
    assert_eq!(changed["result"]["revision"], "1");
    let undone=adapter.call("scene_undo",json!({"connection":first,"expected_session_token":changed["result"]["session_token"],"expected_revision":"1"}));
    assert_eq!(undone["ok"], true, "{undone}");
    let persistence = adapter.call("scene_persistence_inspect", json!({"connection":first}));
    let saved=adapter.call("scene_save",json!({"connection":first,"expected_session_token":changed["result"]["session_token"],"expected_revision":"2","expected_persistence_token":persistence["result"]["persistence_token"]}));
    assert_eq!(saved["ok"], true, "{saved}");
    let fresh = Session::open(roots.0[0].join("scene.json")).unwrap();
    assert!(fresh.scene().objects.is_empty());
    assert_eq!(fresh.scene().revision, 2);
    assert_eq!(
        adapter.call("project_connect", json!({"project":"two"}))["error"]["code"],
        "project_pinned"
    );
    assert_eq!(
        adapter.call("project_disconnect", json!({"connection":first}))["ok"],
        true
    );
    assert_eq!(
        adapter.call("scene_inspect", json!({"connection":first}))["error"]["code"],
        "project_required"
    );
    let attached = adapter.call("project_connect", json!({"project":"one"}));
    assert_eq!(attached["ok"], true, "{attached}");
    let second = attached["result"]["connection"].clone();
    assert_ne!(first, second);
    let publication: Value =
        serde_json::from_slice(&std::fs::read(roots.0[0].join("native.json")).unwrap()).unwrap();
    assert_eq!(publication["native_pid"], pid, "must attach original owner");
    assert_eq!(
        adapter.call("project_disconnect", json!({"connection":second}))["ok"],
        true
    );
    // A closed native owner must be restartable by this same adapter, without
    // a manual MCP restart. Stop only the disposable cooperating fixture.
    std::fs::write(roots.0[0].join("stop-fixture"), "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while crate::project_startup::process_start(pid.as_u64().unwrap() as u32).is_ok()
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::remove_file(roots.0[0].join("stop-fixture")).unwrap();
    let reopened = adapter.call("project_connect", json!({"project":"one"}));
    assert_eq!(reopened["ok"], true, "{reopened}");
    let third = reopened["result"]["connection"].clone();
    let live_publication: Value =
        serde_json::from_slice(&std::fs::read(roots.0[0].join("native.json")).unwrap()).unwrap();
    let live_pid = live_publication["native_pid"].as_u64().unwrap() as u32;
    std::fs::write(roots.0[0].join("stop-fixture"), "").unwrap();
    let deadline = Instant::now() + Duration::from_secs(4);
    while crate::project_startup::process_start(live_pid).is_ok() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    std::fs::remove_file(roots.0[0].join("stop-fixture")).unwrap();
    let direct_reopen = adapter.call("project_connect", json!({"project":"one"}));
    assert_eq!(
        direct_reopen["ok"], true,
        "explicit connect after native close must not require manual MCP restart: {direct_reopen}"
    );
    let fourth = direct_reopen["result"]["connection"].clone();
    assert_ne!(fourth, third);
    assert_eq!(
        adapter.call("project_disconnect", json!({"connection":fourth}))["ok"],
        true
    );
    let other = adapter.call("project_connect", json!({"project":"two"}));
    assert_eq!(other["ok"], true, "{other}");
    let current = other["result"]["connection"].clone();
    assert_eq!(
        adapter.call("scene_inspect", json!({}))["error"]["code"],
        "connection_required"
    );
    let old=adapter.call("scene_create",json!({"connection":first,"expected_session_token":other["result"]["session_token"],"expected_revision":"0","object_id":"same-object","name":"Must never land in two"}));
    assert_eq!(old["error"]["code"], "connection_changed");
    assert_eq!(
        adapter.call("scene_inspect", json!({"connection":current}))["result"]["revision"],
        "0"
    );
    let mut parallel = Adapter::spawn(&binary, &registry);
    let parallel_connected = parallel.call("project_connect", json!({"project":"one"}));
    assert_eq!(parallel_connected["ok"], true, "{parallel_connected}");
    let parallel_binding = parallel_connected["result"]["connection"].clone();
    crate::enrollment::Grant::load(&enrollment, "two")
        .unwrap()
        .revoke(true)
        .unwrap();
    assert_ne!(
        parallel.call("scene_inspect", json!({"connection":parallel_binding}))["ok"],
        true,
        "all-project revoke must stop another live native owner"
    );
    drop(parallel);
    assert_ne!(
        adapter.call("scene_inspect", json!({"connection":current}))["ok"],
        true
    );
    assert!(crate::enrollment::Grant::load(&enrollment, "one").is_err());
    drop(adapter);
    drop(roots);
}
