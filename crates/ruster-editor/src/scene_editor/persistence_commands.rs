//! Explicit destination authority for guarded scene saves. No project replacement.
use super::SceneEditor;
use serde_json::{json, Value};
#[cfg(test)]
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

type Result<T> = std::result::Result<T, (String, String)>;
fn error(code: &str, message: &str) -> (String, String) {
    (code.into(), bounded(message))
}
pub(super) fn bounded(text: &str) -> String {
    text.chars().take(4096).collect()
}
#[cfg(test)]
pub(super) fn document_hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) struct SaveGrant(crate::directory_grant::DirectoryGrant);
impl SaveGrant {
    pub(super) fn from_environment(enabled: bool) -> std::result::Result<Option<Self>, String> {
        if !enabled {
            return Ok(None);
        }
        std::env::var_os("RUSTER_MCP_SAVE_DIR")
            .map(|path| Self::new(Path::new(&path)))
            .transpose()
    }
    pub(super) fn new(path: &Path) -> std::result::Result<Self, String> {
        crate::directory_grant::DirectoryGrant::new(path).map(Self)
    }
    pub(super) fn root(&self) -> &Path {
        self.0.root()
    }
    fn validate(&self) -> Result<&Path> {
        self.0.validate().map_err(|_| error("save_permission_required", "Save directory changed or became unavailable; restart with an explicit valid RUSTER_MCP_SAVE_DIR grant"))
    }
}

fn filename(value: &Value) -> Result<&str> {
    let name = value
        .as_str()
        .ok_or_else(|| error("invalid_arguments", "filename must be a string"))?;
    // This is a Linux filesystem leaf, never a shell argument. Preserve normal
    // creator names (including Unicode/internal spaces) while excluding path
    // components, hidden names, controls and accidentally trimmed text.
    if name.len() > 128
        || !name.ends_with(".json")
        || name.starts_with('.')
        || name.trim() != name
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        return Err(error("invalid_filename", "Use a visible fresh .json leaf name of at most 128 UTF-8 bytes, without '/', '\\', control characters or leading/trailing whitespace"));
    }
    Ok(name)
}
fn reject_symlink(path: &Path) -> Result<()> {
    match path.symlink_metadata() {
        Ok(metadata) if !metadata.is_file() => Err(error(
            "invalid_destination",
            "Scene destination must be a regular file, never a symlink or directory",
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(error("save_error", &e.to_string())),
    }
}
impl SceneEditor {
    pub(super) fn persistence_token(&self) -> String {
        format!("scene:{}:save:{}", self.generation, self.save_sequence)
    }
    fn persistence_guard(&self) -> Result<()> {
        self.guard().map_err(|code| error(&code, "Finish or revert human drafts and gestures, and resolve dialogs before saving through MCP"))?;
        if self.manipulation.offset_pending() {
            return Err(error(
                "drafts_pending",
                "Apply or reset the pending translation offset before saving through MCP",
            ));
        }
        Ok(())
    }
    fn granted_save_root(&self) -> Result<&Path> {
        self.save_grant.as_ref().ok_or_else(|| error("save_permission_required", "Restart with RUSTER_MCP_ENABLE=1 and an explicit RUSTER_MCP_SAVE_DIR; an opened scene does not grant write authority"))?.validate()
    }
    fn associated_save_path(&self, root: &Path) -> Result<PathBuf> {
        let path = self.session.path().ok_or_else(|| {
            error(
                "fresh_filename_required",
                "This scene has no saved association; provide a fresh filename",
            )
        })?;
        if path.parent() != Some(root) {
            return Err(error("save_permission_required", "The associated scene is outside the granted save directory; provide a fresh filename within the grant"));
        }
        reject_symlink(path)?;
        Ok(path.to_path_buf())
    }
    fn save_as_root_guard(&self, root: &Path) -> Result<()> {
        if self.session.has_asset_references()
            && self.canonical_asset_root().as_deref() != Some(root)
        {
            return Err(error("asset_root_mismatch", "Save As across asset roots is unavailable while scene or history references meshes; use the existing project root"));
        }
        Ok(())
    }
    pub(super) fn admit_agent_save(
        &self,
        token: &str,
        args: &Value,
    ) -> Result<(PathBuf, crate::directory_grant::DirectoryGrant, bool)> {
        let map = args
            .as_object()
            .ok_or_else(|| error("invalid_arguments", "Expected an argument object"))?;
        if map.keys().any(|key| {
            ![
                "expected_session_token",
                "expected_revision",
                "expected_persistence_token",
                "filename",
            ]
            .contains(&key.as_str())
        }) {
            return Err(error("invalid_arguments", "Unknown argument"));
        }
        if args["expected_session_token"].as_str() != Some(token) {
            return Err(error(
                "stale_session",
                "Inspect the current scene incarnation",
            ));
        }
        let revision = args["expected_revision"].as_str().ok_or_else(|| {
            error(
                "invalid_arguments",
                "expected_revision must be a canonical u64 decimal string",
            )
        })?;
        if revision.is_empty()
            || (revision != "0" && revision.starts_with('0'))
            || !revision.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(error(
                "invalid_arguments",
                "expected_revision must be a canonical u64 decimal string",
            ));
        }
        let revision: u64 = revision
            .parse()
            .map_err(|_| error("invalid_arguments", "Revision out of range"))?;
        if revision != self.session.scene().revision {
            return Err(error("stale_revision", "Inspect authored scene again"));
        }
        if args["expected_persistence_token"].as_str() != Some(self.persistence_token().as_str()) {
            return Err(error("stale_persistence", "A save changed the scene association or committed identity; inspect persistence again"));
        }
        self.persistence_guard()?;
        let grant = self
            .save_grant
            .as_ref()
            .ok_or_else(|| {
                error(
                    "save_permission_required",
                    "Restart with an explicit RUSTER_MCP_SAVE_DIR grant",
                )
            })?
            .0
            .clone();
        let root = grant.root();
        let path = if let Some(value) = args.get("filename") {
            let path = root.join(filename(value)?);
            path
        } else {
            let associated = self.session.path().ok_or_else(|| {
                error(
                    "fresh_filename_required",
                    "This scene has no saved association; provide a fresh filename",
                )
            })?;
            if associated.parent() != Some(root) {
                return Err(error("save_permission_required", "Associated scene is outside the granted save directory; provide a fresh filename within the grant"));
            }
            associated.to_path_buf()
        };
        Ok((path, grant, args.get("filename").is_some()))
    }
    pub(super) fn persistence_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value> {
        let map = args
            .as_object()
            .ok_or_else(|| error("invalid_arguments", "Expected an argument object"))?;
        let fields: &[&str] = if op == "scene_persistence_inspect" {
            &[]
        } else {
            &[
                "expected_session_token",
                "expected_revision",
                "expected_persistence_token",
                "filename",
            ]
        };
        if map.keys().any(|key| !fields.contains(&key.as_str())) {
            return Err(error("invalid_arguments", "Unknown argument"));
        }
        if op == "scene_persistence_inspect" {
            let save = self.persistence_guard().and_then(|()| {
                let root = self.granted_save_root()?;
                self.associated_save_path(root).map(|_| ())
            });
            let save_as = self
                .persistence_guard()
                .and_then(|()| self.save_as_root_guard(self.granted_save_root()?));
            return Ok(json!({
                "session_token": token,
                "revision": self.session.scene().revision.to_string(),
                "persistence_token": self.persistence_token(),
                "associated_path": self.session.path().map(|path| bounded(&path.display().to_string())),
                "granted_directory": self.save_grant.as_ref().map(|grant| bounded(&grant.root().display().to_string())),
                "grant_valid": self.granted_save_root().is_ok(),
                "can_save": save.is_ok(),
                "can_save_as": save_as.is_ok(),
                "save_block": save.err().map(|(code, message)| json!({"code":code,"message":message})),
                "save_as_block": save_as.err().map(|(code, message)| json!({"code":code,"message":message})),
                "dirty": self.session.is_dirty(),
                "drafts_pending": self.drafts() || self.manipulation.active() || self.manipulation.offset_pending(),
                "recovered_copy": self.recovered_origin.is_some(),
                "last_committed_save": self.save_receipt,
                "receipt_scope": "Last successful save in this scene incarnation; not a claim that the file remains unchanged on disk. After a timeout inspect this receipt before deciding the next action; do not blindly retry.",
                "filename_policy": "Fresh visible .json leaf, <=128 UTF-8 bytes; Unicode/internal spaces allowed; no '/', '\\', controls, leading dot or leading/trailing whitespace",
            }));
        }
        #[cfg(test)]
        {
            self.save_sync_command(token, args)
        }
        #[cfg(not(test))]
        {
            Err(error(
                "deferred_dispatch_required",
                "scene_save must run through deferred Host dispatch",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruster_scene::{BehaviorSource, Scene, Session};

    fn inspect(editor: &mut SceneEditor) -> Value {
        let reply = editor.command("test", "scene_persistence_inspect", json!({}));
        assert_eq!(reply["ok"], true, "{reply}");
        reply["result"].clone()
    }
    fn arguments(editor: &mut SceneEditor, name: Option<&str>) -> Value {
        let state = inspect(editor);
        let mut args = json!({
            "expected_session_token":state["session_token"],
            "expected_revision":state["revision"],
            "expected_persistence_token":state["persistence_token"],
        });
        if let Some(name) = name {
            args["filename"] = name.into();
        }
        args
    }
    fn save(editor: &mut SceneEditor, name: Option<&str>) -> Value {
        let args = arguments(editor, name);
        #[cfg(unix)]
        {
            let ctx = editor.export_context.clone().unwrap_or_default();
            editor.export_context = Some(ctx.clone());
            let (lease, mut observe) = crate::bridge::preparing_test_lease(
                super::super::Instant::now() + super::super::Duration::from_secs(2),
            );
            editor.dispatch_save_request(&ctx, "test", &args, lease);
            editor.test_finish_asset_work();
            observe().expect("production deferred Save must return its terminal response")
        }
        #[cfg(not(unix))]
        {
            editor.command("test", "scene_save", args)
        }
    }
    fn rejected(reply: Value, code: &str) {
        assert_eq!(reply["ok"], false, "{reply}");
        assert_eq!(reply["error"]["code"], code, "{reply}");
    }

    // Permission/protocol boundary not covered by Session's baseline/atomic-write
    // tests: human saves, MCP guarded authority, protected drafts and recovery copy.
    #[test]
    fn guarded_persistence_roundtrip_and_permission_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let path = root.join("scene.json");
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        editor
            .session
            .set_source(
                0,
                "unfinished",
                Some(BehaviorSource {
                    label: Some("Unfinished Lua".into()),
                    language_id: "lua54".into(),
                    host_api_version: 1,
                    text: "return { update=function(".into(),
                }),
            )
            .unwrap();
        editor.save_scene(path.clone()).unwrap(); // human save grants nothing
        assert_eq!(inspect(&mut editor)["can_save"], false);
        rejected(save(&mut editor, None), "save_permission_required");
        editor.save_grant = Some(SaveGrant::new(&root).unwrap());
        let before = inspect(&mut editor);
        assert_eq!(before["can_save"], true);
        assert_eq!(before["can_save_as"], true);
        let stale = arguments(&mut editor, None);
        editor.save_scene(path.clone()).unwrap(); // same authored revision
        assert_ne!(
            inspect(&mut editor)["persistence_token"],
            before["persistence_token"]
        );
        assert_eq!(
            inspect(&mut editor)["session_token"],
            before["session_token"],
            "same-root saves retain the content incarnation"
        );
        rejected(
            editor.command("test", "scene_save", stale),
            "stale_persistence",
        );

        editor.error = "Unrelated rendering diagnostic".into();
        let reply = save(&mut editor, None);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(editor.error, "Unrelated rendering diagnostic");
        let receipt = reply["result"]["receipt"].clone();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(receipt["document_sha256"], document_hash(&bytes));
        let reopened = Session::open(&path).unwrap();
        assert_eq!(reopened.authoring(), editor.session.authoring());
        assert_eq!(reopened.undo_len(), editor.session.undo_len());
        assert_eq!(inspect(&mut editor)["last_committed_save"], receipt);

        for (field, value, code) in [
            ("expected_session_token", "old", "stale_session"),
            ("expected_revision", "0", "stale_revision"),
            ("expected_revision", "01", "invalid_arguments"),
            (
                "expected_revision",
                "18446744073709551616",
                "invalid_arguments",
            ),
            ("expected_persistence_token", "old", "stale_persistence"),
        ] {
            let mut args = arguments(&mut editor, None);
            args[field] = value.into();
            rejected(editor.command("test", "scene_save", args), code);
        }
        for name in [
            "../bad.json",
            "a/b.json",
            "a\\b.json",
            ".hidden.json",
            "bad\n.json",
            "bad.txt",
            " leading.json",
            "trailing.json ",
        ] {
            rejected(save(&mut editor, Some(name)), "invalid_filename");
        }
        rejected(save(&mut editor, Some("scene.json")), "destination_exists");
        std::fs::write(root.join("foreign.json"), b"unrelated contents").unwrap();
        rejected(
            save(&mut editor, Some("foreign.json")),
            "destination_exists",
        );
        assert_eq!(
            std::fs::read(root.join("foreign.json")).unwrap(),
            b"unrelated contents"
        );
        editor.draft.as_mut().unwrap().translation[0] = "-".into();
        rejected(save(&mut editor, None), "drafts_pending");
        editor.sync_draft();
        editor.manipulation.delta[0] = "-".into();
        rejected(save(&mut editor, None), "drafts_pending");
        editor.manipulation.delta[0] = "0".into();
        editor.rename_pending = true;
        rejected(save(&mut editor, None), "recovery_pending");
        editor.rename_pending = false;
        assert_eq!(std::fs::read(&path).unwrap(), bytes);

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root.join("absent.json"), root.join("alias.json")).unwrap();
            rejected(save(&mut editor, Some("alias.json")), "invalid_destination");
            assert!(!root.join("absent.json").exists());
        }
        // Full-document conflict is delegated to the existing Session contract.
        let mut external = Session::open(&path).unwrap();
        external
            .rename(external.scene().revision, "left", Some("External".into()))
            .unwrap();
        external.save(&path).unwrap();
        let external_bytes = std::fs::read(&path).unwrap();
        rejected(save(&mut editor, None), "save_error");
        assert_eq!(std::fs::read(&path).unwrap(), external_bytes);
        assert_eq!(inspect(&mut editor)["last_committed_save"], receipt);

        // Restored Session is deliberately unassociated; an explicit grant still
        // cannot turn a fresh-name operation into overwrite of its original.
        let snapshot = editor.recovery_snapshot().unwrap();
        let mut recovered = SceneEditor::new(snapshot.restore().unwrap());
        recovered.recovered_origin = Some(path.clone());
        assert_eq!(inspect(&mut recovered)["grant_valid"], false);
        recovered.save_grant = Some(SaveGrant::new(&root).unwrap());
        rejected(save(&mut recovered, None), "fresh_filename_required");
        rejected(
            save(&mut recovered, Some("scene.json")),
            "destination_exists",
        );
        assert_eq!(
            save(&mut recovered, Some("My creation 世界.json"))["ok"],
            true
        );
        assert_eq!(
            Session::open(root.join("My creation 世界.json"))
                .unwrap()
                .authoring(),
            recovered.session.authoring()
        );
        assert_eq!(std::fs::read(&path).unwrap(), external_bytes);
        assert!(recovered.recovered_origin.is_none());

        let elsewhere = tempfile::tempdir().unwrap();
        let old = arguments(&mut recovered, None);
        let old_content = recovered.command("test", "asset_inspect", json!({}))["result"].clone();
        assert_eq!(old_content["asset_revision"], "0");
        assert!(!recovered.session.has_asset_references());
        std::fs::write(elsewhere.path().join("occupied.json"), b"existing creation").unwrap();
        assert!(recovered
            .save_scene(elsewhere.path().join("occupied.json"))
            .is_err());
        assert_eq!(
            inspect(&mut recovered)["session_token"],
            old_content["session_token"]
        );
        std::fs::write(
            elsewhere.path().join("triangle.obj"),
            b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        let document_before = recovered.session.snapshot_bytes().unwrap();
        recovered
            .save_scene(elsewhere.path().join("elsewhere.json"))
            .unwrap();
        assert_eq!(recovered.session.snapshot_bytes().unwrap(), document_before);
        assert_eq!(recovered.assets.as_ref().unwrap().revision(), 0);
        // Both roots have catalog revision zero, so revision checks alone must
        // not authorize an old request against the new project's source files.
        rejected(
            recovered.command(
                "test",
                "asset_import",
                json!({
                    "expected_session_token": old_content["session_token"],
                    "expected_revision": old_content["revision"],
                    "expected_asset_revision": old_content["asset_revision"],
                    "asset_id": "triangle", "source": "triangle.obj",
                }),
            ),
            "stale_session",
        );
        rejected(
            recovered.command(
                "test",
                "shader_refresh",
                json!({
                    "expected_session_token": old_content["session_token"],
                    "expected_revision": old_content["revision"],
                    "expected_shader_revision": old_content["shader_revision"],
                }),
            ),
            "stale_session",
        );
        assert_eq!(recovered.assets.as_ref().unwrap().revision(), 0);
        rejected(
            recovered.command("test", "scene_save", old),
            "stale_session",
        );
        assert_eq!(inspect(&mut recovered)["can_save"], false);
        rejected(save(&mut recovered, None), "save_permission_required");
        let prior_token = inspect(&mut recovered)["session_token"].clone();
        let returned = save(&mut recovered, Some("Returned.json"));
        assert_eq!(returned["ok"], true, "{returned}");
        let returned_state = inspect(&mut recovered);
        assert_ne!(returned_state["session_token"], prior_token);
        assert_eq!(
            returned["result"]["session_token"],
            returned_state["session_token"]
        );
        assert_eq!(
            returned["result"]["persistence_token"],
            returned_state["persistence_token"]
        );
        // CP1: human replacement revokes the destination grant as well as the
        // association. Fresh explicit consent is required for the new project.
        let old = arguments(&mut recovered, None);
        recovered.replace_with_empty();
        recovered.wait_recovery();
        let replaced = inspect(&mut recovered);
        assert_eq!(replaced["grant_valid"], false);
        assert!(replaced["granted_directory"].is_null());
        assert!(replaced["last_committed_save"].is_null());
        assert!(replaced["associated_path"].is_null());
        rejected(
            recovered.command("test", "scene_save", old),
            "stale_session",
        );

        let parent = tempfile::tempdir().unwrap();
        let granted = parent.path().join("granted");
        std::fs::create_dir(&granted).unwrap();
        editor.save_grant = Some(SaveGrant::new(&granted).unwrap());
        editor
            .session
            .set_mesh(editor.session.scene().revision, "left", Some("mesh".into()))
            .unwrap();
        assert_eq!(inspect(&mut editor)["can_save_as"], false);
        rejected(
            save(&mut editor, Some("different-root.json")),
            "asset_root_mismatch",
        );
        std::fs::rename(&granted, parent.path().join("moved")).unwrap();
        std::fs::create_dir(&granted).unwrap();
        rejected(
            save(&mut editor, Some("replacement.json")),
            "save_permission_required",
        );
        assert!(!granted.join("replacement.json").exists());
    }

    #[test]
    fn committed_save_completes_coordination_after_shader_reset_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("saved.json");
        let mut editor = SceneEditor::new(Session::new(Scene::demo()).unwrap());
        let before = editor.persistence_token();
        editor.recovered_origin = Some(dir.path().join("origin.json"));
        editor.shader_revision = 3;
        editor.save_grant = Some(SaveGrant::new(dir.path()).unwrap());
        editor.save_shader_reset_failure = Some("injected reset failure".into());
        let reply = save(&mut editor, Some("saved.json"));
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(editor.session.path(), Some(path.as_path()));
        assert!(!editor.session.is_dirty());
        assert!(editor.recovered_origin.is_none());
        assert_eq!(editor.asset_origin.as_deref(), Some(dir.path()));
        assert_ne!(editor.persistence_token(), before);
        let state = inspect(&mut editor);
        let receipt = &state["last_committed_save"];
        assert_eq!(receipt["committed"], true);
        assert_eq!(
            receipt["document_sha256"],
            document_hash(&std::fs::read(&path).unwrap())
        );
        assert!(receipt["warning"]
            .as_str()
            .unwrap()
            .contains("injected reset failure"));
        assert!(editor.error.contains("Scene saved"));
        assert_eq!(
            editor.shader_revision, 3,
            "failed reset must not report a built-in shader as active"
        );
        assert_eq!(save(&mut editor, None)["ok"], true); // baseline remains usable through worker after warning
        assert!(inspect(&mut editor)["last_committed_save"]["warning"].is_null());
    }
}
