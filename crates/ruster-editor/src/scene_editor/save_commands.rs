//! Save capture/adoption for the single document worker; disk work stays in asset_save.
use super::*;
use crate::asset_save::{Request, Saved};
use crate::asset_worker::{AssetMaps, RetiredAssets};
use ruster_assets::AssetStateIdentity;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Continuation {
    #[default]
    SaveOnly,
    ThenNew,
    ThenClose,
}
pub(super) struct AfterSave {
    pub kind: Continuation,
    pub generation: u64,
    pub sequence: u64,
    pub revision: u64,
}
pub(super) struct Pin {
    generation: u64,
    revision: u64,
    association: Option<PathBuf>,
    sequence: u64,
    persistence: String,
    pub next_sequence: u64,
    pub accepted_root: Option<PathBuf>,
    origin: Option<PathBuf>,
    recovered: Option<PathBuf>,
    identity: Option<AssetStateIdentity>,
    drafts: (u64, u64, u64),
    offset: [String; 3],
    path_form: String,
    continuation: Continuation,
}
impl Pin {
    pub(super) fn permits_continuation(&self, continuation: Continuation) -> bool {
        continuation == self.continuation || continuation == Continuation::SaveOnly
    }
}
impl SceneEditor {
    pub(super) fn canonical_asset_root(&self) -> Option<PathBuf> {
        self.assets
            .as_ref()
            .map(|store| store.root().to_path_buf())
            .or_else(|| self.asset_origin.clone())
    }
    pub(super) fn capture_save(
        &self,
        path: PathBuf,
        grant: Option<crate::directory_grant::DirectoryGrant>,
        fresh: bool,
        continuation: Continuation,
    ) -> Result<(Box<Request>, Pin), (String, String)> {
        let next_sequence = self
            .save_sequence
            .checked_add(1)
            .ok_or_else(|| ("save_error".into(), "Save identity exhausted".into()))?;
        let accepted_root = self.canonical_asset_root();
        let pin = Pin {
            generation: self.generation,
            revision: self.session.scene().revision,
            association: self.session.path().map(PathBuf::from),
            sequence: self.save_sequence,
            persistence: self.persistence_token(),
            next_sequence,
            accepted_root: accepted_root.clone(),
            origin: self.asset_origin.clone(),
            recovered: self.recovered_origin.clone(),
            identity: self.assets.as_ref().map(AssetStore::state_identity),
            drafts: (
                self.manipulation.draft_generation(),
                self.draft_generation,
                self.behavior.generation(),
            ),
            offset: self.manipulation.delta.clone(),
            path_form: self.path_text.clone(),
            continuation,
        };
        Ok((
            Box::new(Request {
                session: self.session.clone(),
                destination: path,
                original_asset_root: accepted_root,
                grant,
                fresh_destination: fresh,
            }),
            pin,
        ))
    }
    pub(super) fn validate_save_pin(&self, pin: &Pin) -> Result<(), (String, String)> {
        let stale = || {
            ("stale_persistence".into(), "Document, content, drafts or save form changed while Save prepared; accepted work was preserved.".into())
        };
        if self.generation != pin.generation
            || self.session.scene().revision != pin.revision
            || self.session.path() != pin.association.as_deref()
            || self.save_sequence != pin.sequence
            || self.persistence_token() != pin.persistence
            || self.asset_origin != pin.origin
            || self.recovered_origin != pin.recovered
            || self.canonical_asset_root() != pin.accepted_root
            || (
                self.manipulation.draft_generation(),
                self.draft_generation,
                self.behavior.generation(),
            ) != pin.drafts
            || self.manipulation.delta != pin.offset
            || self.drafts()
            || self.path_text != pin.path_form
            || !match (&pin.identity, &self.assets) {
                (None, None) => true,
                (Some(identity), Some(store)) => store.matches_identity(identity),
                _ => false,
            }
        {
            return Err(stale());
        }
        Ok(())
    }
    pub(super) fn save_owner_guard(&self) -> Result<(), (String, String)> {
        if self.drafts() {
            return Err((
                "drafts_pending".into(),
                "New human drafts changed the prepared Save.".into(),
            ));
        }
        if self.dialog.is_some()
            || self.asset_dialog
            || self.export_dialog
            || self.rename_pending
            || self.new_scene_pending
            || self.close_pending
            || self.close_approved
            || self.workspace.settings_open
            || self.recovery.open
            || self.recovery_transition.is_some()
            || self.manipulation.active()
            || self.asset_loading()
        {
            return Err((
                "recovery_pending".into(),
                "Finish the newer dialog or gesture before saving; prepared Save was cancelled."
                    .into(),
            ));
        }
        Ok(())
    }
    pub(super) fn saved_result(&self) -> Value {
        json!({"session_token":null,"revision":self.session.scene().revision.to_string(), "persistence_token":self.persistence_token(),
            "dirty":self.session.is_dirty(), "receipt":self.save_receipt})
    }
    // The returned bundle includes the old Session, so its final owned source /
    // history buffers are released on the worker after the paint boundary.
    pub(super) fn adopt_saved(&mut self, saved: Saved, pin: &Pin) -> (RetiredAssets, Option<u64>) {
        let reset = |render: Option<&eframe::egui_wgpu::RenderState>| match render {
            Some(render) => ruster_render::refresh_shader(render, ruster_render::default_shader()),
            None => Ok(()),
        };
        #[cfg(test)]
        let failure = self.save_shader_reset_failure.take();
        self.adopt_saved_with_reset(saved, pin, |render| {
            #[cfg(test)]
            if let Some(failure) = failure {
                return Err(failure);
            }
            reset(render)
        })
    }
    fn adopt_saved_with_reset(
        &mut self,
        saved: Saved,
        pin: &Pin,
        reset: impl FnOnce(Option<&eframe::egui_wgpu::RenderState>) -> Result<(), String>,
    ) -> (RetiredAssets, Option<u64>) {
        let Saved {
            session,
            destination_store,
            root,
            path,
            document_hash,
            document_bytes,
        } = saved;
        let mut retired = RetiredAssets {
            session: Some(Box::new(std::mem::replace(&mut self.session, session))),
            ..Default::default()
        };
        self.save_sequence = pin.next_sequence;
        self.save_warning = None;
        let changed_root = pin.accepted_root.as_deref() != Some(root.as_path());
        if changed_root {
            self.generation = GENERATION.fetch_add(1, Ordering::Relaxed);
            self.creation_export.cancel();
            retired.store = std::mem::replace(&mut self.assets, destination_store);
            let meshes = self
                .assets
                .as_ref()
                .map(AssetStore::mesh_snapshots)
                .unwrap_or_default();
            let textures = self
                .assets
                .as_ref()
                .map(AssetStore::texture_snapshots)
                .unwrap_or_default();
            retired.maps[0] = AssetMaps {
                meshes: std::mem::replace(&mut self.resolved, meshes),
                textures: std::mem::replace(&mut self.resolved_textures, textures),
            };
            if let Some((_, meshes, textures)) = self.reset_assets.take() {
                retired.maps[2] = AssetMaps { meshes, textures };
            }
            self.content_search.key = None;
            match reset(self.render.as_ref()) {
                Ok(()) => {
                    self.shader_revision = 0;
                    self.shader_builtin = true;
                    self.shader_notice = "Built-in scene shader".into();
                }
                Err(error) => {
                    let warning = format!(
                        "Scene saved; viewport shader reset failed: {}",
                        persistence_commands::bounded(&error)
                    );
                    self.shader_notice = warning.clone();
                    self.error = warning.clone();
                    self.save_warning = Some(warning);
                }
            }
            self.observe_runtime();
        }
        self.asset_origin = Some(root);
        self.recovered_origin = None;
        self.draft_generation = self.draft_generation.wrapping_add(1);
        if let Some(path) = self.session.path() {
            self.workspace.remember(path);
        }
        self.remember_camera();
        self.workspace.flush();
        if self.workspace.state_path().is_some() && !self.workspace.error.is_empty() {
            self.append_save_warning(&format!("workspace settings: {}", self.workspace.error));
        }
        self.external.saved(&self.session, self.generation);
        let recovery_request = if !self.pending_recovery_drafts() {
            Some(self.recovery.retire())
        } else {
            None
        };
        self.save_receipt = Some(
            json!({"committed":true,"persistence_token":self.persistence_token(),
            "revision":self.session.scene().revision.to_string(),"path":persistence_commands::bounded(&path.display().to_string()),
            "document_sha256":document_hash,"document_bytes":document_bytes,"warning":self.save_warning,"followup_pending":true}),
        );
        self.notice = self
            .save_warning
            .clone()
            .unwrap_or_else(|| "Scene saved; finishing cleanup…".into());
        (retired, recovery_request)
    }
    pub(super) fn finalize_saved_followup(
        &mut self,
        continuation: Continuation,
        result: Result<(), String>,
    ) {
        if let Err(message) = result {
            self.append_save_warning(&format!("recovery cleanup failed: {message}"));
        }
        if let Some(receipt) = &mut self.save_receipt {
            receipt["warning"] = json!(self.save_warning);
            receipt["followup_pending"] = json!(false);
        }
        self.notice = self.save_warning.clone().unwrap_or_else(|| {
            if self.pending_recovery_drafts() {
                "Scene saved; unapplied fields remain. Apply or reset them before New or Close."
                    .into()
            } else {
                "Scene saved.".into()
            }
        });
        if continuation != Continuation::SaveOnly && self.save_warning.is_some() {
            self.notice = format!(
                "{} Stayed in this scene; New or Close was not performed.",
                self.notice
            );
        }
        if continuation != Continuation::SaveOnly
            && self.save_warning.is_none()
            && !self.pending_recovery_drafts()
        {
            self.save_continuation = Some(AfterSave {
                kind: continuation,
                generation: self.generation,
                sequence: self.save_sequence,
                revision: self.session.scene().revision,
            });
        }
    }
    pub(super) fn append_save_warning(&mut self, warning: &str) {
        let text = match &self.save_warning {
            Some(old) => format!("{old}; {warning}"),
            None => format!("Scene saved; {warning}"),
        };
        self.save_warning = Some(persistence_commands::bounded(&text));
        self.error = self.save_warning.clone().unwrap();
        if let Some(receipt) = &mut self.save_receipt {
            receipt["warning"] = json!(self.save_warning);
        }
    }
    pub(super) fn finish_save_continuation(&mut self) {
        if self.asset_busy() {
            return;
        }
        let Some(after) = self.save_continuation.take() else {
            return;
        };
        if after.generation != self.generation
            || after.sequence != self.save_sequence
            || after.revision != self.session.scene().revision
            || self.pending_recovery_drafts()
            || self.save_owner_guard().is_err()
        {
            return;
        }
        match after.kind {
            Continuation::SaveOnly => (),
            Continuation::ThenNew => self.replace_with_empty(),
            Continuation::ThenClose => self.request_close(),
        }
    }
    pub(super) fn save_with_continuation(&mut self, kind: Continuation) {
        if let Err(error) = self.asset_root_guard() {
            self.error = error;
            return;
        }
        if !self.apply() {
            return;
        }
        if let Some(path) = self.session.path().map(PathBuf::from) {
            self.start_human_save(path, kind);
        } else {
            self.save_dialog_continuation = kind;
            self.dialog = Some(false);
        }
    }
    pub(super) fn start_human_save(&mut self, path: PathBuf, kind: Continuation) {
        if self.asset_busy() {
            self.error = "Content or Save work is already in progress.".into();
            return;
        }
        if !self.apply() {
            return;
        }
        let ctx = self.export_context.clone().unwrap_or_default();
        let mut controller = std::mem::take(&mut self.asset_ops);
        match self.capture_save(path, None, false, kind) {
            Ok((request, pin)) => {
                match controller.begin_save(&ctx, request, pin, kind, asset_commands::Reply::Human)
                {
                    Ok(()) => {
                        self.dialog = None;
                        self.path_focused = false;
                        self.new_scene_pending = false;
                        self.close_pending = false;
                        ctx.memory_mut(|memory| {
                            if let Some(id) = memory.focused() {
                                memory.surrender_focus(id);
                            }
                        });
                    }
                    Err((failure, request, reply)) => {
                        reply.finish(self, Err(failure));
                        controller.retire_unsent_save(request);
                    }
                }
            }
            Err((_, message)) => self.error = message,
        }
        self.asset_ops = controller;
    }
    #[cfg(test)]
    pub(super) fn save_scene_with_reset(
        &mut self,
        path: PathBuf,
        reset: impl FnOnce(Option<&eframe::egui_wgpu::RenderState>) -> Result<(), String>,
    ) -> Result<(), String> {
        self.asset_root_guard()?;
        let (request, pin) = self
            .capture_save(path, None, false, Continuation::SaveOnly)
            .map_err(|(_, message)| message)?;
        let saved = (*request)
            .prepare()
            .and_then(|prepared| prepared.publish())
            .map_err(|error| error.to_string())?;
        let (retired, request) = self.adopt_saved_with_reset(saved, &pin, reset);
        self.asset_ops.merge_save_retirement(retired);
        if let Some(request) = request {
            self.recovery.wait_idle();
            if let Some((id, result)) = self.recovery.take_ack() {
                assert_eq!(id, request);
                self.finalize_saved_followup(Continuation::SaveOnly, result);
            }
        } else {
            self.finalize_saved_followup(Continuation::SaveOnly, Ok(()));
        }
        self.test_finish_asset_work();
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn save_scene(&mut self, path: PathBuf) -> Result<(), String> {
        self.save_scene_with_reset(path, |render| match render {
            Some(render) => ruster_render::refresh_shader(render, ruster_render::default_shader()),
            None => Ok(()),
        })
    }
    #[cfg(test)]
    pub(super) fn save_sync_command(
        &mut self,
        token: &str,
        args: &Value,
    ) -> Result<Value, (String, String)> {
        let (path, grant, fresh) = self.admit_agent_save(token, args)?;
        let (request, pin) = self.capture_save(path, Some(grant), fresh, Continuation::SaveOnly)?;
        let saved = (*request)
            .prepare()
            .and_then(|prepared| prepared.publish())
            .map_err(|failure| (failure.code().into(), failure.message))?;
        let (retired, request) = self.adopt_saved(saved, &pin);
        self.asset_ops.merge_save_retirement(retired);
        if let Some(request) = request {
            self.recovery.wait_idle();
            if let Some((id, result)) = self.recovery.take_ack() {
                assert_eq!(id, request);
                self.finalize_saved_followup(Continuation::SaveOnly, result);
            }
        } else {
            self.finalize_saved_followup(Continuation::SaveOnly, Ok(()));
        }
        self.test_finish_asset_work();
        Ok(self.saved_result())
    }
}
