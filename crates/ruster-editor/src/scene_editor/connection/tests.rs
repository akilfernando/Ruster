use super::*;
use crate::bridge::tests::{approved, frame, receive};
use std::os::unix::net::UnixStream;

pub(super) fn host(id: &str) -> Host {
    Host {
        editor: SceneEditor::new(Session::new(Scene::authored_empty(id)).unwrap()),
        input: Default::default(),
        bridge: None,
    }
}
fn tick(host: &mut Host, ctx: &egui::Context) {
    host.input.observe(&egui::RawInput::default(), ctx);
    host.logic_tick(ctx);
}
fn call(host: &mut Host, ctx: &egui::Context, stream: &UnixStream, request: Value) -> Value {
    let mut stream = stream.try_clone().unwrap();
    let worker = std::thread::spawn(move || {
        crate::connection::send(&mut stream, &request).unwrap();
        receive(&mut stream)
    });
    let deadline = Instant::now() + Duration::from_secs(4);
    while !worker.is_finished() {
        tick(host, ctx);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    worker.join().unwrap()
}
pub(super) fn retire(host: &mut Host, ctx: &egui::Context) {
    host.disconnect_adapter();
    let deadline = Instant::now() + Duration::from_secs(3);
    while host.bridge.is_some() {
        tick(host, ctx);
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(host.connection_state(), "retired");
}

#[test]
#[ignore = "bounded external Hermes qualification fixture; explicit CP1_FIXTURE_DIRECTORY required"]
fn bounded_hermes_owner_fixture() {
    let directory = PathBuf::from(
        std::env::var_os("CP1_FIXTURE_DIRECTORY").expect("explicit private fixture directory"),
    );
    assert!(directory.is_absolute() && directory.is_dir());
    let ctx = egui::Context::default();
    let mut host = host("cp1-hermes-disposable");
    let locator = host.connect_adapter(&ctx).unwrap();
    let state_path = directory.join("state.json");
    let mut previous = None;
    let deadline = Instant::now() + Duration::from_secs(240);
    loop {
        tick(&mut host, &ctx);
        let owner = host.pending_adapter();
        if previous != owner || !state_path.exists() {
            std::fs::write(&state_path, serde_json::to_vec(&json!({"locator":locator,"fixture_pid":std::process::id(),"owner":owner,"state":host.connection_state()})).unwrap()).unwrap();
            previous = owner;
        }
        let approval = directory.join("approve.json");
        if approval.exists() {
            let selected: (String, u32) =
                serde_json::from_slice(&std::fs::read(&approval).unwrap()).unwrap();
            let approved = host.approve_adapter(&selected);
            std::fs::remove_file(&approval).unwrap();
            std::fs::write(
                directory.join("approval-result.json"),
                json!({"approved":approved}).to_string(),
            )
            .unwrap();
        }
        if directory.join("stop").exists() || Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    retire(&mut host, &ctx);
    let receipt = json!({"state":host.connection_state(),"revision":host.editor.session.scene().revision,"objects":host.editor.session.scene().objects,"retained_terminal_receipts":host.editor.retired_connection_receipts.len(),"locator_absent":!locator.exists(),"fixture_completed":true});
    std::fs::write(
        directory.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    assert!(!locator.exists());
}

#[test]
fn cp2_local_status_keeps_disconnected_listening_and_revoked_distinct() {
    let ctx = egui::Context::default();
    let mut host = host("cp2-local-status");
    let authored = host.editor.session.snapshot_bytes().unwrap();
    let paint = |host: &mut Host| {
        let mut text = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            host.connection_status_ui(ui);
            host.binding_controls_ui(ui, &mut native::NativeUi::default());
        });
        for shape in &output.shapes {
            if let egui::epaint::Shape::Text(shape) = &shape.shape {
                text.push(shape.galley.job.text.clone());
            }
        }
        output.textures_delta.clear();
        text.join("\n")
    };
    let text = paint(&mut host);
    assert!(text.contains("Disconnected"));
    assert!(
        text.contains("Connect AI"),
        "native status must expose a supported Connect action: {text}"
    );
    assert!(!text.contains("Registered"));
    let locator = host.connect_adapter(&ctx).unwrap();
    let text = paint(&mut host);
    assert!(text.contains("Waiting for an adapter"));
    assert!(text.contains("Registration and client reload are separate"));
    assert!(!text.contains("Connected to this project"));
    let (owner, _) = approved(host.bridge.as_ref().unwrap());
    let text = paint(&mut host);
    assert!(text.contains("Connected to this project"));
    assert!(text.contains("Asset catalog changes can persist immediately, before Save."));
    assert!(!text.contains("Registered"));
    host.disconnect_adapter();
    assert!(paint(&mut host).contains("Authority revoked; retiring connection"));
    drop(owner);
    retire(&mut host, &ctx);
    assert!(paint(&mut host).contains("Disconnected"));
    assert!(!locator.exists());
    assert_eq!(host.editor.session.snapshot_bytes().unwrap(), authored);
}

#[test]
fn save_root_rebase_is_a_new_project_authority_generation() {
    let ctx = egui::Context::default();
    let mut host = host("save-root");
    host.connect_adapter(&ctx).unwrap();
    let (owner, _) = approved(host.bridge.as_ref().unwrap());
    let old = host.editor.connection_generation.clone();
    let authority = host.editor.connection_authority.clone().unwrap();
    let destination = tempfile::tempdir().unwrap();
    host.editor
        .save_scene(destination.path().join("saved.json"))
        .unwrap();
    assert!(
        !authority.live(),
        "root-rebased Save must not carry a binding into its new generation"
    );
    assert_ne!(host.editor.connection_generation, old);
    drop(owner);
    retire(&mut host, &ctx);
}

#[test]
fn real_hosts_never_disclose_or_adopt_across_canceled_transitions_and_restart() {
    let ctx = egui::Context::default();
    let mut a = host("project-a");
    let mut b = host("project-b");
    assert_eq!(a.connection_state(), "retired");
    let locator_a = a.connect_adapter(&ctx).unwrap();
    b.connect_adapter(&ctx).unwrap();
    let (owner_a, handshake_a) = approved(a.bridge.as_ref().unwrap());
    let (owner_b, handshake_b) = approved(b.bridge.as_ref().unwrap());
    let before_a = a.editor.session.snapshot_bytes().unwrap();
    assert!(
        !serde_json::to_string(&a.editor.recovery_snapshot().unwrap())
            .unwrap()
            .contains(handshake_a["capability"].as_str().unwrap())
    );
    assert!(!String::from_utf8(before_a.clone())
        .unwrap()
        .contains(handshake_a["capability"].as_str().unwrap()));
    let before_b = b.editor.session.snapshot_bytes().unwrap();
    for operation in ["scene_inspect", "scene_create"] {
        let wrong = frame(&handshake_a, operation, json!({}));
        let result = call(&mut b, &ctx, &owner_b, wrong);
        assert_eq!(result["error"]["code"], "unauthorized");
        assert!(result.get("result").is_none());
    }
    let inspected = call(
        &mut a,
        &ctx,
        &owner_a,
        frame(&handshake_a, "scene_inspect", json!({})),
    );
    assert_eq!(inspected["ok"], true, "{inspected}");
    assert!(inspected.to_string().contains("project-a"));
    assert!(!inspected.to_string().contains("project-b"));
    assert_eq!(a.editor.session.snapshot_bytes().unwrap(), before_a);
    assert_eq!(b.editor.session.snapshot_bytes().unwrap(), before_b);
    let inspected_b = call(
        &mut b,
        &ctx,
        &owner_b,
        frame(&handshake_b, "scene_inspect", json!({})),
    )["result"]
        .clone();
    let created = call(
        &mut b,
        &ctx,
        &owner_b,
        frame(
            &handshake_b,
            "scene_create",
            json!({"expected_session_token":inspected_b["session_token"],"expected_revision":inspected_b["revision"],"object_id":"owned-b","name":"Owned B"}),
        ),
    );
    assert_eq!(created["ok"], true, "{created}");
    assert_eq!(b.editor.session.scene().objects.len(), 1);
    assert!(a.editor.session.scene().objects.is_empty());
    a.editor.queued_action = Some(Action::Open);
    a.editor.process_queued_action();
    a.editor.dialog = None;
    assert!(!a.editor.connection_authority.as_ref().unwrap().live());
    retire(&mut a, &ctx);
    assert_eq!(a.editor.session.snapshot_bytes().unwrap(), before_a);
    assert!(crate::connection::Client::open(locator_a).is_err());
    let locator = a.connect_adapter(&ctx).unwrap();
    let (new_owner, new_handshake) = approved(a.bridge.as_ref().unwrap());
    assert_eq!(
        handshake_a["binding"]["incarnation"],
        new_handshake["binding"]["incarnation"]
    );
    assert_ne!(
        handshake_a["binding"]["connection"],
        new_handshake["binding"]["connection"]
    );
    // Keep a dirty document so cancellation is an actual no-replacement path.
    a.editor.object_action("create");
    let before_new = a.editor.session.snapshot_bytes().unwrap();
    a.editor.request_new();
    a.editor.new_scene_pending = false;
    retire(&mut a, &ctx);
    assert_eq!(a.editor.session.snapshot_bytes().unwrap(), before_new);
    assert!(crate::connection::Client::open(locator).is_err());
    a.connect_adapter(&ctx).unwrap();
    let (restore_owner, _) = approved(a.bridge.as_ref().unwrap());
    assert!(a.editor.restore_checkpoint("missing").is_err());
    retire(&mut a, &ctx);
    drop(owner_a);
    drop(owner_b);
    drop(new_owner);
    drop(restore_owner);
    retire(&mut b, &ctx);
    let restarted = host("project-a");
    assert_ne!(
        a.editor.connection_incarnation,
        restarted.editor.connection_incarnation
    );
    assert_ne!(
        a.editor.connection_generation,
        restarted.editor.connection_generation
    );
}
