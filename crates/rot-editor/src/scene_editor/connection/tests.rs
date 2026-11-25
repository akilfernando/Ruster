use super::*;
use crate::bridge::tests::{approved, frame, receive};
use rot_scene::component_scene::{ComponentScene, ComponentSession};
use std::ops::{Deref, DerefMut};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) struct TestHost {
    dir: Option<tempfile::TempDir>,
    host: Host,
}

impl Deref for TestHost {
    type Target = Host;
    fn deref(&self) -> &Self::Target {
        &self.host
    }
}

impl DerefMut for TestHost {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.host
    }
}

pub(super) fn host(id: &str) -> TestHost {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("scene.json");
    let mut scene = ComponentScene::from_legacy(
        &rot_scene::Session::new(rot_scene::Scene::authored_empty(id)).unwrap(),
    )
    .unwrap();
    scene.id = id.to_owned();
    scene.save_new(&path).unwrap();
    let session = ComponentSession::open(&path).unwrap();
    TestHost {
        dir: Some(dir),
        host: Host {
            editor: SceneEditor::with_component_for_test(path, session),
            input: Default::default(),
            bridge: None,
        },
    }
}

pub(super) fn host_at(scene_path: &Path) -> TestHost {
    let scene = ComponentScene::from_legacy(
        &rot_scene::Session::new(rot_scene::Scene::authored_empty("trust")).unwrap(),
    )
    .unwrap();
    scene.save_new(scene_path).unwrap();
    let session = ComponentSession::open(scene_path).unwrap();
    TestHost {
        dir: None,
        host: Host {
            editor: SceneEditor::with_component_for_test(scene_path.to_path_buf(), session),
            input: Default::default(),
            bridge: None,
        },
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

fn disk_bytes(host: &Host) -> Vec<u8> {
    let path = host.editor.component.as_ref().unwrap().ai_document().0;
    std::fs::read(path).unwrap()
}

#[test]
fn cp2_local_status_keeps_disconnected_listening_and_revoked_distinct() {
    let ctx = egui::Context::default();
    let mut host = host("cp2-local-status");
    let authored = disk_bytes(&host);
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
    assert!(text.contains("Not listening"));
    assert!(
        text.contains("Start Listening"),
        "native status must expose a supported Connect action: {text}"
    );
    assert!(!text.contains("Registered"));
    let locator = host.connect_adapter(&ctx).unwrap();
    let text = paint(&mut host);
    assert!(text.contains("Ready for AI apps"));
    assert!(text.contains("Open your AI app; it finds this editor by itself."));
    assert!(!text.contains("AI app connected"));
    let (owner, _) = approved(host.bridge.as_ref().unwrap());
    let text = paint(&mut host);
    assert!(text.contains("AI app connected"));
    assert!(text.contains("Scene edits share Undo."));
    assert!(!text.contains("Registered"));
    host.disconnect_adapter();
    assert!(paint(&mut host).contains("Disconnecting…"));
    drop(owner);
    retire(&mut host, &ctx);
    assert!(paint(&mut host).contains("Not listening"));
    assert!(!locator.exists());
    assert_eq!(disk_bytes(&host), authored);
}

#[test]
fn document_replacement_is_a_new_project_authority_generation() {
    let ctx = egui::Context::default();
    let mut host = host("save-root");
    host.connect_adapter(&ctx).unwrap();
    let (owner, _) = approved(host.bridge.as_ref().unwrap());
    let old = host.editor.connection_generation.clone();
    let authority = host.editor.connection_authority.clone().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let next_path = destination.path().join("other.scene.json");
    ComponentScene::from_legacy(
        &rot_scene::Session::new(rot_scene::Scene::authored_empty("other")).unwrap(),
    )
    .unwrap()
    .save_new(&next_path)
    .unwrap();
    host.editor
        .open_document(OpenedScene::open(&next_path).unwrap());
    assert!(
        !authority.live(),
        "opening another project must not carry a binding into its new generation"
    );
    drop(owner);
    retire(&mut host, &ctx);
    assert_ne!(host.editor.connection_generation, old);
}

#[test]
fn real_hosts_never_disclose_or_adopt_across_canceled_transitions_and_restart() {
    let ctx = egui::Context::default();
    let mut a = host("project-a");
    let mut b = host("project-b");
    let _hold_a = a.dir.as_ref().unwrap().path().to_path_buf();
    assert_eq!(a.connection_state(), "retired");
    let locator_a = a.connect_adapter(&ctx).unwrap();
    b.connect_adapter(&ctx).unwrap();
    let (owner_a, handshake_a) = approved(a.bridge.as_ref().unwrap());
    let (owner_b, handshake_b) = approved(b.bridge.as_ref().unwrap());
    let before_a = disk_bytes(&a);
    assert!(!String::from_utf8(before_a.clone())
        .unwrap()
        .contains(handshake_a["capability"].as_str().unwrap()));
    let before_b = disk_bytes(&b);
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
    assert_eq!(disk_bytes(&a), before_a);
    assert_eq!(disk_bytes(&b), before_b);
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
            json!({
                "expected_session_token": inspected_b["session_token"],
                "expected_revision": inspected_b["revision"],
                "object_id": "owned-b",
                "name": "Owned B"
            }),
        ),
    );
    assert_eq!(created["ok"], true, "{created}");
    let after_a = a
        .editor
        .component
        .as_mut()
        .unwrap()
        .ai_command("t", "scene_inspect", &json!({}));
    assert_eq!(after_a["result"]["revision"], "0");
    assert!(!after_a["result"]["objects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|object| object["id"] == "owned-b"));
    retire(&mut a, &ctx);
    assert_eq!(disk_bytes(&a), before_a);
    assert!(crate::connection::Client::open(locator_a).is_err());
    let _locator = a.connect_adapter(&ctx).unwrap();
    let (new_owner, new_handshake) = approved(a.bridge.as_ref().unwrap());
    assert_eq!(
        handshake_a["binding"]["incarnation"],
        new_handshake["binding"]["incarnation"]
    );
    assert_ne!(
        handshake_a["binding"]["connection"],
        new_handshake["binding"]["connection"]
    );
    assert!(a
        .editor
        .recover_component_checkpoint("missing".to_owned(), &ctx)
        .is_err());
    retire(&mut a, &ctx);
    drop(owner_a);
    drop(owner_b);
    drop(new_owner);
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
