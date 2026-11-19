//! Deferred asset writes on the active v23 project's worker.
use super::ComponentWorkspace;
use crate::bridge::{CommitLease, RequestLease};
use crate::scene_editor::{asset_commands, asset_inspection, project_assets::AssetWrite};
use eframe::egui;
use rot_assets::{AssetOutcome, AssetStateIdentity};
use serde_json::{json, Value};
use std::sync::mpsc::TryRecvError;

pub(super) struct PendingWrite {
    token: String,
    scene_revision: u64,
    catalog: AssetStateIdentity,
    worker: AssetWrite,
    phase: Phase,
}

enum Phase {
    Preparing(RequestLease),
    Publishing(CommitLease),
}

pub(in crate::scene_editor) fn supported(op: &str) -> bool {
    matches!(op, "asset_import" | "asset_refresh")
}

impl ComponentWorkspace {
    pub(in crate::scene_editor) fn dispatch_asset_write(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
        lease: RequestLease,
    ) {
        let result = (|| {
            let map = asset_commands::asset_arguments(op, args)?;
            if args["expected_session_token"].as_str() != Some(token) {
                return Err((
                    "stale_session".into(),
                    "Inspect the open scene again.".into(),
                ));
            }
            let revision =
                asset_commands::decimal(&args["expected_revision"], "expected_revision")?;
            let asset_revision = asset_commands::decimal(
                &args["expected_asset_revision"],
                "expected_asset_revision",
            )?;
            if revision != self.session.revision() {
                return Err((
                    "stale_revision".into(),
                    "Inspect the current scene revision again.".into(),
                ));
            }
            let store = self.assets.as_ref().ok_or_else(|| {
                (
                    "assets_unavailable".to_owned(),
                    "Wait for the project assets to load.".to_owned(),
                )
            })?;
            if asset_revision != store.revision() {
                return Err((
                    "stale_asset_revision".into(),
                    "Inspect the project assets again.".into(),
                ));
            }
            if self.ai_asset_write.is_some() || self.asset_import.is_some() {
                return Err(("asset_busy".into(), "An asset operation is running.".into()));
            }
            if self.ai_transition_pending() || self.draft_dirty() {
                return Err((
                    "drafts_pending".into(),
                    "Finish the pending editor change first.".into(),
                ));
            }
            let mutation = asset_commands::asset_mutation(store, op, args, map)?;
            let request = store
                .request_at(asset_revision, mutation)
                .map_err(|error| ("asset_error".into(), error.to_string()))?;
            let watcher = self.asset_watcher.as_ref().ok_or_else(|| {
                (
                    "assets_unavailable".to_owned(),
                    "The project asset worker is unavailable.".to_owned(),
                )
            })?;
            let worker = watcher
                .write_asset(request, lease.deadline())
                .map_err(|message| ("asset_busy".into(), message))?;
            Ok((revision, store.state_identity(), worker))
        })();
        match result {
            Ok((scene_revision, catalog, worker)) => {
                self.ai_asset_write = Some(PendingWrite {
                    token: token.into(),
                    scene_revision,
                    catalog,
                    worker,
                    phase: Phase::Preparing(lease),
                });
            }
            Err(failure) => {
                lease.respond(self.ai_response(token, Err(failure)));
            }
        }
    }

    pub(in crate::scene_editor) fn poll_asset_write(&mut self, ctx: &egui::Context, paused: bool) {
        let Some(mut pending) = self.ai_asset_write.take() else {
            return;
        };
        // Snapshots are sent before completion. Take the most recent one
        // before checking an admission token or returning an accepted write.
        self.poll_assets(ctx);
        match pending.phase {
            Phase::Preparing(lease) => {
                if lease.is_expired() {
                    return;
                }
                let refusal = if paused {
                    Some(("ai_paused", "The creator paused AI edits."))
                } else if self.session.revision() != pending.scene_revision {
                    Some((
                        "stale_revision",
                        "The scene changed during asset preparation.",
                    ))
                } else if self.ai_transition_pending() || self.draft_dirty() {
                    Some(("drafts_pending", "Finish the pending editor change first."))
                } else if self
                    .assets
                    .as_ref()
                    .is_none_or(|store| !store.matches_identity(&pending.catalog))
                {
                    Some((
                        "stale_asset_revision",
                        "The project assets changed during preparation.",
                    ))
                } else {
                    None
                };
                if let Some((code, message)) = refusal {
                    lease.respond(
                        self.ai_response(&pending.token, Err((code.into(), message.into()))),
                    );
                    return;
                }
                match pending.worker.prepared.try_recv() {
                    Ok(()) => {
                        if let Ok(commit) = lease.try_commit() {
                            if pending.worker.publish.try_send(()).is_ok() {
                                pending.phase = Phase::Publishing(commit);
                                self.ai_asset_write = Some(pending);
                                ctx.request_repaint();
                            } else {
                                commit.complete(
                                    self.ai_response(&pending.token, Err(worker_stopped())),
                                );
                            }
                        }
                    }
                    Err(TryRecvError::Empty) => {
                        pending.phase = Phase::Preparing(lease);
                        self.ai_asset_write = Some(pending);
                    }
                    Err(TryRecvError::Disconnected) => match pending.worker.result.try_recv() {
                        Ok(Err(message)) => {
                            lease.respond(
                                self.ai_response(
                                    &pending.token,
                                    Err(("asset_error".into(), message)),
                                ),
                            );
                        }
                        Err(TryRecvError::Empty) => {
                            // Result publication follows the catalog snapshot.
                            pending.phase = Phase::Preparing(lease);
                            self.ai_asset_write = Some(pending);
                        }
                        _ => {
                            lease.respond(self.ai_response(&pending.token, Err(worker_stopped())));
                        }
                    },
                }
            }
            Phase::Publishing(commit) => match pending.worker.result.try_recv() {
                Ok(outcome) => {
                    self.poll_assets(ctx);
                    let reply = outcome
                        .map(|published| {
                            let revision = self
                                .assets
                                .as_ref()
                                .map_or(0, rot_assets::AssetStore::revision);
                            let mut info = json!({"asset_revision":revision.to_string()});
                            if let Some(warning) = published.warning {
                                info["warning"] = json!(warning);
                            }
                            match published.outcome {
                                AssetOutcome::Asset(asset) => info["asset"] = json!(asset),
                                AssetOutcome::Model(model) => {
                                    info["model"] = asset_inspection::model_summary(&model)
                                }
                            }
                            info
                        })
                        .map_err(|message| ("asset_error".into(), message));
                    commit.complete(self.ai_response(&pending.token, reply));
                }
                Err(TryRecvError::Empty) => {
                    pending.phase = Phase::Publishing(commit);
                    self.ai_asset_write = Some(pending);
                }
                Err(TryRecvError::Disconnected) => {
                    commit.complete(self.ai_response(&pending.token, Err(worker_stopped())));
                }
            },
        }
    }

    pub(super) fn asset_publish_pending(&self) -> bool {
        self.ai_asset_write
            .as_ref()
            .is_some_and(|pending| matches!(pending.phase, Phase::Publishing(_)))
    }
}

fn worker_stopped() -> (String, String) {
    (
        "assets_unavailable".into(),
        "The asset worker stopped.".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Action;
    use rot_scene::{
        component_scene::{ComponentScene, ComponentSession},
        Scene, Session,
    };
    use std::{
        fs,
        time::{Duration, Instant},
    };

    #[test]
    fn v23_asset_publication_requires_current_document_catalog_and_connection() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path();
        let path = project.join("scene.json");
        ComponentScene::from_legacy(&Session::new(Scene::empty("assets")).unwrap())
            .unwrap()
            .save_new(&path)
            .unwrap();
        fs::write(
            project.join("Triangle.obj"),
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
        )
        .unwrap();
        fs::create_dir(project.join("Assets")).unwrap();
        fs::write(
            project.join("Assets/Missing.obj.rotmeta"),
            r#"{"id":"reserved"}"#,
        )
        .unwrap();
        let mut editor =
            ComponentWorkspace::new(path.clone(), ComponentSession::open(&path).unwrap());
        let ctx = egui::Context::default();
        let deadline = || Instant::now() + Duration::from_secs(5);
        let await_frame = |limit| {
            assert!(Instant::now() < limit, "asset worker did not finish");
            std::thread::sleep(Duration::from_millis(5));
        };
        let limit = deadline();
        while editor.assets_loading() {
            editor.poll_assets(&ctx);
            await_frame(limit);
        }
        let arguments = |editor: &ComponentWorkspace, id| {
            json!({
                "expected_session_token":"test-token",
                "expected_revision":editor.session.revision().to_string(),
                "expected_asset_revision":editor.assets.as_ref().unwrap().revision().to_string(),
                "asset_id":id,
                "source":"Triangle.obj",
            })
        };
        let run_until_reply =
            |editor: &mut ComponentWorkspace, reply: &mut dyn FnMut() -> Option<Value>| {
                let limit = deadline();
                loop {
                    editor.poll_asset_write(&ctx, false);
                    if let Some(result) = reply() {
                        return result;
                    }
                    await_frame(limit);
                }
            };
        let assert_clean = || {
            assert!(!project.join(".rot-assets.json").exists());
            assert!(!project.join(".rot-cache").exists());
            assert!(!project.join("Triangle.obj.rotmeta").exists());
        };

        let (lease, mut reply) = crate::bridge::preparing_test_lease(deadline());
        let mut wrong_catalog = arguments(&editor, "triangle");
        wrong_catalog["expected_asset_revision"] = json!("100");
        editor.dispatch_asset_write("test-token", "asset_import", &wrong_catalog, lease);
        assert_eq!(reply().unwrap()["error"]["code"], "stale_asset_revision");
        assert_clean();

        let (lease, mut reply) = crate::bridge::preparing_test_lease(deadline());
        editor.dispatch_asset_write(
            "test-token",
            "asset_import",
            &arguments(&editor, "triangle"),
            lease,
        );
        editor
            .ai_asset_write
            .as_ref()
            .unwrap()
            .worker
            .prepared
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        editor.command(Action::CreateEmpty);
        assert_eq!(
            run_until_reply(&mut editor, &mut reply)["error"]["code"],
            "stale_revision"
        );
        assert_clean();

        let (lease, _reply, authority) = crate::bridge::owned_test_lease(deadline());
        editor.dispatch_asset_write(
            "test-token",
            "asset_import",
            &arguments(&editor, "triangle"),
            lease,
        );
        editor
            .ai_asset_write
            .as_ref()
            .unwrap()
            .worker
            .prepared
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        authority.revoke();
        editor.poll_asset_write(&ctx, false);
        assert!(editor.ai_asset_write.is_none());
        assert_clean();

        let (lease, mut reply) = crate::bridge::preparing_test_lease(deadline());
        editor.dispatch_asset_write(
            "test-token",
            "asset_import",
            &arguments(&editor, "reserved"),
            lease,
        );
        let result = run_until_reply(&mut editor, &mut reply);
        assert!(
            result["error"]["message"]
                .as_str()
                .unwrap()
                .contains("reserved by another sidecar"),
            "{result}"
        );
        assert_clean();

        let (lease, mut reply) = crate::bridge::preparing_test_lease(deadline());
        editor.dispatch_asset_write(
            "test-token",
            "asset_import",
            &arguments(&editor, "triangle"),
            lease,
        );
        let result = run_until_reply(&mut editor, &mut reply);
        assert_eq!(result["result"]["asset"]["id"], "triangle", "{result}");
        assert_eq!(result["result"]["asset_revision"], "1");
        assert!(editor.meshes.contains_key("triangle"));
        let sidecar = fs::read_to_string(project.join("Triangle.obj.rotmeta")).unwrap();
        assert!(sidecar.contains("triangle"));
        assert!(sidecar.contains("flat"));
    }
}
