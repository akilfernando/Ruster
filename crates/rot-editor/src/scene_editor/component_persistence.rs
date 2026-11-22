//! Guarded publication of the active v23 scene.
use super::ComponentWorkspace;
use crate::bridge::RequestLease;
use crate::scene_editor::persistence_commands::{
    bounded, canonical_parent, reject_symlink, SaveGrant,
};
use eframe::egui;
use rot_scene::component_scene::{PreparedComponentSave, SavedComponentScene};
use rot_scene::SceneError;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::mpsc::{self, TryRecvError};

type Failure = (String, String);

pub(super) struct PendingSave {
    token: String,
    args: Value,
    grant: SaveGrant,
    lease: RequestLease,
    prepared: mpsc::Receiver<Result<PreparedComponentSave, SceneError>>,
}
const IN_PLACE_ONLY: &str = "v23 scenes can only be saved in place through MCP. Omit filename.";

fn error(code: &str, message: &str) -> Failure {
    (code.into(), message.into())
}

fn validate_grant(grant: Option<&SaveGrant>) -> Result<&Path, Failure> {
    grant
        .ok_or_else(|| {
            error(
                "save_permission_required",
                "Connect an approved AI app to save the active scene.",
            )
        })?
        .validate()
}

fn validate_destination(root: &Path, active: &Path, associated: &Path) -> Result<(), Failure> {
    if associated.parent() != Some(root)
        || active.file_name() != associated.file_name()
        || canonical_parent(active).as_deref() != Some(root)
    {
        return Err(error(
            "save_permission_required",
            "The active scene is outside the granted save directory.",
        ));
    }
    reject_symlink(associated)?;
    if !associated.is_file() {
        return Err(error("save_error", "The associated scene file is missing."));
    }
    Ok(())
}

impl ComponentWorkspace {
    pub(in crate::scene_editor) fn saved_association(&self) -> Option<&Path> {
        self.session.path()
    }

    fn persistence_token(&self) -> String {
        format!(
            "scene:{}:save:{}",
            self.save_incarnation, self.save_sequence
        )
    }

    pub(super) fn next_save_sequence(&self) -> Result<u64, Failure> {
        self.save_sequence.checked_add(1).ok_or_else(|| {
            error(
                "save_error",
                "Save sequence exhausted. Reopen the scene before saving again.",
            )
        })
    }

    pub(super) fn record_save(&mut self, sequence: u64, saved: SavedComponentScene) {
        self.save_sequence = sequence;
        self.last_save = Some(json!({
            "committed":true,
            "persistence_token":self.persistence_token(),
            "revision":self.session.revision().to_string(),
            "path":self.saved_association().map(|path| bounded(&path.display().to_string())),
            "document_sha256":saved.sha256,
            "document_bytes":saved.bytes,
        }));
    }

    fn save_destination(&self, grant: Option<&SaveGrant>) -> Result<&Path, Failure> {
        if self.ai_transition_pending() || self.authored_edit_pending() {
            return Err(error(
                "drafts_pending",
                "Finish the pending editor change before saving.",
            ));
        }
        let root = validate_grant(grant)?;
        let associated = self.saved_association().ok_or_else(|| {
            error(
                "save_permission_required",
                "Save the scene in the editor first.",
            )
        })?;
        validate_destination(root, &self.path, associated)?;
        Ok(associated)
    }

    pub(in crate::scene_editor) fn inspect_persistence(
        &self,
        token: &str,
        args: &Value,
        grant: Option<&SaveGrant>,
    ) -> Value {
        if !args.as_object().is_some_and(serde_json::Map::is_empty) {
            return self.ai_response(
                token,
                Err(error(
                    "invalid_arguments",
                    "Expected an empty argument object.",
                )),
            );
        }
        let save = self.save_destination(grant);
        self.ai_response(token, Ok(json!({
            "persistence_token":self.persistence_token(),
            "associated_path":self.saved_association().map(|path| bounded(&path.display().to_string())),
            "granted_directory":grant.map(|grant| bounded(&grant.root().display().to_string())),
            "grant_valid":validate_grant(grant).is_ok(),
            "can_save":save.is_ok(),
            "can_save_as":false,
            "save_block":save.err().map(|(code, message)| json!({"code":code,"message":message})),
            "save_as_block":{"code":"save_as_unsupported","message":IN_PLACE_ONLY},
            "dirty":self.session.is_dirty(),
            "drafts_pending":self.authored_edit_pending(),
            "last_committed_save":self.last_save,
            "receipt_scope":"Last successful save in this scene incarnation. Inspect the receipt after an unknown outcome before deciding whether to save again.",
        })))
    }

    fn admit_save(
        &self,
        token: &str,
        args: &Value,
        grant: Option<&SaveGrant>,
    ) -> Result<&Path, Failure> {
        const FIELDS: [&str; 3] = [
            "expected_session_token",
            "expected_revision",
            "expected_persistence_token",
        ];
        let map = args
            .as_object()
            .ok_or_else(|| error("invalid_arguments", "Expected an argument object."))?;
        if map.contains_key("filename") {
            return Err(error("save_as_unsupported", IN_PLACE_ONLY));
        }
        if map.len() != FIELDS.len()
            || FIELDS
                .iter()
                .any(|field| !map.get(*field).is_some_and(Value::is_string))
        {
            return Err(error(
                "invalid_arguments",
                "Provide the inspected session, revision and persistence tokens.",
            ));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(error("stale_session", "Inspect the open scene again."));
        }
        let revision = super::mcp::decimal(&args["expected_revision"]).ok_or_else(|| {
            error(
                "invalid_arguments",
                "Expected a canonical decimal revision.",
            )
        })?;
        if revision != self.session.revision() {
            return Err(error(
                "stale_revision",
                "Inspect the current revision again.",
            ));
        }
        if args["expected_persistence_token"].as_str() != Some(self.persistence_token().as_str()) {
            return Err(error("stale_persistence", "Inspect persistence again."));
        }
        self.next_save_sequence()?;
        self.save_destination(grant)
    }

    pub(in crate::scene_editor) fn dispatch_save(
        &mut self,
        ctx: &egui::Context,
        token: &str,
        args: Value,
        grant: Option<&SaveGrant>,
        lease: RequestLease,
    ) {
        let result = (|| {
            let destination = self.admit_save(token, &args, grant)?;
            if self.pending_save.is_some() {
                return Err(error("save_busy", "An AI save is already preparing."));
            }
            let snapshot = self.session.active_save(destination).map_err(save_error)?;
            let grant = grant.expect("admission validates grant").clone();
            let worker_grant = grant.clone();
            let (send, prepared) = mpsc::sync_channel(1);
            let ctx = ctx.clone();
            std::thread::Builder::new()
                .name("rot-component-save".into())
                .spawn(move || {
                    let result = if worker_grant.validate().is_ok() {
                        snapshot.prepare()
                    } else {
                        Err(SceneError::Conflict)
                    };
                    let _ = send.send(result);
                    ctx.request_repaint();
                })
                .map_err(|err| error("save_error", &bounded(&err.to_string())))?;
            Ok((grant, prepared))
        })();
        match result {
            Ok((grant, prepared)) => {
                self.pending_save = Some(PendingSave {
                    token: token.into(),
                    args,
                    grant,
                    lease,
                    prepared,
                });
            }
            Err(failure) => {
                lease.respond(self.ai_response(token, Err(failure)));
            }
        }
    }

    pub(in crate::scene_editor) fn poll_save(
        &mut self,
        current_grant: Option<&SaveGrant>,
        paused: bool,
    ) {
        let Some(pending) = self.pending_save.take() else {
            return;
        };
        let result = match pending.prepared.try_recv() {
            Ok(result) => result.map_err(save_error),
            Err(TryRecvError::Empty) => {
                self.pending_save = Some(pending);
                return;
            }
            Err(TryRecvError::Disconnected) => {
                Err(error("save_error", "Save preparation stopped."))
            }
        };
        let PendingSave {
            token,
            args,
            grant,
            lease,
            ..
        } = pending;
        if lease.is_expired() {
            return;
        }
        if paused {
            lease.respond(self.ai_response(
                &token,
                Err(error("ai_paused", "The creator paused AI edits.")),
            ));
            return;
        }
        let ready = (|| {
            let path = self.admit_save(&token, &args, current_grant)?.to_path_buf();
            let sequence = self.next_save_sequence()?;
            Ok((path, sequence, result?))
        })();
        let (path, sequence, prepared) = match ready {
            Ok(ready) => ready,
            Err(failure) => {
                lease.respond(self.ai_response(&token, Err(failure)));
                return;
            }
        };
        let mut lease = Some(lease);
        let mut commit = None;
        let mut destination_failure = None;
        let result = self.session.publish_active_save(prepared, || {
            commit = lease.take().and_then(|lease| lease.try_commit().ok());
            if commit.is_none() {
                return Err(SceneError::Conflict);
            }
            grant
                .validate()
                .and_then(|root| validate_destination(root, &self.path, &path))
                .map_err(|failure| {
                    destination_failure = Some(failure);
                    SceneError::Conflict
                })
        });
        let result = match result {
            Ok(saved) => {
                self.record_save(sequence, saved);
                Ok(json!({
                    "persistence_token":self.persistence_token(),
                    "dirty":self.session.is_dirty(),
                    "receipt":self.last_save,
                }))
            }
            Err(err) => Err(destination_failure.unwrap_or_else(|| save_error(err))),
        };
        if let Some(commit) = commit {
            commit.complete(self.ai_response(&token, result));
        } else if let Some(lease) = lease {
            lease.respond(self.ai_response(&token, result));
        }
    }
}

fn save_error(source: SceneError) -> Failure {
    error("save_error", &bounded(&source.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::{owned_test_lease, preparing_test_lease};
    use crate::workspace::Action;
    use rot_scene::component_scene::{ComponentScene, ComponentSession};
    use rot_scene::{Scene, Session};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    const TOKEN: &str = "session";

    fn setup(root: &Path) -> (ComponentWorkspace, PathBuf, SaveGrant) {
        let path = root.join("scene.json");
        let scene = ComponentScene::from_legacy(&Session::new(Scene::demo()).unwrap()).unwrap();
        scene.save_new(&path).unwrap();
        let editor = ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let grant = SaveGrant::new(root).unwrap();
        (editor, path, grant)
    }

    fn inspect(editor: &ComponentWorkspace, grant: Option<&SaveGrant>) -> Value {
        let reply = editor.inspect_persistence(TOKEN, &json!({}), grant);
        assert_eq!(reply["ok"], true, "{reply}");
        reply["result"].clone()
    }

    fn arguments(editor: &ComponentWorkspace, grant: &SaveGrant) -> Value {
        let state = inspect(editor, Some(grant));
        json!({
            "expected_session_token":state["session_token"],
            "expected_revision":state["revision"],
            "expected_persistence_token":state["persistence_token"],
        })
    }

    fn dispatch(editor: &mut ComponentWorkspace, args: &Value, grant: Option<&SaveGrant>) -> Value {
        let (lease, mut response) = preparing_test_lease(Instant::now() + Duration::from_secs(2));
        start(editor, args, grant, lease);
        finish(editor, grant, false);
        response().expect("Save must respond while request is live")
    }

    fn start(
        editor: &mut ComponentWorkspace,
        args: &Value,
        grant: Option<&SaveGrant>,
        lease: RequestLease,
    ) {
        editor.dispatch_save(&egui::Context::default(), TOKEN, args.clone(), grant, lease);
    }

    fn finish(editor: &mut ComponentWorkspace, grant: Option<&SaveGrant>, paused: bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while editor.pending_save.is_some() {
            assert!(Instant::now() < deadline);
            editor.poll_save(grant, paused);
            std::thread::yield_now();
        }
    }

    fn wait_for_preparation(editor: &mut ComponentWorkspace) {
        let pending = editor.pending_save.as_mut().unwrap();
        let prepared = pending
            .prepared
            .recv_timeout(Duration::from_secs(2))
            .unwrap();
        assert!(prepared.is_ok(), "test needs successful preparation");
        let (send, ready) = mpsc::sync_channel(1);
        assert!(send.send(prepared).is_ok());
        pending.prepared = ready;
    }

    #[test]
    fn v23_save_advances_persistence_without_editing_and_retains_receipt() {
        let root = tempfile::tempdir().unwrap();
        let (mut editor, path, grant) = setup(root.path());
        let original = fs::read(&path).unwrap();
        let first = arguments(&editor, &grant);
        assert_eq!(inspect(&editor, None)["can_save"], false);
        assert_eq!(
            dispatch(&mut editor, &first, None)["error"]["code"],
            "save_permission_required"
        );
        assert_eq!(inspect(&editor, Some(&grant))["can_save_as"], false);
        let mut save_as = first.clone();
        save_as["filename"] = json!("scene.json");
        assert_eq!(
            dispatch(&mut editor, &save_as, Some(&grant))["error"]["code"],
            "save_as_unsupported"
        );
        let mut unexpected = first.clone();
        unexpected
            .as_object_mut()
            .unwrap()
            .remove("expected_revision");
        unexpected["unknown"] = json!("a");
        assert_eq!(
            dispatch(&mut editor, &unexpected, Some(&grant))["error"]["code"],
            "invalid_arguments"
        );

        let clean = dispatch(&mut editor, &first, Some(&grant));
        assert_eq!(clean["ok"], true, "{clean}");
        assert_eq!(clean["result"]["revision"], "0");
        assert_ne!(
            clean["result"]["persistence_token"],
            first["expected_persistence_token"]
        );
        assert_eq!(fs::read(&path).unwrap(), original);

        assert!(editor.save());
        let mut stale = first;
        stale["expected_persistence_token"] = clean["result"]["persistence_token"].clone();
        assert_eq!(
            dispatch(&mut editor, &stale, Some(&grant))["error"]["code"],
            "stale_persistence"
        );
        assert_eq!(inspect(&editor, Some(&grant))["revision"], "0");

        editor.command(Action::CreateCube);
        editor.command(Action::CreateSphere);
        editor.command(Action::Undo);
        assert!(editor.session.is_dirty());
        let args = arguments(&editor, &grant);
        let expected = editor.session.scene().clone();
        let (lease, _) = preparing_test_lease(Instant::now() + Duration::from_secs(2));
        start(&mut editor, &args, Some(&grant), lease); // client discards reply
        finish(&mut editor, Some(&grant), false);
        let state = inspect(&editor, Some(&grant));
        let receipt = &state["last_committed_save"];
        let bytes = fs::read(&path).unwrap();
        assert_eq!(receipt["committed"], true);
        assert_eq!(receipt["persistence_token"], state["persistence_token"]);
        assert_eq!(receipt["revision"], args["expected_revision"]);
        assert_eq!(receipt["document_bytes"], bytes.len());
        assert_eq!(
            receipt["document_sha256"],
            crate::scene_editor::persistence_commands::document_hash(&bytes)
        );
        assert_eq!(ComponentSession::open(&path).unwrap().scene(), &expected);
        assert_eq!(state["dirty"], false);
        editor.command(Action::Redo);
        assert_ne!(editor.session.scene(), &expected);
        editor.command(Action::Undo);
        assert_eq!(editor.session.scene(), &expected);
        assert!(!editor.session.is_dirty());
    }

    #[test]
    fn v23_save_failures_preserve_draft_and_disk() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("project");
        fs::create_dir(&root).unwrap();
        let (mut editor, path, grant) = setup(&root);
        let original = fs::read(&path).unwrap();
        let args = arguments(&editor, &grant);
        editor.draft.as_mut().unwrap().name = "Pending".into();
        assert_eq!(
            dispatch(&mut editor, &args, Some(&grant))["error"]["code"],
            "drafts_pending"
        );
        assert_eq!(editor.draft.as_ref().unwrap().name, "Pending");
        editor.rebuild_draft();

        fs::write(&path, b"externally changed").unwrap();
        assert_eq!(
            dispatch(&mut editor, &args, Some(&grant))["error"]["code"],
            "save_error"
        );
        assert_eq!(fs::read(&path).unwrap(), b"externally changed");
        fs::remove_file(&path).unwrap();
        assert_eq!(
            dispatch(&mut editor, &args, Some(&grant))["error"]["code"],
            "save_error"
        );
        assert!(!path.exists());
        let other = temporary.path().join("external.json");
        fs::write(&other, &original).unwrap();
        std::os::unix::fs::symlink(&other, &path).unwrap();
        assert_eq!(
            dispatch(&mut editor, &args, Some(&grant))["error"]["code"],
            "invalid_destination"
        );
        assert_eq!(fs::read(&other).unwrap(), original);

        fs::remove_file(&path).unwrap();
        fs::write(&path, &original).unwrap();
        let old = temporary.path().join("old");
        fs::rename(&root, &old).unwrap();
        fs::create_dir(&root).unwrap();
        fs::write(&path, &original).unwrap();
        assert_eq!(
            dispatch(&mut editor, &args, Some(&grant))["error"]["code"],
            "save_permission_required"
        );
        assert_eq!(
            inspect(&editor, Some(&grant))["persistence_token"],
            args["expected_persistence_token"]
        );
        assert!(inspect(&editor, Some(&grant))["last_committed_save"].is_null());
        assert_eq!(fs::read(old.join("scene.json")).unwrap(), original);
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn v23_save_publication_rechecks_editor_authority_and_disk() {
        let root = tempfile::tempdir().unwrap();
        let (mut editor, path, grant) = setup(root.path());
        let original = fs::read(&path).unwrap();
        editor.command(Action::CreateCube);
        let args = arguments(&editor, &grant);

        let second_root = tempfile::tempdir().unwrap();
        let (mut second, _, second_grant) = setup(second_root.path());
        second.command(Action::CreateCube);
        assert_eq!(
            dispatch(&mut second, &args, Some(&second_grant))["error"]["code"],
            "stale_persistence"
        );

        let (expired, mut response) =
            preparing_test_lease(Instant::now() - Duration::from_millis(1));
        start(&mut editor, &args, Some(&grant), expired);
        wait_for_preparation(&mut editor);
        finish(&mut editor, Some(&grant), false);
        assert!(response().is_none());

        let (revoked, mut response, authority) =
            owned_test_lease(Instant::now() + Duration::from_secs(2));
        start(&mut editor, &args, Some(&grant), revoked);
        wait_for_preparation(&mut editor);
        authority.revoke();
        finish(&mut editor, Some(&grant), false);
        assert!(response().is_none());

        let (paused, mut response) = preparing_test_lease(Instant::now() + Duration::from_secs(2));
        start(&mut editor, &args, Some(&grant), paused);
        wait_for_preparation(&mut editor);
        finish(&mut editor, Some(&grant), true);
        assert_eq!(response().unwrap()["error"]["code"], "ai_paused");

        let (draft, mut response) = preparing_test_lease(Instant::now() + Duration::from_secs(2));
        start(&mut editor, &args, Some(&grant), draft);
        wait_for_preparation(&mut editor);
        editor.draft.as_mut().unwrap().name = "Pending during Save".into();
        finish(&mut editor, Some(&grant), false);
        assert_eq!(response().unwrap()["error"]["code"], "drafts_pending");
        assert_eq!(editor.draft.as_ref().unwrap().name, "Pending during Save");
        editor.rebuild_draft();

        assert_eq!(fs::read(&path).unwrap(), original);
        let state = inspect(&editor, Some(&grant));
        assert_eq!(
            state["persistence_token"],
            args["expected_persistence_token"]
        );
        assert_eq!(state["dirty"], true);
        assert!(state["last_committed_save"].is_null());

        let (external, mut response) =
            preparing_test_lease(Instant::now() + Duration::from_secs(2));
        start(&mut editor, &args, Some(&grant), external);
        wait_for_preparation(&mut editor);
        fs::write(&path, b"changed after hashing").unwrap();
        finish(&mut editor, Some(&grant), false);
        assert_eq!(response().unwrap()["error"]["code"], "save_error");
        assert_eq!(fs::read(&path).unwrap(), b"changed after hashing");
        assert!(editor.session.is_dirty());
        assert!(inspect(&editor, Some(&grant))["last_committed_save"].is_null());

        fs::write(&path, &original).unwrap();
        let (stale, mut response) = preparing_test_lease(Instant::now() + Duration::from_secs(2));
        start(&mut editor, &args, Some(&grant), stale);
        wait_for_preparation(&mut editor);
        assert!(editor.save());
        finish(&mut editor, Some(&grant), false);
        assert_eq!(response().unwrap()["error"]["code"], "stale_persistence");
    }
}
