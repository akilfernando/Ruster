//! Placement-root animation authoring: clip choice, loop flag and removal.
//!
//! Place owns the node-to-ID map; these edits only change `clip`/`looping`
//! or remove the binding, each as one shared Undo step.
use super::{combo_focus::ComboFocus, SceneEditor};
use eframe::egui;
use rot_scene::AnimationBinding;
use serde_json::{json, Value};

pub(super) fn supported(op: &str) -> bool {
    op == "scene_animation_set"
}

/// Owned row data so the Inspector never borrows the store across commits.
struct ClipRow {
    id: String,
    name: String,
    duration: f32,
}

impl SceneEditor {
    /// Validate a replacement clip against the owner's existing binding.
    fn replacement_binding(
        &self,
        owner: &str,
        clip: &str,
        looping: Option<bool>,
    ) -> Result<AnimationBinding, (&'static str, String)> {
        let current = self.session.scene().animations.get(owner).ok_or((
            "animation_unbound",
            format!(
                "{owner} has no animation binding. Place the imported model to create one; the node map only exists from placement."
            ),
        ))?;
        let store = self.assets.as_ref().ok_or((
            "asset_unavailable",
            "Open a project with a valid catalog.".to_owned(),
        ))?;
        let (model, _) = store.clip_owner(&current.clip).ok_or((
            "clip_unavailable",
            format!(
                "Current clip {} is unavailable; refresh or re-import its source model, or remove the animation.",
                current.clip
            ),
        ))?;
        let (_, source) = model
            .clips
            .iter()
            .find(|(_, source)| source.id == clip)
            .ok_or((
                "invalid_clip",
                format!("{clip} is not a clip of model {}.", model.id),
            ))?;
        if let Some(target) = source
            .targets
            .iter()
            .find(|target| !current.targets.contains_key(&target.node))
        {
            return Err((
                "invalid_clip",
                format!(
                    "{clip} animates node {:?}, which this placement does not map. Place the model again.",
                    target.node
                ),
            ));
        }
        Ok(AnimationBinding {
            clip: clip.to_owned(),
            looping: looping.unwrap_or(current.looping),
            targets: current.targets.clone(),
        })
    }

    fn commit_animation(
        &mut self,
        revision: u64,
        owner: &str,
        binding: Option<AnimationBinding>,
    ) -> Result<(), String> {
        let removed = binding.is_none();
        self.session
            .set_animation(revision, owner, binding)
            .map_err(|error| error.to_string())?;
        self.sync_draft();
        self.observe_runtime();
        self.error.clear();
        self.notice = if removed {
            "Animation removed; one Undo restores it."
        } else {
            "Animation updated; Reset the preview to play it. One Undo restores the previous clip."
        }
        .into();
        Ok(())
    }

    fn animation_observation(&self, owner: &str) -> Value {
        let Some(binding) = self.session.scene().animations.get(owner) else {
            return Value::Null;
        };
        let source = self
            .assets
            .as_ref()
            .and_then(|store| store.clip_owner(&binding.clip))
            .map(|(model, name)| {
                json!({"model_id":model.id,"clip_name":name,
                    "duration":model.clips.get(name).map(|clip| clip.duration)})
            });
        json!({"clip_id":binding.clip,"looping":binding.looping,
            "target_count":binding.targets.len(),"source":source})
    }

    pub(super) fn animation_command(
        &mut self,
        token: &str,
        args: &Value,
    ) -> Result<Value, (String, String)> {
        let fail = |code: &str, message: String| (code.to_owned(), message);
        let invalid = || {
            fail(
                "invalid_arguments",
                "Expected session/revision guards, owner_id and either clip_id (optional looping) or remove: true.".into(),
            )
        };
        let map = args.as_object().ok_or_else(invalid)?;
        let required = ["expected_session_token", "expected_revision", "owner_id"];
        if !required.iter().all(|key| map.contains_key(*key))
            || map.keys().any(|key| {
                !required.contains(&key.as_str())
                    && !["clip_id", "looping", "remove"].contains(&key.as_str())
            })
        {
            return Err(invalid());
        }
        let remove = match map.get("remove") {
            None => false,
            Some(Value::Bool(true)) => true,
            Some(_) => return Err(invalid()),
        };
        let clip = map.get("clip_id").map(|value| value.as_str());
        let looping = map.get("looping").map(Value::as_bool);
        if remove == clip.is_some()
            || (remove && looping.is_some())
            || clip.is_some_and(|clip| clip.is_none())
            || looping.is_some_and(|looping| looping.is_none())
        {
            return Err(invalid());
        }
        let owner = map["owner_id"].as_str().ok_or_else(invalid)?.to_owned();
        if map["expected_session_token"].as_str() != Some(token) {
            return Err(fail(
                "stale_session",
                "Inspect the current scene session.".into(),
            ));
        }
        let revision =
            super::view_commands::number(&map["expected_revision"]).ok_or_else(invalid)?;
        if revision != self.session.scene().revision {
            return Err(fail(
                "stale_revision",
                "Inspect the current scene revision.".into(),
            ));
        }
        let busy = "Finish the current human edit before changing an animation.";
        self.guard().map_err(|code| fail(&code, busy.into()))?;
        if self.manipulation.offset_pending()
            || self.hud_open
            || self.input_actions_open
            || self.runtime.input_owned()
        {
            return Err(fail("drafts_pending", busy.into()));
        }
        let binding = if remove {
            if !self.session.scene().animations.contains_key(&owner) {
                return Err(fail(
                    "animation_unbound",
                    format!("{owner} has no animation binding to remove."),
                ));
            }
            None
        } else {
            let clip = clip.flatten().expect("validated clip_id");
            Some(
                self.replacement_binding(&owner, clip, looping.flatten())
                    .map_err(|(code, message)| fail(code, message))?,
            )
        };
        self.commit_animation(revision, &owner, binding)
            .map_err(|error| fail("invalid_animation", error))?;
        let scene = self.session.scene();
        Ok(json!({
            "session_token":token, "revision":scene.revision.to_string(),
            "owner_id":owner, "animation":self.animation_observation(&owner),
            "dirty":self.session.is_dirty(),
        }))
    }

    /// Inspector section for a selected placement root that owns a binding.
    pub(super) fn animation_inspector_ui(&mut self, ui: &mut egui::Ui) {
        if self.manipulation.ids.len() != 1 {
            return;
        }
        let Some(owner) = self.selected.clone() else {
            return;
        };
        let Some(binding) = self.session.scene().animations.get(&owner).cloned() else {
            return;
        };
        let source = self
            .assets
            .as_ref()
            .and_then(|store| store.clip_owner(&binding.clip))
            .map(|(model, name)| {
                let rows: Vec<_> = model
                    .clips
                    .iter()
                    .map(|(name, clip)| ClipRow {
                        id: clip.id.clone(),
                        name: name.clone(),
                        duration: clip.duration,
                    })
                    .collect();
                (model.id.clone(), name.to_owned(), rows)
            });
        let enabled = self.guard().is_ok() && !self.manipulation.offset_pending();
        ui.separator();
        ui.strong("Animation");
        let mut clip = binding.clip.clone();
        let mut looping = binding.looping;
        let mut remove = false;
        ui.add_enabled_ui(enabled, |ui| {
            match &source {
                Some((model, name, rows)) => {
                    ui.horizontal_wrapped(|ui| {
                        let label = ui.label("Clip");
                        let focus = ComboFocus::capture(ui, "animation_clip");
                        let mut selected = false;
                        let response = egui::ComboBox::from_id_salt("animation_clip")
                            .selected_text(name.as_str())
                            .show_ui(ui, |ui| {
                                for row in rows {
                                    let text = format!("{} ({:.2} s)", row.name, row.duration);
                                    if ui.selectable_value(&mut clip, row.id.clone(), text).clicked()
                                    {
                                        selected = true;
                                        ui.close();
                                    }
                                }
                            });
                        focus.restore(ui, &response.response, selected);
                        let _ = response.response.labelled_by(label.id);
                    });
                    let duration = rows
                        .iter()
                        .find(|row| row.id == binding.clip)
                        .map_or(0.0, |row| row.duration);
                    ui.small(format!(
                        "{duration:.2} s from model {model} · {} mapped nodes",
                        binding.targets.len()
                    ));
                }
                None => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!(
                            "Clip unavailable: {}. Refresh or re-import the source model; the binding is kept.",
                            binding.clip
                        ),
                    );
                }
            }
            ui.add_enabled(source.is_some(), egui::Checkbox::new(&mut looping, "Loop"))
                .on_hover_text("Repeat the clip; otherwise hold its last pose.");
            remove = ui
                .button("Remove animation")
                .on_hover_text("Stop animating this placement. One Undo restores it.")
                .clicked();
        });
        ui.small("Preview Reset or Play uses the authored clip.");
        let revision = self.session.scene().revision;
        let result = if remove {
            self.commit_animation(revision, &owner, None)
        } else if clip != binding.clip || looping != binding.looping {
            self.replacement_binding(&owner, &clip, Some(looping))
                .map_err(|(_, message)| message)
                .and_then(|next| self.commit_animation(revision, &owner, Some(next)))
        } else {
            Ok(())
        };
        if let Err(error) = result {
            self.error = format!("Animation unchanged: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::SceneEditor;
    use rot_assets::fixtures::{animated_glb, AnimatedFixture};
    use rot_scene::{Scene, Session};
    use serde_json::{json, Value};
    use std::fs;

    // Unique boundary: the guarded command edits only clip/loop on a binding
    // created by real placement, refuses stale and foreign clips atomically,
    // and shares one Undo history with every other scene edit.
    #[test]
    fn placed_animation_loop_clip_guard_remove_and_undo() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("rig.glb"),
            animated_glb(AnimatedFixture::default()),
        )
        .unwrap();
        fs::write(
            dir.path().join("wave.glb"),
            animated_glb(AnimatedFixture {
                clip_name: "Wave",
                ..Default::default()
            }),
        )
        .unwrap();
        let path = dir.path().join("scene.json");
        Session::new(Scene::authored_empty("animated"))
            .unwrap()
            .save(&path)
            .unwrap();
        let mut editor = SceneEditor::test_loaded(Session::open(&path).unwrap());
        let token =
            editor.command("test", "scene_inspect", json!({}))["result"]["session_token"].clone();
        let call = |editor: &mut SceneEditor, op: &str, mut args: Value| {
            args["expected_session_token"] = token.clone();
            if args.get("expected_revision").is_none() {
                args["expected_revision"] = json!(editor.session.scene().revision.to_string());
            }
            if op.starts_with("asset_") || op == "scene_model_place" {
                args["expected_asset_revision"] =
                    json!(editor.assets.as_ref().unwrap().revision().to_string());
            }
            let response = editor.command("test", op, args);
            editor.test_finish_asset_work();
            response
        };
        for (id, source) in [("rig", "rig.glb"), ("wave", "wave.glb")] {
            let imported = call(
                &mut editor,
                "asset_import",
                json!({"asset_id":id,"source":source}),
            );
            assert_eq!(imported["ok"], true, "{imported}");
        }
        let catalog = editor.command("test", "asset_inspect", json!({"model_offset":"0"}));
        let clips = &catalog["result"]["models"][0]["clips"];
        assert_eq!(clips[0]["name"], "Swing", "{catalog}");
        assert_eq!(clips[0]["duration"], 2.0);
        let swing = clips[0]["id"].as_str().unwrap().to_owned();
        let wave = catalog["result"]["models"][1]["clips"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let placed = call(&mut editor, "scene_model_place", json!({"asset_id":"rig"}));
        let owner = placed["result"]["root_id"].as_str().unwrap().to_owned();
        let bound = editor.session.scene().animations[&owner].clone();
        let inspected = editor.command("test", "scene_inspect", json!({}));
        assert_eq!(inspected["result"]["animations"][0]["owner_id"], owner);
        assert_eq!(inspected["result"]["animations"][0]["clip_id"], swing);

        let looped = call(
            &mut editor,
            "scene_animation_set",
            json!({"owner_id":owner,"clip_id":swing,"looping":false}),
        );
        assert_eq!(looped["ok"], true, "{looped}");
        assert_eq!(looped["result"]["animation"]["looping"], false);
        assert_eq!(
            looped["result"]["animation"]["source"]["clip_name"],
            "Swing"
        );
        let after_loop = editor.session.snapshot_bytes().unwrap();
        for (args, code) in [
            (
                json!({"owner_id":owner,"clip_id":swing,"expected_revision":"0"}),
                "stale_revision",
            ),
            (json!({"owner_id":owner,"clip_id":wave}), "invalid_clip"),
            (
                json!({"owner_id":owner,"clip_id":"missing"}),
                "invalid_clip",
            ),
            (
                json!({"owner_id":bound.targets["Rig"],"clip_id":swing}),
                "animation_unbound",
            ),
            (
                json!({"owner_id":owner,"remove":true,"looping":true}),
                "invalid_arguments",
            ),
        ] {
            let refused = call(&mut editor, "scene_animation_set", args);
            assert_eq!(refused["error"]["code"], code, "{refused}");
            assert_eq!(editor.session.snapshot_bytes().unwrap(), after_loop);
        }
        let removed = call(
            &mut editor,
            "scene_animation_set",
            json!({"owner_id":owner,"remove":true}),
        );
        assert_eq!(removed["ok"], true, "{removed}");
        assert!(removed["result"]["animation"].is_null());
        assert!(editor.session.scene().animations.is_empty());
        let undone = call(&mut editor, "scene_undo", json!({}));
        assert_eq!(undone["ok"], true, "{undone}");
        let restored = &editor.session.scene().animations[&owner];
        assert_eq!(
            (restored.clip.as_str(), restored.looping),
            (swing.as_str(), false)
        );
        call(&mut editor, "scene_undo", json!({}));
        assert_eq!(editor.session.scene().animations[&owner], bound);
    }
}
