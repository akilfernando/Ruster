//! Exact, bounded native commands sharing Scene lighting transactions.
use super::SceneEditor;
use ruster_scene::{
    Ambient, Light, LightKind, SceneError, SunShadow, MAX_DIRECTIONAL_LIGHTS, MAX_LIGHTS,
    MAX_OBJECTS, MAX_POINT_LIGHTS,
};
use serde_json::{json, Value};

type CommandError = (String, String);
fn error(code: &str, message: impl Into<String>) -> CommandError {
    (code.into(), message.into())
}
pub(super) fn supported(op: &str) -> bool {
    matches!(
        op,
        "lighting_inspect" | "scene_light_create" | "scene_light_set" | "scene_ambient_set"
    )
}
fn number(value: &Value) -> Result<u64, CommandError> {
    value
        .as_str()
        .filter(|text| {
            !text.is_empty()
                && (*text == "0" || !text.starts_with('0'))
                && text.bytes().all(|byte| byte.is_ascii_digit())
        })
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                "Expected canonical decimal u64 revision string",
            )
        })
}
fn finite(value: &Value, min: f64, max: f64) -> Result<(), CommandError> {
    if value
        .as_f64()
        .is_some_and(|value| value.is_finite() && (min..=max).contains(&value))
    {
        Ok(())
    } else {
        Err(error(
            "invalid_arguments",
            format!("Expected finite number in {min}..={max}"),
        ))
    }
}
fn vector(value: &Value, min: f64, max: f64) -> Result<(), CommandError> {
    let vector = value
        .as_array()
        .filter(|array| array.len() == 3)
        .ok_or_else(|| {
            error(
                "invalid_arguments",
                "Expected exactly three numeric channels",
            )
        })?;
    for channel in vector {
        finite(channel, min, max)?;
    }
    Ok(())
}
fn light_transform(value: &Value) -> Result<(), CommandError> {
    vector(&value["translation"], -10_000.0, 10_000.0)?;
    vector(&value["rotation"], -10_000.0, 10_000.0)?;
    vector(&value["scale"], 1.0, 1.0).map_err(|_| {
        error(
            "invalid_arguments",
            "Lights cannot be scaled; keep unit scale and use intensity or range",
        )
    })
}

pub(super) fn validate_transform(
    scene: &ruster_scene::Scene,
    id: &str,
    value: &Value,
) -> Result<(), CommandError> {
    if scene.lighting.light(id).is_some() {
        light_transform(value)?;
    }
    Ok(())
}

fn light(value: &Value) -> Result<Light, CommandError> {
    // Check original f64 values before serde's f32 narrowing can hide an invalid
    // bound, especially unit scale, tiny negative colors and point range.
    vector(&value["color"], 0.0, 1.0)?;
    finite(&value["intensity"], 0.0, 16.0)?;
    light_transform(&value["transform"])?;
    if value["kind"]["type"] == "point" {
        finite(&value["kind"]["range"], 0.1, 10_000.0)?;
    }
    if let Some(shadow) = value["kind"].get("shadow") {
        if value["kind"]["type"] != "directional" {
            return Err(error(
                "invalid_arguments",
                "Only directional lights support Sun shadow settings",
            ));
        }
        // Preserve original-f64 admission: values just outside0..4 must not
        // become accepted by rounding to a valid f32. Typed Light decoding
        // below owns the complete strict shape, bool and quality enum checks.
        finite(&shadow["depth_bias_texels"], 0.0, 4.0)?;
        finite(&shadow["normal_bias_texels"], 0.0, 4.0)?;
    }
    let candidate: Light = serde_json::from_value(value.clone())
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    candidate
        .validate()
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    Ok(candidate)
}
fn ambient(value: &Value) -> Result<Ambient, CommandError> {
    vector(&value["color"], 0.0, 1.0)?;
    finite(&value["intensity"], 0.0, 1.0)?;
    let candidate: Ambient = serde_json::from_value(value.clone())
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    candidate
        .validate()
        .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
    Ok(candidate)
}
fn scene_error(failure: SceneError) -> CommandError {
    let code = match failure {
        SceneError::LightNotFound(_) => "light_not_found",
        SceneError::StaleRevision { .. } => "stale_revision",
        SceneError::RevisionOverflow => "revision_exhausted",
        _ => "invalid_arguments",
    };
    error(code, failure.to_string())
}

impl SceneEditor {
    pub(super) fn lighting_command(
        &mut self,
        token: &str,
        op: &str,
        args: &Value,
    ) -> Result<Value, CommandError> {
        let required: &[&str] = match op {
            "lighting_inspect" => &[],
            "scene_light_create" | "scene_light_set" => &[
                "expected_session_token",
                "expected_revision",
                "light_id",
                "light",
            ],
            "scene_ambient_set" => &["expected_session_token", "expected_revision", "ambient"],
            _ => return Err(error("invalid_arguments", "Unknown lighting operation")),
        };
        args.as_object()
            .filter(|map| {
                map.len() == required.len() && required.iter().all(|key| map.contains_key(*key))
            })
            .ok_or_else(|| {
                error(
                    "invalid_arguments",
                    "Unexpected or missing lighting arguments",
                )
            })?;
        if op == "lighting_inspect" {
            let scene = self.session.scene();
            let directional = scene
                .lighting
                .lights()
                .filter(|(_, light)| matches!(light.kind, LightKind::Directional { .. }))
                .count();
            let point = scene.lighting.light_count() - directional;
            let enabled = scene
                .lighting
                .lights()
                .filter(|(_, light)| light.enabled)
                .count();
            let legacy = scene.lighting.is_legacy();
            let length = (0.4_f32 * 0.4 + 0.8 * 0.8 + 0.6 * 0.6).sqrt();
            return Ok(
                json!({"session_token":token,"revision":scene.revision.to_string(),"dirty":self.session.is_dirty(),
                "mode":if legacy {"legacy"} else {"authored"},"lighting":scene.lighting,
                "effective_legacy_builtin":legacy.then(|| json!({"ambient":Ambient::default(),"incoming_direction":[0.4/length,0.8/length,0.6/length],"color":[1,1,1],"intensity":0.72,"synthesized_scene_light":false})),
                "counts":{"total":scene.lighting.light_count(),"directional":directional,"point":point,"enabled":enabled,"authoring_targets":scene.target_count()},
                "limits":{"directional":MAX_DIRECTIONAL_LIGHTS,"point":MAX_POINT_LIGHTS,"total_lights":MAX_LIGHTS,"authoring_targets":MAX_OBJECTS,"disabled_lights_count_toward_capacity":true},
                "active_shader":if self.shader_builtin {"builtin"} else {"custom"},"builtin_appearance":self.shader_builtin,
                "lighting_appearance":if self.shader_builtin {"Built-in diffuse lighting with authored Sun-shadow support; last-rendered status is not observed here"} else {"Active custom shader controls diffuse appearance and may ignore authored lighting; Sun shadows are unsupported and authored settings are retained"},
                "sun_shadow_contract":{"light_kind":"directional","quality_map_sizes":{"standard":1024,"high":2048},
                    "bias_bounds_texels":[0,4],"bias_units":"World shadow texels, not normalized depth or screen pixels",
                    "omitted_shadow_default":SunShadow::default(),"present_shadow_requires_all_fields":true,
                    "shader_support":if self.shader_builtin {"builtin_pipeline"} else {"unsupported_custom_shader"},
                    "point_shadows_supported":false,"observation":"Authored settings and pipeline support only; no last-rendered shadow status"},
                "units":"Relative artistic linear RGB diffuse, not photometric units; Sun biases use world shadow texels",
                "drafts_pending":self.drafts()||self.manipulation.active()||self.manipulation.offset_pending()}),
            );
        }
        let expected_token = args["expected_session_token"]
            .as_str()
            .filter(|text| !text.is_empty() && text.len() <= 256)
            .ok_or_else(|| error("invalid_arguments", "Expected a bounded session token"))?;
        if expected_token != token {
            return Err(error("stale_session", "Inspect the current scene session"));
        }
        self.guard()
            .and_then(|()| {
                if self.manipulation.offset_pending() || self.lighting_draft.is_some() {
                    Err("drafts_pending".into())
                } else {
                    Ok(())
                }
            })
            .map_err(|code| {
                error(
                    &code,
                    "Apply/cancel drafts or resolve dialogs, then inspect again",
                )
            })?;
        let revision = number(&args["expected_revision"])?;
        if revision != self.session.scene().revision {
            return Err(error("stale_revision", "Inspect authored lighting again"));
        }
        let mut created_ids = Vec::new();
        let mut light_id = None;
        let result = match op {
            "scene_light_create" | "scene_light_set" => {
                let id = args["light_id"]
                    .as_str()
                    .ok_or_else(|| error("invalid_arguments", "Expected stable light ID"))?;
                ruster_scene::validate_id(id)
                    .map_err(|failure| error("invalid_arguments", failure.to_string()))?;
                let candidate = light(&args["light"])?;
                light_id = Some(id.to_owned());
                if op == "scene_light_create" {
                    let edit = self
                        .session
                        .create_light(revision, id, candidate)
                        .map_err(scene_error)?;
                    created_ids = edit.created_ids;
                    edit.revision
                } else {
                    self.session
                        .set_light(revision, id, candidate)
                        .map_err(scene_error)?
                }
            }
            "scene_ambient_set" => {
                let candidate = ambient(&args["ambient"])?;
                let edit = self
                    .session
                    .set_ambient(revision, candidate)
                    .map_err(scene_error)?;
                created_ids = edit.created_ids;
                edit.revision
            }
            _ => unreachable!(),
        };
        if result != revision {
            if op == "scene_light_create" {
                self.select_object(light_id.clone(), false);
            } else {
                self.sync_draft();
            }
            self.notice = format!("Agent {op}: revision {result}. Authored lighting changed; preview unchanged. Created lights: {}.", created_ids.join(", "));
        }
        Ok(
            json!({"session_token":token,"revision":result.to_string(),"changed":result!=revision,"light_id":light_id,"created_ids":created_ids,"lighting":self.session.scene().lighting,"dirty":self.session.is_dirty()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::lighting_ui::LightingDraft;
    use super::*;
    #[test]
    fn commands_guard_human_authority_revision_and_capacity_without_preview_mutation() {
        let mut editor = SceneEditor::new(
            ruster_scene::Session::new(ruster_scene::Scene::empty("commands")).unwrap(),
        );
        let preview = editor.preview.clone();
        let make = |revision: u64, id: &str| json!({"expected_session_token":"scene","expected_revision":revision.to_string(),"light_id":id,"light":Light::point(None)});
        let result = editor
            .lighting_command("scene", "scene_light_create", &make(0, "key"))
            .unwrap();
        assert_eq!(result["created_ids"].as_array().unwrap().len(), 2);
        assert_eq!(editor.session.undo_len(), 1);
        let before = editor.session.snapshot_bytes().unwrap();
        assert_eq!(
            editor
                .lighting_command("scene", "scene_light_set", &make(0, "key"))
                .unwrap_err()
                .0,
            "stale_revision"
        );
        let mut invalid = make(1, "key");
        invalid["light"]["transform"]["scale"][0] = json!(1.0000000001);
        assert_eq!(
            editor
                .lighting_command("scene", "scene_light_set", &invalid)
                .unwrap_err()
                .0,
            "invalid_arguments"
        );
        invalid = make(1, "key");
        invalid["expected_session_token"] = json!("old");
        assert_eq!(
            editor
                .lighting_command("scene", "scene_light_set", &invalid)
                .unwrap_err()
                .0,
            "stale_session"
        );
        editor.lighting_draft = Some(LightingDraft::ambient(editor.session.scene()));
        assert_eq!(
            editor
                .lighting_command("scene", "scene_light_set", &make(1, "key"))
                .unwrap_err()
                .0,
            "drafts_pending"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), before);
        editor.lighting_draft = None;
        for index in 1..4 {
            let revision = editor.session.scene().revision;
            editor
                .lighting_command(
                    "scene",
                    "scene_light_create",
                    &make(revision, &format!("point-{index}")),
                )
                .unwrap();
        }
        let full = editor.session.snapshot_bytes().unwrap();
        let revision = editor.session.scene().revision;
        assert!(editor
            .lighting_command("scene", "scene_light_create", &make(revision, "overflow"))
            .is_err());
        assert_eq!(editor.session.snapshot_bytes().unwrap(), full);
        assert_eq!(editor.preview, preview);
        // Generic transform aliases must preserve the same original-f64 light
        // bounds as the full light command, before serde narrows to f32.
        for batch in [false, true] {
            let mut transform = serde_json::to_value(
                &editor
                    .session
                    .scene()
                    .lighting
                    .light("key")
                    .unwrap()
                    .transform,
            )
            .unwrap();
            transform["scale"][0] = json!(1.0000000001);
            let (op, args) = if batch {
                (
                    "scene_transform_many",
                    json!({"expected_session_token":"scene","expected_revision":revision.to_string(),"transforms":[{"object_id":"key","transform":transform}]}),
                )
            } else {
                (
                    "scene_transform",
                    json!({"expected_session_token":"scene","expected_revision":revision.to_string(),"object_id":"key","transform":transform}),
                )
            };
            assert_eq!(
                editor.execute("scene", op, &args).unwrap_err().0,
                "invalid_arguments"
            );
            assert_eq!(editor.session.snapshot_bytes().unwrap(), full);
        }
        let mut transform = editor
            .session
            .scene()
            .lighting
            .light("key")
            .unwrap()
            .transform
            .clone();
        transform.translation[0] = 1.;
        let moved = editor.execute("scene", "scene_transform", &json!({"expected_session_token":"scene","expected_revision":revision.to_string(),"object_id":"key","transform":transform})).unwrap();
        assert_eq!(moved["changed_target_count"], 1);
        assert_eq!(moved["light_diff"][0]["light_id"], "key");
        assert_eq!(moved["lighting_changed"], true);
        assert_eq!(editor.preview, preview);
        // A full directional shadow transaction shares history, while the raw
        // command boundary rejects f64 biases that would round into range.
        let sun_id = editor
            .session
            .scene()
            .lighting
            .lights()
            .find(|(_, light)| matches!(light.kind, LightKind::Directional { .. }))
            .unwrap()
            .0
            .to_owned();
        let old_sun = editor
            .session
            .scene()
            .lighting
            .light(&sun_id)
            .unwrap()
            .clone();
        let mut shadowed = serde_json::to_value(&old_sun).unwrap();
        shadowed["kind"]["shadow"] = json!({"enabled":true,"quality":"high","depth_bias_texels":0.15,"normal_bias_texels":0});
        let shadow_args = json!({"expected_session_token":"scene","expected_revision":editor.session.scene().revision.to_string(),
            "light_id":sun_id,"light":shadowed});
        editor
            .lighting_command("scene", "scene_light_set", &shadow_args)
            .unwrap();
        let authored_shadow = editor
            .session
            .scene()
            .lighting
            .light(&sun_id)
            .unwrap()
            .clone();
        let after_shadow = editor.session.snapshot_bytes().unwrap();
        for field in ["depth_bias_texels", "normal_bias_texels"] {
            let mut invalid = shadow_args.clone();
            invalid["expected_revision"] = json!(editor.session.scene().revision.to_string());
            invalid["light"]["kind"]["shadow"][field] = json!(4.0000000001);
            assert_eq!(
                editor
                    .lighting_command("scene", "scene_light_set", &invalid)
                    .unwrap_err()
                    .0,
                "invalid_arguments"
            );
            assert_eq!(editor.session.snapshot_bytes().unwrap(), after_shadow);
        }
        let mut point_shadow = make(editor.session.scene().revision, "key");
        point_shadow["light"]["kind"]["shadow"] = shadow_args["light"]["kind"]["shadow"].clone();
        assert_eq!(
            editor
                .lighting_command("scene", "scene_light_set", &point_shadow)
                .unwrap_err()
                .0,
            "invalid_arguments"
        );
        assert_eq!(editor.session.snapshot_bytes().unwrap(), after_shadow);
        editor.execute("scene", "scene_undo", &json!({"expected_session_token":"scene","expected_revision":editor.session.scene().revision.to_string()})).unwrap();
        assert_eq!(
            editor.session.scene().lighting.light(&sun_id),
            Some(&old_sun)
        );
        editor.execute("scene", "scene_redo", &json!({"expected_session_token":"scene","expected_revision":editor.session.scene().revision.to_string()})).unwrap();
        assert_eq!(
            editor.session.scene().lighting.light(&sun_id),
            Some(&authored_shadow)
        );
        assert_eq!(editor.preview, preview);
        editor.shader_builtin = false;
        editor.shader_notice = "Rejected candidate; last-good custom still active".into();
        assert_eq!(
            editor
                .lighting_command("scene", "lighting_inspect", &json!({}))
                .unwrap()["active_shader"],
            "custom"
        );
        let inspection = editor
            .lighting_command("scene", "lighting_inspect", &json!({}))
            .unwrap();
        assert_eq!(
            inspection["sun_shadow_contract"]["shader_support"],
            "unsupported_custom_shader"
        );
        assert_eq!(
            inspection["lighting"]["lights"][&sun_id]["kind"]["shadow"]["enabled"],
            true
        );
        assert!(editor
            .lighting_command("scene", "lighting_inspect", &json!({"path":"ignored"}))
            .is_err());
    }
}
